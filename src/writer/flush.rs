use crate::db::DbPool;
use crate::ingest::models::StorableEvent;
use crate::ingest::parse_log::compress_log_bytes;
use crate::queries::event_writes;
use anyhow::Result;
use std::time::{Duration, Instant};

use super::accumulator::Accumulators;
use super::aggregation::flush_aggregation_inner;
use super::alerting::check_threshold_alerts;
use super::msg::WriteMsg;

/// Compress event payloads and pre-extracted log entries with zstd. For event
/// payloads this is a fallback: the accept path already compresses on send, so
/// it is a no-op for events that came through the `WriterHandle`. Log entries
/// are always compressed here, one zstd frame per entry. Moves the CPU-bound
/// work off the async runtime's cooperative budget where the runtime allows
/// it. Shares `should_compress_payload` with the accept path so the two can't
/// diverge.
fn compress_batch(batch: &mut [WriteMsg]) {
    // The retry re-entry arrives fully compressed; skip the blocking section then.
    let pending = batch.iter().any(|msg| match msg {
        WriteMsg::Event(event) | WriteMsg::EventWithAttachments(event, _) => {
            (!event.compressed && event.should_compress_payload())
                || has_uncompressed_log_entry(event)
        }
        _ => false,
    });
    if !pending {
        return;
    }
    super::block_in_place_if_multi_thread(|| {
        for msg in batch.iter_mut() {
            if let WriteMsg::Event(event) | WriteMsg::EventWithAttachments(event, _) = msg {
                if event.should_compress_payload() {
                    event.compress_payload();
                }
                for entry in event.log_entries.iter_mut().flatten() {
                    if !entry.compressed {
                        entry.payload = compress_log_bytes(&entry.payload);
                        entry.compressed = true;
                    }
                }
            }
        }
    });
}

fn has_uncompressed_log_entry(event: &StorableEvent) -> bool {
    event
        .log_entries
        .iter()
        .flatten()
        .any(|entry| !entry.compressed)
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
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
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
    let mut merged = false;

    if !should_agg {
        // Common path: no clone; do_flush_tx merges the batch scratch into the accumulators only after its commit succeeds, so a failed attempt leaves them clean and the retry is idempotent.
        match do_flush_tx(pool, batch, false, accumulators, &mut pending, &mut merged).await {
            Ok(t) => {
                log_flush_timings(batch.len(), false, compress, &t);
                return true;
            }
            Err(e) => {
                tracing::warn!("batch flush failed, retrying once: {e}");
            }
        }
        match do_flush_tx(pool, batch, false, accumulators, &mut pending, &mut merged).await {
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
        // Aggregation path: do_flush_tx merges the batch scratch into the accumulators before the aggregation flush, so keep a snapshot to revert on failure (neither double-counting nor losing prior deltas). Restores happen only when the failed attempt actually merged, and only the intermediate one clones (`snapshot` must survive for the retry's rollback); the final restore moves.
        let snapshot = accumulators.clone();
        match do_flush_tx(pool, batch, true, accumulators, &mut pending, &mut merged).await {
            Ok(t) => {
                finalize_agg_flush(accumulators, pending, notify_tx);
                log_flush_timings(batch.len(), true, compress, &t);
                return true;
            }
            Err(e) => {
                tracing::warn!("batch flush failed, retrying once: {e}");
                pending.clear();
                if merged {
                    accumulators.clone_from(&snapshot);
                }
            }
        }
        merged = false;
        match do_flush_tx(pool, batch, true, accumulators, &mut pending, &mut merged).await {
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
                if merged {
                    *accumulators = snapshot;
                }
                false
            }
        }
    }
}

