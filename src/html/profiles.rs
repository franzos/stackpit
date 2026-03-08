use askama::Template;
use axum::extract::{Path, Query};
use axum::http::StatusCode;

use crate::extractors::ReadPool;
use crate::html::render_template;
use crate::html::utils::ListParams;
use crate::queries;
use crate::queries::types::{Page, PagedResult, ProfileSummary};
use crate::queries::ProjectNavCounts;

use super::html_error;

use crate::html::filters;

#[derive(Template)]
#[template(path = "profile_list.html")]
struct ProfileListTemplate {
    project_id: u64,
    result: PagedResult<ProfileSummary>,
    nav: ProjectNavCounts,
}

pub async fn list_handler(
    ReadPool(pool): ReadPool,
    Path(project_id): Path<u64>,
    Query(params): Query<ListParams>,
) -> axum::response::Response {
    let page = Page::new(params.offset, params.limit);

    let result = match queries::profiles::list_profiles(&pool, project_id, &page).await {
        Ok(r) => r,
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let nav = queries::projects::get_nav_counts(&pool, project_id).await;

    let tmpl = ProfileListTemplate {
        project_id,
        result,
        nav,
    };
    render_template(&tmpl)
}

#[derive(Template)]
#[template(path = "profile_detail.html")]
struct ProfileDetailTemplate {
    project_id: u64,
    profile: queries::types::ProfileDetail,
    raw_json: String,
    nav: ProjectNavCounts,
}

pub async fn detail_handler(
    ReadPool(pool): ReadPool,
    Path((project_id, event_id)): Path<(u64, String)>,
) -> axum::response::Response {
    let profile = match queries::profiles::get_profile(&pool, project_id, &event_id).await {
        Ok(Some(p)) => p,
        Ok(None) => return html_error(StatusCode::NOT_FOUND, "Profile not found"),
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let raw_json = serde_json::to_string_pretty(&profile.payload).unwrap_or_default();
    let nav = queries::projects::get_nav_counts(&pool, project_id).await;

    let tmpl = ProfileDetailTemplate {
        project_id,
        profile,
        raw_json,
        nav,
    };
    render_template(&tmpl)
}
