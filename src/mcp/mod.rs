//! MCP endpoint: Streamable HTTP transport, bearer-only auth.
//! Transport framing lives in [`transport`]; identity and per-tool
//! authorization in [`principal`]; the tool table in [`tools`].

pub mod principal;
mod tools;
mod transport;

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{FromRef, State};
use axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use lru::LruCache;
use parking_lot::Mutex;
use serde_json::{json, Value};
use stackpit_auth::axum_ext::mcp::{mcp_auth_middleware, McpAuthLayerState};
use stackpit_auth::bearer::UserProvisioner;
use stackpit_auth::BearerGate;

use crate::db::DbPool;
use crate::ingest::auth::{AuthCache, NegativeAuthCache};
use crate::oidc::client::OidcClient;
use crate::server::AppState;
use crate::util::crypto::SecretEncryptor;

pub use principal::PrincipalCache;
pub use transport::OriginPolicy;

pub const SCOPE_EVENTS_READ: &str = "stackpit:events:read";
pub const SCOPE_PROJECTS_READ: &str = "stackpit:projects:read";
/// Never advertised in `scopes_supported`: a client that sees no `scope` in the
/// challenge asks for everything published there, so listing these would make
/// every first connect prompt for write and admin. They arrive by 403 step-up.
pub const SCOPE_PROJECTS_WRITE: &str = "stackpit:projects:write";
pub const SCOPE_ADMIN: &str = "stackpit:admin";

/// RFC 9728 §3.3: a metadata document whose `resource` does not match the
/// identifier implied by its own URL MUST be discarded, so the path suffix has
/// to mirror the resource identifier's path (`/mcp`).
pub const WELL_KNOWN_PATH: &str = "/.well-known/oauth-protected-resource/mcp";
/// Pre-RFC clients probe the bare path; kept as an alias.
const WELL_KNOWN_ROOT_PATH: &str = "/.well-known/oauth-protected-resource";

/// Shared via `AppState` so handlers can pull metadata + the auth gate.
#[derive(Clone)]
pub struct McpRuntime {
    pub metadata: Arc<ResourceMetadata>,
    pub gate: BearerGate,
    pub origins: Arc<OriginPolicy>,
    /// Principals resolved from the IdP, keyed by token hash.
    pub principals: Arc<PrincipalCache>,
}

/// The slice of `AppState` the MCP surface needs, so the transport handler is
/// exercisable without building a whole `AppState`.
#[derive(Clone)]
pub struct McpState {
    /// Reconcile writes and the membership read that follows it share one pool
    /// so the read always sees the write.
    pub auth_pool: DbPool,
    /// Tool reads.
    pub pool: DbPool,
    /// Tool writes. `mcp_writer_pool` is not available here: it belongs to the
    /// JIT provisioner inside the gate.
    pub writer_pool: DbPool,
    pub oidc: Option<Arc<OidcClient>>,
    /// Decrypts integration secrets for the tracker tool.
    pub encryptor: Option<Arc<SecretEncryptor>>,
    /// Ingest auth caches, flushed when a tool archives a project.
    pub auth_cache: AuthCache,
    pub negative_auth_cache: NegativeAuthCache,
    /// Absolute base for the deep links tools hand to external systems.
    pub web_base: String,
    /// `None` only defensively; the routes mount solely when MCP is configured.
    pub runtime: Option<Arc<McpRuntime>>,
}

impl FromRef<AppState> for McpState {
    fn from_ref(state: &AppState) -> Self {
        Self {
            auth_pool: state.auth_pool.clone(),
            pool: state.pool.clone(),
            writer_pool: state.writer_pool.clone(),
            oidc: state.oidc.clone(),
            encryptor: state.encryptor.clone(),
            auth_cache: state.auth_cache.clone(),
            negative_auth_cache: state.negative_auth_cache.clone(),
            web_base: state.config.server.web_base(),
            runtime: state.mcp.clone(),
        }
    }
}

/// Well-known is public; `/mcp` requires a bearer token.
pub fn routes(runtime: &McpRuntime) -> Router<AppState> {
    // Authentication only. Every scope check is per-tool: gating the route on
    // one scope would stop a token holding just `projects:write` or `admin`
    // from even completing the handshake.
    let auth = axum::middleware::from_fn_with_state(
        McpAuthLayerState {
            gate: runtime.gate.clone(),
            required_scope: String::new(),
        },
        mcp_auth_middleware,
    );

    let authenticated = Router::<AppState>::new()
        .route(transport::MCP_PATH, post(transport::post_handler))
        .layer(auth);

    well_known_routes().merge(transport::routes(runtime.origins.clone(), authenticated))
}

