//! Transaction performance queries over the `transaction_metrics` rollup
//! and the per-instance `events` rows.

use anyhow::Result;
use simple_hll::HyperLogLog;
use sqlx::Row;

use crate::db::sql;
use crate::ingest::models::HLL_REGISTER_COUNT;

use super::types::{
    DurationBucket, Page, PagedResult, SpanAggregation, TransactionDistribution,
    TransactionInstance, TransactionSummary, TransactionTrendPoint,
};

const NUM_BUCKETS: usize = 24;

/// Estimate the `p`-th percentile (0.0..=1.0) of a log2 duration histogram.
/// Bucket `b` spans `[2^b, 2^(b+1))` ms; we walk the cumulative count and
/// linearly interpolate within the target bucket. Returns milliseconds.
pub fn percentile_from_buckets(buckets: &[u64; NUM_BUCKETS], total: u64, p: f64) -> i64 {
    if total == 0 {
        return 0;
    }
    let target = (p * total as f64).clamp(0.0, total as f64);

    let mut cumulative = 0u64;
    for (b, &count) in buckets.iter().enumerate() {
        if count == 0 {
            continue;
        }
        let next = cumulative + count;
        if (next as f64) >= target {
            let lo = (1u64 << b) as f64;
            let hi = (1u64 << (b + 1)) as f64;
            // Position of the target within this bucket's slice of the CDF.
            let within = (target - cumulative as f64) / count as f64;
            return (lo + within * (hi - lo)).round() as i64;
        }
        cumulative = next;
    }
    // Fell through (p == 1.0 on the last populated bucket edge): top of last.
    for b in (0..NUM_BUCKETS).rev() {
        if buckets[b] > 0 {
            return (1u64 << (b + 1)) as i64;
        }
    }
    0
}

/// Human-readable throughput with adaptive units, so low-volume transactions
/// don't all round to 0/min.
fn format_throughput(tpm: f64) -> String {
    if tpm <= 0.0 {
        "0/min".to_string()
    } else if tpm >= 1.0 {
        format!("{:.1}/min", tpm)
    } else if tpm * 60.0 >= 1.0 {
        format!("{:.1}/hr", tpm * 60.0)
    } else {
        format!("{:.2}/day", tpm * 1440.0)
    }
}

/// Merge an HLL blob into an accumulator if it's the right size.
fn merge_hll(acc: &mut HyperLogLog<12>, blob: &Option<Vec<u8>>) {
    if let Some(buf) = blob {
        if buf.len() == HLL_REGISTER_COUNT {
            let other = HyperLogLog::<12>::with_registers(buf.clone());
            acc.merge(&other);
        }
    }
}

struct TxnAgg {
    count: u64,
    sum_duration_ms: u64,
    failed_count: u64,
    buckets: [u64; NUM_BUCKETS],
    users_hll: HyperLogLog<12>,
    has_user_data: bool,
}

impl TxnAgg {
    fn new() -> Self {
        Self {
            count: 0,
            sum_duration_ms: 0,
            failed_count: 0,
            buckets: [0; NUM_BUCKETS],
            users_hll: HyperLogLog::new(),
            has_user_data: false,
        }
    }
}

/// Fold one `transaction_metrics` row (count/sum/failed/buckets/users_hll) into
/// an accumulator. Callers read `transaction_name` separately when grouping.
fn accumulate_row(agg: &mut TxnAgg, row: &crate::db::DbRow) {
    agg.count += row.get::<i64, _>("count") as u64;
    agg.sum_duration_ms += row.get::<i64, _>("sum_duration_ms").max(0) as u64;
    agg.failed_count += row.get::<i64, _>("failed_count") as u64;
    for i in 0..NUM_BUCKETS {
        agg.buckets[i] += row.get::<i64, _>(format!("bucket_{i}").as_str()).max(0) as u64;
    }
    let blob: Option<Vec<u8>> = row.get("users_hll");
    if blob.is_some() {
        agg.has_user_data = true;
    }
    merge_hll(&mut agg.users_hll, &blob);
}

/// Build a [`TransactionSummary`] (percentiles, throughput, failure rate, users)
/// from a rolled-up accumulator. `window_minutes` is the period length used for
/// the throughput rate.
fn summary_from_agg(name: String, agg: &TxnAgg, window_minutes: f64) -> TransactionSummary {
    let count = agg.count;
    // Raw rate kept unrounded so sorting and the adaptive-unit label stay
    // accurate at low volumes.
    let tpm = count as f64 / window_minutes;
    let failure_rate = if count > 0 {
        (agg.failed_count as f64 / count as f64 * 1000.0).round() / 10.0
    } else {
        0.0
    };
    TransactionSummary {
        name,
        tpm,
        throughput: format_throughput(tpm),
        p50_ms: percentile_from_buckets(&agg.buckets, count, 0.50),
        p75_ms: percentile_from_buckets(&agg.buckets, count, 0.75),
        p95_ms: percentile_from_buckets(&agg.buckets, count, 0.95),
        failure_rate,
        count,
        users: if agg.has_user_data {
            agg.users_hll.count() as u64
        } else {
            0
        },
        avg_ms: agg.sum_duration_ms.checked_div(count).unwrap_or(0) as i64,
    }
}

