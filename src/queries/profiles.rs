use anyhow::Result;
use sqlx::Row;

use crate::db::sql;

use super::types::{Page, PagedResult, ProfileDetail, ProfileSummary};

/// `since_ts` of `None` means all time.
pub async fn list_profiles(
    pool: &crate::db::DbPool,
    project_id: u64,
    page: &Page,
    since_ts: Option<i64>,
) -> Result<PagedResult<ProfileSummary>> {
    let since = since_ts.unwrap_or(0);
    let total: i64 = sqlx::query(sql!(
        "SELECT COUNT(*) FROM events
         WHERE project_id = ?1 AND item_type IN ('profile', 'profile_chunk') AND timestamp >= ?2"
    ))
    .bind(project_id as i64)
    .bind(since)
    .fetch_one(pool)
    .await?
    .get::<i64, _>(0);

    let rows = sqlx::query(sql!(
        "SELECT event_id, project_id, timestamp, transaction_name, platform, release, environment
         FROM events
         WHERE project_id = ?1 AND item_type IN ('profile', 'profile_chunk') AND timestamp >= ?2
         ORDER BY timestamp DESC
         LIMIT ?3 OFFSET ?4"
    ))
    .bind(project_id as i64)
    .bind(since)
    .bind(page.limit as i64)
    .bind(page.offset as i64)
    .fetch_all(pool)
    .await?;

    let items: Vec<ProfileSummary> = rows
        .iter()
        .map(|row| ProfileSummary {
            event_id: row.get("event_id"),
            project_id: row.get::<i64, _>("project_id") as u64,
            timestamp: row.get("timestamp"),
            transaction_name: row.get("transaction_name"),
            platform: row.get("platform"),
            release: row.get("release"),
            environment: row.get("environment"),
        })
        .collect();

    Ok(PagedResult::from_page(items, total, page))
}

pub async fn get_profile(
    pool: &crate::db::DbPool,
    project_id: u64,
    event_id: &str,
) -> Result<Option<ProfileDetail>> {
    let row = sqlx::query(sql!(
        "SELECT event_id, project_id, timestamp, transaction_name, platform, release, environment, payload
         FROM events WHERE event_id = ?1 AND project_id = ?2 AND item_type IN ('profile', 'profile_chunk')"
    ))
    .bind(event_id)
    .bind(project_id as i64)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(row) => {
            let blob: Vec<u8> = row.get("payload");
            let payload = super::events::decompress_payload(&blob)?;
            Ok(Some(ProfileDetail {
                event_id: row.get("event_id"),
                timestamp: row.get("timestamp"),
                transaction_name: row.get("transaction_name"),
                platform: row.get("platform"),
                release: row.get("release"),
                environment: row.get("environment"),
                payload,
            }))
        }
        None => Ok(None),
    }
}
