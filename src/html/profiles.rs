use askama::Template;
use axum::extract::{Path, Query};

use crate::extractors::ReadPool;
use crate::html::utils::{render_project_detail, render_project_list, Csrf, ListParams};
use crate::queries;
use crate::queries::types::{Page, PagedResult, ProfileSummary};
use crate::queries::ProjectNavCounts;

use super::HtmlError;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "profile_list.html")]
struct ProfileListTemplate {
    project_id: u64,
    result: PagedResult<ProfileSummary>,
    nav: ProjectNavCounts,
    csrf_token: String,
}

pub async fn list_handler(
    ReadPool(pool): ReadPool,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
    Query(params): Query<ListParams>,
) -> Result<axum::response::Response, HtmlError> {
    let page = Page::new(params.offset, params.limit);
    let result = queries::profiles::list_profiles(&pool, project_id, &page).await?;

    Ok(render_project_list(
        &pool,
        project_id,
        csrf,
        result,
        |project_id, result, nav, csrf_token| ProfileListTemplate {
            project_id,
            result,
            nav,
            csrf_token,
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
    csrf_token: String,
}

pub async fn detail_handler(
    ReadPool(pool): ReadPool,
    Csrf(csrf): Csrf,
    Path((project_id, event_id)): Path<(u64, String)>,
) -> Result<axum::response::Response, HtmlError> {
    let profile = queries::profiles::get_profile(&pool, project_id, &event_id).await?;

    render_project_detail(
        &pool,
        project_id,
        csrf,
        profile,
        "Profile not found",
        |project_id, profile, nav, csrf_token| {
            let raw_json = serde_json::to_string_pretty(&profile.payload).unwrap_or_default();
            ProfileDetailTemplate {
                project_id,
                profile,
                raw_json,
                nav,
                csrf_token,
            }
        },
    )
    .await
}
