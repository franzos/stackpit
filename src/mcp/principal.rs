//! MCP identity and the single authorization choke point for tools.
//!
//! Scopes are a ceiling, never a grant: effective permission is the token's
//! scopes intersected with the caller's role in the org that owns the target.
//! An MCP principal always carries a role, so it can never be mistaken for the
//! superuser (`role: None`) that `require_project_scope` lets past membership.

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use lru::LruCache;
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use stackpit_auth::axum_ext::mcp::render_rejection;
use stackpit_auth::{AuthContext, BearerAuthOutcome, BearerGate, GrantedScopes};

use crate::db::DbPool;
use crate::oidc::client::UserinfoError;
use crate::orgs::extractor::{
    require_owner, require_project_scope, role_in_org, ActiveOrg, ProjectScope,
};
use crate::orgs::{Role, SYSTEM_ORG_ID};

use super::McpState;

/// How long a resolved principal is reused before the IdP is asked again.
/// Bounds how long a Forseti demotion can lag behind on the MCP surface.
const PRINCIPAL_TTL: Duration = Duration::from_secs(60);
const PRINCIPAL_CACHE_CAP: usize = 512;

/// The caller behind an MCP request: the Stackpit user, the OAuth client that
/// presented the token, the token's scopes, and the memberships reconciled from
/// the IdP on this request.
#[derive(Debug, Clone)]
pub struct McpPrincipal {
    pub iss: String,
    pub sub: String,
    pub user_id: i64,
    /// OAuth client that presented the token. Absent when the credential names
    /// no client. The join key for any future per-client project grant.
    pub client_id: Option<String>,
    pub scopes: GrantedScopes,
    memberships: Vec<(i64, Role)>,
}

impl McpPrincipal {
    /// The orgs this principal may read from. The one place a future per-client
    /// grant filter applies; tools must not read memberships directly.
    pub fn accessible_org_ids(&self) -> Vec<i64> {
        self.memberships
            .iter()
            .map(|(id, _)| *id)
            .filter(|id| *id != SYSTEM_ORG_ID)
            .collect()
    }

    /// Never `role: None`: `require_project_scope` reads that as superuser.
    fn active_org(&self) -> ActiveOrg {
        ActiveOrg::from_memberships(self.memberships.clone())
    }

    #[cfg(test)]
    pub(crate) fn for_test(scopes: &str, memberships: Vec<(i64, Role)>) -> Self {
        Self {
            iss: "https://idp.test".to_string(),
            sub: "alice".to_string(),
            user_id: 1,
            client_id: Some("mcp-client".to_string()),
            scopes: GrantedScopes::parse(Some(scopes)),
            memberships,
        }
    }
}

/// Why a principal could not be resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrincipalError {
    /// The IdP refused the access token: it is dead or revoked. The only
    /// recoverable case, and the only one the client is told about.
    TokenRejected,
    /// Fail closed without inviting a re-authorization loop.
    Unavailable,
    /// The admin-token break-glass authenticates the transport but names no
    /// user, so it has no orgs and cannot run tools.
    NoUserIdentity,
}

impl PrincipalError {
    pub fn message(self) -> &'static str {
        match self {
            PrincipalError::TokenRejected => "the authorization server rejected the access token",
            PrincipalError::Unavailable => "identity could not be resolved",
            PrincipalError::NoUserIdentity => {
                "this credential carries no user identity; tools need an OAuth access token"
            }
        }
    }
}

struct CacheEntry {
    principal: Arc<McpPrincipal>,
    stored_at: Instant,
}

/// Resolved principals keyed by SHA-256 of the presented token.
pub struct PrincipalCache {
    entries: Mutex<LruCache<[u8; 32], CacheEntry>>,
}

impl PrincipalCache {
    pub fn new() -> Self {
        let cap = NonZeroUsize::new(PRINCIPAL_CACHE_CAP).expect("PRINCIPAL_CACHE_CAP is non-zero");
        Self {
            entries: Mutex::new(LruCache::new(cap)),
        }
    }

    fn get(&self, key: &[u8; 32]) -> Option<Arc<McpPrincipal>> {
        let mut entries = self.entries.lock();
        match entries.get(key) {
            Some(entry) if entry.stored_at.elapsed() < PRINCIPAL_TTL => {
                Some(entry.principal.clone())
            }
            Some(_) => {
                entries.pop(key);
                None
            }
            None => None,
        }
    }

    fn put(&self, key: [u8; 32], principal: Arc<McpPrincipal>) {
        self.entries.lock().put(
            key,
            CacheEntry {
                principal,
                stored_at: Instant::now(),
            },
        );
    }
}

impl Default for PrincipalCache {
    fn default() -> Self {
        Self::new()
    }
}

