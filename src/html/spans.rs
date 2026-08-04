use askama::Template;
use axum::extract::{Path, Query, State};

use crate::extractors::{ProjectPageCtx, ReadPool};
use crate::html::chrome::PageChrome;
use crate::html::render_template;
use crate::html::utils::{Chrome, ListParams};
use crate::orgs::extractor::ActiveOrg;
use crate::queries;
use crate::queries::types::{
    Page, PagedResult, SpanAggregation, SpanSummary, TraceError, TraceRoot, TraceSummary, Waterfall,
};
use crate::queries::ProjectNavCounts;
use crate::server::AppState;

use super::HtmlError;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "span_list.html")]
struct SpanListTemplate {
    project_id: u64,
    result: PagedResult<SpanSummary>,
    traces: PagedResult<TraceSummary>,
    aggregates: SpanAggregation,
    agg_cap: usize,
    nav: ProjectNavCounts,
    chrome: PageChrome,
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
    chrome: PageChrome,
}

pub async fn list_handler(
    ctx: ProjectPageCtx,
    Query(params): Query<ListParams>,
) -> Result<axum::response::Response, HtmlError> {
    let page = params.page.page();
    let trace_page = Page::new(Some(0), Some(25));

    let (span_result, trace_result, agg_result) = tokio::join!(
        queries::spans::list_spans(&ctx.pool, ctx.project_id, &page),
        queries::spans::list_traces(&ctx.pool, ctx.project_id, &trace_page),
        queries::spans::aggregate_spans(&ctx.pool, ctx.project_id),
    );

    let result = span_result?;
    let traces = trace_result?;
    let aggregates = agg_result?;

    let tmpl = SpanListTemplate {
        project_id: ctx.project_id,
        result,
        traces,
        aggregates,
        agg_cap: queries::spans::MAX_SPAN_GROUPS,
        nav: ctx.nav,
        chrome: ctx.chrome,
    };
    Ok(render_template(&tmpl))
}

pub async fn trace_detail_handler(
    active: ActiveOrg,
    State(state): State<AppState>,
    ReadPool(pool): ReadPool,
    Chrome(chrome): Chrome,
    Path((project_id, trace_id)): Path<(u64, String)>,
) -> Result<axum::response::Response, HtmlError> {
    crate::orgs::extractor::require_project_scope(&active, &pool, project_id as i64)
        .await
        .map_err(|_| HtmlError(axum::http::StatusCode::NOT_FOUND, "Not found".into()))?;
    let (spans, errors, root) = tokio::join!(
        queries::spans::get_trace_spans_for_project(&pool, project_id, &trace_id),
        queries::spans::get_trace_errors(&pool, project_id, &trace_id),
        queries::spans::get_trace_root(&pool, project_id, &trace_id),
    );
    let spans = spans?;
    let errors = errors?;
    let root = root?;

    let span_rows: Vec<queries::spans::SpanRow> = spans.iter().map(Into::into).collect();
    let root_duration_ms = root.as_ref().and_then(|r| r.duration_ms).unwrap_or(0);
    let waterfall = queries::spans::build_waterfall(&span_rows, root_duration_ms);

    let nav = state.nav_counts(project_id).await;

    let tmpl = TraceDetailTemplate {
        project_id,
        trace_id,
        waterfall,
        root,
        errors,
        nav,
        chrome,
    };
    Ok(render_template(&tmpl))
}
