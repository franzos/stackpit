use crate::db::DbPool;
use crate::ingest::models::StorableEvent;
use crate::queries::event_writes;
use anyhow::Result;
use std::time::Instant;

use super::accumulator::Accumulators;
use super::aggregation::flush_aggregation_inner;
use super::alerting::check_threshold_alerts;
use super::msg::WriteMsg;

/// Compress event payloads with zstd. Uses `block_in_place` to move the
/// CPU-bound compression off the async runtime's cooperative budget.
fn compress_batch(batch: &mut [WriteMsg]) {
    tokio::task::block_in_place(|| {
        for msg in batch.iter_mut() {
            match msg {
                WriteMsg::Event(event) | WriteMsg::EventWithAttachments(event, _)
                    if !event.compressed =>
                {
                    match zstd::encode_all(event.payload.as_slice(), 3) {
                        Ok(compressed) => {
                            event.payload = compressed;
                            event.compressed = true;
                        }
                        Err(e) => {
                            tracing::warn!(
                                event_id = %event.event_id,
                                item_type = %event.item_type,
                                payload_len = event.payload.len(),
                                "zstd compression failed, storing uncompressed: {e}"
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    });
}

/// Flush batch of events + aggregated data in one transaction (retry-safe on failure).
pub(super) async fn flush_batch(
    pool: &DbPool,
    batch: &mut [WriteMsg],
    accumulators: &mut Accumulators,
    notify_tx: Option<&tokio::sync::mpsc::Sender<crate::notify::NotificationEvent>>,
) -> bool {
    if batch.is_empty() {
        return true;
    }

    compress_batch(batch);

    // should_flush is evaluated on the pre-batch state, matching prior behavior.
    let should_agg = accumulators.should_flush();

    let mut pending = Vec::new();

    if !should_agg {
        // Common path: no clone; do_flush_tx merges the batch scratch into the accumulators only after its commit succeeds, so a failed attempt leaves them clean and the retry is idempotent.
        match do_flush_tx(pool, batch, false, accumulators, &mut pending).await {
            Ok(()) => {
                tracing::debug!("flushed batch of {} items", batch.len());
                return true;
            }
            Err(e) => {
                tracing::warn!("batch flush failed, retrying once: {e}");
            }
        }
        match do_flush_tx(pool, batch, false, accumulators, &mut pending).await {
            Ok(()) => {
                tracing::info!("batch flush retry succeeded ({} items)", batch.len());
                true
            }
            Err(e2) => {
                tracing::error!(
                    "batch flush failed after retry ({} items), pending re-queue: {e2}",
                    batch.len()
                );
                false
            }
        }
    } else {
        // Aggregation path: do_flush_tx merges the batch scratch into the accumulators before the aggregation flush, so keep a snapshot to revert on failure (neither double-counting nor losing prior deltas).
        let snapshot = accumulators.clone();
        match do_flush_tx(pool, batch, true, accumulators, &mut pending).await {
            Ok(()) => {
                finalize_agg_flush(accumulators, pending, notify_tx);
                tracing::debug!("flushed batch of {} items", batch.len());
                return true;
            }
            Err(e) => {
                tracing::warn!("batch flush failed, retrying once: {e}");
                pending.clear();
                *accumulators = snapshot.clone();
            }
        }
        match do_flush_tx(pool, batch, true, accumulators, &mut pending).await {
            Ok(()) => {
                finalize_agg_flush(accumulators, pending, notify_tx);
                tracing::info!("batch flush retry succeeded ({} items)", batch.len());
                true
            }
            Err(e2) => {
                tracing::error!(
                    "batch flush failed after retry ({} items), pending re-queue: {e2}",
                    batch.len()
                );
                *accumulators = snapshot;
                false
            }
        }
    }
}

/// Inserts events and (if ready) flushes aggregated issue/tag/session/txn data in one transaction, building a batch-local scratch from only the rows the insert actually created (SDK-retried duplicates are dropped by the conflict clause and must not inflate counts) and folding it into `accumulators` after commit on the non-agg path, or before the aggregation flush on the agg path so the flush includes this batch.
async fn do_flush_tx(
    pool: &DbPool,
    batch: &[WriteMsg],
    should_agg: bool,
    accumulators: &mut Accumulators,
    pending: &mut Vec<crate::notify::NotificationEvent>,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let new_ids = do_flush_inner(&mut tx, batch).await?;

    let mut scratch = Accumulators::new();
    for msg in batch {
        if let WriteMsg::Event(event) | WriteMsg::EventWithAttachments(event, _) = msg {
            if new_ids.contains(&event.event_id) {
                scratch.accumulate(event);
            }
        }
    }

    if should_agg {
        accumulators.merge(&scratch);
        let threshold_candidates = flush_aggregation_inner(&mut tx, accumulators, pending).await?;
        tx.commit().await?;
        // Threshold checks run outside the write TX against the pool
        if !threshold_candidates.is_empty() {
            check_threshold_alerts(pool, &threshold_candidates, pending).await;
        }
    } else {
        tx.commit().await?;
        accumulators.merge(&scratch);
    }
    Ok(())
}

/// Post-commit protocol for an aggregation flush: clear the drained deltas, emit notifications, and reset the flush timer.
fn finalize_agg_flush(
    accumulators: &mut Accumulators,
    pending: Vec<crate::notify::NotificationEvent>,
    notify_tx: Option<&tokio::sync::mpsc::Sender<crate::notify::NotificationEvent>>,
) {
    accumulators.issues.clear();
    accumulators.tags.clear();
    accumulators.session_aggregates.clear();
    accumulators.transaction_metrics.clear();
    send_notifications(pending, notify_tx);
    accumulators.last_flush = Instant::now();
}

/// Does the actual event/attachment inserts inside a transaction.
///
/// Collects all events into a single multi-row INSERT for throughput,
/// then handles attachments individually (they're rare).
///
/// Returns the set of events-table `event_id`s that were genuinely inserted, so the caller accumulates issue/tag/session/txn rollups for new rows only.
async fn do_flush_inner(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    batch: &[WriteMsg],
) -> Result<std::collections::HashSet<String>> {
    let mut all_events: Vec<&StorableEvent> = Vec::with_capacity(batch.len());
    let mut attachment_msgs: Vec<usize> = Vec::new();

    for (i, msg) in batch.iter().enumerate() {
        match msg {
            WriteMsg::Event(event) => {
                all_events.push(event);
            }
            WriteMsg::EventWithAttachments(event, _) => {
                all_events.push(event);
                attachment_msgs.push(i);
            }
            _ => {}
        }
    }

    let new_ids = event_writes::insert_event_rows_bulk(tx, &all_events).await?;

    for &idx in &attachment_msgs {
        if let WriteMsg::EventWithAttachments(_, attachments) = &batch[idx] {
            for att in attachments {
                event_writes::insert_attachment(&mut **tx, att).await?;
            }
        }
    }

    Ok(new_ids)
}

/// Inserts a single event row using the pool directly. Returns true if new.
/// Test-only thin wrapper around `event_writes::insert_event_row`.
#[cfg(test)]
pub(super) async fn insert_event(pool: &DbPool, event: &StorableEvent) -> Result<bool> {
    event_writes::insert_event_row(pool, event).await
}

/// Flushes accumulated issue deltas, HLL merges, and tag counts.
pub(super) async fn flush_aggregation(
    pool: &DbPool,
    accumulators: &mut Accumulators,
    notify_tx: Option<&tokio::sync::mpsc::Sender<crate::notify::NotificationEvent>>,
) -> Result<()> {
    if accumulators.issues.is_empty()
        && accumulators.tags.is_empty()
        && accumulators.session_aggregates.is_empty()
        && accumulators.transaction_metrics.is_empty()
    {
        accumulators.last_flush = Instant::now();
        return Ok(());
    }

    let mut pending = Vec::new();
    let mut tx = pool.begin().await?;
    let threshold_candidates = flush_aggregation_inner(&mut tx, accumulators, &mut pending).await?;
    tx.commit().await?;

    // Threshold checks run outside the write TX against the pool
    if !threshold_candidates.is_empty() {
        check_threshold_alerts(pool, &threshold_candidates, &mut pending).await;
    }

    finalize_agg_flush(accumulators, pending, notify_tx);
    Ok(())
}

fn send_notifications(
    notifications: Vec<crate::notify::NotificationEvent>,
    notify_tx: Option<&tokio::sync::mpsc::Sender<crate::notify::NotificationEvent>>,
) {
    if let Some(tx) = notify_tx {
        for event in notifications {
            if let Err(e) = tx.try_send(event) {
                tracing::warn!("notify: dropped notification (channel full): {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::models::{ItemType, StorableEvent};
    use crate::queries::events::decompress_payload;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn compress_batch_is_idempotent() {
        let json = serde_json::json!({"event_id": "idem1", "message": "hello world"});
        let raw = serde_json::to_vec(&json).unwrap();
        let event = StorableEvent::new(
            "idem1".to_string(),
            ItemType::Event,
            raw,
            1,
            "k".to_string(),
        );
        let mut batch = vec![WriteMsg::Event(event)];

        // Two passes simulate the retry_pending re-entry into flush_batch.
        compress_batch(&mut batch);
        compress_batch(&mut batch);

        let WriteMsg::Event(e) = &batch[0] else {
            panic!("expected event");
        };
        let decoded = decompress_payload(&e.payload).expect("payload must round-trip");
        assert_eq!(
            decoded, json,
            "double-compressed payload must decode to original"
        );
    }
}
