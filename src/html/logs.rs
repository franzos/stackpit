use askama::Template;
use axum::extract::{Path, Query};
use serde::Deserialize;

use crate::extractors::ReadPool;
use crate::html::render_template;
use crate::html::utils::{build_filter_qs, Csrf};
use crate::queries;
use crate::queries::types::{LogEntry, LogFilter, Page, PagedResult};
use crate::queries::ProjectNavCounts;

use super::HtmlError;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Deserialize)]
pub struct LogListParams {
    pub query: Option<String>,
    pub level: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Template)]
#[template(path = "log_list.html")]
struct LogListTemplate {
    project_id: u64,
    result: PagedResult<LogEntry>,
    query: String,
    level: String,
    filter_qs: String,
    nav: ProjectNavCounts,
    csrf_token: String,
}

pub async fn list_handler(
    ReadPool(pool): ReadPool,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
    Query(params): Query<LogListParams>,
) -> Result<axum::response::Response, HtmlError> {
    let query_str = params.query.clone().unwrap_or_default();
    let level_str = params.level.clone().unwrap_or_default();

    let filter = LogFilter {
        level: params.level.filter(|s| !s.is_empty()),
        query: params.query.filter(|s| !s.is_empty()),
        trace_id: None,
    };
    let page = Page::new(params.offset, params.limit);

    let result = queries::logs::list_logs(&pool, project_id, &filter, &page).await?;

    let nav = queries::projects::get_nav_counts(&pool, project_id).await;

    let (filter_qs, _) = build_filter_qs(&[("query", &query_str), ("level", &level_str)], "");

    Ok(render_template(&LogListTemplate {
        project_id,
        result,
        query: query_str,
        level: level_str,
        filter_qs,
        nav,
        csrf_token: csrf,
    }))
}
