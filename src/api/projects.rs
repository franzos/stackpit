use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::queries;
use crate::server::AppState;

use super::json_or_500;
use crate::extractors::ApiReadPool;

/// GET /api/0/projects/
pub async fn list(ApiReadPool(pool): ApiReadPool) -> impl IntoResponse {
    json_or_500(queries::projects::list_projects(&pool, None, None, None).await)
}

/// GET /api/0/projects/{org}/{project_id}/
///
/// Minimal project detail endpoint that sentry-cli calls to validate
/// a project before creating releases or uploading sourcemaps.
pub async fn sentry_get(
    State(state): State<AppState>,
    Path((_org, project_slug)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let key_project_id = super::validate_api_key(&state.pool, &headers, "sourcemap").await?;

    let project_id: u64 = project_slug
        .parse()
        .map_err(|_| super::api_error(StatusCode::NOT_FOUND, "project not found"))?;

    if project_id != key_project_id {
        return Err(super::api_error(StatusCode::NOT_FOUND, "project not found"));
    }

    let info = queries::projects::get_project_info(&state.pool, project_id)
        .await
        .map_err(super::internal_error)?;

    match info {
        Some(info) => Ok(Json(json!({
            "id": project_id.to_string(),
            "slug": project_slug,
            "name": info.name.unwrap_or_else(|| format!("Project {project_id}")),
            "status": info.status.as_str(),
        }))),
        None => Err(super::api_error(StatusCode::NOT_FOUND, "project not found")),
    }
}
