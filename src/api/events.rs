use axum::extract::{Path, Query};
use axum::response::IntoResponse;
use serde::Deserialize;

use crate::queries;
use crate::queries::types::Page;

use super::{json_or_404, json_or_500};
use crate::extractors::ApiReadPool;

#[derive(Deserialize)]
pub struct PageParams {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

/// GET /api/0/projects/{project_id}/events/?limit=&offset=
pub async fn list_for_project(
    ApiReadPool(pool): ApiReadPool,
    Path(project_id): Path<u64>,
    Query(params): Query<PageParams>,
) -> impl IntoResponse {
    let page = Page::new(params.offset, params.limit);
    json_or_500(queries::events::list_events(&pool, project_id, &page).await)
}

/// GET /api/0/issues/{fingerprint}/events/?limit=&offset=
pub async fn list_for_issue(
    ApiReadPool(pool): ApiReadPool,
    Path(fingerprint): Path<String>,
    Query(params): Query<PageParams>,
) -> impl IntoResponse {
    let page = Page::new(params.offset, params.limit);
    json_or_500(queries::events::list_events_for_issue(&pool, &fingerprint, &page).await)
}

/// GET /api/0/issues/{fingerprint}/events/latest/
pub async fn latest_for_issue(
    ApiReadPool(pool): ApiReadPool,
    Path(fingerprint): Path<String>,
) -> impl IntoResponse {
    json_or_404(
        queries::events::get_latest_event_for_issue(&pool, &fingerprint).await,
        "no events found for issue",
    )
}

/// GET /api/0/events/{event_id}/
pub async fn get(
    ApiReadPool(pool): ApiReadPool,
    Path(event_id): Path<String>,
) -> impl IntoResponse {
    json_or_404(
        queries::events::get_event_detail(&pool, &event_id).await,
        "event not found",
    )
}
