use askama::Template;
use axum::extract::{Query, State};
use axum::http::StatusCode;

use crate::extractors::ReadPool;
use crate::html::render_template;
use crate::html::utils::{build_filter_qs, period_to_timestamp, ListParams};
use crate::queries;
use crate::queries::types::{Page, PagedResult, ReleaseFilter};
use crate::server::AppState;

use super::html_error;

// askama needs these filters in scope for template derivation
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
}

pub async fn handler(
    State(_state): State<AppState>,
    ReadPool(pool): ReadPool,
    Query(params): Query<ListParams>,
) -> axum::response::Response {
    let query_str = params.query.clone().unwrap_or_default();
    let project_id_str = params.project_id.map(|p| p.to_string()).unwrap_or_default();
    let sort_str = params.sort.clone().unwrap_or_default();
    let period_str = params.period.clone().unwrap_or_else(|| "24h".to_string());

    let adoption_since = period_to_timestamp(&period_str);

    let filter = ReleaseFilter {
        project_id: params.project_id,
        query: params.query.filter(|s| !s.is_empty()),
        sort: params.sort.filter(|s| !s.is_empty()),
    };
    let page = Page::new(params.offset, params.limit);

    let result =
        match queries::releases::list_all_releases(&pool, &filter, &page, adoption_since).await {
            Ok(r) => r,
            Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
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
    };

    render_template(&tmpl)
}
