use askama::Template;
use axum::extract::{Path, Query, State};

use crate::extractors::ReadPool;
use crate::html::chrome::PageChrome;
use crate::html::utils::{render_project_detail, render_project_list, Chrome, ListParams};
use crate::orgs::extractor::ActiveOrg;
use crate::queries;
use crate::queries::types::{PagedResult, ProfileSummary};
use crate::queries::ProjectNavCounts;
use crate::server::AppState;

use super::HtmlError;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "profile_list.html")]
struct ProfileListTemplate {
    project_id: u64,
    result: PagedResult<ProfileSummary>,
    nav: ProjectNavCounts,
    chrome: PageChrome,
}

pub async fn list_handler(
    active: ActiveOrg,
    State(state): State<AppState>,
    ReadPool(pool): ReadPool,
    Chrome(chrome): Chrome,
    Path(project_id): Path<u64>,
    Query(params): Query<ListParams>,
) -> Result<axum::response::Response, HtmlError> {
    crate::orgs::extractor::require_project_scope(&active, &pool, project_id as i64)
        .await
        .map_err(|_| HtmlError(axum::http::StatusCode::NOT_FOUND, "Not found".into()))?;
    let page = params.page.page();
    let result = queries::profiles::list_profiles(&pool, project_id, &page).await?;

    Ok(render_project_list(
        &pool,
        &state.nav_cache,
        project_id,
        chrome,
        result,
        |project_id, result, nav, chrome| ProfileListTemplate {
            project_id,
            result,
            nav,
            chrome,
        },
    )
    .await)
}

#[derive(Template)]
#[template(path = "profile_detail.html")]
struct ProfileDetailTemplate {
    project_id: u64,
    profile: queries::types::ProfileDetail,
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
    let profile = queries::profiles::get_profile(&pool, project_id, &event_id).await?;

    render_project_detail(
        &pool,
        &state.nav_cache,
        project_id,
        chrome,
        profile,
        "Profile not found",
        |project_id, profile, nav, chrome| {
            let raw_json = serde_json::to_string_pretty(&profile.payload).unwrap_or_default();
            ProfileDetailTemplate {
                project_id,
                profile,
                raw_json,
                nav,
                chrome,
            }
        },
    )
    .await
}
