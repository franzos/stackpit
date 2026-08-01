use anyhow::Result;
use sqlx::Row;

use crate::db::sql;
use crate::db::DbPool;

use super::types::{
    AttachmentInfo, EventDetail, EventNav, EventSupplements, ExtractedEventData, ProjectRepo,
    Release, UserReportData,
};

/// Fetch all supplementary data for an event detail page (nav links,
/// attachments, user reports, commit SHA, repos) concurrently via `tokio::join!`.
pub async fn get_event_supplements(pool: &DbPool, event: &EventDetail) -> Result<EventSupplements> {
    let nav_fut = async {
        if let Some(fp) = event.fingerprint.as_deref() {
            get_adjacent_events(pool, fp, event.timestamp, &event.event_id)
                .await
                .unwrap_or_default()
        } else {
            EventNav::default()
        }
    };

    let attachments_fut = async {
        list_attachments_for_event(pool, &event.event_id)
            .await
            .unwrap_or_default()
    };

    let user_reports_fut = async {
        list_user_reports_for_event(pool, &event.event_id)
            .await
            .unwrap_or_default()
    };

    let commit_sha_fut = async {
        if let Some(version) = event.release.as_deref() {
            get_release(pool, event.project_id, version)
                .await
                .ok()
                .flatten()
                .and_then(|r| r.commit_sha)
        } else {
            None
        }
    };

    let repos_fut = async {
        get_project_repos(pool, event.project_id)
            .await
            .unwrap_or_default()
    };

    let (event_nav, attachments, user_reports, commit_sha, repos) = tokio::join!(
        nav_fut,
        attachments_fut,
        user_reports_fut,
        commit_sha_fut,
        repos_fut
    );

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
/// Combines total count, prev, and next into a single query.
pub async fn get_adjacent_events(
    pool: &DbPool,
    fingerprint: &str,
    timestamp: i64,
    event_id: &str,
) -> Result<EventNav> {
    let row = sqlx::query(sql!(
        "SELECT
            (SELECT COUNT(*) FROM events WHERE fingerprint = ?1) AS total,
            (SELECT event_id FROM events WHERE fingerprint = ?1
             AND (timestamp > ?2 OR (timestamp = ?2 AND event_id < ?3))
             ORDER BY timestamp ASC, event_id DESC LIMIT 1) AS prev_event_id,
            (SELECT event_id FROM events WHERE fingerprint = ?1
             AND (timestamp < ?2 OR (timestamp = ?2 AND event_id > ?3))
             ORDER BY timestamp DESC, event_id ASC LIMIT 1) AS next_event_id"
    ))
    .bind(fingerprint)
    .bind(timestamp)
    .bind(event_id)
    .fetch_one(pool)
    .await?;

    Ok(EventNav {
        prev_event_id: row.get("prev_event_id"),
        next_event_id: row.get("next_event_id"),
        total: row.get::<i64, _>("total") as u64,
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

/// Fetch the raw attachment blob, scoped by `project_id` to block cross-project IDOR.
pub async fn get_attachment_data(
    pool: &DbPool,
    project_id: i64,
    event_id: &str,
    filename: &str,
) -> Result<Option<(Vec<u8>, Option<String>)>> {
    let row = sqlx::query(sql!(
        "SELECT data, content_type FROM attachments \
         WHERE project_id = ?1 AND event_id = ?2 AND filename = ?3"
    ))
    .bind(project_id)
    .bind(event_id)
    .bind(filename)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| (r.get("data"), r.get("content_type"))))
}

/// Preload sourcemaps referenced by debug_meta.images (returns debug_id -> SourceMap map).
pub async fn preload_sourcemaps(
    pool: &DbPool,
    payload: &serde_json::Value,
    project_id: u64,
) -> std::collections::HashMap<String, ::sourcemap::SourceMap> {
    let debug_ids = collect_debug_ids(payload);
    if debug_ids.is_empty() {
        return std::collections::HashMap::new();
    }

    match crate::ingest::sourcemap::load_sourcemaps(pool, &debug_ids, project_id).await {
        Ok(map) => map,
        Err(e) => {
            tracing::warn!("failed to load sourcemaps: {e}");
            std::collections::HashMap::new()
        }
    }
}

/// Distinct, plausibly-shaped sourcemap debug ids from an event payload,
/// capped at [`MAX_EVENT_DEBUG_IDS`].
fn collect_debug_ids(payload: &serde_json::Value) -> Vec<String> {
    use crate::ingest::sourcemap::{is_plausible_debug_id, MAX_EVENT_DEBUG_IDS};

    let images = payload
        .get("debug_meta")
        .and_then(|dm| dm.get("images"))
        .and_then(|i| i.as_array());

    let images = match images {
        Some(arr) => arr,
        None => return Vec::new(),
    };

    let mut seen = std::collections::HashSet::new();
    let mut ids = Vec::new();

    for img in images {
        if img.get("type").and_then(|t| t.as_str()) != Some("sourcemap") {
            continue;
        }
        let debug_id = match img.get("debug_id").and_then(|v| v.as_str()) {
            Some(id) => id.to_lowercase(),
            None => continue,
        };
        if !is_plausible_debug_id(&debug_id) || !seen.insert(debug_id.clone()) {
            continue;
        }

        ids.push(debug_id);
        if ids.len() == MAX_EVENT_DEBUG_IDS {
            tracing::debug!("debug_meta.images exceeds {MAX_EVENT_DEBUG_IDS} ids, truncating");
            break;
        }
    }

    ids
}

pub fn get_event_detail_data(
    event: &EventDetail,
    supplements: EventSupplements,
    sourcemap_resolver: Option<&crate::ingest::event_data::FrameResolver>,
) -> ExtractedEventData {
    let exceptions = crate::ingest::event_data::extract_exceptions(
        &event.payload,
        supplements.commit_sha.as_deref(),
        &supplements.repos,
        sourcemap_resolver,
    );
    let breadcrumbs = crate::ingest::event_data::extract_breadcrumbs(&event.payload);
    let tags = crate::ingest::event_data::extract_tags(&event.payload);
    let contexts = crate::ingest::event_data::extract_contexts(&event.payload);
    let request = crate::ingest::event_data::extract_request(&event.payload);
    let user = crate::ingest::event_data::extract_user(&event.payload);
    let summary_tags = crate::ingest::event_data::extract_summary_tags(&tags, &contexts);
    let raw_json = render_raw_json(&event.payload, Some(&event.event_id));

    let own_feedback = (event.item_type == crate::ingest::models::ItemType::UserReport)
        .then(|| extract_user_feedback(&event.payload))
        .filter(|f| f.has_any());

    let measurements = crate::ingest::event_data::extract_measurements(&event.payload);

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
        own_feedback,
        measurements,
        raw_json,
    }
}

/// Payloads of profile/replay item types run to tens of megabytes; the rendered
/// page would carry the pretty-printed copy plus its HTML-escaped render.
const MAX_RAW_JSON_BYTES: usize = 256 * 1024;

/// Pretty-print a payload for display, bounded at [`MAX_RAW_JSON_BYTES`].
/// Pass `full_payload_event_id` only when `GET /api/v1/events/{event_id}/` can
/// actually serve this payload: it decodes stored bytes as JSON, which replay
/// recordings and videos are not.
pub(crate) fn render_raw_json(
    payload: &serde_json::Value,
    full_payload_event_id: Option<&str>,
) -> String {
    let mut writer = BoundedWriter {
        buf: Vec::new(),
        limit: MAX_RAW_JSON_BYTES,
        total: 0,
    };
    if serde_json::to_writer_pretty(&mut writer, payload).is_err() {
        return "{}".to_string();
    }

    let shown = match std::str::from_utf8(&writer.buf) {
        Ok(s) => s,
        Err(e) => std::str::from_utf8(&writer.buf[..e.valid_up_to()]).unwrap_or(""),
    };

    if writer.total <= writer.limit {
        return shown.to_string();
    }

    let shown_kib = shown.len() / 1024;
    let total_kib = writer.total / 1024;
    match full_payload_event_id {
        Some(event_id) => format!(
            "{shown}\n\n[truncated: showing {shown_kib} KiB of {total_kib} KiB - fetch the full payload from GET /api/v1/events/{event_id}/]"
        ),
        None => format!("{shown}\n\n[truncated: showing {shown_kib} KiB of {total_kib} KiB]"),
    }
}

/// Keeps the first `limit` bytes, counts the rest.
struct BoundedWriter {
    buf: Vec<u8>,
    limit: usize,
    total: usize,
}

impl std::io::Write for BoundedWriter {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.total = self.total.saturating_add(data.len());
        if self.buf.len() < self.limit {
            let take = (self.limit - self.buf.len()).min(data.len());
            self.buf.extend_from_slice(&data[..take]);
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Pull feedback fields from a user-report payload. Handles the classic
/// top-level shape (`name`/`email`/`comments`/`event_id`) and falls back to the
/// newer `contexts.feedback` shape.
fn extract_user_feedback(payload: &serde_json::Value) -> crate::queries::types::UserFeedback {
    let fb = payload.get("contexts").and_then(|c| c.get("feedback"));
    let str_at = |obj: Option<&serde_json::Value>, key: &str| -> Option<String> {
        obj.and_then(|o| o.get(key))
            .and_then(|v| v.as_str())
            .map(String::from)
    };
    crate::queries::types::UserFeedback {
        name: str_at(Some(payload), "name").or_else(|| str_at(fb, "name")),
        email: str_at(Some(payload), "email").or_else(|| str_at(fb, "contact_email")),
        comments: str_at(Some(payload), "comments").or_else(|| str_at(fb, "message")),
        event_id: str_at(Some(payload), "event_id").or_else(|| str_at(fb, "associated_event_id")),
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_classic_user_report_fields() {
        let p = json!({"event_id":"abc","name":"Maria","email":"m@x.com","comments":"broken"});
        let f = extract_user_feedback(&p);
        assert_eq!(f.name.as_deref(), Some("Maria"));
        assert_eq!(f.email.as_deref(), Some("m@x.com"));
        assert_eq!(f.comments.as_deref(), Some("broken"));
        assert_eq!(f.event_id.as_deref(), Some("abc"));
        assert!(f.has_any());
    }

    #[test]
    fn falls_back_to_feedback_context() {
        let p = json!({"contexts":{"feedback":{
            "name":"Bob","contact_email":"b@x.com","message":"hi","associated_event_id":"e1"}}});
        let f = extract_user_feedback(&p);
        assert_eq!(f.name.as_deref(), Some("Bob"));
        assert_eq!(f.email.as_deref(), Some("b@x.com"));
        assert_eq!(f.comments.as_deref(), Some("hi"));
        assert_eq!(f.event_id.as_deref(), Some("e1"));
    }

    #[test]
    fn empty_when_no_feedback_fields() {
        assert!(!extract_user_feedback(&json!({"foo": "bar"})).has_any());
    }

    fn payload_with_images(count: usize) -> serde_json::Value {
        let images: Vec<_> = (0..count)
            .map(|i| json!({"type": "sourcemap", "debug_id": format!("{i:032x}")}))
            .collect();
        json!({"debug_meta": {"images": images}})
    }

    #[test]
    fn debug_ids_are_capped() {
        let ids = collect_debug_ids(&payload_with_images(30_000));
        assert_eq!(
            ids.len(),
            crate::ingest::sourcemap::MAX_EVENT_DEBUG_IDS,
            "fan-out must stop at the cap"
        );
    }

    #[test]
    fn debug_ids_dedupe_and_skip_junk() {
        let p = json!({"debug_meta": {"images": [
            {"type": "sourcemap", "debug_id": "AAAAAAAA1111"},
            {"type": "sourcemap", "debug_id": "aaaaaaaa1111"},
            {"type": "sourcemap", "debug_id": "x"},
            {"type": "macho", "debug_id": "bbbbbbbb2222"},
            {"type": "sourcemap"},
            {"type": "sourcemap", "debug_id": "cccccccc3333"},
        ]}});
        assert_eq!(
            collect_debug_ids(&p),
            vec!["aaaaaaaa1111".to_string(), "cccccccc3333".to_string()]
        );
    }

    #[test]
    fn no_debug_meta_yields_no_ids() {
        assert!(collect_debug_ids(&json!({"foo": "bar"})).is_empty());
    }

    #[test]
    fn small_payload_renders_untruncated() {
        let out = render_raw_json(&json!({"a": 1}), Some("e1"));
        assert_eq!(out, "{\n  \"a\": 1\n}");
    }

    #[test]
    fn oversized_payload_is_truncated_with_notice() {
        let big = json!({"blob": "x".repeat(MAX_RAW_JSON_BYTES * 2)});
        let out = render_raw_json(&big, Some("e1"));
        assert!(out.len() < MAX_RAW_JSON_BYTES + 512);
        assert!(out.contains("[truncated: showing 256 KiB of 512 KiB"));
        assert!(out.contains("/api/v1/events/e1/"));
    }

    #[test]
    fn truncation_notice_omits_endpoint_when_unavailable() {
        let big = json!({"blob": "x".repeat(MAX_RAW_JSON_BYTES * 2)});
        let out = render_raw_json(&big, None);
        assert!(out.contains("[truncated: showing 256 KiB of 512 KiB]"));
        assert!(!out.contains("/api/v1/events/"));
    }

    #[test]
    fn truncation_does_not_split_utf8() {
        let big = json!({"blob": "é".repeat(MAX_RAW_JSON_BYTES)});
        let out = render_raw_json(&big, Some("e1"));
        assert!(out.contains("[truncated:"));
    }
}
