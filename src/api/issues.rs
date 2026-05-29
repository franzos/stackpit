use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::queries;
use crate::queries::types::{IssueFilter, Page};
use crate::queries::IssueStatus;
use crate::server::AppState;

use super::{internal_error, json_error, json_or_404, json_or_500};
use crate::extractors::ReadPool;

#[derive(Deserialize)]
pub struct ListParams {
    pub status: Option<String>,
    pub level: Option<String>,
    pub query: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
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
) -> impl IntoResponse {
    // TODO: enforce project/org scoping here before multi-user ships -- AuthContext
    // carries no per-user project ownership today, so any authed caller sees any project.
    let filter = IssueFilter {
        level: params.level,
        status: params.status,
        query: params.query,
        sort: None,
        item_type: None,
        release: None,
        tag: None,
    };
    let page = Page::new(params.offset, params.limit);
    json_or_500(queries::issues::list_issues(&pool, project_id, &filter, &page, None).await)
}

/// GET /api/0/issues/{fingerprint}/
pub async fn get(ReadPool(pool): ReadPool, Path(fingerprint): Path<String>) -> impl IntoResponse {
    // TODO: enforce project/org scoping here before multi-user ships -- no ownership model yet.
    json_or_404(
        queries::issues::get_issue(&pool, &fingerprint).await,
        "issue not found",
    )
}

/// PUT /api/0/issues/{fingerprint}/ with body {"status": "resolved"|"unresolved"|"ignored"}
pub async fn update_status(
    State(state): State<AppState>,
    Path(fingerprint): Path<String>,
    Json(body): Json<UpdateBody>,
) -> impl IntoResponse {
    // TODO: enforce project/org scoping here before multi-user ships -- no ownership model yet.
    match queries::issues::update_issue_status(&state.writer_pool, &fingerprint, body.status).await
    {
        Ok(0) => json_error(StatusCode::NOT_FOUND, "issue not found"),
        Ok(_) => json_or_404(
            queries::issues::get_issue(&state.pool, &fingerprint).await,
            "issue not found",
        ),
        Err(err) => internal_error(err).into_response(),
    }
}
