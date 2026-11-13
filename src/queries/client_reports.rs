//! Client report decoding -- aggregates dropped-event outcomes for display.

use anyhow::Result;
use sqlx::Row;
use std::collections::HashMap;

use crate::db::{sql, DbPool};
use crate::ingest::text::clean_text;
use crate::queries::types::{Page, PagedResult};
use serde::Serialize;

/// One dropped-event outcome bucket from client reports.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ClientReportOutcome {
    pub category: String,
    pub reason: String,
    pub quantity: u64,
}

/// One stored client report, decoded to a compact per-report summary so the
/// list conveys what was dropped instead of an untitled event id.
#[derive(Debug, Clone)]
pub struct ClientReportRow {
    pub event_id: String,
    pub timestamp: i64,
    pub total_dropped: u64,
    /// Top outcome buckets for this single report (capped for display).
    pub outcomes: Vec<ClientReportOutcome>,
}

/// Rollup key of `client_report_outcomes`: (project_id, hour_bucket, category, reason).
pub(crate) type OutcomeKey = (u64, i64, String, String);

/// Per-entry ceiling on an outcome quantity; the rollup column is BIGINT and sums
/// with `quantity + excluded.quantity`, which Postgres rejects on overflow.
pub(crate) const MAX_OUTCOME_QUANTITY: u64 = 1_000_000_000;

/// Max rollup rows per multi-row upsert: 5 binds per row, under SQLite's 32766.
const OUTCOME_UPSERT_CHUNK_SIZE: usize = 6400;

/// Call `f(category, reason, quantity)` for every entry of a client-report
/// payload's `discarded_events` and `rate_limited_events`, with text cleaned and
/// quantity clamped to [`MAX_OUTCOME_QUANTITY`]. Accepts zstd or raw JSON.
pub(crate) fn for_each_outcome(bytes: &[u8], mut f: impl FnMut(String, String, u64)) {
    let decoded = zstd::decode_all(bytes).ok();
    let bytes = decoded.as_deref().unwrap_or(bytes);
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return;
    };
    for field in ["discarded_events", "rate_limited_events"] {
        let Some(arr) = json.get(field).and_then(|v| v.as_array()) else {
            continue;
        };
        for entry in arr {
            let category = clean_text(
                entry
                    .get("category")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown"),
            );
            let reason = clean_text(
                entry
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown"),
            );
            let quantity = entry
                .get("quantity")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                .min(MAX_OUTCOME_QUANTITY);
            f(category, reason, quantity);
        }
    }
}

/// Decode a single client-report payload into (category, reason) -> quantity.
fn accumulate_outcomes(bytes: &[u8], totals: &mut HashMap<(String, String), u64>) {
    for_each_outcome(bytes, |category, reason, quantity| {
        let total = totals.entry((category, reason)).or_insert(0);
        *total = total.saturating_add(quantity);
    });
}

/// Add `outcomes` to the stored rollup rows, creating missing ones.
pub(crate) async fn upsert_outcomes(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    outcomes: &HashMap<OutcomeKey, u64>,
) -> Result<()> {
    let mut rows: Vec<(&OutcomeKey, i64)> = outcomes
        .iter()
        .map(|(key, quantity)| (key, i64::try_from(*quantity).unwrap_or(i64::MAX)))
        .collect();
    // Deterministic key order so concurrent writers acquire row locks in the same order.
    rows.sort_unstable_by(|a, b| a.0.cmp(b.0));

    for chunk in rows.chunks(OUTCOME_UPSERT_CHUNK_SIZE) {
        let mut builder = sqlx::QueryBuilder::<crate::db::Db>::new(
            "INSERT INTO client_report_outcomes (project_id, hour_bucket, category, reason, quantity) ",
        );
        builder.push_values(
            chunk,
            |mut b, ((project_id, hour_bucket, category, reason), quantity)| {
                b.push_bind(*project_id as i64);
                b.push_bind(*hour_bucket);
                b.push_bind(category.as_str());
                b.push_bind(reason.as_str());
                b.push_bind(*quantity);
            },
        );
        builder.push(
            " ON CONFLICT(project_id, hour_bucket, category, reason) DO UPDATE SET \
             quantity = client_report_outcomes.quantity + excluded.quantity",
        );
        builder.build().execute(&mut **tx).await?;
    }
    Ok(())
}