/// Trim the log2 histogram to its populated range and render one bar per bucket,
/// scaled to the busiest bucket. Empty when no samples landed in any bucket.
fn distribution_buckets(buckets: &[u64; NUM_BUCKETS]) -> Vec<DurationBucket> {
    let (Some(first), Some(last)) = (
        buckets.iter().position(|&c| c > 0),
        buckets.iter().rposition(|&c| c > 0),
    ) else {
        return Vec::new();
    };
    let max = buckets[first..=last].iter().copied().max().unwrap_or(0);
    (first..=last)
        .map(|b| {
            let count = buckets[b];
            DurationBucket {
                label: format!("{}-{}ms", 1u64 << b, 1u64 << (b + 1)),
                count,
                pct: if max > 0 {
                    count as f64 / max as f64 * 100.0
                } else {
                    0.0
                },
            }
        })
        .collect()
}

/// Roll up `transaction_metrics` rows by name and compute per-transaction
/// percentiles, throughput, and failure rate. `sort` is one of
/// `p95` (default), `throughput`, `failure_rate`, `count`.
pub async fn list_transactions(
    pool: &crate::db::DbPool,
    project_id: u64,
    since_ts: i64,
    sort: &str,
) -> Result<Vec<TransactionSummary>> {
    let hour_floor = (since_ts / 3600) * 3600;

    let bucket_cols: String = (0..NUM_BUCKETS)
        .map(|i| format!("bucket_{i}"))
        .collect::<Vec<_>>()
        .join(", ");

    let raw = format!(
        "SELECT transaction_name, count, sum_duration_ms, failed_count, {bucket_cols}, users_hll \
         FROM transaction_metrics \
         WHERE project_id = ?1 AND hour_bucket >= ?2"
    );
    let rows = sqlx::query(crate::db::dyn_sql(&raw))
        .bind(project_id as i64)
        .bind(hour_floor)
        .fetch_all(pool)
        .await?;

    let mut by_name: std::collections::HashMap<String, TxnAgg> = std::collections::HashMap::new();

    for row in &rows {
        let name: String = row.get("transaction_name");
        let agg = by_name.entry(name).or_insert_with(TxnAgg::new);
        accumulate_row(agg, row);
    }

    // Throughput window: from since_ts to now, floored to at least one minute.
    let now = chrono::Utc::now().timestamp();
    let window_minutes = (((now - since_ts).max(60)) as f64) / 60.0;

    let mut items: Vec<TransactionSummary> = by_name
        .into_iter()
        .map(|(name, agg)| summary_from_agg(name, &agg, window_minutes))
        .collect();

    match sort {
        "throughput" => items.sort_by(|a, b| b.tpm.total_cmp(&a.tpm)),
        "failure_rate" => items.sort_by(|a, b| b.failure_rate.total_cmp(&a.failure_rate)),
        "count" => items.sort_by_key(|t| std::cmp::Reverse(t.count)),
        _ => items.sort_by_key(|t| std::cmp::Reverse(t.p95_ms)),
    }

    Ok(items)
}

/// Aggregate one transaction's `transaction_metrics` rows over the period into
/// header stats plus a duration distribution. `None` when the transaction has
/// no rollup rows in the window.
pub async fn transaction_distribution(
    pool: &crate::db::DbPool,
    project_id: u64,
    name: &str,
    since_ts: i64,
) -> Result<Option<TransactionDistribution>> {
    let hour_floor = (since_ts / 3600) * 3600;

    let bucket_cols: String = (0..NUM_BUCKETS)
        .map(|i| format!("bucket_{i}"))
        .collect::<Vec<_>>()
        .join(", ");

    let raw = format!(
        "SELECT count, sum_duration_ms, failed_count, {bucket_cols}, users_hll \
         FROM transaction_metrics \
         WHERE project_id = ?1 AND hour_bucket >= ?2 AND transaction_name = ?3"
    );
    let rows = sqlx::query(crate::db::dyn_sql(&raw))
        .bind(project_id as i64)
        .bind(hour_floor)
        .bind(name)
        .fetch_all(pool)
        .await?;

    if rows.is_empty() {
        return Ok(None);
    }

    let mut agg = TxnAgg::new();
    for row in &rows {
        accumulate_row(&mut agg, row);
    }

    let now = chrono::Utc::now().timestamp();
    let window_minutes = (((now - since_ts).max(60)) as f64) / 60.0;

    Ok(Some(TransactionDistribution {
        summary: summary_from_agg(name.to_string(), &agg, window_minutes),
        buckets: distribution_buckets(&agg.buckets),
    }))
}

