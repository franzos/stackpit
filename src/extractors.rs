use std::collections::HashMap;

use axum::extract::{FromRequestParts, Path};
use axum::response::{IntoResponse, Response};

use crate::db::DbPool;
use crate::html::utils::{self, Csrf};
use crate::queries::ProjectNavCounts;
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

/// Clones the read pool from state. Infallible; used by HTML and API handlers.
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

/// Shared preamble for per-project list/detail HTML pages: resolves the
/// `{project_id}` path param, clones the read pool, pulls the CSRF token, and
/// loads the nav badge counts. Migrate handlers whose preamble matches this
/// exact shape; those needing tuple paths or a pre-nav early return keep their
/// own extraction.
pub struct ProjectPageCtx {
    pub pool: DbPool,
    pub project_id: u64,
    pub nav: ProjectNavCounts,
    pub csrf_token: String,
}

impl FromRequestParts<AppState> for ProjectPageCtx {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Path(project_id) = Path::<u64>::from_request_parts(parts, state)
            .await
            .map_err(IntoResponse::into_response)?;
        // Csrf extraction is infallible (falls back to empty for no-auth paths).
        let csrf_token = Csrf::from_request_parts(parts, state)
            .await
            .map(|c| c.0)
            .unwrap_or_default();
        let pool = state.pool.clone();
        let nav = crate::queries::projects::get_nav_counts(&pool, project_id).await;
        Ok(ProjectPageCtx {
            pool,
            project_id,
            nav,
            csrf_token,
        })
    }
}
