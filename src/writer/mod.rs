mod accumulator;
mod dispatch;
mod flush;
mod handle;
pub(crate) mod msg;
pub mod types;

pub use handle::WriterHandle;
pub use msg::WriteMsg;

use crate::db::DbPool;
use crate::filter::FilterEngine;
use crate::stats::IngestStats;
use anyhow::Result;
use std::sync::Arc;

use accumulator::{Accumulators, BATCH_SIZE};
use dispatch::handle_immediate;
use flush::flush_aggregation;
use flush::flush_batch;
use msg::WriteMsg::*;

pub async fn spawn(
    pool: DbPool,
    filter_engine: Option<Arc<FilterEngine>>,
    notify_tx: Option<tokio::sync::mpsc::Sender<crate::notify::NotificationEvent>>,
    ingest_stats: Arc<IngestStats>,
) -> Result<(WriterHandle, tokio::task::JoinHandle<()>)> {
    let (tx, rx) = tokio::sync::mpsc::channel::<WriteMsg>(50_000);

    let stats = ingest_stats.clone();
    let join_handle = tokio::spawn(async move {
        writer_loop(
            &pool,
            rx,
            filter_engine.as_deref(),
            notify_tx.as_ref(),
            &stats,
        )
        .await;
        tracing::info!("writer task exiting");
    });

    Ok((WriterHandle::new(tx, ingest_stats), join_handle))
}

fn count_events_in(batch: &[WriteMsg]) -> u64 {
    batch
        .iter()
        .filter(|m| matches!(m, Event(_) | EventWithAttachments(_, _)))
        .count() as u64
}

