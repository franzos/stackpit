use anyhow::Result;
use sqlx::Row;

use crate::db::sql;
use crate::db::DbPool;

/// Find synced events that are still missing their attachments.
pub async fn list_synced_events_without_attachments(
    pool: &DbPool,
    project_id: u64,
) -> Result<Vec<String>> {
    let rows = sqlx::query(sql!(
        "SELECT e.event_id FROM events e
         WHERE e.project_id = ?1
           AND e.public_key = 'synced'
           AND NOT EXISTS (SELECT 1 FROM attachments a WHERE a.event_id = e.event_id)
         ORDER BY e.timestamp DESC
         LIMIT 1000"
    ))
    .bind(project_id as i64)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(|r| r.get("event_id")).collect())
}

/// Existing (event_id, filename) pairs for synced events -- used to skip
/// attachments we've already downloaded.
pub async fn list_existing_attachment_keys(
    pool: &DbPool,
    project_id: u64,
) -> Result<std::collections::HashSet<(String, String)>> {
    let rows = sqlx::query(sql!(
        "SELECT event_id, filename FROM attachments
         WHERE event_id IN (SELECT e.event_id FROM events e
            WHERE e.project_id = ?1 AND e.public_key = 'synced')"
    ))
    .bind(project_id as i64)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| (r.get("event_id"), r.get("filename")))
        .collect())
}
