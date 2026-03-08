use anyhow::Result;
use sqlx::Row;

use crate::db::sql;
use crate::db::DbPool;

use super::types::{
    AttachmentInfo, EventDetail, EventNav, EventSupplements, ExtractedEventData, ProjectRepo,
    Release, UserReportData,
};

/// Grab all the supplementary data an event detail page needs in one go --
/// nav links, attachments, user reports, commit SHA, repos. That's 5 queries
/// we'd otherwise scatter across the handler.
pub async fn get_event_supplements(pool: &DbPool, event: &EventDetail) -> Result<EventSupplements> {
    let event_nav = if let Some(fp) = event.fingerprint.as_deref() {
        get_adjacent_events(pool, fp, event.timestamp, &event.event_id)
            .await
            .unwrap_or_default()
    } else {
        EventNav::default()
    };

    let attachments = list_attachments_for_event(pool, &event.event_id)
        .await
        .unwrap_or_default();
    let user_reports = list_user_reports_for_event(pool, &event.event_id)
        .await
        .unwrap_or_default();

    let commit_sha = if let Some(version) = event.release.as_deref() {
        get_release(pool, event.project_id, version)
            .await
            .ok()
            .flatten()
            .and_then(|r| r.commit_sha)
    } else {
        None
    };
    let repos = get_project_repos(pool, event.project_id)
        .await
        .unwrap_or_default();

    Ok(EventSupplements {
        event_nav,
        attachments,
        user_reports,
        commit_sha,
        repos,
    })
}

/// Pull user reports attached to a parent event.
pub async fn list_user_reports_for_event(
    pool: &DbPool,
    parent_event_id: &str,
) -> Result<Vec<UserReportData>> {
    let rows = sqlx::query(sql!(
        "SELECT payload, timestamp FROM events
         WHERE item_type = 'user_report' AND parent_event_id = ?1
         ORDER BY timestamp DESC"
    ))
    .bind(parent_event_id)
    .fetch_all(pool)
    .await?;

    let mut reports = Vec::new();
    for row in &rows {
        let blob: Vec<u8> = row.get("payload");
        let timestamp: i64 = row.get("timestamp");

        let json: serde_json::Value = match super::events::decompress_payload(&blob) {
            Ok(v) => v,
            Err(_) => continue,
        };

        reports.push(UserReportData {
            name: json.get("name").and_then(|v| v.as_str()).map(String::from),
            email: json.get("email").and_then(|v| v.as_str()).map(String::from),
            comments: json
                .get("comments")
                .and_then(|v| v.as_str())
                .map(String::from),
            timestamp,
        });
    }
    Ok(reports)
}

/// Find the prev/next events for navigation within an issue.
pub async fn get_adjacent_events(
    pool: &DbPool,
    fingerprint: &str,
    timestamp: i64,
    event_id: &str,
) -> Result<EventNav> {
    let total_row = sqlx::query(sql!("SELECT COUNT(*) FROM events WHERE fingerprint = ?1"))
        .bind(fingerprint)
        .fetch_one(pool)
        .await?;
    let total: u64 = total_row.get::<i64, _>(0) as u64;

    // "Previous" = newer event, since we display newest-first
    let prev_row = sqlx::query(sql!(
        "SELECT event_id FROM events WHERE fingerprint = ?1
         AND (timestamp > ?2 OR (timestamp = ?2 AND event_id < ?3))
         ORDER BY timestamp ASC, event_id DESC LIMIT 1"
    ))
    .bind(fingerprint)
    .bind(timestamp)
    .bind(event_id)
    .fetch_optional(pool)
    .await?;
    let prev_event_id: Option<String> = prev_row.map(|r| r.get("event_id"));

    // "Next" = older event in display order
    let next_row = sqlx::query(sql!(
        "SELECT event_id FROM events WHERE fingerprint = ?1
         AND (timestamp < ?2 OR (timestamp = ?2 AND event_id > ?3))
         ORDER BY timestamp DESC, event_id ASC LIMIT 1"
    ))
    .bind(fingerprint)
    .bind(timestamp)
    .bind(event_id)
    .fetch_optional(pool)
    .await?;
    let next_event_id: Option<String> = next_row.map(|r| r.get("event_id"));

    Ok(EventNav {
        prev_event_id,
        next_event_id,
        total,
    })
}

/// List attachment metadata for an event -- no blob data, just the essentials.
pub async fn list_attachments_for_event(
    pool: &DbPool,
    event_id: &str,
) -> Result<Vec<AttachmentInfo>> {
    let rows = sqlx::query(sql!(
        "SELECT id, filename, content_type, LENGTH(data) FROM attachments WHERE event_id = ?1 ORDER BY filename"
    ))
    .bind(event_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| AttachmentInfo {
            id: row.get("id"),
            filename: row.get("filename"),
            content_type: row.get("content_type"),
            size: usize::try_from(row.get::<i64, _>(3) as u64).unwrap_or(usize::MAX),
        })
        .collect())
}

