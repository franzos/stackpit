use askama::Template;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use serde::Deserialize;

use crate::extractors::ReadPool;
use crate::html::render_template;
use crate::queries;
use crate::queries::types::{LogEntry, LogFilter, Page, PagedResult};
use crate::queries::ProjectNavCounts;

use super::html_error;

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
}

pub async fn list_handler(
    ReadPool(pool): ReadPool,
    Path(project_id): Path<u64>,
    Query(params): Query<LogListParams>,
) -> axum::response::Response {
    let query_str = params.query.clone().unwrap_or_default();
    let level_str = params.level.clone().unwrap_or_default();

    let filter = LogFilter {
        level: params.level.filter(|s| !s.is_empty()),
        query: params.query.filter(|s| !s.is_empty()),
        trace_id: None,
    };
    let page = Page::new(params.offset, params.limit);

    let result = match queries::logs::list_logs(&pool, project_id, &filter, &page).await {
        Ok(r) => r,
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let nav = queries::projects::get_nav_counts(&pool, project_id).await;

    let mut filter_parts = Vec::new();
    if !query_str.is_empty() {
        filter_parts.push(format!(
            "&query={}",
            crate::html::utils::urlencoded(&query_str)
        ));
    }
    if !level_str.is_empty() {
        filter_parts.push(format!(
            "&level={}",
            crate::html::utils::urlencoded(&level_str)
        ));
    }
    let filter_qs = filter_parts.join("");

    render_template(&LogListTemplate {
        project_id,
        result,
        query: query_str,
        level: level_str,
        filter_qs,
        nav,
    })
}
