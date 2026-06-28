//! Client report decoding -- aggregates dropped-event outcomes for display.

use anyhow::Result;
use sqlx::Row;
use std::collections::HashMap;

use crate::db::{sql, DbPool};
use serde::Serialize;

/// One dropped-event outcome bucket from client reports.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ClientReportOutcome {
    pub category: String,
    pub reason: String,
    pub quantity: u64,
}

/// Decode client-report payloads in the window and sum dropped events by
/// (category, reason) across both `discarded_events` and `rate_limited_events`.
/// Returns outcomes sorted by quantity descending.
pub async fn summarize_client_reports(
    pool: &DbPool,
    project_id: u64,
    since_ts: i64,
) -> Result<Vec<ClientReportOutcome>> {
    let rows = sqlx::query(sql!(
        "SELECT payload FROM events
         WHERE project_id = ?1 AND item_type = 'client_report' AND timestamp >= ?2"
    ))
    .bind(project_id as i64)
    .bind(since_ts)
    .fetch_all(pool)
    .await?;

    let mut totals: HashMap<(String, String), u64> = HashMap::new();

    for row in &rows {
        let payload: Vec<u8> = row.get("payload");
        // Payloads are zstd-compressed at write time; fall back to raw.
        let bytes = zstd::decode_all(payload.as_slice())
            .ok()
            .unwrap_or_else(|| payload.clone());
        let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            continue;
        };

        for field in ["discarded_events", "rate_limited_events"] {
            let Some(arr) = json.get(field).and_then(|v| v.as_array()) else {
                continue;
            };
            for entry in arr {
                let category = entry
                    .get("category")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let reason = entry
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let quantity = entry.get("quantity").and_then(|v| v.as_u64()).unwrap_or(0);
                *totals.entry((category, reason)).or_insert(0) += quantity;
            }
        }
    }

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
    Ok(out)
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
            "INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp, received_at)
             VALUES (?1, 'client_report', ?2, ?3, 'k', ?4, ?4)"
        ))
        .bind(event_id)
        .bind(&compressed)
        .bind(project_id)
        .bind(ts)
        .execute(pool)
        .await
        .unwrap();
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
    async fn respects_since_window() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        insert_client_report(
            &pool,
            "old",
            1,
            100,
            &serde_json::json!({"discarded_events":[{"category":"error","reason":"x","quantity":99}]}),
        )
        .await;
        let out = summarize_client_reports(&pool, 1, 500).await.unwrap();
        assert!(out.is_empty());
    }
}
