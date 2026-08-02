use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::orgs::extractor::ActiveOrg;
use crate::queries;
use crate::server::AppState;

use super::ApiError;
use crate::extractors::ReadPool;

/// GET /api/0/projects/
pub async fn list(
    State(state): State<AppState>,
    ReadPool(pool): ReadPool,
    active_org: ActiveOrg,
) -> Result<impl IntoResponse, ApiError> {
    // Cross-org, matching Sentry's equivalent endpoint: "projects available to the
    // authenticated session", not the projects of one active org.
    let projects = queries::projects::list_projects_cached(
        &pool,
        &state.project_list_cache,
        queries::projects::scope_for(&active_org),
        None,
        None,
        None,
    )
    .await
    .map_err(ApiError::internal)?;
    Ok(Json(projects))
}

/// GET /api/0/projects/{org}/{project_id}/ (sentry-cli validation endpoint).
pub async fn sentry_get(
    State(state): State<AppState>,
    Path((_org, project_slug)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let key_project_id = super::validate_api_key(&state.pool, &headers, "sourcemap").await?;

    let project_id: u64 = project_slug
        .parse()
        .map_err(|_| ApiError::not_found("project not found"))?;

    if project_id != key_project_id {
        return Err(ApiError::not_found("project not found"));
    }

    let info = queries::projects::get_project_info(&state.pool, project_id)
        .await
        .map_err(ApiError::internal)?;

    match info {
        Some(info) => Ok(Json(json!({
            "id": project_id.to_string(),
            "slug": project_slug,
            "name": info.name.unwrap_or_else(|| format!("Project {project_id}")),
            "status": info.status.as_str(),
        }))),
        None => Err(ApiError::not_found("project not found")),
    }
}
