use askama::Template;
use axum::extract::{Path, Query};

use crate::extractors::ReadPool;
use crate::orgs::extractor::ActiveOrg;
use crate::html::utils::{render_project_detail, render_project_list, Csrf, ListParams};
use crate::queries;
use crate::queries::types::{PagedResult, ReplaySummary};
use crate::queries::ProjectNavCounts;

use super::HtmlError;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "replay_list.html")]
struct ReplayListTemplate {
    project_id: u64,
    result: PagedResult<ReplaySummary>,
    nav: ProjectNavCounts,
    csrf_token: String,
}

pub async fn list_handler(
    active: ActiveOrg,
    ReadPool(pool): ReadPool,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
    Query(params): Query<ListParams>,
) -> Result<axum::response::Response, HtmlError> {
    crate::orgs::extractor::require_project_scope(&active, &pool, project_id as i64)
        .await
        .map_err(|_| HtmlError(axum::http::StatusCode::NOT_FOUND, "Not found".into()))?;
    let page = params.page.page();
    let result = queries::replays::list_replays(&pool, project_id, &page).await?;

    Ok(render_project_list(
        &pool,
        project_id,
        csrf,
        result,
        |project_id, result, nav, csrf_token| ReplayListTemplate {
            project_id,
            result,
            nav,
            csrf_token,
        },
    )
    .await)
}

#[derive(Template)]
#[template(path = "replay_detail.html")]
struct ReplayDetailTemplate {
    project_id: u64,
    replay: queries::types::ReplayDetail,
    raw_json: String,
    nav: ProjectNavCounts,
    csrf_token: String,
}

pub async fn detail_handler(
    active: ActiveOrg,
    ReadPool(pool): ReadPool,
    Csrf(csrf): Csrf,
    Path((project_id, event_id)): Path<(u64, String)>,
) -> Result<axum::response::Response, HtmlError> {
    crate::orgs::extractor::require_project_scope(&active, &pool, project_id as i64)
        .await
        .map_err(|_| HtmlError(axum::http::StatusCode::NOT_FOUND, "Not found".into()))?;
    let replay = queries::replays::get_replay(&pool, project_id, &event_id).await?;

    render_project_detail(
        &pool,
        project_id,
        csrf,
        replay,
        "Replay not found",
        |project_id, replay, nav, csrf_token| {
            let raw_json = serde_json::to_string_pretty(&replay.payload).unwrap_or_default();
            ReplayDetailTemplate {
                project_id,
                replay,
                raw_json,
                nav,
                csrf_token,
            }
        },
    )
    .await
}
