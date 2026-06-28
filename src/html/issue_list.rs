use askama::Template;
use axum::extract::{Path, Query, RawQuery};
use axum::response::IntoResponse;

use crate::db::DbPool;
use crate::extractors::{BrowserDefaults, ReadPool};
use crate::html::render_template;
use crate::html::utils::{
    build_filter_qs, defaults_redirect_url, issue_filter_from_params, period_to_timestamp, Csrf,
    ListParams,
};
use crate::queries;
use crate::queries::types::PagedResult;
use crate::queries::ProjectNavCounts;

use super::charts;
use super::HtmlError;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "issue_list.html")]
struct IssueListTemplate {
    project_id: u64,
    result: PagedResult<queries::IssueSummary>,
    query: String,
    level: String,
    status: String,
    sort: String,
    release: String,
    tag: String,
    period: String,
    releases: Vec<String>,
    filter_qs: String,
    base_qs: String,
    nav: ProjectNavCounts,
    chart_svg: String,
    csrf_token: String,
}

pub async fn handler(
    BrowserDefaults(defaults): BrowserDefaults,
    RawQuery(raw_qs): RawQuery,
    ReadPool(pool): ReadPool,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
    Query(params): Query<ListParams>,
) -> Result<axum::response::Response, HtmlError> {
    if let Some(url) = defaults_redirect_url(
        &format!("/web/projects/{project_id}/"),
        raw_qs.as_deref(),
        &defaults,
        &["status", "level", "period"],
    ) {
        return Ok(axum::response::Redirect::to(&url).into_response());
    }
    issue_or_transaction_handler(&pool, project_id, params, "event", csrf).await
}

async fn issue_or_transaction_handler(
    pool: &DbPool,
    project_id: u64,
    params: ListParams,
    item_type: &str,
    csrf: String,
) -> Result<axum::response::Response, HtmlError> {
    let query_str = params.query.clone().unwrap_or_default();
    let level_str = params.level.clone().unwrap_or_default();
    let status_str = params.status.clone().unwrap_or_default();
    let sort_str = params.sort.clone().unwrap_or_default();
    let release_str = params.release.clone().unwrap_or_default();
    let tag_str = params.tag.clone().unwrap_or_default();
    let period_str = params.period.clone().unwrap_or_else(|| "7d".to_string());

    let since = period_to_timestamp(&period_str);

    let filter = issue_filter_from_params(&params, item_type);
    let page = params.page.page();

    let result = queries::issues::list_issues(pool, project_id, &filter, &page, since).await?;

    let nav = queries::projects::get_nav_counts(pool, project_id).await;

    let releases = queries::releases::list_releases_for_project(pool, project_id)
        .await
        .unwrap_or_default();

    let chart_svg =
        match queries::events::project_event_histogram(pool, project_id, item_type, &period_str)
            .await
        {
            Ok(buckets) => charts::render_event_chart_wide(&buckets).unwrap_or_default(),
            Err(_) => String::new(),
        };

    let (base_qs, filter_qs) = build_filter_qs(
        &[
            ("query", &query_str),
            ("level", &level_str),
            ("status", &status_str),
            ("release", &release_str),
            ("tag", &tag_str),
            ("period", &period_str),
        ],
        &sort_str,
    );

    Ok(render_template(&IssueListTemplate {
        project_id,
        result,
        query: query_str,
        level: level_str,
        status: status_str,
        sort: sort_str,
        release: release_str,
        tag: tag_str,
        period: period_str,
        releases,
        filter_qs,
        base_qs,
        nav,
        chart_svg,
        csrf_token: csrf,
    }))
}