impl McpState {
    /// Resolve the caller: user row, then the IdP's current `orgs` claim through
    /// [`reconcile`](crate::orgs::reconcile::reconcile), then memberships.
    ///
    /// The userinfo round-trip is what makes an IdP-side demotion reach MCP at
    /// all: a bearer caller never runs the browser callback that reconciles
    /// orgs. It is cached on the token hash for [`PRINCIPAL_TTL`], and a
    /// userinfo failure denies rather than falling back to stored memberships.
    pub async fn principal(
        &self,
        token: &str,
        ctx: &AuthContext,
        scopes: &GrantedScopes,
        client_id: Option<&str>,
    ) -> Result<Arc<McpPrincipal>, PrincipalError> {
        let AuthContext::User { iss, sub, .. } = ctx else {
            return Err(PrincipalError::NoUserIdentity);
        };

        let key = hash_token(token);
        if let Some(cache) = self.runtime.as_ref().map(|rt| &rt.principals) {
            if let Some(hit) = cache.get(&key) {
                return Ok(hit);
            }
        }

        let Some(oidc) = self.oidc.as_deref() else {
            tracing::error!("mcp principal: no OIDC client configured");
            return Err(PrincipalError::Unavailable);
        };

        let claim = oidc.fetch_userinfo_orgs(token).await.map_err(|e| {
            tracing::warn!(error = ?e, sub = %sub, "mcp principal: userinfo failed; failing closed");
            match e {
                UserinfoError::TokenRejected => PrincipalError::TokenRejected,
                UserinfoError::Unavailable | UserinfoError::NotConfigured => {
                    PrincipalError::Unavailable
                }
            }
        })?;

        let user = crate::queries::users::find_by_iss_sub(&self.auth_pool, iss, sub)
            .await
            .map_err(|e| {
                tracing::error!(error = %format!("{e:#}"), "mcp principal: user lookup failed");
                PrincipalError::Unavailable
            })?
            .ok_or_else(|| {
                tracing::warn!(sub = %sub, "mcp principal: no user row after provisioning");
                PrincipalError::Unavailable
            })?;

        crate::orgs::reconcile::reconcile(
            &self.auth_pool,
            crate::orgs::reconcile::ReconcileInput {
                user_id: user.user_id,
                iss,
                orgs: claim.orgs.as_deref(),
                orgs_truncated: claim.truncated,
            },
        )
        .await
        .map_err(|e| {
            tracing::error!(error = %format!("{e:#}"), "mcp principal: org reconcile failed");
            PrincipalError::Unavailable
        })?;

        let memberships = crate::queries::orgs::list_memberships(&self.auth_pool, user.user_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %format!("{e:#}"), "mcp principal: membership load failed");
                PrincipalError::Unavailable
            })?
            .into_iter()
            .map(|m| (m.org_id, Role::parse(&m.role)))
            .collect();

        let principal = Arc::new(McpPrincipal {
            iss: iss.clone(),
            sub: sub.clone(),
            user_id: user.user_id,
            client_id: client_id.map(str::to_string),
            scopes: scopes.clone(),
            memberships,
        });

        if let Some(cache) = self.runtime.as_ref().map(|rt| &rt.principals) {
            cache.put(key, principal.clone());
        }
        Ok(principal)
    }
}

fn hash_token(token: &str) -> [u8; 32] {
    let digest = Sha256::digest(token.as_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

// Authorization

/// What a tool needs before it runs, declared once per tool in the tool table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolPermission {
    /// Scope the token must carry. Empty means authentication only.
    pub scope: &'static str,
    /// Requires owner rank in the org that owns the target.
    pub owner_only: bool,
}

/// The resource a call names, taken from its parsed arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// No org- or project-scoped resource (`whoami`, `list_projects`).
    None,
    Project(i64),
    Org(i64),
}

/// Why a tool call was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Denied {
    /// Must surface as HTTP 403 with an RFC 6750 challenge naming the scope;
    /// that header is the client's only cue to ask for it.
    Scope {
        required: &'static str,
    },
    /// Also covers "exists, but in an org you are not in": a probe must not be
    /// able to tell the two apart.
    NotFound,
    Forbidden,
    Unavailable,
}

/// [`Denied::Scope`] is the only refusal that leaves the JSON-RPC envelope. The
/// challenge header is the client's only cue to go and ask for the scope, so it
/// must not be buried in a JSON-RPC error object.
pub fn scope_step_up(gate: &BearerGate, required: &str) -> Response {
    render_rejection(
        gate,
        BearerAuthOutcome::InsufficientScope {
            required: required.to_string(),
        },
    )
    .unwrap_or_else(|| StatusCode::FORBIDDEN.into_response())
}

