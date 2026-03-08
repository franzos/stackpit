use anyhow::Result;
use sqlx::Row;

use crate::db::sql;
use crate::db::DbPool;

use super::types::{EventSummary, MonitorSummary, Page, PagedResult};

/// All monitors for a project, grouped by slug.
pub async fn list_monitors(pool: &DbPool, project_id: u64) -> Result<Vec<MonitorSummary>> {
    let rows = sqlx::query(sql!(
        "SELECT
            monitor_slug,
            (SELECT title FROM events e2
             WHERE e2.project_id = events.project_id AND e2.monitor_slug = events.monitor_slug
             ORDER BY e2.timestamp DESC LIMIT 1) AS last_title,
            MAX(timestamp) AS last_checkin,
            COUNT(*) AS cnt
         FROM events
         WHERE project_id = ?1 AND monitor_slug IS NOT NULL
         GROUP BY monitor_slug
         ORDER BY last_checkin DESC"
    ))
    .bind(project_id as i64)
    .fetch_all(pool)
    .await?;

    let monitors = rows
        .iter()
        .map(|row| {
            let slug: String = row.get("monitor_slug");
            let last_title: Option<String> = row.get("last_title");
            // Status is embedded in the title as "slug: ok" -- pull it out
            let last_status = last_title
                .and_then(|t| t.split(": ").nth(1).map(String::from))
                .unwrap_or_else(|| "unknown".to_string());
            MonitorSummary {
                monitor_slug: slug,
                last_status,
                last_checkin: row.get("last_checkin"),
                checkin_count: row.get::<i64, _>("cnt") as u64,
            }
        })
        .collect();

    Ok(monitors)
}

/// Check-in events for a specific monitor, paginated.
pub async fn list_checkins_for_monitor(
    pool: &DbPool,
    project_id: u64,
    slug: &str,
    page: &Page,
) -> Result<PagedResult<EventSummary>> {
    let total_row = sqlx::query(sql!(
        "SELECT COUNT(*) FROM events WHERE project_id = ?1 AND monitor_slug = ?2"
    ))
    .bind(project_id as i64)
    .bind(slug)
    .fetch_one(pool)
    .await?;
    let total: u64 = total_row.get::<i64, _>(0) as u64;

    let rows = sqlx::query(sql!(
        "SELECT event_id, item_type, project_id, fingerprint, timestamp, level, title, platform, release, environment
         FROM events WHERE project_id = ?1 AND monitor_slug = ?2
         ORDER BY timestamp DESC
         LIMIT ?3 OFFSET ?4"
    ))
    .bind(project_id as i64)
    .bind(slug)
    .bind(page.limit as i64)
    .bind(page.offset as i64)
    .fetch_all(pool)
    .await?;

    let items: Vec<EventSummary> = rows
        .iter()
        .map(|row| {
            let item_type_str: String = row.get("item_type");
            EventSummary {
                event_id: row.get("event_id"),
                item_type: item_type_str.parse().unwrap_or_default(),
                project_id: row.get::<i64, _>("project_id") as u64,
                fingerprint: row.get("fingerprint"),
                timestamp: row.get("timestamp"),
                level: row.get("level"),
                title: row.get("title"),
                platform: row.get("platform"),
                release: row.get("release"),
                environment: row.get("environment"),
            }
        })
        .collect();

    Ok(PagedResult {
        items,
        total,
        offset: page.offset,
        limit: page.limit,
    })
}
