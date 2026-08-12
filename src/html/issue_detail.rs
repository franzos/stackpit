use askama::Template;
use axum::extract::{Form, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect};
use serde::Deserialize;

use crate::domain::{
    Breadcrumb, ContextGroup, ExceptionData, IssueStatus, RequestInfo, SummaryTag, Tag, UserInfo,
};
// Matched on by name in issue_detail.html's stack-frame loop.
#[allow(unused_imports)]
use crate::domain::FrameGroup;
use crate::extractors::ReadPool;
use crate::html::chrome::PageChrome;
use crate::html::render_template;
use crate::html::utils::Chrome;
use crate::orgs::extractor::ActiveOrg;
use crate::queries;
use crate::queries::types::{AttachmentInfo, EventNav, PagedResult, Pagination, TagFacet};
use crate::server::AppState;
use crate::trackers::{LinkError, LinkRequest};

use crate::queries::event_supplements;

use super::charts;
use super::{html_error, HtmlError};

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Deserialize)]
pub struct PageParams {
    pub tab: Option<String>,
    #[serde(default)]
    pub tracker_flash: Option<String>,
    #[serde(flatten)]
    pub page: Pagination,
}

#[derive(Deserialize)]
pub struct StatusForm {
    pub status: String,
}

#[derive(Deserialize)]
pub struct ExternalIssueForm {
    pub integration_id: i64,
}

#[derive(Template)]
#[template(path = "issue_detail.html")]
#[allow(dead_code)]
struct IssueDetailTemplate {
    issue: queries::IssueSummary,
    nav: queries::ProjectNavCounts,
    tab: String,
    is_discarded: bool,
    // -- details tab --
    event: Option<queries::EventDetail>,
    summary_tags: Vec<SummaryTag>,
    exceptions: Vec<ExceptionData>,
    breadcrumbs: Vec<Breadcrumb>,
    /// Option set for the breadcrumb type filter; derived from `breadcrumbs`.
    breadcrumb_categories: Vec<String>,
    tags: Vec<Tag>,
    contexts: Vec<ContextGroup>,
    request: Option<RequestInfo>,
    user: UserInfo,
    extra: Vec<(String, String)>,
    replay_id: Option<String>,
    trace_id: Option<String>,
    event_nav: EventNav,
    attachments: Vec<AttachmentInfo>,
    user_reports: Vec<queries::UserReportData>,
    raw_json: String,
    tag_facets: Vec<TagFacet>,
    // -- events tab --
    events: PagedResult<queries::EventSummary>,
    // -- shared --
    chart_data: String,
    first_seen_release: Option<String>,
    last_seen_release: Option<String>,
    // -- external trackers --
    available_trackers: Vec<queries::Integration>,
    external_links: Vec<queries::issue_links::ExternalLink>,
    tracker_flash: Option<String>,
    chrome: PageChrome,
}

/// Maps a `tracker_flash` query value to its i18n message. Only known keys
/// resolve, so an attacker can't smuggle arbitrary text into the page.
fn resolve_tracker_flash(value: Option<&str>, chrome: &PageChrome) -> Option<String> {
    match value {
        Some("create-failed") => Some(chrome.t("flash-tracker-create-failed")),
        Some("config") => Some(chrome.t("flash-tracker-config-incomplete")),
        Some("license") => Some(chrome.t("flash-integration-license-required")),
        _ => None,
    }
}

