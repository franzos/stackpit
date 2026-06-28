use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::domain::*;
use crate::extractors::ReadPool;
use crate::html::render_template;
use crate::html::utils::Csrf;
use crate::queries;
use crate::queries::types::{AttachmentInfo, EventNav};
use crate::queries::{event_supplements, ExtractedEventData};
use crate::server::AppState;

use super::{html_error, HtmlError};

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "event_detail.html")]
struct EventDetailTemplate {
    event: queries::EventDetail,
    nav: queries::ProjectNavCounts,
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
    own_feedback: Option<queries::types::UserFeedback>,
    measurements: Vec<Measurement>,
    raw_json: String,
    csrf_token: String,
}

pub async fn handler(
    State(_state): State<AppState>,
    ReadPool(pool): ReadPool,
    Csrf(csrf): Csrf,
    Path((project_id, event_id)): Path<(u64, String)>,
) -> Result<axum::response::Response, HtmlError> {
    let event = match queries::events::get_event_detail(&pool, &event_id).await? {
        Some(e) => e,
        None => return Err(HtmlError(StatusCode::NOT_FOUND, "Event not found".into())),
    };

    if event.project_id != project_id {
        return Err(HtmlError(
            StatusCode::NOT_FOUND,
            "Event not found in this project".into(),
        ));
    }

    let nav = queries::projects::get_nav_counts(&pool, project_id).await;

    let supplements = event_supplements::get_event_supplements(&pool, &event)
        .await
        .unwrap_or_default();

    let sourcemaps: std::collections::HashMap<String, ::sourcemap::SourceMap> =
        event_supplements::preload_sourcemaps(&pool, &event.payload).await;
    let resolver = move |debug_id: &str,
                         line: u32,
                         col: u32|
          -> Option<crate::ingest::sourcemap::ResolvedFrame> {
        let sm = sourcemaps.get(debug_id)?;
        crate::ingest::sourcemap::resolve_frame(sm, line, col)
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
        own_feedback,
        measurements,
        raw_json,
    } = event_supplements::get_event_detail_data(&event, supplements, Some(&resolver));

    let tmpl = EventDetailTemplate {
        event,
        nav,
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
        own_feedback,
        measurements,
        raw_json,
        csrf_token: csrf,
    };

    Ok(render_template(&tmpl))
}

/// Serves an attachment; forces `application/octet-stream` to avoid stored XSS via attacker-set content_type.
pub async fn download_attachment(
    State(_state): State<AppState>,
    ReadPool(pool): ReadPool,
    Path((project_id, event_id, filename)): Path<(u64, String, String)>,
) -> axum::response::Response {
    let project_id_i64 = match i64::try_from(project_id) {
        Ok(v) => v,
        Err(_) => return html_error(StatusCode::NOT_FOUND, "Attachment not found"),
    };
    match queries::event_supplements::get_attachment_data(
        &pool,
        project_id_i64,
        &event_id,
        &filename,
    )
    .await
    {
        Ok(Some((data, _content_type))) => {
            let safe_filename = filename
                .replace('"', "_")
                .replace(['\r', '\n', ';', '=', '\\', '/'], "");
            let disposition = format!("attachment; filename=\"{safe_filename}\"");
            (
                [
                    (
                        axum::http::header::CONTENT_TYPE,
                        "application/octet-stream".to_string(),
                    ),
                    (axum::http::header::CONTENT_DISPOSITION, disposition),
                ],
                data,
            )
                .into_response()
        }
        Ok(None) => html_error(StatusCode::NOT_FOUND, "Attachment not found"),
        Err(e) => {
            tracing::error!(
                project_id, event_id = %event_id, filename = %filename,
                "attachment lookup failed: {e:#}"
            );
            html_error(StatusCode::INTERNAL_SERVER_ERROR, "Attachment unavailable")
        }
    }
}
