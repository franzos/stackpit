use askama::Template;
use axum::extract::{Query, RawQuery, State};
use axum::response::IntoResponse;

use crate::extractors::{BrowserDefaults, ReadPool};
use crate::html::render_template;
use crate::html::utils::{defaults_redirect_url, period_to_timestamp, Csrf, ListParams};
use crate::queries;
use crate::server::AppState;

use super::HtmlError;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "project_list.html")]
struct ProjectListTemplate {
    projects: Vec<queries::ProjectSummary>,
    sort: String,
    query: String,
    period: String,
    csrf_token: String,
}

pub async fn handler(
    BrowserDefaults(defaults): BrowserDefaults,
    RawQuery(raw_qs): RawQuery,
    State(_state): State<AppState>,
    ReadPool(pool): ReadPool,
    Csrf(csrf): Csrf,
    Query(params): Query<ListParams>,
) -> Result<axum::response::Response, HtmlError> {
    if let Some(url) =
        defaults_redirect_url("/web/projects/", raw_qs.as_deref(), &defaults, &["period"])
    {
        return Ok(axum::response::Redirect::to(&url).into_response());
    }
    let sort_str = params.sort.clone().unwrap_or_default();
    let query_str = params.query.clone().unwrap_or_default();
    let period_str = params.period.clone().unwrap_or_else(|| "7d".to_string());

    let since = period_to_timestamp(&period_str);

    let projects = queries::projects::list_projects(
        &pool,
        params.sort.as_deref().filter(|s| !s.is_empty()),
        params.query.as_deref().filter(|s| !s.is_empty()),
        since,
    )
    .await?;

    let tmpl = ProjectListTemplate {
        projects,
        sort: sort_str,
        query: query_str,
        period: period_str,
        csrf_token: csrf,
    };
    Ok(render_template(&tmpl))
}