/// Inserts events and (if ready) flushes aggregated issue/tag/session/txn data in one transaction, building a batch-local scratch from only the rows the insert actually created (SDK-retried duplicates are dropped by the conflict clause and must not inflate counts) and folding it into `accumulators` after commit on the non-agg path, or before the aggregation flush on the agg path so the flush includes this batch. Sets `*merged` once the scratch has been folded in pre-commit, so a failed agg attempt knows whether the caller must roll `accumulators` back.
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
async fn do_flush_tx(
    pool: &DbPool,
    batch: &[WriteMsg],
    should_agg: bool,
    accumulators: &mut Accumulators,
    pending: &mut Vec<crate::notify::NotificationEvent>,
    merged: &mut bool,
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
        *merged = true;
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
    accumulators.releases.clear();
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
    if accumulators.is_empty() {
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
    async fn compress_batch_is_a_noop_when_everything_is_already_compressed() {
        let raw =
            serde_json::to_vec(&serde_json::json!({"event_id": "noop1", "message": "x"})).unwrap();
        let event = StorableEvent::new(
            "noop1".to_string(),
            ItemType::Event,
            raw,
            1,
            "k".to_string(),
        );
        let mut batch = vec![WriteMsg::Event(event)];
        compress_batch(&mut batch);
        let WriteMsg::Event(first) = &batch[0] else {
            panic!("expected event");
        };
        assert!(first.compressed);
        let bytes = first.payload.clone();

        // Second pass (the retry re-entry) must leave the bytes untouched.
        compress_batch(&mut batch);
        let WriteMsg::Event(second) = &batch[0] else {
            panic!("expected event");
        };
        assert!(second.compressed);
        assert_eq!(second.payload, bytes);
    }

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

    // Regression: metric (and pre-extracted log) payloads are decoded straight
    // from the raw bytes at insert, so compressing here is a pure round trip.
    #[tokio::test]
    async fn compress_batch_skips_metrics_and_pre_extracted_logs() {
        let raw = br#"[{"name":"c:custom/x@none","type":"c","value":1}]"#.to_vec();
        let metric = StorableEvent::new(
            "m1".to_string(),
            ItemType::Metric,
            raw.clone(),
            1,
            "k".to_string(),
        );
        let mut log = StorableEvent::new(
            "l1".to_string(),
            ItemType::Log,
            raw.clone(),
            1,
            "k".to_string(),
        );
        log.log_entries = Some(Vec::new());
        let mut batch = vec![WriteMsg::Event(metric), WriteMsg::Event(log)];

        compress_batch(&mut batch);

        for msg in &batch {
            let WriteMsg::Event(e) = msg else {
                panic!("expected event");
            };
            assert!(!e.compressed, "{} must stay raw", e.item_type);
            assert_eq!(e.payload, raw);
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn compress_batch_compresses_pre_extracted_log_entries_and_is_idempotent() {
        let entry_json = serde_json::json!({"body": "first", "level": "warn"});
        let plain = serde_json::to_vec(&entry_json).unwrap();
        let mut log = StorableEvent::new(
            "l2".to_string(),
            ItemType::Log,
            b"[]".to_vec(),
            1,
            "k".to_string(),
        );
        log.log_entries = Some(vec![crate::ingest::parse_log::parse_log_entry(&entry_json)]);
        let mut batch = vec![WriteMsg::Event(log)];

        compress_batch(&mut batch);
        let WriteMsg::Event(e) = &batch[0] else {
            panic!("expected event");
        };
        assert!(!e.compressed, "the batch payload itself stays raw");
        let entry = &e.log_entries.as_ref().unwrap()[0];
        assert!(entry.compressed);
        assert_ne!(entry.payload, plain);
        assert_eq!(zstd::decode_all(entry.payload.as_slice()).unwrap(), plain);
        let stored = entry.payload.clone();

        // The retry re-entry must not wrap the entry in a second zstd frame.
        compress_batch(&mut batch);
        let WriteMsg::Event(e) = &batch[0] else {
            panic!("expected event");
        };
        assert_eq!(e.log_entries.as_ref().unwrap()[0].payload, stored);
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