/// Fetch the raw attachment blob by event_id + filename.
pub async fn get_attachment_data(
    pool: &DbPool,
    event_id: &str,
    filename: &str,
) -> Result<Option<(Vec<u8>, Option<String>)>> {
    let row = sqlx::query(sql!(
        "SELECT data, content_type FROM attachments WHERE event_id = ?1 AND filename = ?2"
    ))
    .bind(event_id)
    .bind(filename)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| (r.get("data"), r.get("content_type"))))
}

/// Pull together everything the event detail page needs -- DB supplements
/// plus parsed payload data -- so the template gets one clean struct.
/// The supplements must be fetched separately via `get_event_supplements`.
/// Preload all sourcemaps referenced by this event's `debug_meta.images`.
/// Returns a HashMap of debug_id → parsed SourceMap for sync resolution.
pub async fn preload_sourcemaps(
    pool: &DbPool,
    payload: &serde_json::Value,
) -> std::collections::HashMap<String, ::sourcemap::SourceMap> {
    let mut map = std::collections::HashMap::new();

    let images = payload
        .get("debug_meta")
        .and_then(|dm| dm.get("images"))
        .and_then(|i| i.as_array());

    let images = match images {
        Some(arr) => arr,
        None => return map,
    };

    for img in images {
        let img_type = img.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if img_type != "sourcemap" {
            continue;
        }
        let debug_id = match img.get("debug_id").and_then(|v| v.as_str()) {
            Some(id) => id.to_lowercase(),
            None => continue,
        };

        if map.contains_key(&debug_id) {
            continue;
        }

        match crate::sourcemap::load_sourcemap(pool, &debug_id).await {
            Ok(Some(sm)) => {
                map.insert(debug_id, sm);
            }
            Ok(None) => {
                tracing::debug!("no sourcemap found for debug_id={debug_id}");
            }
            Err(e) => {
                tracing::warn!("failed to load sourcemap {debug_id}: {e}");
            }
        }
    }

    map
}

pub fn get_event_detail_data(
    event: &EventDetail,
    supplements: EventSupplements,
    sourcemap_resolver: Option<&crate::event_data::FrameResolver>,
) -> ExtractedEventData {
    let exceptions = crate::event_data::extract_exceptions(
        &event.payload,
        supplements.commit_sha.as_deref(),
        &supplements.repos,
        sourcemap_resolver,
    );
    let breadcrumbs = crate::event_data::extract_breadcrumbs(&event.payload);
    let tags = crate::event_data::extract_tags(&event.payload);
    let contexts = crate::event_data::extract_contexts(&event.payload);
    let request = crate::event_data::extract_request(&event.payload);
    let user = crate::event_data::extract_user(&event.payload);
    let summary_tags = crate::event_data::extract_summary_tags(&tags, &contexts);
    let raw_json =
        serde_json::to_string_pretty(&event.payload).unwrap_or_else(|_| "{}".to_string());

    ExtractedEventData {
        summary_tags,
        exceptions,
        breadcrumbs,
        tags,
        contexts,
        request,
        user,
        event_nav: supplements.event_nav,
        attachments: supplements.attachments,
        user_reports: supplements.user_reports,
        raw_json,
    }
}

async fn get_release(pool: &DbPool, project_id: u64, version: &str) -> Result<Option<Release>> {
    let row = sqlx::query(sql!(
        "SELECT id, project_id, version, commit_sha, date_released, first_event, last_event, new_groups, created_at
         FROM releases WHERE project_id = ?1 AND version = ?2"
    ))
    .bind(project_id as i64)
    .bind(version)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| Release {
        id: r.get("id"),
        project_id: r.get::<i64, _>("project_id") as u64,
        version: r.get("version"),
        commit_sha: r.get("commit_sha"),
        date_released: r.get("date_released"),
        first_event: r.get("first_event"),
        last_event: r.get("last_event"),
        new_groups: r.get::<i64, _>("new_groups") as u64,
        created_at: r.get("created_at"),
    }))
}

async fn get_project_repos(pool: &DbPool, project_id: u64) -> Result<Vec<ProjectRepo>> {
    let rows = sqlx::query(sql!(
        "SELECT id, project_id, repo_url, forge_type, url_template
         FROM project_repos WHERE project_id = ?1 ORDER BY id"
    ))
    .bind(project_id as i64)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| ProjectRepo {
            id: r.get("id"),
            project_id: r.get::<i64, _>("project_id") as u64,
            repo_url: r.get("repo_url"),
            forge_type: r.get("forge_type"),
            url_template: r.get("url_template"),
        })
        .collect())
}
