use askama::Template;
use axum::extract::{Path, Query};

use crate::extractors::ReadPool;
use crate::html::render_template;
use crate::html::utils::{Csrf, ListParams};
use crate::queries;
use crate::queries::types::{
    Page, PagedResult, SpanSummary, TraceError, TraceRoot, TraceSummary, Waterfall,
};
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
    waterfall: Waterfall,
    root: Option<TraceRoot>,
    errors: Vec<TraceError>,
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
    let (spans, errors, root) = tokio::join!(
        queries::spans::get_trace_spans(&pool, &trace_id),
        queries::spans::get_trace_errors(&pool, project_id, &trace_id),
        queries::spans::get_trace_root(&pool, project_id, &trace_id),
    );
    let spans = spans?;
    let errors = errors?;
    let root = root?;

    let span_rows: Vec<queries::spans::SpanRow> = spans.iter().map(Into::into).collect();
    let root_duration_ms = root.as_ref().and_then(|r| r.duration_ms).unwrap_or(0);
    let waterfall = queries::spans::build_waterfall(&span_rows, root_duration_ms);

    let nav = queries::projects::get_nav_counts(&pool, project_id).await;

    let tmpl = TraceDetailTemplate {
        project_id,
        trace_id,
        waterfall,
        root,
        errors,
        nav,
        csrf_token: csrf,
    };
    Ok(render_template(&tmpl))
}
