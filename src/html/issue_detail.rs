use askama::Template;
use axum::extract::{Form, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect};
use serde::Deserialize;

use crate::domain::{
    Breadcrumb, ContextGroup, ExceptionData, IssueStatus, RequestInfo, SummaryTag, Tag, UserInfo,
};
use crate::extractors::ReadPool;
use crate::html::chrome::PageChrome;
use crate::html::render_template;
use crate::html::utils::Chrome;
use crate::orgs::extractor::ActiveOrg;
use crate::providers::tracker::{create_issue, issue_api_url, NewExternalIssue, TrackerTarget};
use crate::queries;
use crate::queries::types::{AttachmentInfo, EventNav, PagedResult, Pagination, TagFacet};
use crate::server::AppState;
use crate::util::ssrf::{build_pinned_client, check_ssrf};

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

    let tmpl = IssueDetailTemplate {
        issue,
        nav,
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
        crate::orgs::extractor::require_project_scope(&active, &state.pool, project_id as i64).await
    {
        return r;
    }
    if let Err(r) = crate::orgs::extractor::require_owner(&active) {
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
        crate::orgs::extractor::require_project_scope(&active, &state.pool, project_id as i64).await
    {
        return r;
    }
    if let Err(r) = crate::orgs::extractor::require_owner(&active) {
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

/// Resolves the tracker target: each field (owner, repo, project_id) is taken
/// from the per-project override if present there, otherwise falls back to the
/// integration's default target; base_url always comes from `integrations.url`,
/// not from either target object.
pub fn resolve_target(
    base_url: &str,
    default_target: &serde_json::Value,
    project_override: Option<&serde_json::Value>,
) -> TrackerTarget {
    let field_str = |key: &str| {
        project_override
            .and_then(|o| o.get(key))
            .and_then(|v| v.as_str())
            .or_else(|| default_target.get(key).and_then(|v| v.as_str()))
            .map(String::from)
    };
    let project_id = project_override
        .and_then(|o| o.get("project_id"))
        .and_then(|v| v.as_i64())
        .or_else(|| default_target.get("project_id").and_then(|v| v.as_i64()));
    TrackerTarget {
        base_url: base_url.to_string(),
        owner: field_str("owner"),
        repo: field_str("repo"),
        project_id,
    }
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
    if let Err(r) =
        crate::orgs::extractor::require_project_scope(&active, &state.pool, project_id as i64).await
    {
        return r;
    }
    if let Err(r) = crate::orgs::extractor::require_owner(&active) {
        return r;
    }

    let redirect_url = format!("/web/projects/{project_id}/issues/{fingerprint}/");

    // Derive the acting org from the project itself, not the session: for a
    // superuser, require_project_scope above bypasses without checking that
    // active.org_id even matches this project, so it can't be trusted here.
    let org_id = match queries::orgs::org_of_project(&state.pool, project_id as i64).await {
        Ok(Some(o)) => o,
        Ok(None) => return html_error(StatusCode::NOT_FOUND, "Issue not found"),
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let integration = match queries::integrations::get_integration(
        &state.pool,
        form.integration_id,
        Some(org_id),
    )
    .await
    {
        Ok(Some(i)) if i.kind.is_tracker() => i,
        Ok(_) => return html_error(StatusCode::NOT_FOUND, "Integration not found"),
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    match queries::issue_links::link_exists(&state.pool, &fingerprint, integration.id).await {
        Ok(true) => return Redirect::to(&redirect_url).into_response(),
        Ok(false) => {}
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }

    let token = match (&integration.secret, integration.encrypted, &state.encryptor) {
        (Some(s), true, Some(enc)) => enc.decrypt(s),
        (Some(s), false, _) => Some(s.clone()),
        _ => None,
    };
    let Some(token) = token else {
        return Redirect::to(&format!("{redirect_url}?tracker_flash=config")).into_response();
    };

    let Some(base_url) = integration.url.as_deref() else {
        return Redirect::to(&format!("{redirect_url}?tracker_flash=config")).into_response();
    };

    let default_target: serde_json::Value = integration
        .config
        .as_deref()
        .and_then(|c| serde_json::from_str(c).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    let project_override = match queries::tracker_targets::get_override(
        &state.pool,
        project_id as i64,
        integration.id,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let target = resolve_target(base_url, &default_target, project_override.as_ref());
    let kind = integration.kind;

    let url = match issue_api_url(kind, &target) {
        Ok(u) => u,
        Err(_) => {
            return Redirect::to(&format!("{redirect_url}?tracker_flash=config")).into_response()
        }
    };

    let resolved = match check_ssrf(&url).await {
        Ok(r) => r,
        Err(msg) => return html_error(StatusCode::BAD_REQUEST, &msg),
    };

    let client = match build_pinned_client(&resolved) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("create_external_issue: failed to build pinned client: {e}");
            return html_error(StatusCode::INTERNAL_SERVER_ERROR, "internal error");
        }
    };

    let issue = match queries::issues::get_issue(&state.pool, &fingerprint).await {
        Ok(Some(i)) => i,
        Ok(None) => return html_error(StatusCode::NOT_FOUND, "Issue not found"),
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    let title = issue.title.unwrap_or_else(|| fingerprint.clone());
    let deep_link = format!("{}{redirect_url}", state.config.server.web_base());
    let body = format!("{title}\n\n{deep_link}");

    // Status only in the flash/log below: the tracker response body can
    // reflect submitted input, so it never surfaces past this point.
    let created = match create_issue(
        &client,
        kind,
        &target,
        &token,
        &NewExternalIssue {
            title: &title,
            body: &body,
        },
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("create_external_issue: tracker create_issue failed: {e}");
            return Redirect::to(&format!("{redirect_url}?tracker_flash=create-failed"))
                .into_response();
        }
    };

    let now = chrono::Utc::now().timestamp();
    if let Err(e) = queries::issue_links::insert_link(
        &state.writer_pool,
        project_id as i64,
        &fingerprint,
        integration.id,
        &created.external_id,
        &created.external_url,
        now,
    )
    .await
    {
        // A concurrent request may have already won the insert; that's fine.
        tracing::warn!("create_external_issue: failed to store link: {e}");
    }

    Redirect::to(&redirect_url).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_target_prefers_project_override() {
        let default_cfg = serde_json::json!({ "owner": "acme", "repo": "default" });
        let override_tgt = serde_json::json!({ "owner": "acme", "repo": "frontend" });
        let t = resolve_target("https://api.github.com", &default_cfg, Some(&override_tgt));
        assert_eq!(t.repo.as_deref(), Some("frontend"));
        assert_eq!(t.base_url, "https://api.github.com");

        let t2 = resolve_target("https://api.github.com", &default_cfg, None);
        assert_eq!(t2.repo.as_deref(), Some("default"));
    }

    #[test]
    fn resolve_target_merges_partial_override_with_default() {
        let default_cfg = serde_json::json!({ "owner": "acme", "repo": "default" });
        let repo_only_override = serde_json::json!({ "repo": "frontend" });
        let t = resolve_target(
            "https://api.github.com",
            &default_cfg,
            Some(&repo_only_override),
        );
        assert_eq!(t.owner.as_deref(), Some("acme"));
        assert_eq!(t.repo.as_deref(), Some("frontend"));
    }

    #[test]
    fn resolve_target_empty_override_yields_full_default() {
        let default_cfg = serde_json::json!({ "owner": "acme", "repo": "default" });
        let empty_override = serde_json::json!({});
        let t = resolve_target(
            "https://api.github.com",
            &default_cfg,
            Some(&empty_override),
        );
        assert_eq!(t.owner.as_deref(), Some("acme"));
        assert_eq!(t.repo.as_deref(), Some("default"));
    }

    #[test]
    fn resolve_target_full_override_wins() {
        let default_cfg = serde_json::json!({ "owner": "acme", "repo": "default" });
        let full_override = serde_json::json!({ "owner": "other", "repo": "frontend" });
        let t = resolve_target("https://api.github.com", &default_cfg, Some(&full_override));
        assert_eq!(t.owner.as_deref(), Some("other"));
        assert_eq!(t.repo.as_deref(), Some("frontend"));
    }
}
