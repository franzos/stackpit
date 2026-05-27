use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::server::AppState;

use super::{api_error, internal_error};
use crate::extractors::ReadPool;

#[derive(Deserialize)]
pub struct CreateReleaseRequest {
    pub version: String,
    /// sentry-cli sends slugs as strings and numeric IDs as integers,
    /// so we accept arbitrary JSON values and parse them ourselves.
    #[serde(default)]
    pub projects: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
pub struct UpdateReleaseRequest {
    #[serde(default)]
    pub refs: Vec<CommitRef>,
    #[serde(default, rename = "dateReleased")]
    pub date_released: Option<String>,
}

#[derive(Deserialize)]
pub struct CommitRef {
    #[serde(default)]
    #[allow(dead_code)]
    pub repository: String,
    #[serde(default)]
    pub commit: String,
}

/// POST /api/0/projects/{org}/{project_slug}/releases/
pub async fn create_project_scoped(
    State(state): State<AppState>,
    Path((_org, _project)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<CreateReleaseRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    create_inner(state, headers, body).await
}

/// POST /api/0/organizations/{org}/releases/
pub async fn create(
    State(state): State<AppState>,
    Path(_org): Path<String>,
    headers: HeaderMap,
    Json(body): Json<CreateReleaseRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    create_inner(state, headers, body).await
}

async fn create_inner(
    state: AppState,
    headers: HeaderMap,
    body: CreateReleaseRequest,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let key_project_id = super::validate_api_key(&state.pool, &headers, "sourcemap").await?;
    if body.version.is_empty() {
        return Err(api_error(StatusCode::BAD_REQUEST, "version is required"));
    }
    if body.projects.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "at least one project is required",
        ));
    }

    // Create release for each project in the list (must match the key's project)
    for project_val in &body.projects {
        let project_id: u64 = project_val
            .as_u64()
            .or_else(|| project_val.as_str().and_then(|s| s.parse().ok()))
            .ok_or_else(|| {
                api_error(
                    StatusCode::BAD_REQUEST,
                    &format!("invalid project id: {project_val}"),
                )
            })?;

        if project_id != key_project_id {
            return Err(api_error(
                StatusCode::FORBIDDEN,
                "API key not valid for this project",
            ));
        }

        let info = crate::queries::releases::ReleaseUpsert {
            version: &body.version,
            commit_sha: None,
            date_released: None,
            first_event: None,
            last_event: None,
            new_groups: 0,
        };
        crate::queries::releases::upsert_release(&state.writer_pool, project_id, &info)
            .await
            .map_err(internal_error)?;
    }

    let now = chrono::Utc::now().to_rfc3339();
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "version": body.version,
            "dateCreated": now,
        })),
    ))
}

/// PUT /api/0/projects/{org}/{project_slug}/releases/{version}/
pub async fn update_project_scoped(
    State(state): State<AppState>,
    ReadPool(pool): ReadPool,
    Path((_org, _project, version)): Path<(String, String, String)>,
    headers: HeaderMap,
    Json(body): Json<UpdateReleaseRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    update_inner(state, pool, version, headers, body).await
}

/// PUT /api/0/organizations/{org}/releases/{version}/
pub async fn update(
    State(state): State<AppState>,
    ReadPool(pool): ReadPool,
    Path((_org, version)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<UpdateReleaseRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    update_inner(state, pool, version, headers, body).await
}

async fn update_inner(
    state: AppState,
    pool: crate::db::DbPool,
    version: String,
    headers: HeaderMap,
    body: UpdateReleaseRequest,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let key_project_id = super::validate_api_key(&state.pool, &headers, "sourcemap").await?;
    // Process commit refs — take the first ref's commit as the release commit
    if let Some(ref_info) = body.refs.first() {
        if !ref_info.commit.is_empty() {
            // Only update the release for the key's project
            let project_ids = crate::queries::releases::find_projects_by_version(&pool, &version)
                .await
                .map_err(internal_error)?;

            for project_id in project_ids.into_iter().filter(|&id| id == key_project_id) {
                let info = crate::queries::releases::ReleaseUpsert {
                    version: &version,
                    commit_sha: Some(&ref_info.commit),
                    date_released: None,
                    first_event: None,
                    last_event: None,
                    new_groups: 0,
                };
                crate::queries::releases::upsert_release(&state.writer_pool, project_id, &info)
                    .await
                    .map_err(internal_error)?;
            }
        }
    }

    let now = chrono::Utc::now().to_rfc3339();
    let date_released = body.date_released.as_deref().unwrap_or(&now);
    Ok(Json(json!({
        "version": version,
        "dateCreated": now,
        "dateReleased": date_released,
    })))
}
