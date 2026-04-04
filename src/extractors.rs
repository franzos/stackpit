use std::collections::HashMap;

use axum::extract::FromRequestParts;
use axum::http::StatusCode;

use crate::db::DbPool;
use crate::html::utils;
use crate::server::AppState;

/// Extracts browser defaults from the `sp_defaults` cookie. Never rejects.
pub struct BrowserDefaults(pub HashMap<String, String>);

impl FromRequestParts<AppState> for BrowserDefaults {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let map =
            crate::middleware::cookie::extract_cookie_value(&parts.headers, utils::DEFAULTS_COOKIE)
                .map(|v| utils::parse_defaults_cookie(&v))
                .unwrap_or_default();
        Ok(BrowserDefaults(map))
    }
}

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