/// Most trend points to plot. Beyond this the series is trimmed to the most
/// recent points rather than compressed further, so a long-lived transaction
/// still shows readable recent detail.
const MAX_TREND_POINTS: i64 = 120;

/// Candidate point widths in hours: hourly, quarter-day, half-day, daily, weekly.
const TREND_STEPS_HOURS: [i64; 6] = [1, 3, 6, 12, 24, 168];

/// Hours per trend point: the narrowest step that keeps the series under
/// `MAX_TREND_POINTS`, so the x-axis stays legible without a per-period table.
fn trend_bucket_hours(span_hours: i64) -> i64 {
    TREND_STEPS_HOURS
        .into_iter()
        .find(|&step| span_hours / step <= MAX_TREND_POINTS)
        .unwrap_or(TREND_STEPS_HOURS[TREND_STEPS_HOURS.len() - 1])
}

/// Ratio over the trailing median at which a point is called a regression.
const REGRESSION_FACTOR: f64 = 1.5;
/// Trailing points the median is taken over. The first `REGRESSION_WINDOW`
/// points have no full window and are never marked.
const REGRESSION_WINDOW: usize = 5;

/// Flag each p95 that exceeds `REGRESSION_FACTOR` times the median of the five
/// points before it.
///
/// Deliberately stateless and local to this page: it is a visual marker on a
/// summary chart, unrelated to the issue-regression notifications in
/// `src/notify/`. A zero trailing median (no traffic) never marks, so a
/// transaction waking up after a quiet spell doesn't read as a regression.
fn mark_regressions(p95: &[i64]) -> Vec<bool> {
    p95.iter()
        .enumerate()
        .map(|(i, &value)| {
            if i < REGRESSION_WINDOW {
                return false;
            }
            let mut window: Vec<i64> = p95[i - REGRESSION_WINDOW..i].to_vec();
            window.sort_unstable();
            let median = window[REGRESSION_WINDOW / 2];
            median > 0 && (value as f64) > REGRESSION_FACTOR * median as f64
        })
        .collect()
}

/// Caption for a trend point. Point widths below a day carry the hour, because
/// several points then share one date and a bare date would repeat.
fn trend_label(bucket: i64, bucket_hours: i64) -> String {
    let Some(dt) = chrono::DateTime::from_timestamp(bucket, 0) else {
        return String::new();
    };
    if bucket_hours < 24 {
        dt.format("%b %d %H:%M").to_string()
    } else {
        dt.format("%b %d").to_string()
    }
}

/// Percentiles over time for one transaction, folded out of the hourly
/// `transaction_metrics` histograms. Point width adapts to the span actually
/// present in the data rather than to the requested period, so an all-time view
/// of a young transaction still plots hourly.
pub async fn transaction_percentile_trend(
    pool: &crate::db::DbPool,
    project_id: u64,
    name: &str,
    since_ts: i64,
) -> Result<Vec<TransactionTrendPoint>> {
    let hour_floor = (since_ts / 3600) * 3600;

    let bucket_cols: String = (0..NUM_BUCKETS)
        .map(|i| format!("bucket_{i}"))
        .collect::<Vec<_>>()
        .join(", ");

    let raw = format!(
        "SELECT hour_bucket, count, {bucket_cols} \
         FROM transaction_metrics \
         WHERE project_id = ?1 AND transaction_name = ?2 AND hour_bucket >= ?3 \
         ORDER BY hour_bucket"
    );
    let rows = sqlx::query(crate::db::dyn_sql(&raw))
        .bind(project_id as i64)
        .bind(name)
        .bind(hour_floor)
        .fetch_all(pool)
        .await?;

    if rows.is_empty() {
        return Ok(Vec::new());
    }

    let first_hour: i64 = rows[0].get("hour_bucket");
    let last_hour: i64 = rows[rows.len() - 1].get("hour_bucket");
    let bucket_hours = trend_bucket_hours((last_hour - first_hour) / 3600 + 1);
    let width = bucket_hours * 3600;

    // Rows arrive ordered by hour_bucket, so points accumulate in order and need
    // no sort afterwards.
    let mut points: Vec<(i64, u64, [u64; NUM_BUCKETS])> = Vec::new();
    for row in &rows {
        let hour: i64 = row.get("hour_bucket");
        let bucket = hour.div_euclid(width) * width;
        if points.last().map(|(b, _, _)| *b) != Some(bucket) {
            points.push((bucket, 0, [0; NUM_BUCKETS]));
        }
        let (_, count, buckets) = points.last_mut().expect("just pushed");
        *count += row.get::<i64, _>("count").max(0) as u64;
        for (i, slot) in buckets.iter_mut().enumerate() {
            *slot += row.get::<i64, _>(format!("bucket_{i}").as_str()).max(0) as u64;
        }
    }

    if points.len() > MAX_TREND_POINTS as usize {
        points.drain(..points.len() - MAX_TREND_POINTS as usize);
    }

    let p95: Vec<i64> = points
        .iter()
        .map(|(_, count, buckets)| percentile_from_buckets(buckets, *count, 0.95))
        .collect();
    let regressed = mark_regressions(&p95);

    Ok(points
        .into_iter()
        .zip(p95)
        .zip(regressed)
        .map(
            |(((bucket, count, buckets), p95_ms), regressed)| TransactionTrendPoint {
                bucket,
                label: trend_label(bucket, bucket_hours),
                count,
                p50_ms: percentile_from_buckets(&buckets, count, 0.50),
                p95_ms,
                regressed,
            },
        )
        .collect())
}

