use askama::Template;
use axum::extract::{Form, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect};
use serde::Deserialize;

use crate::event_data::{
    Breadcrumb, ContextGroup, ExceptionData, RequestInfo, SummaryTag, Tag, UserInfo,
};
use crate::extractors::ReadPool;
use crate::html::render_template;
use crate::queries;
use crate::queries::types::{AttachmentInfo, EventNav, Page, PagedResult, TagFacet};
use crate::queries::IssueStatus;
use crate::server::AppState;

use crate::queries::event_supplements;

use super::charts;
use super::html_error;
use super::utils;

// askama needs these filters in scope for template derivation
use crate::html::filters;

#[derive(Deserialize)]
pub struct PageParams {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub tab: Option<String>,
}

#[derive(Deserialize)]
pub struct StatusForm {
    pub status: String,
}

#[derive(Template)]
#[template(path = "issue_detail.html")]
#[allow(dead_code)]
struct IssueDetailTemplate {
    issue: queries::IssueSummary,
    tab: String,
    is_discarded: bool,
    // -- details tab --
    event: Option<queries::EventDetail>,
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
    tag_facets: Vec<TagFacet>,
    // -- events tab --
    events: PagedResult<queries::EventSummary>,
    // -- shared --
    chart_svg: String,
    first_seen_release: Option<String>,
    last_seen_release: Option<String>,
}

pub async fn handler(
    State(_state): State<AppState>,
    ReadPool(pool): ReadPool,
    Path((project_id, fingerprint)): Path<(u64, String)>,
    Query(params): Query<PageParams>,
) -> axum::response::Response {
    let issue = match queries::issues::get_issue(&pool, &fingerprint).await {
        Ok(Some(i)) => i,
        Ok(None) => return html_error(StatusCode::NOT_FOUND, "Issue not found"),
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    if issue.project_id != project_id {
        return html_error(StatusCode::NOT_FOUND, "Issue not found in this project");
    }

    let tab = params.tab.unwrap_or_else(|| "details".to_string());

    let is_discarded = queries::filters::is_fingerprint_discarded(&pool, &fingerprint)
        .await
        .unwrap_or(false);

    let chart_svg = match queries::events::event_histogram(&pool, &fingerprint, 30).await {
        Ok(buckets) => charts::render_event_chart(&buckets).unwrap_or_default(),
        Err(_) => String::new(),
    };

    let tag_facets = queries::events::get_tag_facets(&pool, &fingerprint)
        .await
        .unwrap_or_default();

    let (first_seen_release, last_seen_release) =
        queries::issues::get_issue_release_range(&pool, &fingerprint)
            .await
            .unwrap_or_default();

    if tab == "events" {
        let page = Page::new(params.offset, params.limit);
        let events = match queries::events::list_events_for_issue(&pool, &fingerprint, &page).await
        {
            Ok(r) => r,
            Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        };

        let tmpl = IssueDetailTemplate {
            issue,
            tab,
            is_discarded,
            event: None,
            summary_tags: Vec::new(),
            exceptions: Vec::new(),
            breadcrumbs: Vec::new(),
            tags: Vec::new(),
            contexts: Vec::new(),
            request: None,
            user: UserInfo::default(),
            event_nav: EventNav::default(),
            attachments: Vec::new(),
            user_reports: Vec::new(),
            raw_json: String::new(),
            tag_facets,
            events,
            chart_svg,
            first_seen_release,
            last_seen_release,
        };
        return render_template(&tmpl);
    }

    // Details tab -- show the latest event inline
    let latest = queries::events::get_latest_event_for_issue(&pool, &fingerprint)
        .await
        .ok()
        .flatten();

    let (
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
    ) = if let Some(ref ev) = latest {
        let supplements = event_supplements::get_event_supplements(&pool, ev)
            .await
            .unwrap_or_default();
        let sourcemaps: std::collections::HashMap<String, ::sourcemap::SourceMap> =
            event_supplements::preload_sourcemaps(&pool, &ev.payload).await;
        let resolver =
            move |debug_id: &str, line: u32, col: u32| -> Option<crate::sourcemap::ResolvedFrame> {
                let sm = sourcemaps.get(debug_id)?;
                crate::sourcemap::resolve_frame(sm, line, col)
            };
        let d = event_supplements::get_event_detail_data(ev, supplements, Some(&resolver));
        (
            d.summary_tags,
            d.exceptions,
            d.breadcrumbs,
            d.tags,
            d.contexts,
            d.request,
            d.user,
            d.event_nav,
            d.attachments,
            d.user_reports,
            d.raw_json,
        )
    } else {
        (
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
            UserInfo::default(),
            EventNav::default(),
            Vec::new(),
            Vec::new(),
            String::new(),
        )
    };

    let empty_events = PagedResult {
        items: Vec::new(),
        total: 0,
        offset: 0,
        limit: 25,
    };

    let tmpl = IssueDetailTemplate {
        issue,
        tab,
        is_discarded,
        event: latest,
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
        tag_facets,
        events: empty_events,
        chart_svg,
        first_seen_release,
        last_seen_release,
    };
    render_template(&tmpl)
}

pub async fn toggle_discard(
    State(state): State<AppState>,
    Path((project_id, fingerprint)): Path<(u64, String)>,
) -> axum::response::Response {
    // Figure out if it's currently discarded so we know which way to toggle
    let is_discarded = queries::filters::is_fingerprint_discarded(&state.pool, &fingerprint)
        .await
        .unwrap_or(false);

    let send_result = if is_discarded {
        state.writer.undiscard_fingerprint(fingerprint.clone())
    } else {
        state
            .writer
            .discard_fingerprint(fingerprint.clone(), project_id)
    };

    let result = match utils::await_writer(send_result).await {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    if let Err(err) = result {
        return html_error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string());
    }

    let redirect_url = format!("/web/projects/{project_id}/issues/{fingerprint}/");
    Redirect::to(&redirect_url).into_response()
}

pub async fn update_status(
    State(state): State<AppState>,
    Path((project_id, fingerprint)): Path<(u64, String)>,
    Form(form): Form<StatusForm>,
) -> axum::response::Response {
    let status = match form.status.as_str() {
        "unresolved" => IssueStatus::Unresolved,
        "resolved" => IssueStatus::Resolved,
        "ignored" => IssueStatus::Ignored,
        _ => {
            return html_error(
                StatusCode::BAD_REQUEST,
                &format!("Invalid status '{}'", form.status),
            )
        }
    };

    let result = match utils::await_writer(
        state
            .writer
            .update_issue_status(fingerprint.clone(), status),
    )
    .await
    {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    if let Err(err) = result {
        let status = if err.is_not_found() {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };
        return html_error(status, &err.to_string());
    }

    // 303 back to the issue page so the browser does a GET
    let redirect_url = format!("/web/projects/{project_id}/issues/{fingerprint}/");
    Redirect::to(&redirect_url).into_response()
}
