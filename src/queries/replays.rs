use anyhow::Result;
use sqlx::Row;

use crate::db::sql;
use crate::db::DbRowExt;

use super::types::{Page, PagedResult, ReplayDetail, ReplayError, ReplaySummary};

/// Cap on `error_ids` resolved per replay, bounding the IN-list.
const MAX_REPLAY_ERROR_IDS: usize = 50;

pub async fn list_replays(
    pool: &crate::db::DbPool,
    project_id: u64,
    page: &Page,
) -> Result<PagedResult<ReplaySummary>> {
    let total: i64 = sqlx::query(sql!(
        "SELECT COUNT(*) FROM events WHERE project_id = ?1 AND item_type = 'replay_event'"
    ))
    .bind(project_id as i64)
    .fetch_one(pool)
    .await?
    .get::<i64, _>(0);

    let rows = sqlx::query(sql!(
        "SELECT event_id, project_id, timestamp, item_type, release, environment
         FROM events WHERE project_id = ?1 AND item_type = 'replay_event'
         ORDER BY timestamp DESC
         LIMIT ?2 OFFSET ?3"
    ))
    .bind(project_id as i64)
    .bind(page.limit as i64)
    .bind(page.offset as i64)
    .fetch_all(pool)
    .await?;

    let items: Vec<ReplaySummary> = rows
        .iter()
        .map(|row| ReplaySummary {
            event_id: row.get("event_id"),
            project_id: row.get_u64("project_id"),
            timestamp: row.get("timestamp"),
            replay_type: row.get("item_type"),
            release: row.get("release"),
            environment: row.get("environment"),
        })
        .collect();

    Ok(PagedResult::from_page(items, total, page))
}

pub async fn get_replay(
    pool: &crate::db::DbPool,
    project_id: u64,
    event_id: &str,
) -> Result<Option<ReplayDetail>> {
    let row = sqlx::query(sql!(
        "SELECT event_id, project_id, timestamp, item_type, release, environment, payload
         FROM events WHERE event_id = ?1 AND project_id = ?2 AND item_type IN ('replay_event', 'replay_recording', 'replay_video')"
    ))
    .bind(event_id)
    .bind(project_id as i64)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(row) => {
            let blob: Vec<u8> = row.get("payload");
            let item_type: String = row.get("item_type");
            let payload = if item_type == "replay_recording" || item_type == "replay_video" {
                let decoded = match zstd::decode_all(blob.as_slice()) {
                    Ok(d) => d,
                    Err(e) => {
                        tracing::warn!(event_id, "replay recording zstd decode failed: {e}");
                        blob
                    }
                };
                serde_json::Value::String(base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    &decoded,
                ))
            } else {
                super::events::decompress_payload(&blob)?
            };
            Ok(Some(ReplayDetail {
                event_id: row.get("event_id"),
                project_id: row.get_u64("project_id"),
                timestamp: row.get("timestamp"),
                replay_type: item_type,
                release: row.get("release"),
                environment: row.get("environment"),
                payload,
            }))
        }
        None => Ok(None),
    }
}

/// Top-level `error_ids` from a decompressed replay payload. Defensive: empty
/// when the key is absent, not an array, or holds no non-empty strings.
/// Capped at [`MAX_REPLAY_ERROR_IDS`].
fn extract_error_ids(payload: &serde_json::Value) -> Vec<String> {
    payload
        .get("error_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .take(MAX_REPLAY_ERROR_IDS)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// Resolve a replay's referenced `error_ids` to stored error events in the same
/// project. Ids that don't resolve (event dropped or never stored) are omitted.
pub async fn get_replay_errors(
    pool: &crate::db::DbPool,
    project_id: u64,
    payload: &serde_json::Value,
) -> Result<Vec<ReplayError>> {
    let ids = extract_error_ids(payload);
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut qb: sqlx::QueryBuilder<'_, crate::db::Db> = sqlx::QueryBuilder::new(
        "SELECT event_id, fingerprint, title, level, timestamp FROM events WHERE project_id = ",
    );
    qb.push_bind(project_id as i64);
    qb.push(" AND item_type = 'event' AND event_id IN (");
    let mut sep = qb.separated(", ");
    for id in &ids {
        sep.push_bind(id.as_str());
    }
    qb.push(") ORDER BY timestamp DESC");

    let rows = qb.build().fetch_all(pool).await?;
    Ok(rows
        .iter()
        .map(|row| ReplayError {
            event_id: row.get("event_id"),
            fingerprint: row.get("fingerprint"),
            title: row.get("title"),
            level: row.get("level"),
            timestamp: row.get("timestamp"),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::test_helpers::{insert_test_event, open_test_db};

    fn payload(ids: &[&str]) -> serde_json::Value {
        serde_json::json!({ "error_ids": ids })
    }

    #[tokio::test]
    async fn resolves_error_ids_scoped_to_project() {
        let pool = open_test_db().await;
        insert_test_event(
            &pool,
            "e1",
            1,
            100,
            Some("fp1"),
            Some("error"),
            Some("Boom"),
        )
        .await;
        insert_test_event(
            &pool,
            "e2",
            1,
            200,
            Some("fp2"),
            Some("warning"),
            Some("Uh oh"),
        )
        .await;
        // same id-space value on another project must not leak in
        insert_test_event(
            &pool,
            "other",
            2,
            300,
            Some("fpX"),
            Some("error"),
            Some("Nope"),
        )
        .await;

        let errors = get_replay_errors(&pool, 1, &payload(&["e1", "e2", "other", "missing"]))
            .await
            .unwrap();

        let resolved: std::collections::BTreeMap<String, Option<String>> = errors
            .iter()
            .map(|e| (e.event_id.clone(), e.fingerprint.clone()))
            .collect();
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved.get("e1"), Some(&Some("fp1".to_string())));
        assert_eq!(resolved.get("e2"), Some(&Some("fp2".to_string())));
        // project-2 row and the unstored id are both excluded
        assert!(!resolved.contains_key("other"));
        assert!(!resolved.contains_key("missing"));
    }

    #[tokio::test]
    async fn absent_or_malformed_error_ids_resolve_to_nothing() {
        let pool = open_test_db().await;
        insert_test_event(
            &pool,
            "e1",
            1,
            100,
            Some("fp1"),
            Some("error"),
            Some("Boom"),
        )
        .await;
        for p in [
            serde_json::json!({}),
            serde_json::json!({ "error_ids": [] }),
            serde_json::json!({ "error_ids": "e1" }),
        ] {
            assert!(get_replay_errors(&pool, 1, &p).await.unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn error_id_lookup_is_capped() {
        let pool = open_test_db().await;
        let ids: Vec<String> = (0..MAX_REPLAY_ERROR_IDS + 5)
            .map(|i| format!("evt{i}"))
            .collect();
        for (i, id) in ids.iter().enumerate() {
            insert_test_event(
                &pool,
                id,
                1,
                100 + i as i64,
                Some("fp"),
                Some("error"),
                Some("t"),
            )
            .await;
        }
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let errors = get_replay_errors(&pool, 1, &payload(&refs)).await.unwrap();
        assert_eq!(errors.len(), MAX_REPLAY_ERROR_IDS);
    }
}
