use std::collections::HashMap;

use axum::extract::{FromRequestParts, Path};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::db::DbPool;
use crate::html::chrome::PageChrome;
use crate::html::utils::{self, Chrome};
use crate::orgs::extractor::{require_project_scope, ActiveOrg};
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

/// `{project_id}` path extractor for HTML routes. Renders the styled 404 page on
/// a malformed (non-numeric) id instead of leaking axum's raw path rejection.
pub struct ProjectPath(pub u64);

impl FromRequestParts<AppState> for ProjectPath {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        match Path::<u64>::from_request_parts(parts, state).await {
            Ok(Path(id)) => Ok(ProjectPath(id)),
            Err(_) => Err(crate::html::html_not_found()),
        }
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
    pub chrome: PageChrome,
}

impl FromRequestParts<AppState> for ProjectPageCtx {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Path(project_id) = Path::<u64>::from_request_parts(parts, state)
            .await
            .map_err(|_| crate::html::html_not_found())?;
        // Chrome extraction is infallible (CSRF falls back to empty, locale to en).
        let chrome = Chrome::from_request_parts(parts, state)
            .await
            .map(|c| c.0)
            .unwrap_or_else(|_| {
                PageChrome::new(
                    String::new(),
                    crate::locale::default_locale(),
                    "/web/projects/".to_string(),
                )
            });
        let pool = state.pool.clone();
        // Enforce scope before nav to avoid leaking counts for projects the caller
        // cannot reach. Absent extension means the auth middleware did not run: fail closed.
        let Some(active) = parts.extensions.get::<ActiveOrg>().cloned() else {
            tracing::error!("ActiveOrg extension missing; route mounted outside web_auth");
            return Err(StatusCode::NOT_FOUND.into_response());
        };
        require_project_scope(&active, &pool, project_id as i64).await?;
        let nav = state.nav_counts(project_id).await;
        Ok(ProjectPageCtx {
            pool,
            project_id,
            nav,
            chrome,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::queries::orgs::assert_project_in_org;

    // `ProjectPageCtx` now gates through `require_project_scope`, whose semantics are
    // covered in `orgs::extractor`. What remains here is the one org-pinned check that
    // survived: alert rules may only target projects in their own org.
    #[tokio::test]
    async fn assert_project_in_org_pins_a_project_to_one_org() {
        use crate::db::sql;
        use sqlx::Row;

        let pool = crate::db::open_test_pool().await;

        let mut orgs = Vec::new();
        for suffix in ["a", "b"] {
            let slug = format!("extractor-org-{suffix}");
            sqlx::query(sql!(
                "INSERT INTO organizations (slug, name) VALUES (?1, ?2)"
            ))
            .bind(&slug)
            .bind(format!("Org {suffix}"))
            .execute(&pool)
            .await
            .unwrap();
            orgs.push(
                sqlx::query(sql!("SELECT org_id FROM organizations WHERE slug = ?1"))
                    .bind(&slug)
                    .fetch_one(&pool)
                    .await
                    .unwrap()
                    .get::<i64, _>("org_id"),
            );
        }

        sqlx::query(sql!(
            "INSERT INTO projects (project_id, org_id) VALUES (?1, ?2)"
        ))
        .bind(9001i64)
        .bind(orgs[0])
        .execute(&pool)
        .await
        .unwrap();

        assert!(assert_project_in_org(&pool, 9001, orgs[0]).await.is_ok());
        assert!(assert_project_in_org(&pool, 9001, orgs[1]).await.is_err());
        assert!(assert_project_in_org(&pool, 99999, orgs[0]).await.is_err());
    }
}
