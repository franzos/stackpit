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
use crate::html::flash;
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
    /// `crumbs=all` lifts the render cap for one page load.
    #[serde(default)]
    pub crumbs: Option<String>,
    #[serde(flatten)]
    pub page: Pagination,
}

/// How many breadcrumbs the panel renders by default. The stored payload keeps
/// every crumb — an SDK that sent 400 for a hard-to-reproduce crash sent them
/// for a reason — so this bounds the page, not the data: the Raw JSON tab is
/// still complete and "Show all" renders the rest.
const BREADCRUMB_RENDER_CAP: usize = 100;

#[derive(Deserialize)]
pub struct StatusForm {
    pub status: String,
}

#[derive(Deserialize)]
pub struct ExternalIssueForm {
    /// `"<integration_id>:<repo_id>"` from the picker.
    pub target: String,
}

impl ExternalIssueForm {
    fn parse(&self) -> Option<(i64, i64)> {
        let (integration, repo) = self.target.split_once(':')?;
        Some((integration.trim().parse().ok()?, repo.trim().parse().ok()?))
    }
}

/// One selectable filing target on the issue page.
pub struct TrackerOption {
    pub value: String,
    pub label: String,
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
    /// How many the event actually carries, which is what the header shows.
    breadcrumb_total: usize,
    /// True when `breadcrumbs` is the capped tail rather than all of them.
    crumbs_truncated: bool,
    /// Option set for the breadcrumb type filter; derived from the *full* set,
    /// so capping the render never silently drops a filter option.
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
    tracker_options: Vec<TrackerOption>,
    external_links: Vec<queries::issue_links::ExternalLink>,
    chrome: PageChrome,
}

