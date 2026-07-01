use std::collections::HashMap;

use axum::extract::{FromRequestParts, Path};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::db::DbPool;
use crate::html::utils::{self, Csrf};
use crate::orgs::extractor::ActiveOrg;
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

/// None = extension absent (fail closed), Ok(None) = superuser bypass, Ok(Some) = must check.
fn org_gate(active: Option<&ActiveOrg>) -> Result<Option<i64>, ()> {
    match active {
        None => Err(()),
        Some(a) if a.role.is_none() => Ok(None),
        Some(a) => Ok(Some(a.org_id)),
    }
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
        // Enforce org scope before nav to avoid leaking counts for foreign projects.
        match org_gate(parts.extensions.get::<ActiveOrg>()) {
            Err(()) => return Err(StatusCode::NOT_FOUND.into_response()),
            Ok(Some(org_id)) => {
                crate::queries::orgs::assert_project_in_org(&pool, project_id as i64, org_id)
                    .await
                    .map_err(|_| StatusCode::NOT_FOUND.into_response())?;
            }
            Ok(None) => {}
        }
        let nav = crate::queries::projects::get_nav_counts(&pool, project_id).await;
        Ok(ProjectPageCtx {
            pool,
            project_id,
            nav,
            csrf_token,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orgs::Role;
    use crate::queries::orgs::assert_project_in_org;

    fn make_active(org_id: i64, role: Option<Role>) -> ActiveOrg {
        ActiveOrg { org_id, role }
    }

    #[test]
    fn org_gate_missing_extension_denies() {
        assert!(org_gate(None).is_err());
    }

    #[test]
    fn org_gate_superuser_bypasses() {
        let a = make_active(1, None);
        assert_eq!(org_gate(Some(&a)), Ok(None));
    }

    #[test]
    fn org_gate_scoped_user_returns_org_id() {
        let a = make_active(42, Some(Role::Member));
        assert_eq!(org_gate(Some(&a)), Ok(Some(42)));
        let b = make_active(7, Some(Role::Owner));
        assert_eq!(org_gate(Some(&b)), Ok(Some(7)));
    }

    #[tokio::test]
    async fn org_gate_plus_assert_denies_foreign_org() {
        use crate::db::sql;
        use sqlx::Row;

        let pool = crate::db::open_test_pool().await;

        // Insert org A and org B.
        sqlx::query(sql!("INSERT INTO organizations (slug, name) VALUES (?1, 'Org A')"))
            .bind("extractor-org-a")
            .execute(&pool)
            .await
            .unwrap();
        let org_a: i64 = sqlx::query(sql!("SELECT org_id FROM organizations WHERE slug = ?1"))
            .bind("extractor-org-a")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("org_id");

        sqlx::query(sql!("INSERT INTO organizations (slug, name) VALUES (?1, 'Org B')"))
            .bind("extractor-org-b")
            .execute(&pool)
            .await
            .unwrap();
        let org_b: i64 = sqlx::query(sql!("SELECT org_id FROM organizations WHERE slug = ?1"))
            .bind("extractor-org-b")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("org_id");

        // Project belongs to org A.
        sqlx::query(sql!("INSERT INTO projects (project_id, org_id) VALUES (?1, ?2)"))
            .bind(9001i64)
            .bind(org_a)
            .execute(&pool)
            .await
            .unwrap();

        // Caller is a member of org B (foreign).
        let caller = make_active(org_b, Some(Role::Member));
        let needed_org = org_gate(Some(&caller)).unwrap().unwrap();
        // The DB check must deny.
        assert!(assert_project_in_org(&pool, 9001, needed_org).await.is_err());

        // Same caller in org A must be allowed.
        let caller_a = make_active(org_a, Some(Role::Member));
        let needed_org_a = org_gate(Some(&caller_a)).unwrap().unwrap();
        assert!(assert_project_in_org(&pool, 9001, needed_org_a).await.is_ok());

        // Superuser always bypasses (org_gate returns Ok(None) regardless of project's org).
        let superuser = make_active(org_b, None);
        assert_eq!(org_gate(Some(&superuser)), Ok(None));
    }
}
