use askama::Template;
use axum::extract::{Query, State};

use crate::extractors::ReadPool;
use crate::html::chrome::PageChrome;
use crate::html::render_template;
use crate::html::utils::{build_filter_qs, period_to_timestamp, Chrome, ListParams};
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
    let project_nav = match params.project_id {
        Some(pid)
            if crate::orgs::extractor::require_project_scope(&active, &pool, pid as i64)
                .await
                .is_ok() =>
        {
            Some(queries::projects::nav_counts_cached(&pool, &state.nav_cache, pid).await)
        }
        _ => None,
    };

    let adoption_since = period_to_timestamp(&period_str);

    let filter = ReleaseFilter {
        project_id: params.project_id,
        query: params.query.filter(|s| !s.is_empty()),
        sort: params.sort.filter(|s| !s.is_empty()),
    };
    let page = params.page.page();
    let org_id = if active.role.is_none() {
        None
    } else {
        Some(active.org_id)
    };

    let result =
        queries::releases::list_all_releases(&pool, &filter, &page, adoption_since, org_id).await?;

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
