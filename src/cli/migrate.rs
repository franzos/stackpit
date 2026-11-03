use std::collections::HashMap;
use std::time::Instant;

use anyhow::Result;
use sqlx::Row;

use crate::db::{self, sql, DbPool};
use crate::queries::client_reports::{for_each_outcome, upsert_outcomes, OutcomeKey};

const BACKFILL_CHUNK: i64 = 5000;

/// `sync_state` key written once the client report rollup holds all stored reports.
const CLIENT_REPORT_BACKFILL_KEY: &str = "backfill:client_report_outcomes";

/// Apply every pending migration, then compact and re-analyse the database.
pub async fn run(database_url: &str) -> Result<()> {
    let pool = db::create_migration_pool(database_url).await?;

    db::run_migrations_unchecked(&pool).await?;
    backfill_trace_op_and_status(&pool, BACKFILL_CHUNK).await?;
    backfill_client_report_outcomes(&pool, BACKFILL_CHUNK).await?;
    compact_and_analyze(&pool).await?;
    db::restore_journal_mode(&pool).await?;

    pool.close().await;
    println!("migrate complete");
    Ok(())
}

/// Fill `trace_op` / `trace_status` on transactions stored before the columns existed.
/// Resumable: only rows still NULL are read. Returns the number of rows updated.
pub(crate) async fn backfill_trace_op_and_status(pool: &DbPool, chunk: i64) -> Result<u64> {
    let mut updated: u64 = 0;
    let mut after = String::new();
    loop {
        let rows = sqlx::query(sql!(
            "SELECT e.event_id, p.payload FROM events e \
             LEFT JOIN event_payloads p ON p.event_id = e.event_id \
             WHERE e.item_type = 'transaction' AND e.trace_op IS NULL AND e.event_id > ?1 \
             ORDER BY e.event_id LIMIT ?2"
        ))
        .bind(&after)
        .bind(chunk)
        .fetch_all(pool)
        .await?;
        let Some(last) = rows.last() else { break };
        after = last.get("event_id");

        let mut tx = pool.begin().await?;
        for row in &rows {
            let payload: Option<Vec<u8>> = row.get("payload");
            let (op, status) = payload
                .and_then(|p| crate::queries::events::decompress_payload(&p).ok())
                .map(|json| crate::ingest::envelope::trace_op_and_status(&json))
                .unwrap_or_default();
            sqlx::query(sql!(
                "UPDATE events SET trace_op = ?1, trace_status = ?2 WHERE event_id = ?3"
            ))
            .bind(op.unwrap_or_default())
            .bind(status.unwrap_or_default())
            .bind(row.get::<String, _>("event_id"))
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;

        updated += rows.len() as u64;
        println!("backfill trace_op/trace_status: {updated} transactions");
    }
    Ok(updated)
}

/// Rebuild `client_report_outcomes` from every stored client report, in one
/// transaction that also records completion. Rows the writer already rolled up
/// are replaced, not added to. Returns the number of reports read, `None` when
/// an earlier run completed.
pub(crate) async fn backfill_client_report_outcomes(
    pool: &DbPool,
    chunk: i64,
) -> Result<Option<u64>> {
    let mut tx = pool.begin().await?;
    let done: Option<String> =
        sqlx::query_scalar(sql!("SELECT value FROM sync_state WHERE key = ?1"))
            .bind(CLIENT_REPORT_BACKFILL_KEY)
            .fetch_optional(&mut *tx)
            .await?;
    if done.is_some() {
        return Ok(None);
    }

    sqlx::query("DELETE FROM client_report_outcomes")
        .execute(&mut *tx)
        .await?;

    let mut outcomes: HashMap<OutcomeKey, u64> = HashMap::new();
    let mut read: u64 = 0;
    let mut after = String::new();
    loop {
        let rows = sqlx::query(sql!(
            "SELECT e.event_id, e.project_id, e.timestamp, p.payload FROM events e \
             JOIN event_payloads p ON p.event_id = e.event_id \
             WHERE e.item_type = 'client_report' AND e.event_id > ?1 \
             ORDER BY e.event_id LIMIT ?2"
        ))
        .bind(&after)
        .bind(chunk)
        .fetch_all(&mut *tx)
        .await?;
        let Some(last) = rows.last() else { break };
        after = last.get("event_id");

        for row in &rows {
            let project_id = row.get::<i64, _>("project_id") as u64;
            let hour_bucket = (row.get::<i64, _>("timestamp") / 3600) * 3600;
            let payload: Vec<u8> = row.get("payload");
            for_each_outcome(&payload, |category, reason, quantity| {
                let total = outcomes
                    .entry((project_id, hour_bucket, category, reason))
                    .or_insert(0);
                *total = total.saturating_add(quantity);
            });
        }
        read += rows.len() as u64;
        println!("backfill client report outcomes: {read} reports");
    }

    upsert_outcomes(&mut tx, &outcomes).await?;
    sqlx::query(sql!(
        "INSERT INTO sync_state (key, value, updated_at) VALUES (?1, 'done', ?2)"
    ))
    .bind(CLIENT_REPORT_BACKFILL_KEY)
    .bind(chrono::Utc::now().timestamp())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Some(read))
}

