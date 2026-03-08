use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::server::AppState;

use super::{api_error, internal_error};
use crate::extractors::ApiReadPool;

#[derive(Deserialize)]
pub struct CreateReleaseRequest {
    pub version: String,
    #[serde(default)]
    pub projects: Vec<String>,
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

/// POST /api/0/organizations/{org}/releases/
pub async fn create(
    State(state): State<AppState>,
    Path(_org): Path<String>,
    Json(body): Json<CreateReleaseRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    if body.version.is_empty() {
        return Err(api_error(StatusCode::BAD_REQUEST, "version is required"));
    }
    if body.projects.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "at least one project is required",
        ));
    }

    // Create release for each project in the list
    for project_str in &body.projects {
        let project_id: u64 = project_str.parse().map_err(|_| {
            api_error(
                StatusCode::BAD_REQUEST,
                &format!("invalid project id: {project_str}"),
            )
        })?;

        let rx = state
            .writer
            .upsert_release(project_id, body.version.clone(), None)
            .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "writer unavailable"))?;

        rx.await
            .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "writer dropped reply"))?
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

/// PUT /api/0/organizations/{org}/releases/{version}/
pub async fn update(
    State(state): State<AppState>,
    ApiReadPool(pool): ApiReadPool,
    Path((_org, version)): Path<(String, String)>,
    Json(body): Json<UpdateReleaseRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // Process commit refs — take the first ref's commit as the release commit
    if let Some(ref_info) = body.refs.first() {
        if !ref_info.commit.is_empty() {
            // We need the project_id, but this endpoint doesn't include it directly.
            // Look up the release across all projects to find which one matches.

            let project_ids = crate::queries::releases::find_projects_by_version(&pool, &version)
                .await
                .map_err(internal_error)?;

            for project_id in project_ids {
                let rx = state
                    .writer
                    .upsert_release(project_id, version.clone(), Some(ref_info.commit.clone()))
                    .map_err(|_| {
                        api_error(StatusCode::INTERNAL_SERVER_ERROR, "writer unavailable")
                    })?;

                rx.await
                    .map_err(|_| {
                        api_error(StatusCode::INTERNAL_SERVER_ERROR, "writer dropped reply")
                    })?
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
