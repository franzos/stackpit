use askama::Template;
use axum::extract::{Path, Query};

use crate::extractors::ReadPool;
use crate::html::render_template;
use crate::html::utils::{Csrf, ListParams};
use crate::queries;
use crate::queries::types::{Page, PagedResult, SpanSummary, TraceSpan, TraceSummary};
use crate::queries::ProjectNavCounts;

use super::HtmlError;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "span_list.html")]
struct SpanListTemplate {
    project_id: u64,
    result: PagedResult<SpanSummary>,
    traces: PagedResult<TraceSummary>,
    nav: ProjectNavCounts,
    csrf_token: String,
}

#[derive(Template)]
#[template(path = "trace_detail.html")]
struct TraceDetailTemplate {
    project_id: u64,
    trace_id: String,
    spans: Vec<TraceSpan>,
    nav: ProjectNavCounts,
    csrf_token: String,
}

pub async fn list_handler(
    ReadPool(pool): ReadPool,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
    Query(params): Query<ListParams>,
) -> Result<axum::response::Response, HtmlError> {
    let page = Page::new(params.offset, params.limit);
    let trace_page = Page::new(Some(0), Some(25));

    let (span_result, trace_result) = tokio::join!(
        queries::spans::list_spans(&pool, project_id, &page),
        queries::spans::list_traces(&pool, project_id, &trace_page),
    );

    let result = span_result?;
    let traces = trace_result?;

    let nav = queries::projects::get_nav_counts(&pool, project_id).await;

    let tmpl = SpanListTemplate {
        project_id,
        result,
        traces,
        nav,
        csrf_token: csrf,
    };
    Ok(render_template(&tmpl))
}

pub async fn trace_detail_handler(
    ReadPool(pool): ReadPool,
    Csrf(csrf): Csrf,
    Path((project_id, trace_id)): Path<(u64, String)>,
) -> Result<axum::response::Response, HtmlError> {
    let spans = queries::spans::get_trace_spans(&pool, &trace_id).await?;

    let nav = queries::projects::get_nav_counts(&pool, project_id).await;

    let tmpl = TraceDetailTemplate {
        project_id,
        trace_id,
        spans,
        nav,
        csrf_token: csrf,
    };
    Ok(render_template(&tmpl))
}