fn sorted_outcomes(totals: HashMap<(String, String), u64>) -> Vec<ClientReportOutcome> {
    let mut out: Vec<ClientReportOutcome> = totals
        .into_iter()
        .map(|((category, reason), quantity)| ClientReportOutcome {
            category,
            reason,
            quantity,
        })
        .collect();
    out.sort_by(|a, b| {
        b.quantity
            .cmp(&a.quantity)
            .then_with(|| a.category.cmp(&b.category))
            .then_with(|| a.reason.cmp(&b.reason))
    });
    out
}

const CLIENT_REPORTS_PAGE_SQL: &str =
    "SELECT event_id, payload, timestamp FROM events JOIN event_payloads USING (event_id) \
     WHERE project_id = ?1 AND item_type = 'client_report' AND timestamp >= ?2 \
     ORDER BY timestamp DESC LIMIT ?3 OFFSET ?4";

/// Page of stored client reports, each decoded to its own dropped-event
/// summary. `since_ts` of `None` means all time.
pub async fn list_client_reports(
    pool: &DbPool,
    project_id: u64,
    page: &Page,
    since_ts: Option<i64>,
) -> Result<PagedResult<ClientReportRow>> {
    let since = since_ts.unwrap_or(0);
    let total: i64 = sqlx::query_scalar(sql!(
        "SELECT COUNT(*) FROM events
         WHERE project_id = ?1 AND item_type = 'client_report' AND timestamp >= ?2"
    ))
    .bind(project_id as i64)
    .bind(since)
    .fetch_one(pool)
    .await?;

    let rows = sqlx::query(crate::db::dyn_sql(CLIENT_REPORTS_PAGE_SQL))
        .bind(project_id as i64)
        .bind(since)
        .bind(page.limit as i64)
        .bind(page.offset as i64)
        .fetch_all(pool)
        .await?;

    let items = rows
        .iter()
        .map(|row| {
            let payload: Vec<u8> = row.get("payload");
            let mut totals: HashMap<(String, String), u64> = HashMap::new();
            accumulate_outcomes(&payload, &mut totals);
            let total_dropped = totals.values().fold(0u64, |acc, q| acc.saturating_add(*q));
            let mut outcomes = sorted_outcomes(totals);
            outcomes.truncate(3);
            ClientReportRow {
                event_id: row.get("event_id"),
                timestamp: row.get("timestamp"),
                total_dropped,
                outcomes,
            }
        })
        .collect();

    Ok(PagedResult::from_page(items, total, page))
}

const OUTCOME_SUMMARY_SQL: &str =
    "SELECT category, reason, CAST(SUM(quantity) AS BIGINT) AS quantity \
     FROM client_report_outcomes \
     WHERE project_id = ?1 AND hour_bucket >= ?2 \
     GROUP BY category, reason";

/// Dropped events by (category, reason) from the `client_report_outcomes`
/// rollup, sorted by quantity descending. The window starts at the hour
/// holding `since_ts`.
pub async fn summarize_client_reports(
    pool: &DbPool,
    project_id: u64,
    since_ts: i64,
) -> Result<Vec<ClientReportOutcome>> {
    let rows = sqlx::query(crate::db::dyn_sql(OUTCOME_SUMMARY_SQL))
        .bind(project_id as i64)
        .bind(since_ts.div_euclid(3600) * 3600)
        .fetch_all(pool)
        .await?;

    let totals = rows
        .iter()
        .map(|row| {
            let quantity: i64 = row.get("quantity");
            (
                (row.get("category"), row.get("reason")),
                u64::try_from(quantity).unwrap_or(0),
            )
        })
        .collect();
    Ok(sorted_outcomes(totals))
}

