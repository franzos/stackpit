use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;

use crate::extractors::ReadPool;
use crate::html::render_template;
use crate::queries;
use crate::queries::types::ReleaseHealth;
use crate::queries::ProjectNavCounts;
use crate::server::AppState;

use super::html_error;

// askama needs these filters in scope for template derivation
#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "release_health.html")]
struct ReleaseHealthTemplate {
    project_id: u64,
    releases: Vec<ReleaseHealth>,
    nav: ProjectNavCounts,
}

pub async fn handler(
    State(_state): State<AppState>,
    ReadPool(pool): ReadPool,
    Path(project_id): Path<u64>,
) -> axum::response::Response {
    let releases = match queries::releases::get_release_health(&pool, project_id).await {
        Ok(r) => r,
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let nav = queries::projects::get_nav_counts(&pool, project_id).await;

    let tmpl = ReleaseHealthTemplate {
        project_id,
        releases,
        nav,
    };
    render_template(&tmpl)
}
