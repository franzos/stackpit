use askama::Template;
use axum::extract::{Path, Query, State};

use crate::extractors::{ProjectPath, ReadPool};
use crate::html::chrome::PageChrome;
use crate::html::utils::{render_project_detail, render_project_list, Chrome, ListParams};
use crate::orgs::extractor::ActiveOrg;
use crate::queries;
use crate::queries::types::{PagedResult, ReplaySummary};
use crate::queries::ProjectNavCounts;
use crate::server::AppState;

use super::HtmlError;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "replay_list.html")]
struct ReplayListTemplate {
    project_id: u64,
    result: PagedResult<ReplaySummary>,
    nav: ProjectNavCounts,
    chrome: PageChrome,
}

pub async fn list_handler(
    active: ActiveOrg,
    State(state): State<AppState>,
    ReadPool(pool): ReadPool,
    Chrome(chrome): Chrome,
    ProjectPath(project_id): ProjectPath,
    Query(params): Query<ListParams>,
) -> Result<axum::response::Response, HtmlError> {
    crate::orgs::extractor::require_project_scope(&active, &pool, project_id as i64)
        .await
        .map_err(|_| HtmlError(axum::http::StatusCode::NOT_FOUND, "Not found".into()))?;
    let page = params.page.page();
    let result = queries::replays::list_replays(&pool, project_id, &page).await?;

    Ok(render_project_list(
        &pool,
        &state.nav_cache,
        project_id,
        chrome,
        result,
        |project_id, result, nav, chrome| ReplayListTemplate {
            project_id,
            result,
            nav,
            chrome,
        },
    )
    .await)
}

#[derive(Template)]
#[template(path = "replay_detail.html")]
struct ReplayDetailTemplate {
    project_id: u64,
    replay: queries::types::ReplayDetail,
    errors: Vec<queries::types::ReplayError>,
    raw_json: String,
    nav: ProjectNavCounts,
    chrome: PageChrome,
}

pub async fn detail_handler(
    active: ActiveOrg,
    State(state): State<AppState>,
    ReadPool(pool): ReadPool,
    Chrome(chrome): Chrome,
    Path((project_id, event_id)): Path<(u64, String)>,
) -> Result<axum::response::Response, HtmlError> {
    crate::orgs::extractor::require_project_scope(&active, &pool, project_id as i64)
        .await
        .map_err(|_| HtmlError(axum::http::StatusCode::NOT_FOUND, "Not found".into()))?;
    let replay = queries::replays::get_replay(&pool, project_id, &event_id).await?;
    let errors = match &replay {
        Some(r) => queries::replays::get_replay_errors(&pool, project_id, &r.payload).await?,
        None => Vec::new(),
    };

    render_project_detail(
        &pool,
        &state.nav_cache,
        project_id,
        chrome,
        replay,
        "Replay not found",
        move |project_id, replay, nav, chrome| {
            let raw_json = serde_json::to_string_pretty(&replay.payload).unwrap_or_default();
            ReplayDetailTemplate {
                project_id,
                replay,
                errors,
                raw_json,
                nav,
                chrome,
            }
        },
    )
    .await
}