/// Builds the file-into picker, naming the repo only when an integration
/// matches several.
///
/// Filtering is per `(integration, repository)`, not per integration: filing
/// into one repository of a forge must not make its sibling unfileable, which
/// is what the picker used to do — it offered two targets and silently lost
/// both the moment you used either.
///
/// A link whose repository cannot be parsed filters nothing out. Offering a
/// possibly-duplicate target beats hiding the only reachable one; a duplicate
/// issue can be closed on the forge.
fn tracker_options(
    matches: &[crate::trackers::TrackerMatch],
    already_linked: &std::collections::HashSet<(i64, String)>,
) -> Vec<TrackerOption> {
    matches
        .iter()
        .filter(|m| match crate::trackers::repo_key(&m.forge_ref) {
            Some(key) => !already_linked.contains(&(m.integration.id, key)),
            None => true,
        })
        .map(|m| {
            let ambiguous = matches
                .iter()
                .filter(|o| o.integration.id == m.integration.id)
                .count()
                > 1;
            let label = if ambiguous {
                format!(
                    "{} — {}",
                    m.integration.name,
                    crate::forge::repo_path(&m.repo.repo_url)
                )
            } else {
                m.integration.name.clone()
            };
            TrackerOption {
                value: format!("{}:{}", m.integration.id, m.repo.id),
                label,
            }
        })
        .collect()
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

    let external_links =
        queries::issue_links::links_for_issue(&pool, project_id as i64, &fingerprint)
            .await
            .unwrap_or_default();
    let tracker_options = match queries::orgs::org_of_project(&pool, project_id as i64).await {
        Ok(Some(org_id)) => {
            let linked_ids: std::collections::HashSet<(i64, String)> = external_links
                .iter()
                .filter_map(|l| Some((l.integration_id?, l.repo_key()?)))
                .collect();
            let matches =
                crate::trackers::resolve_matching_trackers(&pool, org_id, project_id as i64)
                    .await
                    .unwrap_or_default();
            tracker_options(&matches, &linked_ids)
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
            breadcrumb_total: 0,
            crumbs_truncated: false,
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
            tracker_options,
            external_links,
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
        mut breadcrumbs,
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

    // Computed before slicing: the filter must offer every category the event
    // carries, not only those surviving the cap.
    let breadcrumb_categories = crate::domain::breadcrumb_categories(&breadcrumbs);
    let breadcrumb_total = breadcrumbs.len();
    let show_all = params.crumbs.as_deref() == Some("all");
    let crumbs_truncated = !show_all && breadcrumb_total > BREADCRUMB_RENDER_CAP;
    // SDKs send oldest-first, so the useful end is the tail.
    if crumbs_truncated {
        breadcrumbs.drain(..breadcrumb_total - BREADCRUMB_RENDER_CAP);
    }

    let tmpl = IssueDetailTemplate {
        issue,
        nav,
        tab,
        is_discarded,
        event: latest,
        summary_tags,
        exceptions,
        breadcrumbs,
        breadcrumb_total,
        crumbs_truncated,
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
        tracker_options,
        external_links,
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

    let Some((integration_id, repo_id)) = form.parse() else {
        return html_error(StatusCode::BAD_REQUEST, "Invalid target");
    };

    let outcome = crate::trackers::link_issue(
        &state.pool,
        &state.writer_pool,
        state.encryptor.as_deref(),
        &state.license,
        &LinkRequest {
            org_id: scope.org_id,
            project_id: project_id as i64,
            fingerprint: &fingerprint,
            integration_id,
            repo_id: Some(repo_id),
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
            flash::redirect(&redirect_url, flash::TRACKER_CONFIG_INCOMPLETE)
        }
        Err(LinkError::Ambiguous(msg)) => {
            tracing::warn!("create_external_issue: {msg}");
            flash::redirect(&redirect_url, flash::TRACKER_AMBIGUOUS)
        }
        Err(LinkError::Blocked(msg)) => html_error(StatusCode::BAD_REQUEST, &msg),
        Err(LinkError::LicenseRequired) => {
            flash::redirect(&redirect_url, flash::INTEGRATION_LICENSE_REQUIRED)
        }
        // Status only in the log: the tracker response body can reflect
        // submitted input, so it never surfaces on the page.
        Err(e @ (LinkError::Rejected(_) | LinkError::Unavailable(_))) => {
            tracing::warn!("create_external_issue: tracker call failed: {e}");
            flash::redirect(&redirect_url, flash::TRACKER_CREATE_FAILED)
        }
        Err(LinkError::Internal(e)) => {
            tracing::error!("create_external_issue: {e:#}");
            html_error(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
        }
    }
}

/// Removes a filed link locally - the issue stays on the forge, since remote delete isn't portable.
pub async fn delete_external_link(
    active: ActiveOrg,
    State(state): State<AppState>,
    Path((project_id, fingerprint, link_id)): Path<(u64, String, i64)>,
) -> axum::response::Response {
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

    let redirect_url = format!("/web/projects/{project_id}/issues/{fingerprint}/");
    match crate::queries::issue_links::delete_link(&state.writer_pool, project_id as i64, link_id)
        .await
    {
        Ok(0) => html_error(StatusCode::NOT_FOUND, "Link not found"),
        Ok(_) => flash::redirect(&redirect_url, flash::TRACKER_UNLINKED),
        Err(e) => {
            tracing::error!("delete_external_link: {e:#}");
            html_error(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use askama::Template;
    use unic_langid::langid;

    fn link(id: i64, integration_id: Option<i64>) -> queries::issue_links::ExternalLink {
        queries::issue_links::ExternalLink {
            id,
            integration_id,
            integration_kind: "github".into(),
            integration_name: "ops-gh".into(),
            external_id: "7".into(),
            external_url: "https://git.test/acme/backend/issues/7".into(),
            external_state: Some("open".into()),
        }
    }

    fn page(
        chrome: PageChrome,
        external_links: Vec<queries::issue_links::ExternalLink>,
    ) -> IssueDetailTemplate {
        IssueDetailTemplate {
            issue: queries::IssueSummary {
                fingerprint: "fp-render".into(),
                project_id: 7,
                title: Some("Boom".into()),
                level: Some("error".into()),
                first_seen: 1_700_000_000,
                last_seen: 1_700_000_100,
                event_count: 3,
                status: Default::default(),
                item_type: crate::ingest::models::ItemType::Event,
                user_count: 1,
            },
            nav: Default::default(),
            tab: "details".into(),
            is_discarded: false,
            event: None,
            summary_tags: Vec::new(),
            exceptions: Vec::new(),
            breadcrumbs: Vec::new(),
            breadcrumb_total: 0,
            crumbs_truncated: false,
            breadcrumb_categories: Vec::new(),
            tags: Vec::new(),
            contexts: Vec::new(),
            request: None,
            user: Default::default(),
            extra: Vec::new(),
            replay_id: None,
            trace_id: None,
            event_nav: Default::default(),
            attachments: Vec::new(),
            user_reports: Vec::new(),
            raw_json: "{}".into(),
            tag_facets: Vec::new(),
            events: queries::PagedResult {
                items: Vec::new(),
                total: 0,
                offset: 0,
                limit: 50,
            },
            chart_data: String::new(),
            first_seen_release: None,
            last_seen_release: None,
            tracker_options: Vec::new(),
            external_links,
            chrome,
        }
    }

    #[test]
    fn a_dangling_link_still_renders_with_an_unlink_control() {
        for locale in [langid!("en"), langid!("de")] {
            let chrome = PageChrome::new("csrf".into(), locale.clone(), "/web/projects/".into());
            let html = page(chrome, vec![link(11, None), link(12, Some(3))])
                .render()
                .expect("issue detail renders");

            assert!(
                !html.contains("Unknown localization"),
                "missing Fluent key in {locale}"
            );
            assert!(html.contains("https://git.test/acme/backend/issues/7"));
            assert!(html.contains("ops-gh"));
            assert!(html.contains("/issues/fp-render/external/11/delete"));
            assert!(html.contains("/issues/fp-render/external/12/delete"));
        }
    }

    /// The breadcrumb panel lives inside the details tab's `event` arm, so a
    /// page that renders crumbs needs one.
    fn an_event() -> queries::EventDetail {
        queries::EventDetail {
            event_id: "e1".into(),
            item_type: crate::ingest::models::ItemType::Event,
            project_id: 7,
            fingerprint: Some("fp-render".into()),
            timestamp: 1_700_000_100,
            level: Some("error".into()),
            title: Some("Boom".into()),
            platform: None,
            release: None,
            environment: None,
            server_name: None,
            transaction_name: None,
            sdk_name: None,
            sdk_version: None,
            received_at: 1_700_000_100,
            payload: serde_json::json!({}),
        }
    }

    fn crumbs(n: usize) -> Vec<crate::domain::Breadcrumb> {
        (0..n)
            .map(|i| crate::domain::Breadcrumb {
                timestamp: format!("2026-08-28T00:00:{:02}Z", i % 60),
                level: "info".into(),
                // Two categories, one of which only ever appears at the head, so
                // a filter option built after slicing would lose it.
                category: if i == 0 { "boot" } else { "http" }.into(),
                message: format!("crumb {i}"),
                data: String::new(),
            })
            .collect()
    }

    /// The stored payload keeps every crumb; only the render is bounded. The
    /// header still reports the true count and the filter still offers every
    /// category, including one that exists only in the truncated head.
    #[test]
    fn breadcrumbs_are_capped_at_render_without_losing_the_count_or_the_filter() {
        let all = crumbs(5_001);
        let categories = crate::domain::breadcrumb_categories(&all);
        let chrome = PageChrome::new("csrf".into(), langid!("en"), "/web/projects/".into());

        let mut capped = page(chrome.clone(), Vec::new());
        capped.event = Some(an_event());
        capped.breadcrumb_total = all.len();
        capped.crumbs_truncated = true;
        capped.breadcrumbs = all[all.len() - BREADCRUMB_RENDER_CAP..].to_vec();
        capped.breadcrumb_categories = categories.clone();
        let html = capped.render().expect("capped page renders");

        assert_eq!(
            html.matches("data-category=").count(),
            BREADCRUMB_RENDER_CAP
        );
        assert!(html.contains("(5001)"), "header shows the true count");
        assert!(html.contains("crumbs=all"), "an escape hatch is offered");
        assert!(html.contains("crumb 5000"), "the tail is what survives");
        assert!(!html.contains(">crumb 0<"), "the head is what is dropped");
        assert!(
            html.contains("value=\"boot\""),
            "a category only present in the dropped head must still be filterable"
        );

        let mut full = page(chrome, Vec::new());
        full.event = Some(an_event());
        full.breadcrumb_total = all.len();
        full.crumbs_truncated = false;
        full.breadcrumbs = all;
        full.breadcrumb_categories = categories;
        let html = full.render().expect("uncapped page renders");
        assert_eq!(html.matches("data-category=").count(), 5_001);
        assert!(!html.contains("crumbs=all"));
    }

    use crate::orgs::Role;
    use crate::server::AppState;

    /// Org 1 is the system org, which `require_project_scope` refuses.
    const ORG: i64 = 5;

    async fn seeded_project(pool: &crate::db::DbPool) {
        sqlx::query(crate::db::sql!(
            "INSERT INTO organizations (org_id, slug, name) VALUES (5, 'acme', 'Acme')"
        ))
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(crate::db::sql!(
            "INSERT INTO projects (project_id, status, source, org_id) \
             VALUES (7, 'active', 'manual', 5)"
        ))
        .execute(pool)
        .await
        .unwrap();
        crate::queries::test_helpers::insert_test_issue(
            pool,
            "fp-h",
            7,
            Some("boom"),
            Some("error"),
            1_000,
            2_000,
            1,
            "unresolved",
        )
        .await;
    }

    fn owner() -> ActiveOrg {
        ActiveOrg {
            session_org_id: ORG,
            role: Some(Role::Owner),
            org_name: None,
            memberships: vec![(ORG, Role::Owner)],
        }
    }

    async fn tracker_integration(pool: &crate::db::DbPool) -> i64 {
        crate::queries::integrations::create_integration(
            pool,
            ORG,
            "gh-handler",
            "github",
            Some("https://api.github.com"),
            Some("tok"),
            None,
            false,
            false,
        )
        .await
        .unwrap()
    }

    async fn add_repo(pool: &crate::db::DbPool, url: &str) -> i64 {
        crate::queries::projects::upsert_project_repo(pool, 7, url, "github", None, None, None)
            .await
            .unwrap();
        crate::queries::projects::get_project_repos(pool, 7)
            .await
            .unwrap()
            .into_iter()
            .find(|r| r.repo_url == url)
            .expect("just inserted")
            .id
    }

    #[tokio::test]
    async fn filing_is_refused_for_a_member_and_for_a_foreign_project() {
        let pool = crate::db::open_test_pool().await;
        seeded_project(&pool).await;
        let integration_id = tracker_integration(&pool).await;
        let repo_id = add_repo(&pool, "https://github.com/acme/api").await;
        let (state, _chans) = AppState::for_test(pool.clone());
        let target = format!("{integration_id}:{repo_id}");

        let member = ActiveOrg {
            session_org_id: ORG,
            role: Some(Role::Member),
            org_name: None,
            memberships: vec![(ORG, Role::Member)],
        };
        let resp = create_external_issue(
            member,
            State(state.clone()),
            Path((7, "fp-h".to_string())),
            Form(ExternalIssueForm {
                target: target.clone(),
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        let outsider = ActiveOrg {
            session_org_id: 9,
            role: Some(Role::Owner),
            org_name: None,
            memberships: vec![(9, Role::Owner)],
        };
        let resp = create_external_issue(
            outsider,
            State(state),
            Path((7, "fp-h".to_string())),
            Form(ExternalIssueForm { target }),
        )
        .await;
        assert_ne!(resp.status(), StatusCode::SEE_OTHER);
    }

    #[tokio::test]
    async fn filing_without_any_repo_flashes_config() {
        let pool = crate::db::open_test_pool().await;
        seeded_project(&pool).await;
        let integration_id = tracker_integration(&pool).await;
        let (state, _chans) = AppState::for_test(pool.clone());

        let resp = create_external_issue(
            owner(),
            State(state),
            Path((7, "fp-h".to_string())),
            Form(ExternalIssueForm {
                target: format!("{integration_id}:1"),
            }),
        )
        .await;

        let location = resp
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(
            location.contains("flash=tracker-config-incomplete"),
            "{location}"
        );
    }

    #[tokio::test]
    async fn a_malformed_target_is_rejected() {
        let pool = crate::db::open_test_pool().await;
        seeded_project(&pool).await;
        let (state, _chans) = AppState::for_test(pool.clone());

        for bad in ["", "7", "a:b", "7:", ":3", "7:3:9"] {
            let resp = create_external_issue(
                owner(),
                State(state.clone()),
                Path((7, "fp-h".to_string())),
                Form(ExternalIssueForm {
                    target: bad.to_string(),
                }),
            )
            .await;
            assert_eq!(
                resp.status(),
                StatusCode::BAD_REQUEST,
                "target {bad:?} should be refused"
            );
        }
    }

    #[tokio::test]
    async fn unlinking_requires_owner_and_removes_only_that_link() {
        let pool = crate::db::open_test_pool().await;
        seeded_project(&pool).await;
        let integration_id = tracker_integration(&pool).await;
        crate::queries::issue_links::insert_link(
            &pool,
            7,
            "fp-h",
            integration_id,
            "gh-handler",
            "github",
            "3",
            "https://github.com/acme/api/issues/3",
            Some("open"),
            1_700_000_000,
        )
        .await
        .unwrap();
        let link_id = crate::queries::issue_links::links_for_issue(&pool, 7, "fp-h")
            .await
            .unwrap()[0]
            .id;
        let (state, _chans) = AppState::for_test(pool.clone());

        let member = ActiveOrg {
            session_org_id: ORG,
            role: Some(Role::Member),
            org_name: None,
            memberships: vec![(ORG, Role::Member)],
        };
        let resp = delete_external_link(
            member,
            State(state.clone()),
            Path((7, "fp-h".to_string(), link_id)),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            crate::queries::issue_links::links_for_issue(&pool, 7, "fp-h")
                .await
                .unwrap()
                .len(),
            1,
            "a member must not be able to unlink"
        );

        let resp = delete_external_link(
            owner(),
            State(state.clone()),
            Path((7, "fp-h".to_string(), link_id)),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        assert!(
            crate::queries::issue_links::links_for_issue(&pool, 7, "fp-h")
                .await
                .unwrap()
                .is_empty()
        );

        let resp = delete_external_link(
            owner(),
            State(state),
            Path((7, "fp-h".to_string(), link_id)),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn the_picker_names_the_repo_only_when_it_has_to() {
        let pool = crate::db::open_test_pool().await;
        seeded_project(&pool).await;
        let integration_id = tracker_integration(&pool).await;
        add_repo(&pool, "https://github.com/acme/api").await;

        let matches = crate::trackers::resolve_matching_trackers(&pool, ORG, 7)
            .await
            .unwrap();
        let opts = tracker_options(&matches, &Default::default());
        assert_eq!(opts.len(), 1);
        assert_eq!(opts[0].label, "gh-handler");

        add_repo(&pool, "https://github.com/acme/web").await;
        let matches = crate::trackers::resolve_matching_trackers(&pool, ORG, 7)
            .await
            .unwrap();
        let opts = tracker_options(&matches, &Default::default());
        assert_eq!(opts.len(), 2);
        assert!(
            opts.iter().any(|o| o.label.contains("acme/api")),
            "{opts:?}",
            opts = opts.iter().map(|o| &o.label).collect::<Vec<_>>()
        );
        assert!(opts.iter().any(|o| o.label.contains("acme/web")));
        assert!(opts
            .iter()
            .all(|o| o.value.starts_with(&format!("{integration_id}:"))));
    }

    /// Filing into one repository must not make its sibling unfileable. The
    /// old behaviour keyed on the integration and hid both.
    #[tokio::test]
    async fn a_linked_repository_is_dropped_but_its_sibling_is_still_offered() {
        let pool = crate::db::open_test_pool().await;
        seeded_project(&pool).await;
        let integration_id = tracker_integration(&pool).await;
        add_repo(&pool, "https://github.com/acme/api").await;
        add_repo(&pool, "https://github.com/acme/web").await;

        let matches = crate::trackers::resolve_matching_trackers(&pool, ORG, 7)
            .await
            .unwrap();
        assert_eq!(tracker_options(&matches, &Default::default()).len(), 2);

        let linked = std::collections::HashSet::from([(integration_id, "acme/api".to_string())]);
        let opts = tracker_options(&matches, &linked);
        assert_eq!(opts.len(), 1, "the sibling repository stays fileable");
        assert!(opts[0].label.contains("acme/web"));

        // Both filed: now there is nothing left to offer.
        let both = std::collections::HashSet::from([
            (integration_id, "acme/api".to_string()),
            (integration_id, "acme/web".to_string()),
        ]);
        assert!(tracker_options(&matches, &both).is_empty());
    }

    /// A link whose repository cannot be recovered must not hide a target.
    #[tokio::test]
    async fn an_unparseable_link_filters_nothing_out() {
        let pool = crate::db::open_test_pool().await;
        seeded_project(&pool).await;
        let integration_id = tracker_integration(&pool).await;
        add_repo(&pool, "https://github.com/acme/api").await;

        let matches = crate::trackers::resolve_matching_trackers(&pool, ORG, 7)
            .await
            .unwrap();
        let unparseable = queries::issue_links::ExternalLink {
            id: 1,
            integration_id: Some(integration_id),
            integration_kind: "github".into(),
            integration_name: "gh-handler".into(),
            external_id: "7".into(),
            external_url: "not a url at all".into(),
            external_state: None,
        };
        assert!(unparseable.repo_key().is_none());

        // The handler builds the set with `filter_map`, so a `None` key never
        // enters it and the target stays offered.
        let linked: std::collections::HashSet<(i64, String)> = [unparseable]
            .iter()
            .filter_map(|l| Some((l.integration_id?, l.repo_key()?)))
            .collect();
        assert_eq!(tracker_options(&matches, &linked).len(), 1);
    }

    #[test]
    fn only_a_dangling_link_is_marked_as_orphaned() {
        let chrome = PageChrome::new("csrf".into(), langid!("en"), "/web/projects/".into());
        let orphaned = page(chrome.clone(), vec![link(11, None)]).render().unwrap();
        let live = page(chrome, vec![link(12, Some(3))]).render().unwrap();

        assert!(orphaned.contains("integration removed"));
        assert!(!live.contains("integration removed"));
    }
}
