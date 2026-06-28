use askama::Template;
use axum::extract::{Path, Query};

use crate::extractors::{ProjectPageCtx, ReadPool};
use crate::html::render_template;
use crate::html::utils::Csrf;
use crate::queries;
use crate::queries::types::{PagedResult, Pagination};
use crate::queries::MonitorSummary;
use crate::queries::ProjectNavCounts;

use super::HtmlError;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "monitors.html")]
struct MonitorListTemplate {
    project_id: u64,
    monitors: Vec<MonitorSummary>,
    nav: ProjectNavCounts,
    csrf_token: String,
}

pub async fn list_handler(ctx: ProjectPageCtx) -> Result<axum::response::Response, HtmlError> {
    let monitors = queries::monitors::list_monitors(&ctx.pool, ctx.project_id).await?;

    let tmpl = MonitorListTemplate {
        project_id: ctx.project_id,
        monitors,
        nav: ctx.nav,
        csrf_token: ctx.csrf_token,
    };
    Ok(render_template(&tmpl))
}

#[derive(Template)]
#[template(path = "monitor_detail.html")]
struct MonitorDetailTemplate {
    project_id: u64,
    slug: String,
    checkins: PagedResult<queries::EventSummary>,
    nav: queries::ProjectNavCounts,
    csrf_token: String,
}

pub async fn detail_handler(
    ReadPool(pool): ReadPool,
    Csrf(csrf): Csrf,
    Path((project_id, slug)): Path<(u64, String)>,
    Query(params): Query<Pagination>,
) -> Result<axum::response::Response, HtmlError> {
    let page = params.page();
    let checkins =
        queries::monitors::list_checkins_for_monitor(&pool, project_id, &slug, &page).await?;

    let nav = queries::projects::get_nav_counts(&pool, project_id).await;

    let tmpl = MonitorDetailTemplate {
        project_id,
        slug,
        checkins,
        nav,
        csrf_token: csrf,
    };
    Ok(render_template(&tmpl))
}