fn well_known_routes<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    McpState: FromRef<S>,
{
    Router::<S>::new()
        .route(WELL_KNOWN_PATH, get(well_known_handler))
        .route(WELL_KNOWN_ROOT_PATH, get(well_known_handler))
}

// RFC 9728 resource-metadata document

/// Built once at startup; Arc-shared so handlers don't re-serialize per request.
#[derive(Debug, Clone)]
pub struct ResourceMetadata {
    body: Value,
}

impl ResourceMetadata {
    /// `scopes_supported` is deliberately the read set only. A client that sees
    /// no `scope` in the challenge asks for everything advertised here, so
    /// listing the write and admin scopes would make every first connect prompt
    /// for admin; those arrive through 403 step-up instead.
    pub fn new(audience: &str, authorization_server: &str) -> Self {
        // `offline_access` (OIDC standard, not Hydra's `offline` alias).
        // Publishing `offline` breaks the DCR client's refresh-token grant.
        let body = json!({
            "resource": audience,
            "authorization_servers": [authorization_server],
            "scopes_supported": [
                SCOPE_EVENTS_READ,
                SCOPE_PROJECTS_READ,
                "offline_access",
            ],
            "bearer_methods_supported": ["header"],
        });
        Self { body }
    }

    pub fn body(&self) -> &Value {
        &self.body
    }
}

/// RFC 9728 metadata is meant to be publicly readable, and a browser-based
/// client fetches it cross-origin before it holds any credential.
async fn well_known_handler(State(state): State<McpState>) -> impl IntoResponse {
    let public = [(ACCESS_CONTROL_ALLOW_ORIGIN, "*")];
    let body = match &state.runtime {
        Some(rt) => rt.metadata.body().clone(),
        // Defensive: route is only mounted when MCP is configured.
        None => {
            return (
                StatusCode::NOT_FOUND,
                public,
                Json(json!({ "error": "not found" })),
            )
        }
    };
    (StatusCode::OK, public, Json(body))
}

// JIT user provisioner (LRU dampens upserts during token refresh cycles)

const PROVISION_TTL: Duration = Duration::from_secs(300);
const PROVISION_LRU_CAP: usize = 1024;

pub(crate) struct DbProvisioner {
    pub pool: DbPool,
    seen: Mutex<LruCache<(String, String), Instant>>,
}

impl DbProvisioner {
    pub fn new(pool: DbPool) -> Self {
        let cap = NonZeroUsize::new(PROVISION_LRU_CAP).expect("PROVISION_LRU_CAP is non-zero");
        Self {
            pool,
            seen: Mutex::new(LruCache::new(cap)),
        }
    }
}

