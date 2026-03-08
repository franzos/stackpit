use anyhow::Result;
use sqlx::Row;

use crate::db::sql;
use crate::db::DbPool;

use super::types::BackfillRow;

/// How many events are still missing a fingerprint.
pub async fn count_missing_fingerprints(pool: &DbPool) -> Result<u64> {
    let row = sqlx::query(sql!(
        "SELECT COUNT(*) FROM events WHERE fingerprint IS NULL"
    ))
    .fetch_one(pool)
    .await?;
    Ok(row.get::<i64, _>(0) as u64)
}

/// Grab a batch of events that still need fingerprinting.
pub async fn fetch_events_without_fingerprint(
    pool: &DbPool,
    limit: u32,
) -> Result<Vec<BackfillRow>> {
    let rows = sqlx::query(sql!(
        "SELECT event_id, item_type, payload, project_id, timestamp, title, level
         FROM events WHERE fingerprint IS NULL LIMIT ?1"
    ))
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| BackfillRow {
            event_id: row.get("event_id"),
            item_type_str: row.get("item_type"),
            payload_blob: row.get("payload"),
            project_id: row.get::<i64, _>("project_id") as u64,
            timestamp: row.get("timestamp"),
            title: row.get("title"),
            level: row.get("level"),
        })
        .collect())
}

/// Stamp a fingerprint onto an event.
pub async fn set_event_fingerprint(pool: &DbPool, event_id: &str, fingerprint: &str) -> Result<()> {
    sqlx::query(sql!(
        "UPDATE events SET fingerprint = ?1 WHERE event_id = ?2"
    ))
    .bind(fingerprint)
    .bind(event_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Upsert an issue from backfill. We prefer existing titles here because
/// the live path usually produces better ones than what we'd extract from
/// old compressed payloads.
pub async fn upsert_backfill_issue(
    pool: &DbPool,
    fingerprint: &str,
    project_id: u64,
    title: Option<&str>,
    level: Option<&str>,
    timestamp: i64,
    item_type: &crate::models::ItemType,
) -> Result<()> {
    super::issues::upsert_issue(
        pool,
        fingerprint,
        project_id,
        title,
        level,
        timestamp,
        timestamp,
        1,
        item_type.as_str(),
        true,
    )
    .await
}
