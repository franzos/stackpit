use askama::Template;
use axum::extract::{Query, State};
use axum::http::StatusCode;

use crate::extractors::ReadPool;
use crate::html::render_template;
use crate::html::utils::{period_to_timestamp, ListParams};
use crate::queries;
use crate::server::AppState;

use super::html_error;

// askama needs these filters in scope for template derivation
use crate::html::filters;

#[derive(Template)]
#[template(path = "project_list.html")]
struct ProjectListTemplate {
    projects: Vec<queries::ProjectSummary>,
    sort: String,
    query: String,
    period: String,
}

pub async fn handler(
    State(_state): State<AppState>,
    ReadPool(pool): ReadPool,
    Query(params): Query<ListParams>,
) -> axum::response::Response {
    let sort_str = params.sort.clone().unwrap_or_default();
    let query_str = params.query.clone().unwrap_or_default();
    let period_str = params.period.clone().unwrap_or_else(|| "7d".to_string());

    let since = period_to_timestamp(&period_str);

    let projects = match queries::projects::list_projects(
        &pool,
        params.sort.as_deref().filter(|s| !s.is_empty()),
        params.query.as_deref().filter(|s| !s.is_empty()),
        since,
    )
    .await
    {
        Ok(p) => p,
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let tmpl = ProjectListTemplate {
        projects,
        sort: sort_str,
        query: query_str,
        period: period_str,
    };
    render_template(&tmpl)
}