pub async fn handler(
    active: ActiveOrg,
    State(state): State<AppState>,
    ReadPool(pool): ReadPool,
    Chrome(chrome): Chrome,
    Path((project_id, fingerprint)): Path<(u64, String)>,
    Query(params): Query<PageParams>,
) -> Result<axum::response::Response, HtmlError> {
    crate::orgs::extractor::require_project_scope(&active, &pool, project_id as i64)
        .await
        .map_err(|_| HtmlError(StatusCode::NOT_FOUND, "Not found".into()))?;

    let issue = match queries::issues::get_issue(&pool, &fingerprint).await? {
        Some(i) => i,
        None => return Err(HtmlError(StatusCode::NOT_FOUND, "Issue not found".into())),
    };

    if issue.project_id != project_id {
        return Err(HtmlError(
            StatusCode::NOT_FOUND,
            "Issue not found in this project".into(),
        ));
    }

    let tab = params.tab.unwrap_or_else(|| "details".to_string());
    let tracker_flash = resolve_tracker_flash(params.tracker_flash.as_deref(), &chrome);

    let nav = state.nav_counts(project_id).await;

    let is_discarded = queries::filters::is_fingerprint_discarded(&pool, &fingerprint)
        .await
        .unwrap_or(false);

    let chart_data = match queries::events::event_histogram(&pool, &fingerprint, 30).await {
        Ok(buckets) => charts::chart_json(&buckets, "Events"),
        Err(_) => String::new(),
    };

    let tag_facets = queries::events::get_tag_facets(&pool, &fingerprint)
        .await
        .unwrap_or_default();

    let (first_seen_release, last_seen_release) =
        queries::issues::get_issue_release_range(&pool, &fingerprint)
            .await
            .unwrap_or_default();

    let external_links = queries::issue_links::links_for_issue(&pool, &fingerprint)
        .await
        .unwrap_or_default();
    let available_trackers = match queries::orgs::org_of_project(&pool, project_id as i64).await {
        Ok(Some(org_id)) => {
            let linked_ids: std::collections::HashSet<i64> =
                external_links.iter().map(|l| l.integration_id).collect();
            queries::integrations::list_integrations(&pool, Some(org_id))
                .await
                .unwrap_or_default()
                .into_iter()
                .filter(|i| i.kind.is_tracker() && !linked_ids.contains(&i.id))
                .collect()
        }
        _ => Vec::new(),
    };

    if tab == "events" {
        let page = params.page.page();
        let events = queries::events::list_events_for_issue(&pool, &fingerprint, &page).await?;

        let tmpl = IssueDetailTemplate {
            issue,
            nav: nav.clone(),
            tab,
            is_discarded,
            event: None,
            summary_tags: Vec::new(),
            exceptions: Vec::new(),
            breadcrumbs: Vec::new(),
            breadcrumb_categories: Vec::new(),
            tags: Vec::new(),
            contexts: Vec::new(),
            request: None,
            user: UserInfo::default(),
            extra: Vec::new(),
            replay_id: None,
            trace_id: None,
            event_nav: EventNav::default(),
            attachments: Vec::new(),
            user_reports: Vec::new(),
            raw_json: String::new(),
            tag_facets,
            events,
            chart_data,
            first_seen_release,
            last_seen_release,
            available_trackers,
            external_links,
            tracker_flash,
            chrome,
        };
        return Ok(render_template(&tmpl));
    }

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
            event_supplements::preload_sourcemaps(&pool, &ev.payload, ev.project_id).await;
        let resolver = move |debug_id: &str,
                             line: u32,
                             col: u32|
              -> Option<crate::ingest::sourcemap::ResolvedFrame> {
            let sm = sourcemaps.get(debug_id)?;
            crate::ingest::sourcemap::resolve_frame(sm, line, col)
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

    let (extra, replay_id, trace_id) = if let Some(ref ev) = latest {
        (
            crate::ingest::event_data::extract_extra(&ev.payload),
            tags.iter()
                .find(|t| t.key == "replayId")
                .map(|t| t.value.clone()),
            crate::ingest::event_data::extract_trace_id(&ev.payload),
        )
    } else {
        (Vec::new(), None, None)
    };

    let empty_events = PagedResult {
        items: Vec::new(),
        total: 0,
        offset: 0,
        limit: 25,
    };

    let breadcrumb_categories = crate::domain::breadcrumb_categories(&breadcrumbs);

    let tmpl = IssueDetailTemplate {
        issue,
        nav,
        tab,
        is_discarded,
        event: latest,
        summary_tags,
        exceptions,
        breadcrumbs,
        breadcrumb_categories,
        tags,
        contexts,
        request,
        user,
        extra,
        replay_id,
        trace_id,
        event_nav,
        attachments,
        user_reports,
        raw_json,
        tag_facets,
        events: empty_events,
        chart_data,
        first_seen_release,
        last_seen_release,
        available_trackers,
        external_links,
        tracker_flash,
        chrome,
    };
    Ok(render_template(&tmpl))
}

pub async fn toggle_discard(
    active: ActiveOrg,
    State(state): State<AppState>,
    Path((project_id, fingerprint)): Path<(u64, String)>,
) -> axum::response::Response {
    // Bind fingerprint to its owning project, then verify project belongs to active org.
    let fp_project =
        match crate::queries::orgs::project_of_fingerprint(&state.pool, &fingerprint).await {
            Ok(Some(p)) => p,
            Ok(None) => return html_error(StatusCode::NOT_FOUND, "Issue not found"),
            Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        };
    if fp_project != project_id as i64 {
        return html_error(StatusCode::NOT_FOUND, "Issue not found");
    }
    if let Err(r) =
        crate::orgs::extractor::require_project_owner(&active, &state.pool, project_id as i64).await
    {
        return r;
    }

    let is_discarded = queries::filters::is_fingerprint_discarded(&state.pool, &fingerprint)
        .await
        .unwrap_or(false);

    let result = if is_discarded {
        queries::filters::undiscard_fingerprint(&state.writer_pool, &fingerprint).await
    } else {
        queries::filters::discard_fingerprint(&state.writer_pool, &fingerprint, project_id).await
    };
    if let Err(err) = result {
        return html_error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string());
    }
    if is_discarded {
        state
            .filter_engine
            .remove_discarded_fingerprint(&fingerprint);
    } else {
        state.filter_engine.add_discarded_fingerprint(&fingerprint);
    }

    let redirect_url = format!("/web/projects/{project_id}/issues/{fingerprint}/");
    Redirect::to(&redirect_url).into_response()
}

