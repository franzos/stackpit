use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;

use crate::extractors::ReadPool;
use crate::html::render_template;
use crate::queries;
use crate::queries::types::{Page, PagedResult};
use crate::queries::MonitorSummary;
use crate::queries::ProjectNavCounts;
use crate::server::AppState;

use super::html_error;

// askama needs these filters in scope for template derivation
use crate::html::filters;

#[derive(Template)]
#[template(path = "monitors.html")]
struct MonitorListTemplate {
    project_id: u64,
    monitors: Vec<MonitorSummary>,
    nav: ProjectNavCounts,
}

pub async fn list_handler(
    State(_state): State<AppState>,
    ReadPool(pool): ReadPool,
    Path(project_id): Path<u64>,
) -> axum::response::Response {
    let monitors = match queries::monitors::list_monitors(&pool, project_id).await {
        Ok(m) => m,
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let nav = queries::projects::get_nav_counts(&pool, project_id).await;

    let tmpl = MonitorListTemplate {
        project_id,
        monitors,
        nav,
    };
    render_template(&tmpl)
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
}

pub async fn detail_handler(
    State(_state): State<AppState>,
    ReadPool(pool): ReadPool,
    Path((project_id, slug)): Path<(u64, String)>,
    Query(params): Query<PageParams>,
) -> axum::response::Response {
    let page = Page::new(params.offset, params.limit);
    let checkins =
        match queries::monitors::list_checkins_for_monitor(&pool, project_id, &slug, &page).await {
            Ok(r) => r,
            Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        };

    let nav = queries::projects::get_nav_counts(&pool, project_id).await;

    let tmpl = MonitorDetailTemplate {
        project_id,
        slug,
        checkins,
        nav,
    };
    render_template(&tmpl)
}
