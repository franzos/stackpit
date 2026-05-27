use askama::Template;
use axum::extract::{Path, State};

use crate::extractors::ReadPool;
use crate::html::render_template;
use crate::html::utils::Csrf;
use crate::queries;
use crate::queries::types::ReleaseHealth;
use crate::queries::ProjectNavCounts;
use crate::server::AppState;

use super::HtmlError;

// askama needs these filters in scope for template derivation
#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "release_health.html")]
struct ReleaseHealthTemplate {
    project_id: u64,
    releases: Vec<ReleaseHealth>,
    nav: ProjectNavCounts,
    csrf_token: String,
}

pub async fn handler(
    State(_state): State<AppState>,
    ReadPool(pool): ReadPool,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
) -> Result<axum::response::Response, HtmlError> {
    let releases = queries::releases::get_release_health(&pool, project_id).await?;

    let nav = queries::projects::get_nav_counts(&pool, project_id).await;

    let tmpl = ReleaseHealthTemplate {
        project_id,
        releases,
        nav,
        csrf_token: csrf,
    };
    Ok(render_template(&tmpl))
}
