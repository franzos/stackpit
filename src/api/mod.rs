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

/// Sentry-compatible API routes (releases, sourcemaps) with per-project API key auth.
/// Registered without trailing slashes; `NormalizePathLayer` (see `server.rs`) trims
/// the trailing slash Sentry SDKs send so both forms match.
pub fn sentry_api_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/0/projects/{org}/{project_slug}",
            get(projects::sentry_get),
        )
        .route(
            "/api/0/organizations/{org}/releases",
            post(releases::create),
        )
        .route(
            "/api/0/projects/{org}/{project_slug}/releases",
            post(releases::create_project_scoped),
        )
        .route(
            "/api/0/organizations/{org}/releases/{version}",
            put(releases::update),
        )
        .route(
            "/api/0/projects/{org}/{project_slug}/releases/{version}",
            put(releases::update_project_scoped),
        )
        .route(
            "/api/0/organizations/{org}/chunk-upload",
            get(sourcemaps::chunk_upload_config).post(sourcemaps::chunk_upload),
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
        .route(
            "/api/v1/alerts/rules",
            get(alerts::list_rules).post(alerts::create_rule),
        )
        .route(
            "/api/v1/alerts/rules/{id}",
            put(alerts::update_rule).delete(alerts::delete_rule),
        )
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
/// Returns the associated project_id on success, or a 401 error.
pub async fn validate_api_key(
    pool: &DbPool,
    headers: &HeaderMap,
    scope: &str,
) -> Result<u64, ApiError> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        // RFC 7235: the auth scheme is case-insensitive ("Bearer"/"bearer"/...).
        .and_then(|s| {
            let (scheme, token) = s.split_at_checked(7)?;
            scheme.eq_ignore_ascii_case("Bearer ").then_some(token)
        })
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    let token = match token {
        Some(t) => t,
        None => {
            return Err(ApiError::new(
                StatusCode::UNAUTHORIZED,
                "authentication required",
            ))
        }
    };

    // Hash the presented token once; keep the raw 32-byte digest so every
    // code path compares the same number of bytes.
    let hash_bytes = Sha256::digest(token.as_bytes());
    let hash_hex = hex::encode(hash_bytes);

    // Equalizes only the in-process compare across hit/miss/error so the error
    // arm doesn't present a third timing profile. Does NOT mask the DB lookup,
    // which dominates and isn't constant-time; keys are 256-bit `sk_` randoms,
    // so a compare-timing oracle isn't practically exploitable regardless.
    let dummy = Sha256::digest(b"stackpit-dummy-api-key-for-timing");

    let row = crate::queries::api_keys::get_api_key_by_hash(pool, &hash_hex, scope).await;

    match row {
        Ok(Some(info)) => {
            // SQL `WHERE key_hash = ? AND scope = ?` already narrowed to the
            // unique credential; no Rust-side recheck needed.
            Ok(info.project_id)
        }
        Ok(None) => {
            // Wrong hash or wrong scope: the SQL filter rejects both. Match the
            // hit path's compare cost.
            let _equal: bool = hash_bytes.as_slice().ct_eq(dummy.as_slice()).into();
            Err(ApiError::new(StatusCode::UNAUTHORIZED, "invalid API key"))
        }
        Err(_) => {
            let _equal: bool = hash_bytes.as_slice().ct_eq(dummy.as_slice()).into();
            Err(ApiError::new(StatusCode::UNAUTHORIZED, "invalid API key"))
        }
    }
}

/// JSON API error. Serializes to the canonical `{"detail": ...}` body with a
/// status code and implements `IntoResponse`, so handlers return
/// `Result<_, ApiError>` and use `?`.
pub struct ApiError {
    status: StatusCode,
    detail: String,
}

impl ApiError {
    pub fn new(status: StatusCode, detail: impl Into<String>) -> Self {
        Self {
            status,
            detail: detail.into(),
        }
    }

    pub fn not_found(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, detail)
    }

    /// Generic 500 to the client; logs the real cause.
    pub fn internal(e: impl std::fmt::Display) -> Self {
        tracing::error!("API internal error: {e}");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    }
}

impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.status, Json(json!({ "detail": self.detail }))).into_response()
    }
}