/// The payload fold the rollup replaces, kept to check the two agree.
#[cfg(test)]
pub(crate) async fn summarize_from_payloads(
    pool: &DbPool,
    project_id: u64,
    since_ts: i64,
) -> Result<Vec<ClientReportOutcome>> {
    let rows = sqlx::query(sql!(
        "SELECT payload FROM events JOIN event_payloads USING (event_id)
         WHERE project_id = ?1 AND item_type = 'client_report' AND timestamp >= ?2"
    ))
    .bind(project_id as i64)
    .bind(since_ts)
    .fetch_all(pool)
    .await?;

    let mut totals: HashMap<(String, String), u64> = HashMap::new();
    for row in &rows {
        let payload: Vec<u8> = row.get("payload");
        accumulate_outcomes(&payload, &mut totals);
    }
    Ok(sorted_outcomes(totals))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::sql;

    async fn insert_client_report(
        pool: &DbPool,
        event_id: &str,
        project_id: i64,
        ts: i64,
        body: &serde_json::Value,
    ) {
        let bytes = serde_json::to_vec(body).unwrap();
        let compressed = zstd::encode_all(bytes.as_slice(), 3).unwrap();
        sqlx::query(sql!(
            "INSERT INTO events (event_id, item_type, project_id, public_key, timestamp, received_at)
             VALUES (?1, 'client_report', ?2, 'k', ?3, ?3)"
        ))
        .bind(event_id)
        .bind(project_id)
        .bind(ts)
        .execute(pool)
        .await
        .unwrap();
        crate::queries::test_helpers::insert_test_payload(pool, event_id, &compressed).await;
    }

    async fn rebuild_rollup(pool: &DbPool) {
        crate::cli::migrate::backfill_client_report_outcomes(pool, 2)
            .await
            .unwrap()
            .expect("first rebuild runs");
    }

    #[tokio::test]
    async fn sums_discarded_and_rate_limited_by_category_reason() {
        let pool = crate::queries::test_helpers::open_test_db().await;

        insert_client_report(
            &pool,
            "cr1",
            1,
            1000,
            &serde_json::json!({
                "discarded_events": [
                    {"category": "error", "reason": "sample_rate", "quantity": 10},
                    {"category": "error", "reason": "sample_rate", "quantity": 5}
                ],
                "rate_limited_events": [
                    {"category": "transaction", "reason": "rate_limit", "quantity": 7}
                ]
            }),
        )
        .await;
        insert_client_report(
            &pool,
            "cr2",
            1,
            2000,
            &serde_json::json!({
                "discarded_events": [
                    {"category": "error", "reason": "sample_rate", "quantity": 3},
                    {"category": "transaction", "reason": "rate_limit", "quantity": 1}
                ]
            }),
        )
        .await;
        rebuild_rollup(&pool).await;

        let out = summarize_client_reports(&pool, 1, 0).await.unwrap();

        // error/sample_rate = 10+5+3 = 18, transaction/rate_limit = 7+1 = 8
        assert_eq!(out.len(), 2);
        assert_eq!(
            out[0],
            ClientReportOutcome {
                category: "error".to_string(),
                reason: "sample_rate".to_string(),
                quantity: 18
            }
        );
        assert_eq!(
            out[1],
            ClientReportOutcome {
                category: "transaction".to_string(),
                reason: "rate_limit".to_string(),
                quantity: 8
            }
        );
    }

    #[tokio::test]
    async fn respects_since_window_at_hour_granularity() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        let body = |n: u64| serde_json::json!({"discarded_events":[{"category":"error","reason":"x","quantity":n}]});
        insert_client_report(&pool, "old", 1, 100, &body(99)).await;
        insert_client_report(&pool, "same-hour", 1, 7300, &body(2)).await;
        rebuild_rollup(&pool).await;

        // The window opens at the hour holding `since_ts`, so 7300 counts for 7500.
        let out = summarize_client_reports(&pool, 1, 7500).await.unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].quantity, 2);
    }

    // The reports table used to read the whole project while the rollup above it
    // was pinned to 30 days, so the two disagreed about the page's window.
    #[tokio::test]
    async fn list_respects_since_window() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        let body = serde_json::json!({"discarded_events":[{"category":"error","reason":"x","quantity":1}]});
        insert_client_report(&pool, "old", 1, 100, &body).await;
        insert_client_report(&pool, "new", 1, 900, &body).await;

        let page = Page::new(Some(0), Some(25));
        let all = list_client_reports(&pool, 1, &page, None).await.unwrap();
        assert_eq!(all.total, 2);

        let windowed = list_client_reports(&pool, 1, &page, Some(500))
            .await
            .unwrap();
        assert_eq!(windowed.total, 1);
        assert_eq!(windowed.items[0].event_id, "new");
    }

    #[tokio::test]
    async fn outcomes_are_clamped_and_cleaned() {
        let reason = format!("a\0b{}", "r".repeat(5000));
        let body = serde_json::json!({
            "discarded_events": [{"category": "er\u{0}ror", "reason": reason, "quantity": u64::MAX}]
        });
        let bytes = serde_json::to_vec(&body).unwrap();
        let mut seen = Vec::new();
        for_each_outcome(&bytes, |c, r, q| seen.push((c, r, q)));
        let clean_reason = clean_text(&reason);
        assert_eq!(
            clean_reason.chars().count(),
            crate::ingest::text::MAX_TEXT_CHARS
        );
        assert!(clean_reason.starts_with("abr"));
        assert_eq!(
            seen,
            vec![(
                "error".to_string(),
                clean_reason.clone(),
                MAX_OUTCOME_QUANTITY
            )]
        );

        let pool = crate::queries::test_helpers::open_test_db().await;
        insert_client_report(&pool, "huge1", 1, 1000, &body).await;
        insert_client_report(&pool, "huge2", 1, 1000, &body).await;
        rebuild_rollup(&pool).await;

        let out = summarize_client_reports(&pool, 1, 0).await.unwrap();
        assert_eq!(
            out,
            vec![ClientReportOutcome {
                category: "error".to_string(),
                reason: clean_reason,
                quantity: 2 * MAX_OUTCOME_QUANTITY,
            }]
        );
        let page = Page::new(Some(0), Some(25));
        let list = list_client_reports(&pool, 1, &page, None).await.unwrap();
        assert_eq!(list.items[0].total_dropped, MAX_OUTCOME_QUANTITY);
    }

    #[tokio::test]
    async fn summary_plan_seeks_the_rollup_key() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        sqlx::query(sql!(
            "WITH RECURSIVE seq(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM seq WHERE x < 2000)
             INSERT INTO client_report_outcomes (project_id, hour_bucket, category, reason, quantity)
             SELECT 1 + x % 4, (x / 10) * 3600, 'error', 'r' || (x % 10), 1 FROM seq"
        ))
        .execute(&pool)
        .await
        .unwrap();

        let mut tx =
            crate::queries::test_helpers::plan_tx(&pool, &["client_report_outcomes"]).await;
        let plan = crate::queries::test_helpers::query_plan(&mut *tx, OUTCOME_SUMMARY_SQL, |q| {
            q.bind(1i64).bind(360_000i64)
        })
        .await;

        let pk = crate::queries::test_helpers::pk_index("client_report_outcomes");
        assert!(plan.iter().any(|d| d.contains(&pk)), "{plan:#?}");
        assert!(
            !plan
                .iter()
                .any(|d| d.starts_with("SCAN ") || d.contains("Seq Scan")),
            "{plan:#?}"
        );
    }

    #[tokio::test]
    async fn page_plan_walks_the_project_type_index_and_seeks_payloads() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        sqlx::query(sql!(
            "WITH RECURSIVE seq(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM seq WHERE x < 3000)
             INSERT INTO events (event_id, item_type, project_id, public_key, timestamp, received_at)
             SELECT 'cr-' || x, CASE x % 3 WHEN 0 THEN 'client_report' WHEN 1 THEN 'event' ELSE 'transaction' END,
                    1 + x % 30, 'testkey', 1000 + x, 1000 + x
             FROM seq"
        ))
        .execute(&pool)
        .await
        .unwrap();
        crate::queries::test_helpers::insert_missing_test_payloads(&pool).await;

        let mut tx =
            crate::queries::test_helpers::plan_tx(&pool, &["events", "event_payloads"]).await;
        let plan =
            crate::queries::test_helpers::query_plan(&mut *tx, CLIENT_REPORTS_PAGE_SQL, |q| {
                q.bind(1i64).bind(0i64).bind(25i64).bind(0i64)
            })
            .await;

        assert!(
            plan.iter()
                .any(|d| d.contains("idx_events_project_type_time")),
            "{plan:#?}"
        );
        let pk = crate::queries::test_helpers::pk_index("event_payloads");
        assert!(plan.iter().any(|d| d.contains(&pk)), "{plan:#?}");
        assert!(
            !plan
                .iter()
                .any(|d| d.contains("TEMP B-TREE") || d.contains("Sort")),
            "{plan:#?}"
        );
    }
}
