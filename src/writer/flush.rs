use crate::db::DbPool;
use crate::ingest::models::StorableEvent;
use crate::queries::event_writes;
use anyhow::Result;
use std::time::{Duration, Instant};

use super::accumulator::Accumulators;
use super::aggregation::flush_aggregation_inner;
use super::alerting::check_threshold_alerts;
use super::msg::WriteMsg;

/// Compress event payloads with zstd. A fallback: the accept path already
/// compresses on send, so this is a no-op for events that came through the
/// `WriterHandle`. Moves any remaining CPU-bound compression off the async
/// runtime's cooperative budget where the runtime allows it.
fn compress_batch(batch: &mut [WriteMsg]) {
    super::block_in_place_if_multi_thread(|| {
        for msg in batch.iter_mut() {
            if let WriteMsg::Event(event) | WriteMsg::EventWithAttachments(event, _) = msg {
                event.compress_payload();
            }
        }
    });
}

/// Per-stage durations of one flush transaction, for bottleneck diagnosis.
#[derive(Default)]
struct TxTimings {
    insert: Duration,
    agg: Duration,
    commit: Duration,
}

fn log_flush_timings(items: usize, agg: bool, compress: Duration, t: &TxTimings) {
    tracing::debug!(
        items,
        agg,
        compress_us = compress.as_micros() as u64,
        insert_us = t.insert.as_micros() as u64,
        agg_us = t.agg.as_micros() as u64,
        commit_us = t.commit.as_micros() as u64,
        "batch flush timings"
    );
}

