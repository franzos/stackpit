use askama::Template;
use axum::extract::Query;
use serde::Deserialize;

use crate::extractors::ProjectPageCtx;
use crate::html::render_template;
use crate::html::utils::build_filter_qs;
use crate::queries;
use crate::queries::types::{LogEntry, LogFilter, PagedResult, Pagination};
use crate::queries::ProjectNavCounts;

use super::HtmlError;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Deserialize)]
pub struct LogListParams {
    pub query: Option<String>,
    pub level: Option<String>,
    #[serde(flatten)]
    pub page: Pagination,
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
    ctx: ProjectPageCtx,
    Query(params): Query<LogListParams>,
) -> Result<axum::response::Response, HtmlError> {
    let query_str = params.query.clone().unwrap_or_default();
    let level_str = params.level.clone().unwrap_or_default();

    let filter = LogFilter {
        level: params.level.filter(|s| !s.is_empty()),
        query: params.query.filter(|s| !s.is_empty()),
        trace_id: None,
    };
    let page = params.page.page();

    let result = queries::logs::list_logs(&ctx.pool, ctx.project_id, &filter, &page).await?;

    let (filter_qs, _) = build_filter_qs(&[("query", &query_str), ("level", &level_str)], "");

    Ok(render_template(&LogListTemplate {
        project_id: ctx.project_id,
        result,
        query: query_str,
        level: level_str,
        filter_qs,
        nav: ctx.nav,
        csrf_token: ctx.csrf_token,
    }))
}