/// Break one transaction's traces down by span (op, description), reusing the
/// spans page's fold so both surfaces agree on percentiles, ordering and cap.
///
/// Membership is an EXISTS subquery rather than a join: several transaction
/// events share one `trace_id` whenever a transaction is recorded more than
/// once in the same trace, and a join would count each span once per match.
pub async fn transaction_span_breakdown(
    pool: &crate::db::DbPool,
    project_id: u64,
    name: &str,
    since_ts: i64,
) -> Result<SpanAggregation> {
    let rows = sqlx::query(sql!(
        "SELECT s.op AS op, s.description AS description, s.duration_ms AS duration_ms \
         FROM spans s \
         WHERE s.project_id = ?1 AND s.duration_ms IS NOT NULL AND s.trace_id IS NOT NULL \
           AND EXISTS (SELECT 1 FROM events e \
                       WHERE e.trace_id = s.trace_id AND e.project_id = ?2 \
                         AND e.item_type = 'transaction' AND e.transaction_name = ?3 \
                         AND e.timestamp >= ?4) \
         ORDER BY s.timestamp DESC \
         LIMIT ?5"
    ))
    .bind(project_id as i64)
    .bind(project_id as i64)
    .bind(name)
    .bind(since_ts)
    .bind(super::spans::SPAN_AGG_SCAN_LIMIT)
    .fetch_all(pool)
    .await?;

    Ok(super::spans::fold_span_rows(&rows))
}