async fn compact_and_analyze(pool: &DbPool) -> Result<()> {
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        step(pool, "vacuum", "VACUUM").await?;
        // Unlimited: the periodic ANALYZE samples, this one runs once while nothing else writes.
        sqlx::query("PRAGMA analysis_limit = 0")
            .execute(pool)
            .await?;
    }
    step(pool, "analyze", "ANALYZE").await
}

async fn step(pool: &DbPool, name: &str, sql: &'static str) -> Result<()> {
    println!("{name} ...");
    let start = Instant::now();
    sqlx::query(sql).execute(pool).await?;
    println!("  done in {:.1?}", start.elapsed());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn trace_backfill_walks_chunks_and_skips_done_rows() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        let zstd_json = |v: serde_json::Value| {
            zstd::encode_all(serde_json::to_vec(&v).unwrap().as_slice(), 3).unwrap()
        };
        let rows: [(&str, &str, Option<Vec<u8>>); 5] = [
            (
                "a",
                "transaction",
                Some(zstd_json(
                    serde_json::json!({"contexts": {"trace": {"op": "db", "status": "ok"}}}),
                )),
            ),
            (
                "b",
                "transaction",
                Some(br#"{"contexts":{"trace":{"op":"raw"}}}"#.to_vec()),
            ),
            ("c", "transaction", Some(zstd_json(serde_json::json!({})))),
            ("d", "transaction", None),
            (
                "e",
                "event",
                Some(zstd_json(
                    serde_json::json!({"contexts": {"trace": {"op": "x"}}}),
                )),
            ),
        ];
        for (event_id, item_type, payload) in &rows {
            sqlx::query(sql!(
                "INSERT INTO events (event_id, item_type, project_id, public_key, timestamp) \
                 VALUES (?1, ?2, 1, 'pk', 100)"
            ))
            .bind(event_id)
            .bind(item_type)
            .execute(&pool)
            .await
            .unwrap();
            if let Some(payload) = payload {
                crate::queries::test_helpers::insert_test_payload(&pool, event_id, payload).await;
            }
        }

        assert_eq!(backfill_trace_op_and_status(&pool, 2).await.unwrap(), 4);
        assert_eq!(backfill_trace_op_and_status(&pool, 2).await.unwrap(), 0);

        let got: Vec<(String, Option<String>, Option<String>)> =
            sqlx::query_as("SELECT event_id, trace_op, trace_status FROM events ORDER BY event_id")
                .fetch_all(&pool)
                .await
                .unwrap();
        let s = |v: &str| Some(v.to_string());
        assert_eq!(
            got,
            vec![
                ("a".to_string(), s("db"), s("ok")),
                ("b".to_string(), s("raw"), s("")),
                ("c".to_string(), s(""), s("")),
                ("d".to_string(), s(""), s("")),
                ("e".to_string(), None, None),
            ]
        );
    }

    #[tokio::test]
    async fn client_report_backfill_rebuilds_once() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        let report = |q: u64| {
            let body = serde_json::json!({
                "discarded_events": [{"category": "error", "reason": "sample_rate", "quantity": q}],
                "rate_limited_events": [{"category": "span", "reason": "ratelimit_backoff", "quantity": 1}]
            });
            zstd::encode_all(serde_json::to_vec(&body).unwrap().as_slice(), 3).unwrap()
        };
        let rows: [(&str, i64, i64, &str, Vec<u8>); 4] = [
            ("a", 1, 3_600, "client_report", report(2)),
            ("b", 1, 7_199, "client_report", report(3)),
            ("c", 2, 7_200, "client_report", report(5)),
            ("d", 1, 3_600, "event", report(100)),
        ];
        for (event_id, project_id, ts, item_type, payload) in &rows {
            sqlx::query(sql!(
                "INSERT INTO events (event_id, item_type, project_id, public_key, timestamp) \
                 VALUES (?1, ?2, ?3, 'pk', ?4)"
            ))
            .bind(event_id)
            .bind(item_type)
            .bind(project_id)
            .bind(ts)
            .execute(&pool)
            .await
            .unwrap();
            crate::queries::test_helpers::insert_test_payload(&pool, event_id, payload).await;
        }
        // A row the writer rolled up before the backfill ran; rebuilt, not added to.
        sqlx::query(
            "INSERT INTO client_report_outcomes (project_id, hour_bucket, category, reason, quantity) \
             VALUES (1, 3600, 'error', 'sample_rate', 2)",
        )
        .execute(&pool)
        .await
        .unwrap();

        assert_eq!(
            backfill_client_report_outcomes(&pool, 2).await.unwrap(),
            Some(3)
        );
        let read = || async {
            sqlx::query_as::<_, (i64, i64, String, String, i64)>(
                "SELECT project_id, hour_bucket, category, reason, quantity \
                 FROM client_report_outcomes ORDER BY project_id, hour_bucket, category",
            )
            .fetch_all(&pool)
            .await
            .unwrap()
        };
        let r = |p, h, c: &str, reason: &str, q| (p, h, c.to_string(), reason.to_string(), q);
        let expected = vec![
            r(1, 3600, "error", "sample_rate", 5),
            r(1, 3600, "span", "ratelimit_backoff", 2),
            r(2, 7200, "error", "sample_rate", 5),
            r(2, 7200, "span", "ratelimit_backoff", 1),
        ];
        assert_eq!(read().await, expected);

        assert_eq!(
            backfill_client_report_outcomes(&pool, 2).await.unwrap(),
            None
        );
        assert_eq!(read().await, expected);
    }
}
