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
                WriteMsg::Event(event) | WriteMsg::EventWithAttachments(event, _) => {
                    match zstd::encode_all(event.payload.as_slice(), 3) {
                        Ok(compressed) => {
                            event.payload = compressed;
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

    let should_agg = accumulators.should_flush();
    let mut pending_notifications = Vec::new();

    let result = do_flush_tx(
        pool,
        batch,
        should_agg,
        accumulators,
        &mut pending_notifications,
    )
    .await;

    match result {
        Ok(()) => {
            tracing::debug!("flushed batch of {} items", batch.len());
            if should_agg {
                accumulators.issues.clear();
                accumulators.tags.clear();
                accumulators.session_aggregates.clear();
                accumulators.transaction_metrics.clear();
                send_notifications(pending_notifications, notify_tx);
                accumulators.last_flush = Instant::now();
            }
            true
        }
        Err(e) => {
            tracing::warn!("batch flush failed, retrying once: {e}");
            pending_notifications.clear();
            match do_flush_tx(
                pool,
                batch,
                should_agg,
                accumulators,
                &mut pending_notifications,
            )
            .await
            {
                Ok(()) => {
                    tracing::info!("batch flush retry succeeded ({} items)", batch.len());
                    if should_agg {
                        accumulators.issues.clear();
                        accumulators.tags.clear();
                        accumulators.session_aggregates.clear();
                        accumulators.transaction_metrics.clear();
                        send_notifications(pending_notifications, notify_tx);
                        accumulators.last_flush = Instant::now();
                    }
                    true
                }
                Err(e2) => {
                    tracing::error!(
                        "batch flush failed after retry ({} items), pending re-queue: {e2}",
                        batch.len()
                    );
                    if should_agg {
                        accumulators.issues.clear();
                        accumulators.tags.clear();
                        accumulators.session_aggregates.clear();
                        accumulators.transaction_metrics.clear();
                        accumulators.last_flush = Instant::now();
                    }
                    false
                }
            }
        }
    }
}

/// Inserts events, accumulates them, and (if ready) flushes aggregated
/// issue/tag data -- all in one transaction. Accumulation happens after
/// event insertion but before the aggregation flush so that events in the
/// current batch are always included when their issues are upserted.
async fn do_flush_tx(
    pool: &DbPool,
    batch: &[WriteMsg],
    should_agg: bool,
    accumulators: &mut Accumulators,
    pending: &mut Vec<crate::notify::NotificationEvent>,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let new_events = do_flush_inner(&mut tx, batch).await?;

    for event in &new_events {
        accumulators.accumulate(event);
    }

    let threshold_candidates = if should_agg {
        flush_aggregation_inner(&mut tx, accumulators, pending).await?
    } else {
        Vec::new()
    };
    tx.commit().await?;
    // Threshold checks run outside the write TX against the pool
    if !threshold_candidates.is_empty() {
        check_threshold_alerts(pool, &threshold_candidates, pending).await;
    }
    Ok(())
}

/// Does the actual event/attachment inserts inside a transaction.
///
/// Collects all events into a single multi-row INSERT for throughput,
/// then handles attachments individually (they're rare).
///
/// Returns references to inserted events (all are considered new since
/// duplicate event IDs are rare UUIDs).
async fn do_flush_inner<'a>(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    batch: &'a [WriteMsg],
) -> Result<Vec<&'a StorableEvent>> {
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

    event_writes::insert_event_rows_bulk(tx, &all_events).await?;

    for &idx in &attachment_msgs {
        if let WriteMsg::EventWithAttachments(_, attachments) = &batch[idx] {
            for att in attachments {
                event_writes::insert_attachment(&mut **tx, att).await?;
            }
        }
    }

    Ok(all_events)
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

    accumulators.issues.clear();
    accumulators.tags.clear();
    accumulators.session_aggregates.clear();
    accumulators.transaction_metrics.clear();
    send_notifications(pending, notify_tx);
    accumulators.last_flush = Instant::now();
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
