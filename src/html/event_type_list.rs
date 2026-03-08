use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;

use crate::extractors::ReadPool;
use crate::html::render_template;
use crate::html::utils::ListParams;
use crate::queries;
use crate::queries::types::{EventFilter, EventSummary, Page, PagedResult};
use crate::queries::ProjectNavCounts;
use crate::server::AppState;

use super::html_error;

use crate::html::filters;

#[derive(Template)]
#[template(path = "user_report_list.html")]
struct UserReportListTemplate {
    project_id: u64,
    result: PagedResult<EventSummary>,
    nav: ProjectNavCounts,
}

#[derive(Template)]
#[template(path = "client_report_list.html")]
struct ClientReportListTemplate {
    project_id: u64,
    result: PagedResult<EventSummary>,
    nav: ProjectNavCounts,
}

pub async fn user_reports_handler(
    State(_state): State<AppState>,
    ReadPool(pool): ReadPool,
    Path(project_id): Path<u64>,
    Query(params): Query<ListParams>,
) -> axum::response::Response {
    let filter = EventFilter {
        project_id: Some(project_id),
        item_type: Some("user_report".to_string()),
        ..Default::default()
    };
    let page = Page::new(params.offset, params.limit);

    let result = match queries::events::list_all_events(&pool, &filter, &page).await {
        Ok(r) => r,
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let nav = queries::projects::get_nav_counts(&pool, project_id).await;

    let tmpl = UserReportListTemplate {
        project_id,
        result,
        nav,
    };
    render_template(&tmpl)
}

pub async fn client_reports_handler(
    State(_state): State<AppState>,
    ReadPool(pool): ReadPool,
    Path(project_id): Path<u64>,
    Query(params): Query<ListParams>,
) -> axum::response::Response {
    let filter = EventFilter {
        project_id: Some(project_id),
        item_type: Some("client_report".to_string()),
        ..Default::default()
    };
    let page = Page::new(params.offset, params.limit);

    let result = match queries::events::list_all_events(&pool, &filter, &page).await {
        Ok(r) => r,
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let nav = queries::projects::get_nav_counts(&pool, project_id).await;

    let tmpl = ClientReportListTemplate {
        project_id,
        result,
        nav,
    };
    render_template(&tmpl)
}