/// Flush batch of events + aggregated data in one transaction (retry-safe on failure).
#[cfg(any(feature = "sqlite", test))]
pub(super) async fn flush_batch(
    pool: &DbPool,
    batch: &mut [WriteMsg],
    accumulators: &mut Accumulators,
    notify_tx: Option<&tokio::sync::mpsc::Sender<crate::notify::NotificationEvent>>,
) -> bool {
    if batch.is_empty() {
        return true;
    }

    let compress_started = Instant::now();
    compress_batch(batch);
    let compress = compress_started.elapsed();

    // should_flush is evaluated on the pre-batch state, matching prior behavior.
    let should_agg = accumulators.should_flush();

    let mut pending = Vec::new();

    if !should_agg {
        // Common path: no clone; do_flush_tx merges the batch scratch into the accumulators only after its commit succeeds, so a failed attempt leaves them clean and the retry is idempotent.
        match do_flush_tx(pool, batch, false, accumulators, &mut pending).await {
            Ok(t) => {
                log_flush_timings(batch.len(), false, compress, &t);
                return true;
            }
            Err(e) => {
                tracing::warn!("batch flush failed, retrying once: {e}");
            }
        }
        match do_flush_tx(pool, batch, false, accumulators, &mut pending).await {
            Ok(t) => {
                log_flush_timings(batch.len(), false, compress, &t);
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
            Ok(t) => {
                finalize_agg_flush(accumulators, pending, notify_tx);
                log_flush_timings(batch.len(), true, compress, &t);
                return true;
            }
            Err(e) => {
                tracing::warn!("batch flush failed, retrying once: {e}");
                pending.clear();
                *accumulators = snapshot.clone();
            }
        }
        match do_flush_tx(pool, batch, true, accumulators, &mut pending).await {
            Ok(t) => {
                finalize_agg_flush(accumulators, pending, notify_tx);
                log_flush_timings(batch.len(), true, compress, &t);
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
#[cfg(any(feature = "sqlite", test))]
async fn do_flush_tx(
    pool: &DbPool,
    batch: &[WriteMsg],
    should_agg: bool,
    accumulators: &mut Accumulators,
    pending: &mut Vec<crate::notify::NotificationEvent>,
) -> Result<TxTimings> {
    let mut timings = TxTimings::default();
    let insert_started = Instant::now();
    let mut tx = pool.begin().await?;
    let new_ids = do_flush_inner(&mut tx, batch).await?;
    timings.insert = insert_started.elapsed();

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
        let agg_started = Instant::now();
        let threshold_candidates = flush_aggregation_inner(&mut tx, accumulators, pending).await?;
        timings.agg = agg_started.elapsed();
        let commit_started = Instant::now();
        tx.commit().await?;
        timings.commit = commit_started.elapsed();
        // Threshold checks run outside the write TX against the pool
        if !threshold_candidates.is_empty() {
            check_threshold_alerts(pool, &threshold_candidates, pending).await;
        }
    } else {
        let commit_started = Instant::now();
        tx.commit().await?;
        timings.commit = commit_started.elapsed();
        accumulators.merge(&scratch);
    }
    Ok(timings)
}

/// Insert-only flush for the split pipeline: one transaction of bulk inserts,
/// no aggregation SQL. Returns the batch-local scratch of genuinely-new events
/// for the aggregation task, or None after a failed inline retry (caller
/// re-queues the batch). Compression is a no-op fallback here.
#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
pub(super) async fn flush_batch_insert_only(
    pool: &DbPool,
    batch: &mut [WriteMsg],
) -> Option<Accumulators> {
    if batch.is_empty() {
        return Some(Accumulators::new());
    }

    let compress_started = Instant::now();
    compress_batch(batch);
    let compress = compress_started.elapsed();

    match do_insert_tx(pool, batch).await {
        Ok((scratch, t)) => {
            log_flush_timings(batch.len(), false, compress, &t);
            return Some(scratch);
        }
        Err(e) => {
            tracing::warn!("batch flush failed, retrying once: {e}");
        }
    }
    match do_insert_tx(pool, batch).await {
        Ok((scratch, t)) => {
            log_flush_timings(batch.len(), false, compress, &t);
            tracing::info!("batch flush retry succeeded ({} items)", batch.len());
            Some(scratch)
        }
        Err(e2) => {
            tracing::error!(
                "batch flush failed after retry ({} items), pending re-queue: {e2}",
                batch.len()
            );
            None
        }
    }
}

#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
async fn do_insert_tx(pool: &DbPool, batch: &[WriteMsg]) -> Result<(Accumulators, TxTimings)> {
    let mut timings = TxTimings::default();
    let insert_started = Instant::now();
    let mut tx = pool.begin().await?;
    let new_ids = do_flush_inner(&mut tx, batch).await?;
    timings.insert = insert_started.elapsed();
    let commit_started = Instant::now();
    tx.commit().await?;
    timings.commit = commit_started.elapsed();

    let mut scratch = Accumulators::new();
    for msg in batch {
        if let WriteMsg::Event(event) | WriteMsg::EventWithAttachments(event, _) = msg {
            if new_ids.contains(&event.event_id) {
                scratch.accumulate(event);
            }
        }
    }
    Ok((scratch, timings))
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
    let agg_started = Instant::now();
    let mut tx = pool.begin().await?;
    let threshold_candidates = flush_aggregation_inner(&mut tx, accumulators, &mut pending).await?;
    tx.commit().await?;
    tracing::debug!(
        agg_us = agg_started.elapsed().as_micros() as u64,
        "standalone aggregation flush timings"
    );

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

    // Regression: block_in_place panics on a current-thread runtime; the shared
    // guard must fall back to running inline.
    #[tokio::test]
    async fn compress_batch_works_on_current_thread_runtime() {
        let json = serde_json::json!({"event_id": "ct1", "message": "current thread"});
        let raw = serde_json::to_vec(&json).unwrap();
        let event = StorableEvent::new("ct1".to_string(), ItemType::Event, raw, 1, "k".to_string());
        let mut batch = vec![WriteMsg::Event(event)];

        compress_batch(&mut batch);

        let WriteMsg::Event(e) = &batch[0] else {
            panic!("expected event");
        };
        assert!(e.compressed);
    }
}
