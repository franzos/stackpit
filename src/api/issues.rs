use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::queries;
use crate::queries::types::{IssueFilter, Page};
use crate::queries::IssueStatus;
use crate::server::AppState;

use super::{api_error, internal_error, json_or_404, json_or_500};
use crate::extractors::ApiReadPool;

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
    ApiReadPool(pool): ApiReadPool,
    Path(project_id): Path<u64>,
    Query(params): Query<ListParams>,
) -> impl IntoResponse {
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
pub async fn get(
    ApiReadPool(pool): ApiReadPool,
    Path(fingerprint): Path<String>,
) -> impl IntoResponse {
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
    let reply_rx = match state
        .writer
        .update_issue_status(fingerprint.clone(), body.status)
    {
        Ok(rx) => rx,
        Err(e) => {
            tracing::error!("failed to send to writer: {e}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
                .into_response();
        }
    };

    match reply_rx.await {
        Ok(Ok(())) => json_or_404(
            queries::issues::get_issue(&state.pool, &fingerprint).await,
            "issue not found",
        ),
        Ok(Err(err)) => {
            if err.is_not_found() {
                api_error(StatusCode::NOT_FOUND, "issue not found").into_response()
            } else {
                internal_error(err).into_response()
            }
        }
        Err(_) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "writer channel closed unexpectedly",
        )
        .into_response(),
    }
}