async fn writer_loop(
    pool: &DbPool,
    mut rx: tokio::sync::mpsc::Receiver<WriteMsg>,
    filter_engine: Option<&FilterEngine>,
    notify_tx: Option<&tokio::sync::mpsc::Sender<crate::notify::NotificationEvent>>,
    ingest_stats: &IngestStats,
) {
    use accumulator::AGGREGATION_FLUSH_INTERVAL_MS;

    let mut accumulators = Accumulators::new();
    let mut retry_pending: Vec<WriteMsg> = Vec::new();

    let flush_interval = std::time::Duration::from_millis(AGGREGATION_FLUSH_INTERVAL_MS as u64);
    let mut tick = tokio::time::interval(flush_interval);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Skip the immediate first tick
    tick.tick().await;

    loop {
        // Retry previously failed batch before accepting new work
        if !retry_pending.is_empty() {
            if flush_batch(pool, &mut retry_pending, &mut accumulators, notify_tx).await {
                retry_pending.clear();
            } else {
                let dropped = count_events_in(&retry_pending);
                tracing::error!("dropping {dropped} events after repeated flush failures");
                ingest_stats
                    .events_dropped
                    .fetch_add(dropped, std::sync::atomic::Ordering::Relaxed);
                retry_pending.clear();
            }
        }

        // Wait for either a message or the periodic flush timer
        let first = tokio::select! {
            biased;

            msg = rx.recv() => match msg {
                Some(Shutdown) | None => {
                    if !accumulators.is_empty() {
                        if let Err(e) = flush_aggregation(pool, &mut accumulators, notify_tx).await {
                            tracing::error!("final aggregation flush failed: {e}");
                        }
                    }
                    return;
                }
                Some(msg) => msg,
            },

            _ = tick.tick() => {
                if !accumulators.is_empty() {
                    if let Err(e) = flush_aggregation(pool, &mut accumulators, notify_tx).await {
                        tracing::error!("periodic aggregation flush failed: {e}");
                    }
                }
                continue;
            }
        };

        // Reply-channel messages get handled right away
        let first = match first {
            msg @ Event(_) | msg @ EventWithAttachments(_, _) => msg,
            msg => {
                handle_immediate(pool, msg, filter_engine).await;
                continue;
            }
        };

        // Grab as many events as we can, up to BATCH_SIZE
        let mut batch = Vec::with_capacity(BATCH_SIZE);
        batch.push(first);

        while batch.len() < BATCH_SIZE {
            match rx.try_recv() {
                Ok(Shutdown) => {
                    flush_batch(pool, &mut batch, &mut accumulators, notify_tx).await;
                    if !accumulators.is_empty() {
                        if let Err(e) = flush_aggregation(pool, &mut accumulators, notify_tx).await
                        {
                            tracing::error!("shutdown aggregation flush failed: {e}");
                        }
                    }
                    return;
                }
                Ok(msg @ Event(_)) | Ok(msg @ EventWithAttachments(_, _)) => {
                    batch.push(msg);
                }
                Ok(msg) => {
                    if !batch.is_empty() {
                        if !flush_batch(pool, &mut batch, &mut accumulators, notify_tx).await {
                            std::mem::swap(&mut retry_pending, &mut batch);
                        }
                        batch.clear();
                    }
                    handle_immediate(pool, msg, filter_engine).await;
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    flush_batch(pool, &mut batch, &mut accumulators, notify_tx).await;
                    if !accumulators.is_empty() {
                        if let Err(e) = flush_aggregation(pool, &mut accumulators, notify_tx).await
                        {
                            tracing::error!("disconnect aggregation flush failed: {e}");
                        }
                    }
                    return;
                }
            }
        }

        if !flush_batch(pool, &mut batch, &mut accumulators, notify_tx).await {
            std::mem::swap(&mut retry_pending, &mut batch);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::models::{StorableAttachment, StorableEvent};
    use accumulator::Accumulators;
    use flush::{flush_aggregation, flush_batch, insert_event};
    use simple_hll::HyperLogLog;

    async fn test_pool() -> DbPool {
        db::open_test_pool().await
    }

    fn test_stats() -> Arc<IngestStats> {
        Arc::new(IngestStats::new())
    }

    fn make_event(event_id: &str) -> StorableEvent {
        StorableEvent::test_default(event_id)
    }

    #[tokio::test]
    async fn spawn_and_send_event() {
        let pool = test_pool().await;
        let (handle, _join) = spawn(pool.clone(), None, None, test_stats()).await.unwrap();

        handle.send_event(make_event("w1")).unwrap();
        // Give the writer task time to process
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let _ = handle.shutdown();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let event = db::get_event(&pool, "w1").await.unwrap();
        assert!(event.is_some());
        assert_eq!(event.unwrap().title.as_deref(), Some("test"));
    }

    #[tokio::test]
    async fn spawn_and_send_event_with_attachment() {
        let pool = test_pool().await;
        let (handle, _join) = spawn(pool.clone(), None, None, test_stats()).await.unwrap();

        let att = StorableAttachment {
            event_id: "w2".to_string(),
            filename: "log.txt".to_string(),
            content_type: Some("text/plain".to_string()),
            data: b"log contents".to_vec(),
        };

        handle
            .send_event_with_attachments(make_event("w2"), vec![att])
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let _ = handle.shutdown();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        assert!(db::get_event(&pool, "w2").await.unwrap().is_some());

        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM attachments WHERE event_id = 'w2'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.0, 1);
    }

    #[tokio::test]
    async fn batch_flushing_multiple_events() {
        let pool = test_pool().await;
        let (handle, _join) = spawn(pool.clone(), None, None, test_stats()).await.unwrap();

        for i in 0..10 {
            handle.send_event(make_event(&format!("batch{i}"))).unwrap();
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let _ = handle.shutdown();
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM events")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.0, 10);
    }

    #[tokio::test]
    async fn shutdown_without_events() {
        let pool = test_pool().await;
        let (handle, _join) = spawn(pool.clone(), None, None, test_stats()).await.unwrap();
        let _ = handle.shutdown();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    #[tokio::test]
    async fn insert_event_with_fingerprint_creates_issue() {
        let pool = test_pool().await;
        let (handle, _join) = spawn(pool.clone(), None, None, test_stats()).await.unwrap();

        let mut event = make_event("fp1");
        event.fingerprint = Some("abcdef0123456789".to_string());

        handle.send_event(event).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let _ = handle.shutdown();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let row: (i64,) =
            sqlx::query_as("SELECT event_count FROM issues WHERE fingerprint = 'abcdef0123456789'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.0, 1);
    }

    #[tokio::test]
    async fn duplicate_event_does_not_increment_issue_count() {
        let pool = test_pool().await;

        let mut event = make_event("dup1");
        event.fingerprint = Some("aaaa000000000000".to_string());

        let mut acc = Accumulators::new();

        if insert_event(&pool, &event).await.unwrap() {
            acc.accumulate(&event);
        }
        if insert_event(&pool, &event).await.unwrap() {
            acc.accumulate(&event);
        }
        flush_aggregation(&pool, &mut acc, None).await.unwrap();

        let row: (i64,) =
            sqlx::query_as("SELECT event_count FROM issues WHERE fingerprint = 'aaaa000000000000'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.0, 1);
    }

    #[tokio::test]
    async fn two_events_same_fingerprint_increments_count() {
        let pool = test_pool().await;

        let mut event1 = make_event("e1");
        event1.fingerprint = Some("bbbb000000000000".to_string());
        event1.timestamp = 100;

        let mut event2 = make_event("e2");
        event2.fingerprint = Some("bbbb000000000000".to_string());
        event2.timestamp = 200;

        let mut acc = Accumulators::new();
        if insert_event(&pool, &event1).await.unwrap() {
            acc.accumulate(&event1);
        }
        if insert_event(&pool, &event2).await.unwrap() {
            acc.accumulate(&event2);
        }
        flush_aggregation(&pool, &mut acc, None).await.unwrap();

        use sqlx::Row;
        let row = sqlx::query(
            "SELECT event_count, first_seen, last_seen FROM issues WHERE fingerprint = 'bbbb000000000000'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let count: i64 = row.get("event_count");
        let first_seen: i64 = row.get("first_seen");
        let last_seen: i64 = row.get("last_seen");
        assert_eq!(count, 2);
        assert_eq!(first_seen, 100);
        assert_eq!(last_seen, 200);
    }

    #[tokio::test]
    async fn resolved_issue_reopens_on_new_event() {
        let pool = test_pool().await;

        let mut event1 = make_event("r1");
        event1.fingerprint = Some("cccc000000000000".to_string());
        let mut acc = Accumulators::new();
        if insert_event(&pool, &event1).await.unwrap() {
            acc.accumulate(&event1);
        }
        flush_aggregation(&pool, &mut acc, None).await.unwrap();

        sqlx::query("UPDATE issues SET status = 'resolved' WHERE fingerprint = 'cccc000000000000'")
            .execute(&pool)
            .await
            .unwrap();

        let mut event2 = make_event("r2");
        event2.fingerprint = Some("cccc000000000000".to_string());
        let mut acc2 = Accumulators::new();
        if insert_event(&pool, &event2).await.unwrap() {
            acc2.accumulate(&event2);
        }
        flush_aggregation(&pool, &mut acc2, None).await.unwrap();

        let row: (String,) =
            sqlx::query_as("SELECT status FROM issues WHERE fingerprint = 'cccc000000000000'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.0, "unresolved");
    }

    #[tokio::test]
    async fn deferred_flush_batches_tags() {
        let pool = test_pool().await;

        let mut event1 = make_event("t1");
        event1.fingerprint = Some("tttt000000000000".to_string());
        event1.tags = vec![("browser".to_string(), "chrome".to_string())];

        let mut event2 = make_event("t2");
        event2.fingerprint = Some("tttt000000000000".to_string());
        event2.tags = vec![
            ("browser".to_string(), "chrome".to_string()),
            ("os".to_string(), "linux".to_string()),
        ];

        let mut acc = Accumulators::new();
        if insert_event(&pool, &event1).await.unwrap() {
            acc.accumulate(&event1);
        }
        if insert_event(&pool, &event2).await.unwrap() {
            acc.accumulate(&event2);
        }
        flush_aggregation(&pool, &mut acc, None).await.unwrap();

        let row: (i64,) = sqlx::query_as(
            "SELECT count FROM issue_tag_values WHERE fingerprint = 'tttt000000000000' AND tag_key = 'browser' AND tag_value = 'chrome'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, 2);

        let row: (i64,) = sqlx::query_as(
            "SELECT count FROM issue_tag_values WHERE fingerprint = 'tttt000000000000' AND tag_key = 'os' AND tag_value = 'linux'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, 1);
    }

    /// Regression test for the accumulate-then-clear race: when `should_agg=true`,
    /// events in the current batch must be accumulated *before* the aggregation
    /// flush runs, so every fingerprint gets a matching issue row.
    #[tokio::test]
    async fn flush_batch_with_agg_creates_issue_for_all_events() {
        let pool = test_pool().await;
        let mut acc = Accumulators::new();

        // Batch 1: insert events, accumulate only (should_agg=false because <1s)
        let mut e1 = make_event("race1");
        e1.fingerprint = Some("race_fp_1".to_string());
        let mut batch1 = vec![WriteMsg::Event(e1)];
        assert!(flush_batch(&pool, &mut batch1, &mut acc, None).await);
        // Issue should NOT exist yet -- accumulators hold the delta in memory
        let row: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM issues WHERE fingerprint = 'race_fp_1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.0, 0, "issue should not be flushed yet");

        // Force the next flush_batch to trigger aggregation
        acc.last_flush = std::time::Instant::now() - std::time::Duration::from_secs(2);

        // Batch 2: new fingerprint -- this batch triggers should_agg=true.
        // The bug was that batch-2 events were accumulated *after* the
        // aggregation flush, leaving them orphaned with no issue row.
        let mut e2 = make_event("race2");
        e2.fingerprint = Some("race_fp_2".to_string());
        let mut batch2 = vec![WriteMsg::Event(e2)];
        assert!(flush_batch(&pool, &mut batch2, &mut acc, None).await);

        // Both fingerprints should now have matching issue rows
        let row: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM issues WHERE fingerprint = 'race_fp_1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.0, 1, "batch-1 fingerprint must have an issue row");

        let row: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM issues WHERE fingerprint = 'race_fp_2'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.0, 1, "batch-2 fingerprint must have an issue row");
    }

    #[tokio::test]
    async fn deferred_flush_merges_hll() {
        let pool = test_pool().await;

        let mut event1 = make_event("h1");
        event1.fingerprint = Some("hhhh000000000000".to_string());
        event1.user_identifier = Some("user-a".to_string());

        let mut event2 = make_event("h2");
        event2.fingerprint = Some("hhhh000000000000".to_string());
        event2.user_identifier = Some("user-b".to_string());

        let mut acc = Accumulators::new();
        if insert_event(&pool, &event1).await.unwrap() {
            acc.accumulate(&event1);
        }
        if insert_event(&pool, &event2).await.unwrap() {
            acc.accumulate(&event2);
        }
        flush_aggregation(&pool, &mut acc, None).await.unwrap();

        use sqlx::Row;
        let row = sqlx::query("SELECT user_hll FROM issues WHERE fingerprint = 'hhhh000000000000'")
            .fetch_one(&pool)
            .await
            .unwrap();
        let hll_blob: Vec<u8> = row.get("user_hll");
        let hll: HyperLogLog<12> = HyperLogLog::with_registers(hll_blob);
        assert_eq!(hll.count() as u64, 2);
    }

    /// The periodic timer should flush accumulated issue deltas to the DB
    /// even when no new events arrive -- without needing a shutdown.
    #[tokio::test]
    async fn periodic_timer_flushes_issues_without_new_events() {
        let pool = test_pool().await;
        let (handle, _join) = spawn(pool.clone(), None, None, test_stats()).await.unwrap();

        let mut event = make_event("timer1");
        event.fingerprint = Some("timer_fp_1".to_string());
        handle.send_event(event).unwrap();

        // The first batch flush inserts the event and accumulates the delta
        // but won't aggregate yet (should_flush = false, <1s elapsed).
        // Wait for the event to be processed, then check no issue row yet.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let row: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM issues WHERE fingerprint = 'timer_fp_1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.0, 0, "issue should not exist before timer fires");

        // Wait for the periodic timer to fire (~1s interval + margin)
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

        let row: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM issues WHERE fingerprint = 'timer_fp_1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.0, 1, "periodic timer should have flushed the issue");

        let _ = handle.shutdown();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    /// Multiple fingerprints accumulated in a single batch should all get
    /// flushed together when the periodic timer fires.
    #[tokio::test]
    async fn periodic_timer_flushes_multiple_fingerprints() {
        let pool = test_pool().await;
        let (handle, _join) = spawn(pool.clone(), None, None, test_stats()).await.unwrap();

        for i in 0..5 {
            let mut e = make_event(&format!("multi{i}"));
            e.fingerprint = Some(format!("multi_fp_{i}"));
            handle.send_event(e).unwrap();
        }

        // Let events process, then wait for timer
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

        let row: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM issues WHERE fingerprint LIKE 'multi_fp_%'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.0, 5, "all 5 fingerprints should have issue rows");

        let _ = handle.shutdown();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    /// Same fingerprint across two timer cycles: event_count should
    /// correctly increment, not reset or double-count.
    #[tokio::test]
    async fn periodic_timer_same_fingerprint_across_cycles() {
        let pool = test_pool().await;
        let (handle, _join) = spawn(pool.clone(), None, None, test_stats()).await.unwrap();

        let mut e1 = make_event("cross1");
        e1.fingerprint = Some("cross_fp".to_string());
        e1.timestamp = 100;
        handle.send_event(e1).unwrap();

        // Wait for cycle 1 to flush
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

        let row: (i64,) =
            sqlx::query_as("SELECT event_count FROM issues WHERE fingerprint = 'cross_fp'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.0, 1, "should be 1 after first cycle");

        // Send second event with same fingerprint
        let mut e2 = make_event("cross2");
        e2.fingerprint = Some("cross_fp".to_string());
        e2.timestamp = 200;
        handle.send_event(e2).unwrap();

        // Wait for cycle 2 to flush
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

        use sqlx::Row;
        let row = sqlx::query(
            "SELECT event_count, first_seen, last_seen FROM issues WHERE fingerprint = 'cross_fp'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            row.get::<i64, _>("event_count"),
            2,
            "should be 2 after second cycle"
        );
        assert_eq!(row.get::<i64, _>("first_seen"), 100);
        assert_eq!(row.get::<i64, _>("last_seen"), 200);

        let _ = handle.shutdown();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    /// Tags and HLL data should be flushed by the periodic timer,
    /// not just issue rows.
    #[tokio::test]
    async fn periodic_timer_flushes_tags_and_hll() {
        let pool = test_pool().await;
        let (handle, _join) = spawn(pool.clone(), None, None, test_stats()).await.unwrap();

        let mut e1 = make_event("th1");
        e1.fingerprint = Some("th_fp".to_string());
        e1.tags = vec![("browser".to_string(), "firefox".to_string())];
        e1.user_identifier = Some("user-x".to_string());
        handle.send_event(e1).unwrap();

        let mut e2 = make_event("th2");
        e2.fingerprint = Some("th_fp".to_string());
        e2.tags = vec![("browser".to_string(), "firefox".to_string())];
        e2.user_identifier = Some("user-y".to_string());
        handle.send_event(e2).unwrap();

        // Wait for periodic flush
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

        // Tag count should be 2
        let row: (i64,) = sqlx::query_as(
            "SELECT count FROM issue_tag_values WHERE fingerprint = 'th_fp' AND tag_key = 'browser' AND tag_value = 'firefox'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, 2, "tag count should be flushed by timer");

        // HLL should reflect 2 distinct users
        use sqlx::Row;
        let row = sqlx::query("SELECT user_hll FROM issues WHERE fingerprint = 'th_fp'")
            .fetch_one(&pool)
            .await
            .unwrap();
        let hll_blob: Vec<u8> = row.get("user_hll");
        let hll: HyperLogLog<12> = HyperLogLog::with_registers(hll_blob);
        assert_eq!(hll.count() as u64, 2, "HLL should reflect 2 users");

        let _ = handle.shutdown();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    /// After a periodic flush, new events for fresh fingerprints should
    /// accumulate and flush correctly on the next cycle.
    #[tokio::test]
    async fn periodic_timer_handles_subsequent_events() {
        let pool = test_pool().await;
        let (handle, _join) = spawn(pool.clone(), None, None, test_stats()).await.unwrap();

        // First event, wait for periodic flush
        let mut e1 = make_event("seq1");
        e1.fingerprint = Some("seq_fp_1".to_string());
        handle.send_event(e1).unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

        let row: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM issues WHERE fingerprint = 'seq_fp_1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.0, 1, "first fingerprint should be flushed");

        // Second event with a different fingerprint, after the first cycle
        let mut e2 = make_event("seq2");
        e2.fingerprint = Some("seq_fp_2".to_string());
        handle.send_event(e2).unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

        let row: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM issues WHERE fingerprint = 'seq_fp_2'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            row.0, 1,
            "second fingerprint should be flushed by next cycle"
        );

        // First fingerprint count should still be 1 (not double-counted)
        let row: (i64,) =
            sqlx::query_as("SELECT event_count FROM issues WHERE fingerprint = 'seq_fp_1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.0, 1, "first fingerprint event_count must not double");

        let _ = handle.shutdown();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    /// The periodic timer should be a no-op when there's nothing accumulated.
    #[tokio::test]
    async fn periodic_timer_noop_when_empty() {
        let pool = test_pool().await;
        let (handle, _join) = spawn(pool.clone(), None, None, test_stats()).await.unwrap();

        // Let a few timer ticks pass with no events sent
        tokio::time::sleep(std::time::Duration::from_millis(2500)).await;

        // No issues should exist
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM issues")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.0, 0, "no issues should exist when no events were sent");

        let _ = handle.shutdown();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    /// Events arriving right as the timer would fire should not lose data.
    /// The biased select prioritizes rx.recv(), so events get batched first;
    /// accumulated deltas from the batch flush via the next timer tick.
    #[tokio::test]
    async fn periodic_timer_events_near_tick_boundary() {
        let pool = test_pool().await;
        let (handle, _join) = spawn(pool.clone(), None, None, test_stats()).await.unwrap();

        // Send first event right away
        let mut e1 = make_event("boundary1");
        e1.fingerprint = Some("boundary_fp".to_string());
        handle.send_event(e1).unwrap();

        // Wait ~900ms (just under the 1s timer interval) then send another
        tokio::time::sleep(std::time::Duration::from_millis(900)).await;
        let mut e2 = make_event("boundary2");
        e2.fingerprint = Some("boundary_fp".to_string());
        handle.send_event(e2).unwrap();

        // Wait for timer to fire and flush
        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;

        let row: (i64,) =
            sqlx::query_as("SELECT event_count FROM issues WHERE fingerprint = 'boundary_fp'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            row.0, 2,
            "both events must be counted even near tick boundary"
        );

        let _ = handle.shutdown();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}
