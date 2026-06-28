use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::domain::IssueStatus;
use crate::queries;
use crate::queries::types::{IssueFilter, Pagination};
use crate::server::AppState;

use super::ApiError;
use crate::extractors::ReadPool;

#[derive(Deserialize)]
pub struct ListParams {
    pub status: Option<String>,
    pub level: Option<String>,
    pub query: Option<String>,
    #[serde(flatten)]
    pub page: Pagination,
}

#[derive(Deserialize)]
pub struct UpdateBody {
    pub status: IssueStatus,
}

/// GET /api/0/projects/{project_id}/issues/?status=&level=&query=&limit=&offset=
pub async fn list_for_project(
    ReadPool(pool): ReadPool,
    Path(project_id): Path<u64>,
    Query(params): Query<ListParams>,
) -> Result<impl IntoResponse, ApiError> {
    // TODO: enforce project/org scoping; no per-user project ownership model yet.
    let filter = IssueFilter {
        level: params.level,
        status: params.status,
        query: params.query,
        sort: None,
        item_type: None,
        release: None,
        tag: None,
    };
    let page = params.page.page();
    let issues = queries::issues::list_issues(&pool, project_id, &filter, &page, None)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(issues))
}

/// GET /api/0/issues/{fingerprint}/
pub async fn get(
    ReadPool(pool): ReadPool,
    Path(fingerprint): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    // TODO: enforce project/org scoping; no ownership model yet.
    let issue = queries::issues::get_issue(&pool, &fingerprint)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("issue not found"))?;
    Ok(Json(issue))
}

/// PUT /api/0/issues/{fingerprint}/ with body {"status": "resolved"|"unresolved"|"ignored"}
pub async fn update_status(
    State(state): State<AppState>,
    Path(fingerprint): Path<String>,
    Json(body): Json<UpdateBody>,
) -> Result<impl IntoResponse, ApiError> {
    // TODO: enforce project/org scoping; no ownership model yet.
    let affected =
        queries::issues::update_issue_status(&state.writer_pool, &fingerprint, body.status)
            .await
            .map_err(ApiError::internal)?;
    if affected == 0 {
        return Err(ApiError::not_found("issue not found"));
    }
    let issue = queries::issues::get_issue(&state.pool, &fingerprint)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("issue not found"))?;
    Ok(Json(issue))
}
