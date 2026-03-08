use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::event_data::*;
use crate::extractors::ReadPool;
use crate::html::render_template;
use crate::queries;
use crate::queries::types::{AttachmentInfo, EventNav};
use crate::queries::{event_supplements, ExtractedEventData};
use crate::server::AppState;

use super::html_error;

// askama needs these filters in scope for template derivation
use crate::html::filters;

#[derive(Template)]
#[template(path = "event_detail.html")]
struct EventDetailTemplate {
    event: queries::EventDetail,
    summary_tags: Vec<SummaryTag>,
    exceptions: Vec<ExceptionData>,
    breadcrumbs: Vec<Breadcrumb>,
    tags: Vec<Tag>,
    contexts: Vec<ContextGroup>,
    request: Option<RequestInfo>,
    user: UserInfo,
    event_nav: EventNav,
    attachments: Vec<AttachmentInfo>,
    user_reports: Vec<queries::UserReportData>,
    raw_json: String,
}

pub async fn handler(
    State(_state): State<AppState>,
    ReadPool(pool): ReadPool,
    Path((project_id, event_id)): Path<(u64, String)>,
) -> axum::response::Response {
    let event = match queries::events::get_event_detail(&pool, &event_id).await {
        Ok(Some(e)) => e,
        Ok(None) => return html_error(StatusCode::NOT_FOUND, "Event not found"),
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    if event.project_id != project_id {
        return html_error(StatusCode::NOT_FOUND, "Event not found in this project");
    }

    let supplements = event_supplements::get_event_supplements(&pool, &event)
        .await
        .unwrap_or_default();

    let sourcemaps: std::collections::HashMap<String, ::sourcemap::SourceMap> =
        event_supplements::preload_sourcemaps(&pool, &event.payload).await;
    let resolver =
        move |debug_id: &str, line: u32, col: u32| -> Option<crate::sourcemap::ResolvedFrame> {
            let sm = sourcemaps.get(debug_id)?;
            crate::sourcemap::resolve_frame(sm, line, col)
        };

    let ExtractedEventData {
        summary_tags,
        exceptions,
        breadcrumbs,
        tags,
        contexts,
        request,
        user,
        event_nav,
        attachments,
        user_reports,
        raw_json,
    } = event_supplements::get_event_detail_data(&event, supplements, Some(&resolver));

    let tmpl = EventDetailTemplate {
        event,
        summary_tags,
        exceptions,
        breadcrumbs,
        tags,
        contexts,
        request,
        user,
        event_nav,
        attachments,
        user_reports,
        raw_json,
    };

    render_template(&tmpl)
}

/// Serves an attachment file -- looked up by event_id + filename.
pub async fn download_attachment(
    State(_state): State<AppState>,
    ReadPool(pool): ReadPool,
    Path((_project_id, event_id, filename)): Path<(u64, String, String)>,
) -> axum::response::Response {
    match queries::event_supplements::get_attachment_data(&pool, &event_id, &filename).await {
        Ok(Some((data, content_type))) => {
            let ct = content_type.unwrap_or_else(|| "application/octet-stream".to_string());
            let safe_filename = filename
                .replace('"', "_")
                .replace(['\r', '\n', ';', '=', '\\'], "");
            let disposition = format!("attachment; filename=\"{safe_filename}\"");
            (
                [
                    (axum::http::header::CONTENT_TYPE, ct),
                    (axum::http::header::CONTENT_DISPOSITION, disposition),
                ],
                data,
            )
                .into_response()
        }
        Ok(None) => html_error(StatusCode::NOT_FOUND, "Attachment not found"),
        Err(e) => html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}
