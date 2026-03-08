use askama::Template;
use axum::extract::{Path, Query};
use axum::http::StatusCode;

use crate::extractors::ReadPool;
use crate::html::render_template;
use crate::html::utils::ListParams;
use crate::queries;
use crate::queries::types::{Page, PagedResult, ReplaySummary};
use crate::queries::ProjectNavCounts;

use super::html_error;

use crate::html::filters;

#[derive(Template)]
#[template(path = "replay_list.html")]
struct ReplayListTemplate {
    project_id: u64,
    result: PagedResult<ReplaySummary>,
    nav: ProjectNavCounts,
}

pub async fn list_handler(
    ReadPool(pool): ReadPool,
    Path(project_id): Path<u64>,
    Query(params): Query<ListParams>,
) -> axum::response::Response {
    let page = Page::new(params.offset, params.limit);

    let result = match queries::replays::list_replays(&pool, project_id, &page).await {
        Ok(r) => r,
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let nav = queries::projects::get_nav_counts(&pool, project_id).await;

    let tmpl = ReplayListTemplate {
        project_id,
        result,
        nav,
    };
    render_template(&tmpl)
}

#[derive(Template)]
#[template(path = "replay_detail.html")]
struct ReplayDetailTemplate {
    project_id: u64,
    replay: queries::types::ReplayDetail,
    raw_json: String,
    nav: ProjectNavCounts,
}

pub async fn detail_handler(
    ReadPool(pool): ReadPool,
    Path((project_id, event_id)): Path<(u64, String)>,
) -> axum::response::Response {
    let replay = match queries::replays::get_replay(&pool, project_id, &event_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return html_error(StatusCode::NOT_FOUND, "Replay not found"),
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let raw_json = serde_json::to_string_pretty(&replay.payload).unwrap_or_default();
    let nav = queries::projects::get_nav_counts(&pool, project_id).await;

    let tmpl = ReplayDetailTemplate {
        project_id,
        replay,
        raw_json,
        nav,
    };
    render_template(&tmpl)
}
