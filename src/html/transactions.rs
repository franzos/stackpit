use askama::Template;
use axum::extract::Query;
use serde::Deserialize;

use crate::extractors::ProjectPageCtx;
use crate::html::render_template;
use crate::html::utils::{period_to_timestamp, ListParams};
use crate::queries;
use crate::queries::types::{PagedResult, Pagination, TransactionInstance, TransactionSummary};
use crate::queries::ProjectNavCounts;

use super::HtmlError;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "transaction_list.html")]
struct TransactionListTemplate {
    project_id: u64,
    items: Vec<TransactionSummary>,
    sort: String,
    period: String,
    nav: ProjectNavCounts,
    csrf_token: String,
}

#[derive(Template)]
#[template(path = "transaction_detail.html")]
struct TransactionDetailTemplate {
    project_id: u64,
    name: String,
    op: Option<String>,
    result: PagedResult<TransactionInstance>,
    nav: ProjectNavCounts,
    csrf_token: String,
}

#[derive(Deserialize)]
pub struct DetailParams {
    pub name: Option<String>,
    #[serde(flatten)]
    pub page: Pagination,
}

pub async fn list_handler(
    ctx: ProjectPageCtx,
    Query(params): Query<ListParams>,
) -> Result<axum::response::Response, HtmlError> {
    let sort = params.sort.clone().unwrap_or_else(|| "p95".to_string());
    let period = params.period.clone().unwrap_or_else(|| "7d".to_string());
    let since = period_to_timestamp(&period).unwrap_or(0);

    let items =
        queries::transactions::list_transactions(&ctx.pool, ctx.project_id, since, &sort).await?;

    Ok(render_template(&TransactionListTemplate {
        project_id: ctx.project_id,
        items,
        sort,
        period,
        nav: ctx.nav,
        csrf_token: ctx.csrf_token,
    }))
}

pub async fn detail_handler(
    ctx: ProjectPageCtx,
    Query(params): Query<DetailParams>,
) -> Result<axum::response::Response, HtmlError> {
    let name = params.name.unwrap_or_default();
    let page = params.page.page();

    let result =
        queries::transactions::list_transaction_instances(&ctx.pool, ctx.project_id, &name, &page)
            .await?;
    let op = result.items.first().and_then(|i| i.op.clone());

    Ok(render_template(&TransactionDetailTemplate {
        project_id: ctx.project_id,
        name,
        op,
        result,
        nav: ctx.nav,
        csrf_token: ctx.csrf_token,
    }))
}
