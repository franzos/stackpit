use askama::Template;
use axum::extract::{Query, State};

use crate::extractors::ReadPool;
use crate::html::chrome::PageChrome;
use crate::html::render_template;
use crate::html::utils::{
    build_filter_qs, cross_org_scope, period_to_timestamp, Chrome, CrossOrgScope, ListParams,
};
use crate::orgs::extractor::ActiveOrg;
use crate::queries;
use crate::queries::types::{PagedResult, ReleaseFilter};
use crate::queries::ProjectNavCounts;
use crate::server::AppState;

use super::HtmlError;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "release_list.html")]
struct ReleaseListTemplate {
    result: PagedResult<queries::ReleaseSummary>,
    query: String,
    project_id: String,
    sort: String,
    period: String,
    filter_qs: String,
    base_qs: String,
    // When the page is scoped to one project (reached from its sidebar), carry
    // its nav so we can keep the project rail instead of dropping to the global one.
    project_nav: Option<ProjectNavCounts>,
    project_id_num: u64,
    chrome: PageChrome,
}

pub async fn handler(
    State(state): State<AppState>,
    ReadPool(pool): ReadPool,
    Chrome(chrome): Chrome,
    Query(params): Query<ListParams>,
    active: ActiveOrg,
) -> Result<axum::response::Response, HtmlError> {
    let query_str = params.query.clone().unwrap_or_default();
    let project_id_str = params.project_id.map(|p| p.to_string()).unwrap_or_default();
    let sort_str = params.sort.clone().unwrap_or_default();
    let period_str = params.period.clone().unwrap_or_else(|| "7d".to_string());

    // Keep the project sidebar when scoped to a project the caller can access.
    let project_scope = match params.project_id {
        Some(pid) => crate::orgs::extractor::require_project_scope(&active, &pool, pid as i64)
            .await
            .ok(),
        None => None,
    };
    let project_nav = match (params.project_id, &project_scope) {
        (Some(pid), Some(_)) => {
            Some(queries::projects::nav_counts_cached(&pool, &state.nav_cache, pid).await)
        }
        _ => None,
    };

    // The canonical option set added "All time" here. `list_all_releases` reads a
    // `None` window as its own 24h default, so an explicit blank has to bind 0
    // rather than fall through to that.
    let adoption_since = if period_str.is_empty() {
        Some(0)
    } else {
        period_to_timestamp(&period_str)
    };

    let filter = ReleaseFilter {
        project_id: params.project_id,
        query: params.query.filter(|s| !s.is_empty()),
        sort: params.sort.filter(|s| !s.is_empty()),
    };
    let page = params.page.page();
    // Scoped to a project, follow that project's org, not the session's. Unscoped,
    // this used to fall back to the session's own org, which owns no projects for a
    // user sitting in their personal org — so the whole cross-project view was empty.
    let result = match cross_org_scope(&active, project_scope.as_ref()) {
        CrossOrgScope::All => {
            queries::releases::list_all_releases(&pool, &filter, &page, adoption_since, None)
                .await?
        }
        CrossOrgScope::Project(org_id) => {
            queries::releases::list_all_releases(
                &pool,
                &filter,
                &page,
                adoption_since,
                Some(org_id),
            )
            .await?
        }
        CrossOrgScope::Memberships(org_ids) => {
            queries::releases::list_all_releases_for_orgs(
                &pool,
                &filter,
                &page,
                adoption_since,
                org_ids,
            )
            .await?
        }
    };

    let (base_qs, filter_qs) = build_filter_qs(
        &[
            ("query", &query_str),
            ("project_id", &project_id_str),
            ("period", &period_str),
        ],
        &sort_str,
    );

    let tmpl = ReleaseListTemplate {
        result,
        query: query_str,
        project_id: project_id_str,
        sort: sort_str,
        period: period_str,
        filter_qs,
        base_qs,
        project_nav,
        project_id_num: params.project_id.unwrap_or(0),
        chrome,
    };

    Ok(render_template(&tmpl))
}
