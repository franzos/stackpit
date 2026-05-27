use askama::Template;
use axum::extract::{Path, Query, State};
use serde::Deserialize;

use crate::extractors::ReadPool;
use crate::html::render_template;
use crate::html::utils::Csrf;
use crate::queries;
use crate::queries::types::{Page, PagedResult};
use crate::queries::MonitorSummary;
use crate::queries::ProjectNavCounts;
use crate::server::AppState;

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

pub async fn list_handler(
    State(_state): State<AppState>,
    ReadPool(pool): ReadPool,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
) -> Result<axum::response::Response, HtmlError> {
    let monitors = queries::monitors::list_monitors(&pool, project_id).await?;

    let nav = queries::projects::get_nav_counts(&pool, project_id).await;

    let tmpl = MonitorListTemplate {
        project_id,
        monitors,
        nav,
        csrf_token: csrf,
    };
    Ok(render_template(&tmpl))
}

#[derive(Deserialize)]
pub struct PageParams {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
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
    State(_state): State<AppState>,
    ReadPool(pool): ReadPool,
    Csrf(csrf): Csrf,
    Path((project_id, slug)): Path<(u64, String)>,
    Query(params): Query<PageParams>,
) -> Result<axum::response::Response, HtmlError> {
    let page = Page::new(params.offset, params.limit);
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
