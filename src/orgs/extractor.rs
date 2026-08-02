use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;

use crate::orgs::{Role, SYSTEM_ORG_ID};
use crate::server::AppState;
use crate::util::crypto::SecretEncryptor;

/// Scope of a project-scoped request: the org that actually owns the project plus
/// the caller's role *in that org*. Produced only by [`require_project_scope`].
///
/// This is not interchangeable with [`ActiveOrg`]: the two differ whenever a member
/// opens a project belonging to another of their orgs, which is the normal case once
/// the project list spans orgs. Project-scoped handlers must key their queries on
/// `ProjectScope::org_id`, never on `ActiveOrg::session_org_id`.
#[derive(Clone, Debug)]
pub struct ProjectScope {
    pub org_id: i64,
    /// None on the admin-token and loopback paths (no org-scoped role).
    pub role: Option<Role>,
}

/// Returns 403 when the caller is a plain member of the project's org; owners and
/// superusers pass. Takes [`ProjectScope`] rather than [`ActiveOrg`] so the role
/// checked is always the one in the org that owns the project.
#[allow(clippy::result_large_err)]
pub fn require_owner(scope: &ProjectScope) -> Result<(), Response> {
    match scope.role {
        Some(Role::Member) => Err(StatusCode::FORBIDDEN.into_response()),
        _ => Ok(()),
    }
}

/// [`require_owner`] for org-scoped pages that have no project in play, where the
/// session's own org is the subject of the request.
#[allow(clippy::result_large_err)]
pub fn require_org_owner(active: &ActiveOrg) -> Result<(), Response> {
    match active.role {
        Some(Role::Member) => Err(StatusCode::FORBIDDEN.into_response()),
        _ => Ok(()),
    }
}

/// Returns 403 for any caller with a role (member or owner); only superusers (role=None) pass.
#[allow(clippy::result_large_err)]
pub fn require_superuser(active: &ActiveOrg) -> Result<(), Response> {
    if active.role.is_some() {
        Err(StatusCode::FORBIDDEN.into_response())
    } else {
        Ok(())
    }
}

/// Resolve a project to the org that owns it and authorize the caller against *that*
/// org, not the session's. Superusers get the org id without a membership check.
///
/// Fails closed as 404 (never 403) so a probe cannot distinguish "exists elsewhere"
/// from "does not exist".
pub async fn require_project_scope(
    active: &ActiveOrg,
    pool: &crate::db::DbPool,
    project_id: i64,
) -> Result<ProjectScope, Response> {
    let not_found = || StatusCode::NOT_FOUND.into_response();
    // A read-pool outage would otherwise turn every project page into a bare 404 with
    // nothing in the logs pointing at the database.
    let org_id = crate::queries::orgs::org_of_project(pool, project_id)
        .await
        .inspect_err(|e| tracing::error!("org_of_project({project_id}) failed: {e:#}"))
        .ok()
        .flatten()
        .ok_or_else(not_found)?;

    if active.role.is_none() {
        return Ok(ProjectScope { org_id, role: None });
    }

    // Auto-provisioned projects default into the system org. A membership row for it
    // must not grant access: `can_switch_to` already makes org 1 unreachable as an
    // active org, and resolving scope from the project would otherwise reopen it.
    if org_id == SYSTEM_ORG_ID {
        return Err(not_found());
    }

    let role = active
        .memberships
        .iter()
        .find(|(id, _)| *id == org_id)
        .map(|(_, r)| *r)
        .ok_or_else(not_found)?;

    Ok(ProjectScope {
        org_id,
        role: Some(role),
    })
}

