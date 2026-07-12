//! Transaction performance queries over the `transaction_metrics` rollup
//! and the per-instance `events` rows.

use anyhow::Result;
use simple_hll::HyperLogLog;
use sqlx::Row;

use crate::db::sql;
use crate::ingest::models::HLL_REGISTER_COUNT;

use super::types::{
    DurationBucket, Page, PagedResult, TransactionDistribution, TransactionInstance,
    TransactionSummary,
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
    let query = crate::db::translate_sql(&raw);

    let rows = sqlx::query(&query)
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
    let query = crate::db::translate_sql(&raw);

    let rows = sqlx::query(&query)
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
