use std::collections::HashMap;

use axum::extract::FromRequestParts;

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
        let map = crate::middleware::cookie::read_cookie(&parts.headers, utils::DEFAULTS_COOKIE)
            .map(utils::parse_defaults_cookie)
            .unwrap_or_default();
        Ok(BrowserDefaults(map))
    }
}

/// Clones the read pool from state. Infallible -- used by both HTML and API handlers.
pub struct ReadPool(pub DbPool);

impl FromRequestParts<AppState> for ReadPool {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(ReadPool(state.pool.clone()))
    }
}
