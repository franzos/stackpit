use askama::Template;
use axum::extract::Query;

use crate::extractors::ProjectPageCtx;
use crate::html::render_template;
use crate::html::utils::ListParams;
use crate::queries;
use crate::queries::types::{EventFilter, EventSummary, PagedResult};
use crate::queries::ProjectNavCounts;

use super::HtmlError;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "user_report_list.html")]
struct UserReportListTemplate {
    project_id: u64,
    result: PagedResult<EventSummary>,
    nav: ProjectNavCounts,
    csrf_token: String,
}

#[derive(Template)]
#[template(path = "client_report_list.html")]
struct ClientReportListTemplate {
    project_id: u64,
    result: PagedResult<EventSummary>,
    outcomes: Vec<crate::queries::client_reports::ClientReportOutcome>,
    nav: ProjectNavCounts,
    csrf_token: String,
}

pub async fn user_reports_handler(
    ctx: ProjectPageCtx,
    Query(params): Query<ListParams>,
) -> Result<axum::response::Response, HtmlError> {
    let filter = EventFilter {
        project_id: Some(ctx.project_id),
        item_type: Some("user_report".to_string()),
        ..Default::default()
    };
    let page = params.page.page();

    let result = queries::events::list_all_events(&ctx.pool, &filter, &page).await?;

    let tmpl = UserReportListTemplate {
        project_id: ctx.project_id,
        result,
        nav: ctx.nav,
        csrf_token: ctx.csrf_token,
    };
    Ok(render_template(&tmpl))
}

pub async fn client_reports_handler(
    ctx: ProjectPageCtx,
    Query(params): Query<ListParams>,
) -> Result<axum::response::Response, HtmlError> {
    let filter = EventFilter {
        project_id: Some(ctx.project_id),
        item_type: Some("client_report".to_string()),
        ..Default::default()
    };
    let page = params.page.page();

    let result = queries::events::list_all_events(&ctx.pool, &filter, &page).await?;

    let since = chrono::Utc::now().timestamp() - 30 * 86400;
    let outcomes =
        queries::client_reports::summarize_client_reports(&ctx.pool, ctx.project_id, since).await?;

    let tmpl = ClientReportListTemplate {
        project_id: ctx.project_id,
        result,
        outcomes,
        nav: ctx.nav,
        csrf_token: ctx.csrf_token,
    };
    Ok(render_template(&tmpl))
}
