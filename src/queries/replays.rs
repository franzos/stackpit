use anyhow::Result;
use sqlx::Row;

use crate::db::sql;

use super::types::{Page, PagedResult, ReplayDetail, ReplaySummary};

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
            project_id: row.get::<i64, _>("project_id") as u64,
            timestamp: row.get("timestamp"),
            replay_type: row.get("item_type"),
            release: row.get("release"),
            environment: row.get("environment"),
        })
        .collect();

    Ok(PagedResult {
        items,
        total: total as u64,
        offset: page.offset,
        limit: page.limit,
    })
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
                project_id: row.get::<i64, _>("project_id") as u64,
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