/// The single authorization choke point for MCP tools: tools never call
/// [`require_project_scope`] or [`require_owner`] themselves, so narrowing MCP
/// access later (per-client project grants) is a change to this function alone.
///
/// Returns `Some` exactly when `target` names a resource, carrying the org that
/// owns it and the caller's role there.
pub async fn authorize_tool(
    principal: &McpPrincipal,
    pool: &DbPool,
    permission: ToolPermission,
    target: Target,
) -> Result<Option<ProjectScope>, Denied> {
    if !permission.scope.is_empty() && !principal.scopes.has(permission.scope) {
        return Err(Denied::Scope {
            required: permission.scope,
        });
    }

    let scope = match target {
        Target::None => None,
        Target::Project(project_id) => Some(
            require_project_scope(&principal.active_org(), pool, project_id)
                .await
                .map_err(|_| Denied::NotFound)?,
        ),
        Target::Org(org_id) => Some(ProjectScope {
            org_id,
            role: Some(role_in_org(&principal.memberships, org_id).ok_or(Denied::NotFound)?),
        }),
    };

    if permission.owner_only {
        // An owner-only tool with no target is a wiring bug; fail closed.
        let scope = scope.as_ref().ok_or(Denied::Forbidden)?;
        require_owner(scope).map_err(|_| Denied::Forbidden)?;
    }

    Ok(scope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::sql;
    use sqlx::Row;

    const READ: ToolPermission = ToolPermission {
        scope: "stackpit:events:read",
        owner_only: false,
    };
    const WRITE_OWNER: ToolPermission = ToolPermission {
        scope: "stackpit:projects:write",
        owner_only: true,
    };

    async fn seed_org(pool: &DbPool, slug: &str) -> i64 {
        sqlx::query(sql!(
            "INSERT INTO organizations (slug, name) VALUES (?1, ?2)"
        ))
        .bind(slug)
        .bind(slug)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(sql!("SELECT org_id FROM organizations WHERE slug = ?1"))
            .bind(slug)
            .fetch_one(pool)
            .await
            .unwrap()
            .get("org_id")
    }

    async fn seed_project(pool: &DbPool, project_id: i64, org_id: i64) {
        sqlx::query(sql!(
            "INSERT INTO projects (project_id, org_id) VALUES (?1, ?2)"
        ))
        .bind(project_id)
        .bind(org_id)
        .execute(pool)
        .await
        .unwrap();
    }

    fn principal(scopes: &str, memberships: Vec<(i64, Role)>) -> McpPrincipal {
        McpPrincipal::for_test(scopes, memberships)
    }

    // The whole point of moving the gate off the router: a token holding only a
    // write scope must still reach a write tool.
    #[tokio::test]
    async fn scope_is_checked_per_tool_not_per_route() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "authz-per-tool").await;
        seed_project(&pool, 9100, org).await;

        let writer = principal("stackpit:projects:write", vec![(org, Role::Owner)]);
        assert!(
            authorize_tool(&writer, &pool, WRITE_OWNER, Target::Project(9100))
                .await
                .is_ok(),
            "a write-only token must reach a write tool"
        );
        assert_eq!(
            authorize_tool(&writer, &pool, READ, Target::Project(9100)).await,
            Err(Denied::Scope {
                required: "stackpit:events:read"
            }),
        );
    }

    // Scopes are a ceiling, not a grant: admin scope never promotes a member.
    #[tokio::test]
    async fn admin_scope_does_not_grant_owner_actions() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "authz-admin-member").await;
        seed_project(&pool, 9101, org).await;

        let member = principal(
            "stackpit:admin stackpit:projects:write",
            vec![(org, Role::Member)],
        );
        assert_eq!(
            authorize_tool(&member, &pool, WRITE_OWNER, Target::Project(9101)).await,
            Err(Denied::Forbidden),
        );
        // The same token still reaches owner tools in an org it owns.
        let owner = principal(
            "stackpit:admin stackpit:projects:write",
            vec![(org, Role::Owner)],
        );
        assert!(
            authorize_tool(&owner, &pool, WRITE_OWNER, Target::Project(9101))
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn a_project_in_a_foreign_org_is_not_found() {
        let pool = crate::db::open_test_pool().await;
        let mine = seed_org(&pool, "authz-mine").await;
        let theirs = seed_org(&pool, "authz-theirs").await;
        seed_project(&pool, 9102, theirs).await;

        let outsider = principal("stackpit:events:read", vec![(mine, Role::Owner)]);
        assert_eq!(
            authorize_tool(&outsider, &pool, READ, Target::Project(9102)).await,
            Err(Denied::NotFound),
        );
        // Same answer for the org-scoped path, so neither leaks existence.
        assert_eq!(
            authorize_tool(&outsider, &pool, READ, Target::Org(theirs)).await,
            Err(Denied::NotFound),
        );
    }

    #[tokio::test]
    async fn zero_memberships_reach_nothing() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "authz-empty").await;
        seed_project(&pool, 9103, org).await;

        let orphan = principal("stackpit:events:read", Vec::new());
        assert_eq!(
            authorize_tool(&orphan, &pool, READ, Target::Project(9103)).await,
            Err(Denied::NotFound),
        );
        assert_eq!(
            authorize_tool(&orphan, &pool, READ, Target::Org(org)).await,
            Err(Denied::NotFound),
        );
        assert!(orphan.accessible_org_ids().is_empty());
    }

    // `require_project_scope` reads `role: None` as superuser and hands back the
    // system org. Asserted here, at that function, so a later refactor of the
    // principal cannot quietly reintroduce it.
    #[tokio::test]
    async fn principal_never_reaches_the_system_org() {
        let pool = crate::db::open_test_pool().await;
        seed_project(&pool, 9104, SYSTEM_ORG_ID).await;

        let system_member = principal("stackpit:events:read", vec![(SYSTEM_ORG_ID, Role::Owner)]);
        assert_eq!(system_member.active_org().role, Some(Role::Member));
        assert!(
            require_project_scope(&system_member.active_org(), &pool, 9104)
                .await
                .is_err(),
            "a membership row for the system org must not open its projects"
        );
        assert_eq!(
            authorize_tool(&system_member, &pool, READ, Target::Project(9104)).await,
            Err(Denied::NotFound),
        );
        assert_eq!(
            authorize_tool(&system_member, &pool, READ, Target::Org(SYSTEM_ORG_ID)).await,
            Err(Denied::NotFound),
        );
        assert!(system_member.accessible_org_ids().is_empty());
    }

    #[tokio::test]
    async fn org_target_resolves_the_role_of_the_argument_org() {
        let pool = crate::db::open_test_pool().await;
        let owned = seed_org(&pool, "authz-owned").await;
        let joined = seed_org(&pool, "authz-joined").await;

        let mixed = principal(
            "stackpit:admin",
            vec![(owned, Role::Owner), (joined, Role::Member)],
        );
        let permission = ToolPermission {
            scope: "stackpit:admin",
            owner_only: true,
        };
        let scope = authorize_tool(&mixed, &pool, permission, Target::Org(owned))
            .await
            .unwrap()
            .expect("org target yields a scope");
        assert_eq!(scope.org_id, owned);
        assert_eq!(scope.role, Some(Role::Owner));
        assert_eq!(
            authorize_tool(&mixed, &pool, permission, Target::Org(joined)).await,
            Err(Denied::Forbidden),
            "owning one org must not confer owner rights in another",
        );
    }

    #[tokio::test]
    async fn a_targetless_tool_needs_only_its_scope() {
        let pool = crate::db::open_test_pool().await;
        let anyone = principal("", Vec::new());
        let base = ToolPermission {
            scope: "",
            owner_only: false,
        };
        assert_eq!(
            authorize_tool(&anyone, &pool, base, Target::None).await,
            Ok(None)
        );
        assert_eq!(
            authorize_tool(&anyone, &pool, READ, Target::None).await,
            Err(Denied::Scope {
                required: "stackpit:events:read"
            }),
        );
    }

    // The admin token authenticates the transport but names no user, so it has
    // no memberships. Refusing it here is what keeps "MCP principal" and
    // "superuser" from ever being the same thing.
    #[tokio::test]
    async fn the_admin_token_resolves_to_no_principal() {
        let state = crate::mcp::test_support::state().await;
        let err = state
            .principal(
                "admin-token",
                &AuthContext::Admin,
                &GrantedScopes::default(),
                None,
            )
            .await
            .expect_err("admin token must not produce a principal");
        assert_eq!(err, PrincipalError::NoUserIdentity);
    }

    // The step-up signal: a client that cannot read this header has no way to
    // learn which scope to ask for.
    #[test]
    fn a_scope_refusal_is_a_403_naming_the_scope() {
        let resp = scope_step_up(&crate::mcp::test_support::gate(), "stackpit:admin");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let www = resp
            .headers()
            .get(axum::http::header::WWW_AUTHENTICATE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(www.contains("error=\"insufficient_scope\""), "got {www}");
        assert!(www.contains("scope=\"stackpit:admin\""), "got {www}");
        assert!(
            www.contains(crate::mcp::test_support::TEST_METADATA_URL),
            "got {www}"
        );
    }

    #[test]
    fn cache_entries_expire() {
        let cache = PrincipalCache::new();
        let key = [3u8; 32];
        let principal = Arc::new(principal("", Vec::new()));
        cache.put(key, principal);
        assert!(cache.get(&key).is_some());

        cache.entries.lock().get_mut(&key).unwrap().stored_at =
            Instant::now() - PRINCIPAL_TTL - Duration::from_secs(1);
        assert!(
            cache.get(&key).is_none(),
            "expired entry must not be served"
        );
    }
}
