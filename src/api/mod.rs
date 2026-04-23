use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde_json::json;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::db::DbPool;
use crate::server::AppState;

pub mod alerts;
pub mod events;
pub mod issues;
pub mod projects;
pub mod releases;
pub mod sourcemaps;

/// Sentry-compatible API routes (releases, sourcemaps) that authenticate
/// via per-project API keys. Served on the ingest port since sentry-cli
/// sends requests to the DSN host (SENTRY_URL).
pub fn sentry_api_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/0/projects/{org}/{project_slug}/",
            get(projects::sentry_get),
        )
        .route(
            "/api/0/projects/{org}/{project_slug}",
            get(projects::sentry_get),
        )
        .route(
            "/api/0/organizations/{org}/releases/",
            post(releases::create),
        )
        .route(
            "/api/0/organizations/{org}/releases",
            post(releases::create),
        )
        .route(
            "/api/0/projects/{org}/{project_slug}/releases/",
            post(releases::create_project_scoped),
        )
        .route(
            "/api/0/projects/{org}/{project_slug}/releases",
            post(releases::create_project_scoped),
        )
        .route(
            "/api/0/organizations/{org}/releases/{version}/",
            put(releases::update),
        )
        .route(
            "/api/0/organizations/{org}/releases/{version}",
            put(releases::update),
        )
        .route(
            "/api/0/projects/{org}/{project_slug}/releases/{version}/",
            put(releases::update_project_scoped),
        )
        .route(
            "/api/0/projects/{org}/{project_slug}/releases/{version}",
            put(releases::update_project_scoped),
        )
        .route(
            "/api/0/organizations/{org}/chunk-upload/",
            get(sourcemaps::chunk_upload_config).post(sourcemaps::chunk_upload),
        )
        .route(
            "/api/0/organizations/{org}/chunk-upload",
            get(sourcemaps::chunk_upload_config).post(sourcemaps::chunk_upload),
        )
        .route(
            "/api/0/organizations/{org}/artifactbundle/assemble/",
            post(sourcemaps::assemble),
        )
        .route(
            "/api/0/organizations/{org}/artifactbundle/assemble",
            post(sourcemaps::assemble),
        )
}

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

/// Validate an API key from the Authorization header.
/// Returns the associated project_id on success, or a 401 response.
pub async fn validate_api_key(
    pool: &DbPool,
    headers: &HeaderMap,
    scope: &str,
) -> Result<u64, (StatusCode, Json<serde_json::Value>)> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    let token = match token {
        Some(t) => t,
        None => {
            return Err(api_error(
                StatusCode::UNAUTHORIZED,
                "authentication required",
            ))
        }
    };

    // Hash the presented token once; keep the raw 32-byte digest so every
    // code path compares the same number of bytes.
    let hash_bytes = Sha256::digest(token.as_bytes());
    let hash_hex = hex::encode(hash_bytes);

    // Compute the dummy digest unconditionally so hit, miss, and DB-error
    // paths all pay the same CPU cost. Without this the Err(_) arm would
    // skip the ct_eq and leak a third timing profile.
    let dummy = Sha256::digest(b"stackpit-dummy-api-key-for-timing");

    let row = crate::queries::api_keys::get_api_key_by_hash(pool, &hash_hex).await;

    match row {
        Ok(Some(info)) => {
            // The SQL WHERE already narrowed to an exact match, but run a
            // constant-time compare on the raw 32-byte digest to match the
            // CPU profile of the miss/error paths below.
            let matches: bool = hash_bytes.as_slice().ct_eq(hash_bytes.as_slice()).into();
            if matches && info.scope == scope {
                Ok(info.project_id)
            } else {
                Err(api_error(StatusCode::UNAUTHORIZED, "invalid API key"))
            }
        }
        Ok(None) => {
            // DB miss: burn the same compare cycles so the response time
            // doesn't leak "key prefix matches nothing" vs "key exists but
            // scope wrong".
            let _equal: bool = hash_bytes.as_slice().ct_eq(dummy.as_slice()).into();
            Err(api_error(StatusCode::UNAUTHORIZED, "invalid API key"))
        }
        Err(_) => {
            // DB error: still burn the compare cycles so timing is
            // indistinguishable from hit/miss.
            let _equal: bool = hash_bytes.as_slice().ct_eq(dummy.as_slice()).into();
            Err(api_error(StatusCode::UNAUTHORIZED, "invalid API key"))
        }
    }
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
