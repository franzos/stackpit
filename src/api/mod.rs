use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde_json::json;

use crate::server::AppState;

pub mod alerts;
pub mod events;
pub mod issues;
pub mod projects;
pub mod releases;
pub mod sourcemaps;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/projects/", get(projects::list))
        .route(
            "/api/v1/projects/{project_id}/issues/",
            get(issues::list_for_project),
        )
        .route(
            "/api/v1/projects/{project_id}/events/",
            get(events::list_for_project),
        )
        .route(
            "/api/v1/issues/{fingerprint}/",
            get(issues::get).put(issues::update_status),
        )
        .route(
            "/api/v1/issues/{fingerprint}/events/",
            get(events::list_for_issue),
        )
        .route(
            "/api/v1/issues/{fingerprint}/events/latest/",
            get(events::latest_for_issue),
        )
        .route("/api/v1/events/{event_id}/", get(events::get))
        // Sentry-compatible release API
        .route(
            "/api/0/organizations/{org}/releases/",
            post(releases::create),
        )
        .route(
            "/api/0/organizations/{org}/releases/{version}/",
            put(releases::update),
        )
        // Sentry-compatible sourcemap / artifact bundle API
        .route(
            "/api/0/organizations/{org}/chunk-upload/",
            get(sourcemaps::chunk_upload_config).post(sourcemaps::chunk_upload),
        )
        .route(
            "/api/0/organizations/{org}/artifactbundle/assemble/",
            post(sourcemaps::assemble),
        )
        // Alert rules
        .route(
            "/api/v1/alerts/rules",
            get(alerts::list_rules).post(alerts::create_rule),
        )
        .route(
            "/api/v1/alerts/rules/{id}",
            put(alerts::update_rule).delete(alerts::delete_rule),
        )
        // Digest schedules
        .route(
            "/api/v1/digests",
            get(alerts::list_digests).post(alerts::create_digest),
        )
        .route(
            "/api/v1/digests/{id}",
            put(alerts::update_digest).delete(alerts::delete_digest),
        )
}

pub fn api_error(status: StatusCode, detail: &str) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(json!({ "detail": detail })))
}

/// Return a generic 500 error to the client while logging the real cause.
pub fn internal_error(e: impl std::fmt::Display) -> (StatusCode, Json<serde_json::Value>) {
    tracing::error!("API internal error: {e}");
    api_error(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
}

/// Convert a query result into a JSON response, mapping errors to 500.
pub fn json_or_500<T: serde::Serialize>(result: anyhow::Result<T>) -> axum::response::Response {
    use axum::response::IntoResponse;
    match result {
        Ok(value) => Json(value).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}

/// Convert a query result that returns `Option<T>` into a JSON response.
/// Returns 404 with the provided message if `None`.
pub fn json_or_404<T: serde::Serialize>(
    result: anyhow::Result<Option<T>>,
    not_found_msg: &str,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    match result {
        Ok(Some(value)) => Json(value).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, not_found_msg).into_response(),
        Err(e) => internal_error(e).into_response(),
    }
}
