use askama::Template;
use axum::extract::{Path, Query, RawQuery};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::db::DbPool;
use crate::extractors::{BrowserDefaults, ReadPool};
use crate::html::render_template;
use crate::html::utils::{build_filter_qs, defaults_redirect_url, period_to_timestamp, ListParams};
use crate::queries;
use crate::queries::types::{IssueFilter, Page, PagedResult};
use crate::queries::ProjectNavCounts;

use super::charts;
use super::html_error;

// askama needs these filters in scope for template derivation
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
}

#[derive(Template)]
#[template(path = "transaction_list.html")]
struct TransactionListTemplate {
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
}

pub async fn handler(
    BrowserDefaults(defaults): BrowserDefaults,
    RawQuery(raw_qs): RawQuery,
    ReadPool(pool): ReadPool,
    Path(project_id): Path<u64>,
    Query(params): Query<ListParams>,
) -> axum::response::Response {
    if let Some(url) = defaults_redirect_url(
        &format!("/web/projects/{project_id}/"),
        raw_qs.as_deref(),
        &defaults,
        &["status", "level", "period"],
    ) {
        return axum::response::Redirect::to(&url).into_response();
    }
    issue_or_transaction_handler(&pool, project_id, params, "event").await
}

pub async fn transaction_handler(
    BrowserDefaults(defaults): BrowserDefaults,
    RawQuery(raw_qs): RawQuery,
    ReadPool(pool): ReadPool,
    Path(project_id): Path<u64>,
    Query(params): Query<ListParams>,
) -> axum::response::Response {
    if let Some(url) = defaults_redirect_url(
        &format!("/web/projects/{project_id}/transactions/"),
        raw_qs.as_deref(),
        &defaults,
        &["status", "level", "period"],
    ) {
        return axum::response::Redirect::to(&url).into_response();
    }
    issue_or_transaction_handler(&pool, project_id, params, "transaction").await
}

async fn issue_or_transaction_handler(
    pool: &DbPool,
    project_id: u64,
    params: ListParams,
    item_type: &str,
) -> axum::response::Response {
    let query_str = params.query.clone().unwrap_or_default();
    let level_str = params.level.clone().unwrap_or_default();
    let status_str = params.status.clone().unwrap_or_default();
    let sort_str = params.sort.clone().unwrap_or_default();
    let release_str = params.release.clone().unwrap_or_default();
    let tag_str = params.tag.clone().unwrap_or_default();
    let period_str = params.period.clone().unwrap_or_else(|| "7d".to_string());

    let since = period_to_timestamp(&period_str);

    let tag_parsed = if let Some(pos) = tag_str.find('=') {
        let k = tag_str[..pos].to_string();
        let v = tag_str[pos + 1..].to_string();
        if !k.is_empty() && !v.is_empty() {
            Some((k, v))
        } else {
            None
        }
    } else {
        None
    };

    let filter = IssueFilter {
        level: params.level.filter(|s| !s.is_empty()),
        status: params.status.filter(|s| !s.is_empty()),
        query: params.query.filter(|s| !s.is_empty()),
        sort: params.sort.filter(|s| !s.is_empty()),
        item_type: Some(item_type.to_string()),
        release: params.release.filter(|s| !s.is_empty()),
        tag: tag_parsed,
    };
    let page = Page::new(params.offset, params.limit);

    let result = match queries::issues::list_issues(pool, project_id, &filter, &page, since).await {
        Ok(r) => r,
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

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

    if item_type == "transaction" {
        render_template(&TransactionListTemplate {
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
        })
    } else {
        render_template(&IssueListTemplate {
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
        })
    }
}
