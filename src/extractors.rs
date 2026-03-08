use axum::extract::FromRequestParts;
use axum::http::StatusCode;

use crate::db::DbPool;
use crate::server::AppState;

/// Axum extractor for HTML handlers -- clones the read pool from state.
pub struct ReadPool(pub DbPool);

impl FromRequestParts<AppState> for ReadPool {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(ReadPool(state.pool.clone()))
    }
}

/// Same thing but for API routes -- just returns a status code on error, no HTML.
pub struct ApiReadPool(pub DbPool);

impl FromRequestParts<AppState> for ApiReadPool {
    type Rejection = StatusCode;

    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(ApiReadPool(state.pool.clone()))
    }
}
