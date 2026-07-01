use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;

use crate::orgs::Role;
use crate::server::AppState;
use crate::util::crypto::SecretEncryptor;

/// Returns 403 when the caller is a plain member; owners and superusers (role=None) pass.
pub fn require_owner(active: &ActiveOrg) -> Result<(), Response> {
    match active.role {
        Some(Role::Member) => Err(StatusCode::FORBIDDEN.into_response()),
        _ => Ok(()),
    }
}

/// Returns 403 for any caller with a role (member or owner); only superusers (role=None) pass.
pub fn require_superuser(active: &ActiveOrg) -> Result<(), Response> {
    if active.role.is_some() {
        Err(StatusCode::FORBIDDEN.into_response())
    } else {
        Ok(())
    }
}

/// Superuser (role None) bypasses; otherwise the project must belong to the active org.
pub async fn require_project_scope(
    active: &ActiveOrg,
    pool: &crate::db::DbPool,
    project_id: i64,
) -> Result<(), Response> {
    if active.role.is_none() {
        return Ok(());
    }
    crate::queries::orgs::assert_project_in_org(pool, project_id, active.org_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND.into_response())
}

pub const ACTIVE_ORG_COOKIE: &str = "sp_active_org";

const AAD: &[u8] = b"stackpit:active-org:v1";

pub fn pack(enc: &SecretEncryptor, org_id: i64) -> Option<String> {
    let ct = enc.encrypt_bytes_with_aad(org_id.to_string().as_bytes(), AAD)?;
    Some(URL_SAFE_NO_PAD.encode(ct))
}

pub fn unpack(enc: &SecretEncryptor, blob_b64: &str) -> Option<i64> {
    let ct = URL_SAFE_NO_PAD.decode(blob_b64.trim()).ok()?;
    let pt = enc.decrypt_bytes_with_aad(&ct, AAD)?;
    std::str::from_utf8(&pt).ok()?.parse().ok()
}

/// Return cookie's org if still a member, else fall back to the personal org.
pub fn resolve_active_org(cookie_org: Option<i64>, memberships: &[i64], personal_org_id: i64) -> i64 {
    match cookie_org {
        Some(id) if memberships.contains(&id) => id,
        _ => personal_org_id,
    }
}

/// Active org for the current request; injected by auth middleware, never computed per-handler.
#[derive(Clone, Debug)]
pub struct ActiveOrg {
    pub org_id: i64,
    // None means admin/superuser path (no org-scoped role).
    pub role: Option<Role>,
}

impl FromRequestParts<AppState> for ActiveOrg {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        parts.extensions.get::<ActiveOrg>().cloned().ok_or_else(|| {
            tracing::error!("ActiveOrg extension missing; auth middleware bug");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orgs::Role;

    #[test]
    fn resolve_falls_back_to_personal_when_not_member() {
        let memberships = vec![10i64, 11];
        assert_eq!(resolve_active_org(Some(99), &memberships, 10), 10); // 99 not a member -> personal
        assert_eq!(resolve_active_org(Some(11), &memberships, 10), 11); // valid -> kept
        assert_eq!(resolve_active_org(None, &memberships, 10), 10); // none -> personal
    }

    #[test]
    fn require_owner_blocks_members() {
        let member = ActiveOrg { org_id: 1, role: Some(Role::Member) };
        assert!(require_owner(&member).is_err());
    }

    #[test]
    fn require_owner_allows_owner() {
        let owner = ActiveOrg { org_id: 1, role: Some(Role::Owner) };
        assert!(require_owner(&owner).is_ok());
    }

    #[test]
    fn require_owner_allows_superuser() {
        let superuser = ActiveOrg { org_id: 1, role: None };
        assert!(require_owner(&superuser).is_ok());
    }

    #[test]
    fn require_superuser_blocks_member() {
        let member = ActiveOrg { org_id: 1, role: Some(Role::Member) };
        assert!(require_superuser(&member).is_err());
    }

    #[test]
    fn require_superuser_blocks_owner() {
        let owner = ActiveOrg { org_id: 1, role: Some(Role::Owner) };
        assert!(require_superuser(&owner).is_err());
    }

    #[test]
    fn require_superuser_allows_superuser() {
        let su = ActiveOrg { org_id: 1, role: None };
        assert!(require_superuser(&su).is_ok());
    }

    #[tokio::test]
    async fn require_project_scope_superuser_bypasses() {
        let pool = crate::db::open_test_pool().await;
        let su = ActiveOrg { org_id: 999, role: None };
        assert!(require_project_scope(&su, &pool, 99999).await.is_ok());
    }

    #[tokio::test]
    async fn require_project_scope_scoped_deny_foreign_org() {
        use crate::db::sql;
        use sqlx::Row;

        let pool = crate::db::open_test_pool().await;

        sqlx::query(sql!("INSERT INTO organizations (slug, name) VALUES (?1, 'Scope A')"))
            .bind("pscope-org-a")
            .execute(&pool)
            .await
            .unwrap();
        let org_a: i64 = sqlx::query(sql!("SELECT org_id FROM organizations WHERE slug = ?1"))
            .bind("pscope-org-a")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("org_id");

        sqlx::query(sql!("INSERT INTO organizations (slug, name) VALUES (?1, 'Scope B')"))
            .bind("pscope-org-b")
            .execute(&pool)
            .await
            .unwrap();
        let org_b: i64 = sqlx::query(sql!("SELECT org_id FROM organizations WHERE slug = ?1"))
            .bind("pscope-org-b")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("org_id");

        sqlx::query(sql!("INSERT INTO projects (project_id, org_id) VALUES (?1, ?2)"))
            .bind(8001i64)
            .bind(org_a)
            .execute(&pool)
            .await
            .unwrap();

        let member_a = ActiveOrg { org_id: org_a, role: Some(Role::Member) };
        let member_b = ActiveOrg { org_id: org_b, role: Some(Role::Member) };

        assert!(require_project_scope(&member_a, &pool, 8001).await.is_ok());
        assert!(require_project_scope(&member_b, &pool, 8001).await.is_err());
        assert!(require_project_scope(&member_b, &pool, 99999).await.is_err());
    }
}