#[async_trait::async_trait]
impl UserProvisioner for DbProvisioner {
    async fn provision(&self, iss: &str, sub: &str) -> stackpit_auth::ProvisionResult {
        // Fast path: skip the DB write if seen within the TTL window.
        {
            let mut seen = self.seen.lock();
            if let Some(t) = seen.get(&(iss.to_string(), sub.to_string())) {
                if t.elapsed() < PROVISION_TTL {
                    return Ok(());
                }
            }
        }

        let user =
            match crate::queries::users::upsert_from_oidc(&self.pool, iss, sub, None, None).await {
                Ok(u) => u,
                Err(e) => {
                    // Don't touch the LRU; next request retries the upsert.
                    return Err(stackpit_auth::BackendError::Backend(format!("{e:#}")));
                }
            };

        if let Err(e) = crate::queries::orgs::ensure_personal_org(&self.pool, user.user_id).await {
            // Don't touch the LRU; next request retries both steps.
            return Err(stackpit_auth::BackendError::Backend(format!("{e:#}")));
        }

        self.seen
            .lock()
            .put((iss.to_string(), sub.to_string()), Instant::now());
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use stackpit_auth::{BearerGateConfig, JwksCache, JwtVerifierConfig};

    pub const TEST_AUDIENCE: &str = "https://stackpit.example.com/mcp";
    pub const TEST_ISSUER: &str = "https://idp.test";
    pub const TEST_METADATA_URL: &str =
        "https://stackpit.example.com/.well-known/oauth-protected-resource/mcp";

    pub fn gate() -> BearerGate {
        BearerGate::new(BearerGateConfig {
            introspection_url: None,
            audience: TEST_AUDIENCE.to_string(),
            resource_metadata_url: TEST_METADATA_URL.to_string(),
            realm: "stackpit".to_string(),
            expected_issuer: Some(TEST_ISSUER.to_string()),
            client_id: String::new(),
            admin_token: None,
            introspection_client_id: None,
            introspection_client_secret: None,
            cache_ttl_secs: 0,
            cache_max_ttl_secs: 30,
            provisioner: None,
            revocation: None,
            jwt: Some(JwtVerifierConfig {
                jwks: JwksCache::new(
                    reqwest::Client::new(),
                    "http://127.0.0.1:0/jwks".to_string(),
                    60,
                ),
            }),
        })
        .expect("test HTTP client builds")
    }

    /// No OIDC client, so principal resolution fails closed; the transport and
    /// discovery tests never need one.
    pub async fn state() -> McpState {
        state_with_pool(crate::db::open_test_pool().await).await
    }

    pub async fn state_with_pool(pool: DbPool) -> McpState {
        McpState {
            auth_pool: pool.clone(),
            pool: pool.clone(),
            writer_pool: pool,
            oidc: None,
            encryptor: None,
            auth_cache: AuthCache::default(),
            negative_auth_cache: NegativeAuthCache::default(),
            web_base: "https://stackpit.example.com".to_string(),
            runtime: Some(Arc::new(McpRuntime {
                metadata: Arc::new(ResourceMetadata::new(TEST_AUDIENCE, TEST_ISSUER)),
                gate: gate(),
                origins: Arc::new(OriginPolicy::new(None, TEST_AUDIENCE, &[])),
                principals: Arc::new(PrincipalCache::new()),
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;
    use crate::queries::orgs::list_memberships;
    use crate::queries::users::find_by_iss_sub;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use tower::ServiceExt;

    async fn well_known(path: &str) -> (StatusCode, axum::http::HeaderMap, Value) {
        let resp = well_known_routes()
            .with_state(state().await)
            .oneshot(
                HttpRequest::builder()
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, headers, serde_json::from_slice(&body).unwrap())
    }

    // RFC 9728 3.3: a document whose `resource` mismatches the identifier
    // implied by its URL MUST be discarded, so the path has to carry `/mcp`.
    #[tokio::test]
    async fn metadata_is_served_path_aware_and_at_the_root_alias() {
        for path in [WELL_KNOWN_PATH, WELL_KNOWN_ROOT_PATH] {
            let (status, _, body) = well_known(path).await;
            assert_eq!(status, StatusCode::OK, "{path}");
            assert_eq!(body["resource"], TEST_AUDIENCE, "{path}");
            assert_eq!(
                body["authorization_servers"],
                json!([TEST_ISSUER]),
                "{path}"
            );
        }
    }

    // A browser client fetches this cross-origin before it holds a credential.
    #[tokio::test]
    async fn metadata_is_readable_cross_origin() {
        let (_, headers, _) = well_known(WELL_KNOWN_PATH).await;
        assert_eq!(headers.get(ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(), "*");
    }

    // Clients that see no `scope` in the challenge ask for everything listed
    // here, so advertising the write or admin scope would make every first
    // connect prompt for admin.
    #[tokio::test]
    async fn advertised_scopes_are_read_only() {
        let (_, _, body) = well_known(WELL_KNOWN_PATH).await;
        assert_eq!(
            body["scopes_supported"],
            json!([SCOPE_EVENTS_READ, SCOPE_PROJECTS_READ, "offline_access"]),
        );
    }

    #[tokio::test]
    async fn provision_creates_personal_org() {
        let pool = crate::db::open_test_pool().await;
        let provisioner = DbProvisioner::new(pool.clone());

        provisioner
            .provision("https://idp.test", "sub-mcp")
            .await
            .expect("provision must succeed");

        let user = find_by_iss_sub(&pool, "https://idp.test", "sub-mcp")
            .await
            .unwrap()
            .expect("user must exist after provision");

        let memberships = list_memberships(&pool, user.user_id).await.unwrap();
        assert_eq!(memberships.len(), 1, "exactly one membership");
        assert!(
            memberships[0].is_personal,
            "membership must be personal org"
        );
    }

    #[tokio::test]
    async fn provision_personal_org_is_idempotent_via_lru_bypass() {
        let pool = crate::db::open_test_pool().await;
        let provisioner = DbProvisioner::new(pool.clone());

        // Call twice; LRU fast-path skips DB on second call, still one membership.
        provisioner
            .provision("https://idp.test", "sub-mcp-idem")
            .await
            .unwrap();
        // Clear the LRU so the second call hits the DB again.
        provisioner.seen.lock().clear();
        provisioner
            .provision("https://idp.test", "sub-mcp-idem")
            .await
            .unwrap();

        let user = find_by_iss_sub(&pool, "https://idp.test", "sub-mcp-idem")
            .await
            .unwrap()
            .unwrap();
        let memberships = list_memberships(&pool, user.user_id).await.unwrap();
        assert_eq!(
            memberships.len(),
            1,
            "idempotent: still exactly one membership"
        );
    }
}