/// [`require_project_scope`] followed by [`require_owner`]: the standard preamble for
/// project-scoped mutations. Returns the scope so handlers can key writes on the
/// owning org.
pub async fn require_project_owner(
    active: &ActiveOrg,
    pool: &crate::db::DbPool,
    project_id: i64,
) -> Result<ProjectScope, Response> {
    let scope = require_project_scope(active, pool, project_id).await?;
    require_owner(&scope)?;
    Ok(scope)
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
pub fn resolve_active_org(
    cookie_org: Option<i64>,
    memberships: &[i64],
    personal_org_id: i64,
) -> i64 {
    match cookie_org {
        Some(id) if memberships.contains(&id) => id,
        _ => personal_org_id,
    }
}

/// Active org for the current request; injected by auth middleware, never computed per-handler.
#[derive(Clone, Debug)]
pub struct ActiveOrg {
    /// The org this browser session is pointed at. Correct for org-scoped pages
    /// (alerts, integrations, "new project"); wrong for anything resolved from a
    /// project id, which must use [`ProjectScope::org_id`].
    pub session_org_id: i64,
    // None means admin/superuser path (no org-scoped role).
    pub role: Option<Role>,
    /// Display label for the chrome scope indicator. None on the admin-token and
    /// loopback paths, which resolve an org id without loading memberships.
    pub org_name: Option<String>,
    /// Every org the caller belongs to, with the role held in each. Loaded once by
    /// the auth middleware; the authority for cross-org project access.
    pub memberships: Vec<(i64, Role)>,
}

impl ActiveOrg {
    /// Id and role only, for the bootstrap paths that never load memberships.
    pub fn bare(session_org_id: i64, role: Option<Role>) -> Self {
        Self {
            session_org_id,
            role,
            org_name: None,
            memberships: Vec::new(),
        }
    }

    /// Test/bootstrap constructor for a caller with known memberships.
    #[cfg(test)]
    pub fn with_memberships(
        session_org_id: i64,
        role: Option<Role>,
        memberships: Vec<(i64, Role)>,
    ) -> Self {
        Self {
            session_org_id,
            role,
            org_name: None,
            memberships,
        }
    }
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

    fn scope(role: Option<Role>) -> ProjectScope {
        ProjectScope { org_id: 1, role }
    }

    #[test]
    fn require_owner_blocks_members() {
        assert!(require_owner(&scope(Some(Role::Member))).is_err());
        assert!(require_org_owner(&ActiveOrg::bare(1, Some(Role::Member))).is_err());
    }

    #[test]
    fn require_owner_allows_owner() {
        assert!(require_owner(&scope(Some(Role::Owner))).is_ok());
        assert!(require_org_owner(&ActiveOrg::bare(1, Some(Role::Owner))).is_ok());
    }

    #[test]
    fn require_owner_allows_superuser() {
        assert!(require_owner(&scope(None)).is_ok());
        assert!(require_org_owner(&ActiveOrg::bare(1, None)).is_ok());
    }

    #[test]
    fn require_superuser_blocks_member() {
        let member = ActiveOrg::bare(1, Some(Role::Member));
        assert!(require_superuser(&member).is_err());
    }

    #[test]
    fn require_superuser_blocks_owner() {
        let owner = ActiveOrg::bare(1, Some(Role::Owner));
        assert!(require_superuser(&owner).is_err());
    }

    #[test]
    fn require_superuser_allows_superuser() {
        let su = ActiveOrg::bare(1, None);
        assert!(require_superuser(&su).is_ok());
    }

    /// Two orgs plus one project in the first, returned as `(org_a, org_b, project_id)`.
    async fn two_org_fixture(pool: &crate::db::DbPool, tag: &str, project_id: i64) -> (i64, i64) {
        use crate::db::sql;
        use sqlx::Row;

        let mut ids = Vec::new();
        for suffix in ["a", "b"] {
            let slug = format!("{tag}-org-{suffix}");
            sqlx::query(sql!(
                "INSERT INTO organizations (slug, name) VALUES (?1, ?2)"
            ))
            .bind(&slug)
            .bind(format!("Scope {suffix}"))
            .execute(pool)
            .await
            .unwrap();
            let id: i64 = sqlx::query(sql!("SELECT org_id FROM organizations WHERE slug = ?1"))
                .bind(&slug)
                .fetch_one(pool)
                .await
                .unwrap()
                .get("org_id");
            ids.push(id);
        }

        sqlx::query(sql!(
            "INSERT INTO projects (project_id, org_id) VALUES (?1, ?2)"
        ))
        .bind(project_id)
        .bind(ids[0])
        .execute(pool)
        .await
        .unwrap();

        (ids[0], ids[1])
    }

    #[tokio::test]
    async fn superuser_gets_the_projects_real_org() {
        let pool = crate::db::open_test_pool().await;
        let (org_a, _) = two_org_fixture(&pool, "su", 8000).await;

        // Session org is irrelevant for a superuser, but the scope still resolves
        // to the org that actually owns the project.
        let su = ActiveOrg::bare(999, None);
        let scope = require_project_scope(&su, &pool, 8000).await.unwrap();
        assert_eq!(scope.org_id, org_a);
        assert!(scope.role.is_none());

        // An unknown project is a 404 even for a superuser: there is no org to resolve.
        assert!(require_project_scope(&su, &pool, 99999).await.is_err());
    }

    // The core of resource-derived scope: access follows membership in the
    // project's own org, not whichever org the session cookie happens to hold.
    #[tokio::test]
    async fn membership_in_the_projects_org_grants_access_regardless_of_session() {
        let pool = crate::db::open_test_pool().await;
        let (org_a, org_b) = two_org_fixture(&pool, "cross", 8001).await;

        // Session points at B; the project lives in A; the caller belongs to both.
        let member_of_both = ActiveOrg::with_memberships(
            org_b,
            Some(Role::Member),
            vec![(org_a, Role::Member), (org_b, Role::Member)],
        );
        let scope = require_project_scope(&member_of_both, &pool, 8001)
            .await
            .unwrap();
        assert_eq!(scope.org_id, org_a);
        assert_eq!(scope.role, Some(Role::Member));
    }

    #[tokio::test]
    async fn non_member_is_denied_and_unknown_project_is_denied() {
        let pool = crate::db::open_test_pool().await;
        let (_, org_b) = two_org_fixture(&pool, "deny", 8002).await;

        let outsider =
            ActiveOrg::with_memberships(org_b, Some(Role::Member), vec![(org_b, Role::Member)]);
        assert!(require_project_scope(&outsider, &pool, 8002).await.is_err());
        assert!(require_project_scope(&outsider, &pool, 99999)
            .await
            .is_err());
    }

    // The privilege-escalation case the ProjectScope split exists to prevent:
    // owning org B must not confer owner rights over a project in org A.
    #[tokio::test]
    async fn owner_in_one_org_is_only_a_member_in_another() {
        let pool = crate::db::open_test_pool().await;
        let (org_a, org_b) = two_org_fixture(&pool, "role", 8003).await;

        let owner_of_b = ActiveOrg::with_memberships(
            org_b,
            Some(Role::Owner),
            vec![(org_a, Role::Member), (org_b, Role::Owner)],
        );
        let scope = require_project_scope(&owner_of_b, &pool, 8003)
            .await
            .unwrap();
        assert_eq!(scope.role, Some(Role::Member));
        assert!(
            require_owner(&scope).is_err(),
            "owning another org must not grant owner rights over this project"
        );
        // Guards the regression directly: the session-org role would have passed.
        assert!(require_org_owner(&owner_of_b).is_ok());
    }

    // Auto-provisioned projects default into org 1. `can_switch_to` makes it
    // unreachable as an active org; resolving scope from the project must not
    // reopen it just because a membership row exists.
    #[tokio::test]
    async fn system_org_is_denied_even_with_a_membership_row() {
        use crate::db::sql;

        let pool = crate::db::open_test_pool().await;
        sqlx::query(sql!(
            "INSERT INTO projects (project_id, org_id) VALUES (?1, ?2)"
        ))
        .bind(8004i64)
        .bind(SYSTEM_ORG_ID)
        .execute(&pool)
        .await
        .unwrap();

        let member = ActiveOrg::with_memberships(
            SYSTEM_ORG_ID,
            Some(Role::Member),
            vec![(SYSTEM_ORG_ID, Role::Owner)],
        );
        assert!(require_project_scope(&member, &pool, 8004).await.is_err());

        // Superusers still reach it; that is the only path to unassigned projects.
        let su = ActiveOrg::bare(SYSTEM_ORG_ID, None);
        assert!(require_project_scope(&su, &pool, 8004).await.is_ok());
    }

    #[tokio::test]
    async fn empty_memberships_deny_every_project() {
        let pool = crate::db::open_test_pool().await;
        two_org_fixture(&pool, "empty", 8005).await;

        let no_orgs = ActiveOrg::with_memberships(42, Some(Role::Member), Vec::new());
        assert!(require_project_scope(&no_orgs, &pool, 8005).await.is_err());
    }
}