pub async fn update_status(
    active: ActiveOrg,
    State(state): State<AppState>,
    Path((project_id, fingerprint)): Path<(u64, String)>,
    Form(form): Form<StatusForm>,
) -> axum::response::Response {
    // Bind fingerprint to its owning project, then verify project belongs to active org.
    let fp_project =
        match crate::queries::orgs::project_of_fingerprint(&state.pool, &fingerprint).await {
            Ok(Some(p)) => p,
            Ok(None) => return html_error(StatusCode::NOT_FOUND, "Issue not found"),
            Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        };
    if fp_project != project_id as i64 {
        return html_error(StatusCode::NOT_FOUND, "Issue not found");
    }
    if let Err(r) =
        crate::orgs::extractor::require_project_owner(&active, &state.pool, project_id as i64).await
    {
        return r;
    }

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

    match queries::issues::update_issue_status(&state.writer_pool, &fingerprint, status).await {
        Ok(0) => {
            return html_error(
                StatusCode::NOT_FOUND,
                &format!("not found: issue: {fingerprint}"),
            )
        }
        Ok(_) => {}
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }

    let redirect_url = format!("/web/projects/{project_id}/issues/{fingerprint}/");
    Redirect::to(&redirect_url).into_response()
}

pub async fn create_external_issue(
    active: ActiveOrg,
    State(state): State<AppState>,
    Path((project_id, fingerprint)): Path<(u64, String)>,
    Form(form): Form<ExternalIssueForm>,
) -> axum::response::Response {
    // Bind fingerprint to its owning project, then verify project belongs to active org.
    let fp_project =
        match crate::queries::orgs::project_of_fingerprint(&state.pool, &fingerprint).await {
            Ok(Some(p)) => p,
            Ok(None) => return html_error(StatusCode::NOT_FOUND, "Issue not found"),
            Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        };
    if fp_project != project_id as i64 {
        return html_error(StatusCode::NOT_FOUND, "Issue not found");
    }
    let scope = match crate::orgs::extractor::require_project_owner(
        &active,
        &state.pool,
        project_id as i64,
    )
    .await
    {
        Ok(s) => s,
        Err(r) => return r,
    };

    let redirect_url = format!("/web/projects/{project_id}/issues/{fingerprint}/");
    let issue_url = format!("{}{redirect_url}", state.config.server.web_base());

    let outcome = crate::trackers::link_issue(
        &state.pool,
        &state.writer_pool,
        state.encryptor.as_deref(),
        &state.license,
        &LinkRequest {
            org_id: scope.org_id,
            project_id: project_id as i64,
            fingerprint: &fingerprint,
            integration_id: form.integration_id,
            issue_url: &issue_url,
        },
    )
    .await;

    match outcome {
        Ok(_) => Redirect::to(&redirect_url).into_response(),
        Err(LinkError::IssueNotFound) => html_error(StatusCode::NOT_FOUND, "Issue not found"),
        Err(LinkError::IntegrationNotFound) => {
            html_error(StatusCode::NOT_FOUND, "Integration not found")
        }
        Err(LinkError::Misconfigured(msg)) => {
            tracing::warn!("create_external_issue: {msg}");
            Redirect::to(&format!("{redirect_url}?tracker_flash=config")).into_response()
        }
        Err(LinkError::Blocked(msg)) => html_error(StatusCode::BAD_REQUEST, &msg),
        Err(LinkError::LicenseRequired) => {
            Redirect::to(&format!("{redirect_url}?tracker_flash=license")).into_response()
        }
        // Status only in the log: the tracker response body can reflect
        // submitted input, so it never surfaces on the page.
        Err(e @ (LinkError::Rejected(_) | LinkError::Unavailable(_))) => {
            tracing::warn!("create_external_issue: tracker call failed: {e}");
            Redirect::to(&format!("{redirect_url}?tracker_flash=create-failed")).into_response()
        }
        Err(LinkError::Internal(e)) => {
            tracing::error!("create_external_issue: {e:#}");
            html_error(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
        }
    }
}