/// List individual transaction events for a given name, slowest first.
pub async fn list_transaction_instances(
    pool: &crate::db::DbPool,
    project_id: u64,
    name: &str,
    page: &Page,
) -> Result<PagedResult<TransactionInstance>> {
    let count_row = sqlx::query(sql!(
        "SELECT COUNT(*) FROM events \
         WHERE project_id = ?1 AND item_type = 'transaction' AND transaction_name = ?2"
    ))
    .bind(project_id as i64)
    .bind(name)
    .fetch_one(pool)
    .await?;
    let total = count_row.get::<i64, _>(0);

    // Explicit NULLS-last ordering for cross-backend parity.
    let rows = sqlx::query(sql!(
        "SELECT event_id, trace_id, duration_ms, timestamp, payload FROM events \
         WHERE project_id = ?1 AND item_type = 'transaction' AND transaction_name = ?2 \
         ORDER BY duration_ms IS NULL, duration_ms DESC \
         LIMIT ?3 OFFSET ?4"
    ))
    .bind(project_id as i64)
    .bind(name)
    .bind(page.limit as i64)
    .bind(page.offset as i64)
    .fetch_all(pool)
    .await?;

    // Page is small (<= 100), so per-row payload decode for trace op/status is cheap.
    let items = rows
        .iter()
        .map(|row| {
            let blob: Vec<u8> = row.get("payload");
            let trace = crate::queries::events::decompress_payload(&blob)
                .ok()
                .and_then(|p| p.get("contexts").and_then(|c| c.get("trace")).cloned());
            let str_field = |key: &str| {
                trace
                    .as_ref()
                    .and_then(|t| t.get(key))
                    .and_then(|v| v.as_str())
                    .map(String::from)
            };
            TransactionInstance {
                event_id: row.get("event_id"),
                trace_id: row.get("trace_id"),
                duration_ms: row.get("duration_ms"),
                timestamp: row.get("timestamp"),
                op: str_field("op"),
                status: str_field("status"),
            }
        })
        .collect();

    Ok(PagedResult::from_page(items, total, page))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::sql;
    use crate::queries::test_helpers::open_test_db;
    use crate::queries::types::Page;

    async fn insert_txn_instance(
        pool: &crate::db::DbPool,
        event_id: &str,
        project_id: i64,
        name: &str,
        duration_ms: i64,
        op: &str,
        status: &str,
    ) {
        let payload = serde_json::json!({
            "event_id": event_id,
            "contexts": {"trace": {"op": op, "status": status}},
        });
        let compressed =
            zstd::encode_all(serde_json::to_vec(&payload).unwrap().as_slice(), 3).unwrap();
        sqlx::query(sql!(
            "INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp, transaction_name, duration_ms, received_at)
             VALUES (?1, 'transaction', ?2, ?3, 'testkey', 100, ?4, ?5, 100)"
        ))
        .bind(event_id)
        .bind(&compressed)
        .bind(project_id)
        .bind(name)
        .bind(duration_ms)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn instances_carry_trace_op_and_status() {
        let pool = open_test_db().await;
        insert_txn_instance(
            &pool,
            "t1",
            1,
            "/checkout",
            500,
            "http.server",
            "deadline_exceeded",
        )
        .await;
        insert_txn_instance(&pool, "t2", 1, "/checkout", 100, "http.server", "ok").await;

        let page = Page::new(None, None);
        let result = list_transaction_instances(&pool, 1, "/checkout", &page)
            .await
            .unwrap();
        assert_eq!(result.total, 2);
        // Slowest first.
        assert_eq!(result.items[0].event_id, "t1");
        assert_eq!(result.items[0].op.as_deref(), Some("http.server"));
        assert_eq!(result.items[0].status.as_deref(), Some("deadline_exceeded"));
        assert!(result.items[0].is_failed());

        assert_eq!(result.items[1].status.as_deref(), Some("ok"));
        assert!(!result.items[1].is_failed());
    }

    async fn insert_txn_with_trace(
        pool: &crate::db::DbPool,
        event_id: &str,
        project_id: i64,
        name: &str,
        trace_id: &str,
        timestamp: i64,
    ) {
        let compressed = zstd::encode_all(b"{}".as_slice(), 3).unwrap();
        sqlx::query(sql!(
            "INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp, transaction_name, trace_id, received_at)
             VALUES (?1, 'transaction', ?2, ?3, 'testkey', ?4, ?5, ?6, ?4)"
        ))
        .bind(event_id)
        .bind(&compressed)
        .bind(project_id)
        .bind(timestamp)
        .bind(name)
        .bind(trace_id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn insert_span(
        pool: &crate::db::DbPool,
        span_id: &str,
        project_id: i64,
        trace_id: &str,
        op: &str,
        duration_ms: i64,
    ) {
        let compressed = zstd::encode_all(b"{}".as_slice(), 3).unwrap();
        sqlx::query(sql!(
            "INSERT INTO spans (span_id, payload, project_id, public_key, timestamp, trace_id, op, description, duration_ms)
             VALUES (?1, ?2, ?3, 'testkey', 100, ?4, ?5, 'd', ?6)"
        ))
        .bind(span_id)
        .bind(&compressed)
        .bind(project_id)
        .bind(trace_id)
        .bind(op)
        .bind(duration_ms)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn span_breakdown_groups_only_this_transactions_traces() {
        let pool = open_test_db().await;
        insert_txn_with_trace(&pool, "t1", 1, "/checkout", "trace-a", 100).await;
        insert_txn_with_trace(&pool, "t2", 1, "/search", "trace-b", 100).await;

        insert_span(&pool, "s1", 1, "trace-a", "db.query", 10).await;
        insert_span(&pool, "s2", 1, "trace-a", "db.query", 30).await;
        insert_span(&pool, "s3", 1, "trace-a", "http.client", 50).await;
        // Belongs to a different transaction, and must not leak in.
        insert_span(&pool, "s4", 1, "trace-b", "db.query", 90).await;

        let agg = transaction_span_breakdown(&pool, 1, "/checkout", 0)
            .await
            .unwrap();
        assert_eq!(agg.groups.len(), 2);
        assert!(!agg.truncated);

        let db = agg
            .groups
            .iter()
            .find(|g| g.op.as_deref() == Some("db.query"))
            .expect("db.query group");
        assert_eq!(db.count, 2, "the /search span must not be counted");
        assert_eq!(db.avg_ms, 20);

        let http = agg
            .groups
            .iter()
            .find(|g| g.op.as_deref() == Some("http.client"))
            .expect("http.client group");
        assert_eq!(http.count, 1);
    }

    #[tokio::test]
    async fn span_breakdown_does_not_double_count_a_shared_trace() {
        let pool = open_test_db().await;
        // Two transaction rows recorded in the same trace: a join on trace_id
        // would return each span twice.
        insert_txn_with_trace(&pool, "t1", 1, "/checkout", "trace-a", 100).await;
        insert_txn_with_trace(&pool, "t2", 1, "/checkout", "trace-a", 200).await;
        insert_span(&pool, "s1", 1, "trace-a", "db.query", 10).await;

        let agg = transaction_span_breakdown(&pool, 1, "/checkout", 0)
            .await
            .unwrap();
        assert_eq!(agg.groups.len(), 1);
        assert_eq!(agg.groups[0].count, 1);
    }

    #[tokio::test]
    async fn span_breakdown_honours_the_period() {
        let pool = open_test_db().await;
        insert_txn_with_trace(&pool, "t1", 1, "/checkout", "trace-a", 100).await;
        insert_span(&pool, "s1", 1, "trace-a", "db.query", 10).await;

        assert_eq!(
            transaction_span_breakdown(&pool, 1, "/checkout", 0)
                .await
                .unwrap()
                .groups
                .len(),
            1
        );
        assert!(transaction_span_breakdown(&pool, 1, "/checkout", 500)
            .await
            .unwrap()
            .groups
            .is_empty());
    }

    #[tokio::test]
    async fn span_breakdown_is_project_scoped() {
        let pool = open_test_db().await;
        // Same trace id seen by two projects, as happens in a distributed trace.
        insert_txn_with_trace(&pool, "t1", 1, "/checkout", "trace-a", 100).await;
        insert_txn_with_trace(&pool, "t2", 2, "/checkout", "trace-a", 100).await;
        insert_span(&pool, "s1", 2, "trace-a", "db.query", 10).await;

        assert!(transaction_span_breakdown(&pool, 1, "/checkout", 0)
            .await
            .unwrap()
            .groups
            .is_empty());
        assert_eq!(
            transaction_span_breakdown(&pool, 2, "/checkout", 0)
                .await
                .unwrap()
                .groups
                .len(),
            1
        );
    }

    async fn insert_metric_hour(
        pool: &crate::db::DbPool,
        project_id: i64,
        name: &str,
        hour_bucket: i64,
        count: i64,
        bucket_index: usize,
    ) {
        let cols: String = (0..NUM_BUCKETS)
            .map(|i| format!("bucket_{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let vals: String = (0..NUM_BUCKETS)
            .map(|i| {
                if i == bucket_index {
                    count.to_string()
                } else {
                    "0".to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let raw = format!(
            "INSERT INTO transaction_metrics (project_id, transaction_name, hour_bucket, count, sum_duration_ms, failed_count, {cols}, first_seen, last_seen)
             VALUES (?1, ?2, ?3, ?4, 0, 0, {vals}, ?3, ?3)"
        );
        sqlx::query(crate::db::dyn_sql(&raw))
            .bind(project_id)
            .bind(name)
            .bind(hour_bucket)
            .bind(count)
            .execute(pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn percentile_trend_folds_hours_into_points() {
        let pool = open_test_db().await;
        // Three consecutive hours; the span is short, so points stay hourly.
        // Bucket 6 is [64,128) ms, bucket 10 is [1024,2048) ms.
        insert_metric_hour(&pool, 1, "/checkout", 3600, 4, 6).await;
        insert_metric_hour(&pool, 1, "/checkout", 7200, 4, 6).await;
        insert_metric_hour(&pool, 1, "/checkout", 10800, 4, 10).await;
        // A different transaction and a different project must not leak in.
        insert_metric_hour(&pool, 1, "/search", 3600, 99, 0).await;
        insert_metric_hour(&pool, 2, "/checkout", 3600, 99, 0).await;

        let trend = transaction_percentile_trend(&pool, 1, "/checkout", 0)
            .await
            .unwrap();
        assert_eq!(trend.len(), 3);
        assert_eq!(trend[0].bucket, 3600);
        assert_eq!(trend[0].count, 4);
        assert!((64..=128).contains(&trend[0].p95_ms), "{}", trend[0].p95_ms);
        assert!(
            (1024..=2048).contains(&trend[2].p95_ms),
            "{}",
            trend[2].p95_ms
        );
        // Points are ordered oldest first, and captions carry the hour at this width.
        assert!(trend[0].bucket < trend[2].bucket);
        assert!(trend[0].label.contains(':'), "{}", trend[0].label);
        // Five points are needed before anything can be marked.
        assert!(trend.iter().all(|p| !p.regressed));
    }

    #[tokio::test]
    async fn percentile_trend_widens_points_over_a_long_span() {
        let pool = open_test_db().await;
        // 40 days spans 937 hours: too many to plot hourly, so the step widens to
        // 12h (79 points). Several points then share a date, so captions keep the
        // time-of-day.
        for day in 0..40i64 {
            insert_metric_hour(&pool, 1, "/checkout", day * 86_400, 1, 6).await;
        }
        let trend = transaction_percentile_trend(&pool, 1, "/checkout", 0)
            .await
            .unwrap();
        assert!(trend.len() <= MAX_TREND_POINTS as usize, "{}", trend.len());
        assert!(trend[0].label.contains(':'), "{}", trend[0].label);
        let widths: Vec<i64> = trend
            .windows(2)
            .map(|w| w[1].bucket - w[0].bucket)
            .collect();
        assert!(
            widths.iter().all(|&w| w % (12 * 3600) == 0),
            "12h grid: {widths:?}"
        );
    }

    #[tokio::test]
    async fn percentile_trend_drops_the_hour_once_points_are_daily() {
        let pool = open_test_db().await;
        // 90 days spans 2137 hours, which lands on the 24h step.
        for day in 0..90i64 {
            insert_metric_hour(&pool, 1, "/checkout", day * 86_400, 1, 6).await;
        }
        let trend = transaction_percentile_trend(&pool, 1, "/checkout", 0)
            .await
            .unwrap();
        assert_eq!(trend.len(), 90);
        assert!(!trend[0].label.contains(':'), "{}", trend[0].label);
    }

    #[tokio::test]
    async fn percentile_trend_keeps_the_most_recent_points_when_capped() {
        let pool = open_test_db().await;
        // 1000 weeks is past the coarsest step, so the series is trimmed rather
        // than compressed further — and it is the recent end that survives.
        for week in 0..1000i64 {
            insert_metric_hour(&pool, 1, "/checkout", week * 7 * 86_400, 1, 6).await;
        }
        let trend = transaction_percentile_trend(&pool, 1, "/checkout", 0)
            .await
            .unwrap();
        assert_eq!(trend.len(), MAX_TREND_POINTS as usize);
        let last_week_start = 999 * 7 * 86_400;
        assert!(
            trend[trend.len() - 1].bucket + 168 * 3600 > last_week_start,
            "the newest point must survive the trim"
        );
    }

    #[tokio::test]
    async fn percentile_trend_marks_a_spike() {
        let pool = open_test_db().await;
        // Six flat hours in bucket 6 [64,128), then a spike into bucket 12
        // [4096,8192) — comfortably past 1.5x the trailing median.
        for h in 1..=6i64 {
            insert_metric_hour(&pool, 1, "/checkout", h * 3600, 10, 6).await;
        }
        insert_metric_hour(&pool, 1, "/checkout", 7 * 3600, 10, 12).await;

        let trend = transaction_percentile_trend(&pool, 1, "/checkout", 0)
            .await
            .unwrap();
        assert_eq!(trend.len(), 7);
        assert!(
            trend[..6].iter().all(|p| !p.regressed),
            "the flat run and the ineligible first five must stay unmarked"
        );
        assert!(trend[6].regressed, "the spike must be marked");
    }

    #[tokio::test]
    async fn percentile_trend_empty_without_rows() {
        let pool = open_test_db().await;
        assert!(transaction_percentile_trend(&pool, 1, "/checkout", 0)
            .await
            .unwrap()
            .is_empty());
    }

    #[test]
    fn regression_marker_needs_a_full_trailing_window() {
        // A spike inside the ineligible prefix is never marked.
        let marks = mark_regressions(&[100, 100, 100, 100, 9999]);
        assert_eq!(marks, vec![false; 5]);
    }

    #[test]
    fn regression_marker_flags_a_spike_over_the_trailing_median() {
        let marks = mark_regressions(&[100, 100, 100, 100, 100, 200, 100]);
        assert_eq!(marks, vec![false, false, false, false, false, true, false]);
    }

    #[test]
    fn regression_marker_ignores_growth_under_the_factor() {
        // 1.5x exactly is not "exceeds", so 150 over a median of 100 stays clean.
        let marks = mark_regressions(&[100, 100, 100, 100, 100, 150]);
        assert!(!marks[5]);
        assert!(mark_regressions(&[100, 100, 100, 100, 100, 151])[5]);
    }

    #[test]
    fn regression_marker_survives_a_quiet_trailing_window() {
        // No traffic for five points, then a real number: a zero median must not
        // turn "any value at all" into a regression.
        let marks = mark_regressions(&[0, 0, 0, 0, 0, 500]);
        assert!(!marks[5]);
    }

    #[test]
    fn regression_marker_median_resists_a_single_outlier() {
        // One huge point in the window shifts the mean but not the median, so the
        // point after a spike is still judged against the normal level.
        let marks = mark_regressions(&[100, 9999, 100, 100, 100, 200]);
        assert!(marks[5]);
    }

    #[test]
    fn trend_step_widens_with_the_span() {
        assert_eq!(trend_bucket_hours(24), 1);
        assert_eq!(trend_bucket_hours(168), 3); // 7d
        assert_eq!(trend_bucket_hours(720), 6); // 30d
        assert_eq!(trend_bucket_hours(2160), 24); // 90d
        assert_eq!(trend_bucket_hours(8760), 168); // 365d
                                                   // Beyond the coarsest step it saturates rather than failing.
        assert_eq!(trend_bucket_hours(876_000), 168);
        // Every step keeps the series under the cap, except where it saturates.
        for span in [1i64, 5, 100, 500, 5000, 20_000] {
            assert!(
                span / trend_bucket_hours(span) <= MAX_TREND_POINTS,
                "{span}"
            );
        }
    }

    #[test]
    fn distribution_empty_when_no_samples() {
        let buckets = [0u64; NUM_BUCKETS];
        assert!(distribution_buckets(&buckets).is_empty());
    }

    #[test]
    fn distribution_trims_to_populated_range() {
        let mut buckets = [0u64; NUM_BUCKETS];
        buckets[8] = 10; // [256, 512)
        buckets[10] = 5; // [1024, 2048), a gap at bucket 9 stays visible
        let dist = distribution_buckets(&buckets);
        // First..=last populated, including the empty in-between bucket.
        assert_eq!(dist.len(), 3);
        assert_eq!(dist[0].label, "256-512ms");
        assert_eq!(dist[0].count, 10);
        assert!(
            (dist[0].pct - 100.0).abs() < f64::EPSILON,
            "busiest = full bar"
        );
        assert_eq!(dist[1].count, 0);
        assert!(dist[1].pct.abs() < f64::EPSILON);
        assert_eq!(dist[2].label, "1024-2048ms");
        assert!((dist[2].pct - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn percentile_total_zero() {
        let buckets = [0u64; NUM_BUCKETS];
        assert_eq!(percentile_from_buckets(&buckets, 0, 0.5), 0);
    }

    #[test]
    fn percentile_single_bucket() {
        // All 10 samples land in bucket 10 -> [1024, 2048).
        let mut buckets = [0u64; NUM_BUCKETS];
        buckets[10] = 10;
        let p50 = percentile_from_buckets(&buckets, 10, 0.50);
        let p95 = percentile_from_buckets(&buckets, 10, 0.95);
        assert!((1024..=2048).contains(&p50), "p50={p50}");
        assert!((1024..=2048).contains(&p95), "p95={p95}");
        assert!(p95 >= p50);
    }

    #[test]
    fn percentile_monotonic() {
        let mut buckets = [0u64; NUM_BUCKETS];
        buckets[2] = 5; // [4,8)
        buckets[6] = 5; // [64,128)
        buckets[12] = 5; // [4096,8192)
        let total = 15;
        let p25 = percentile_from_buckets(&buckets, total, 0.25);
        let p50 = percentile_from_buckets(&buckets, total, 0.50);
        let p95 = percentile_from_buckets(&buckets, total, 0.95);
        assert!(p25 <= p50, "p25={p25} p50={p50}");
        assert!(p50 <= p95, "p50={p50} p95={p95}");
        // p50 should fall in the middle bucket [64,128).
        assert!((64..=128).contains(&p50), "p50={p50}");
        // p95 should fall in the high bucket [4096,8192).
        assert!((4096..=8192).contains(&p95), "p95={p95}");
    }

    #[test]
    fn percentile_distribution_low() {
        // Heavy on bucket 0 [1,2), a few slow.
        let mut buckets = [0u64; NUM_BUCKETS];
        buckets[0] = 95;
        buckets[10] = 5;
        let total = 100;
        let p50 = percentile_from_buckets(&buckets, total, 0.50);
        let p95 = percentile_from_buckets(&buckets, total, 0.95);
        assert!((1..=2).contains(&p50), "p50={p50}");
        // p95 sits right at the boundary into the slow bucket.
        assert!(p95 >= 2, "p95={p95}");
    }
}
