use axum::extract::{Path, Query};
use axum::response::IntoResponse;

use crate::queries;
use crate::queries::types::Pagination;

use super::ApiError;
use crate::extractors::ReadPool;

/// GET /api/0/projects/{project_id}/events/?limit=&offset=
pub async fn list_for_project(
    ReadPool(pool): ReadPool,
    Path(project_id): Path<u64>,
    Query(params): Query<Pagination>,
) -> Result<impl IntoResponse, ApiError> {
    // TODO: enforce project/org scoping; no per-user project ownership model yet.
    let page = params.page();
    let events = queries::events::list_events(&pool, project_id, &page)
        .await
        .map_err(ApiError::internal)?;
    Ok(axum::Json(events))
}

/// GET /api/0/issues/{fingerprint}/events/?limit=&offset=
pub async fn list_for_issue(
    ReadPool(pool): ReadPool,
    Path(fingerprint): Path<String>,
    Query(params): Query<Pagination>,
) -> Result<impl IntoResponse, ApiError> {
    // TODO: enforce project/org scoping; no ownership model yet.
    let page = params.page();
    let events = queries::events::list_events_for_issue(&pool, &fingerprint, &page)
        .await
        .map_err(ApiError::internal)?;
    Ok(axum::Json(events))
}

/// GET /api/0/issues/{fingerprint}/events/latest/
pub async fn latest_for_issue(
    ReadPool(pool): ReadPool,
    Path(fingerprint): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    // TODO: enforce project/org scoping; no ownership model yet.
    let event = queries::events::get_latest_event_for_issue(&pool, &fingerprint)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("no events found for issue"))?;
    Ok(axum::Json(event))
}

/// GET /api/0/events/{event_id}/
pub async fn get(
    ReadPool(pool): ReadPool,
    Path(event_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    // TODO: enforce project/org scoping; no ownership model yet.
    let event = queries::events::get_event_detail(&pool, &event_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("event not found"))?;
    Ok(axum::Json(event))
}
