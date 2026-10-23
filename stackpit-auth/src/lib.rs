//! Reusable OIDC/OAuth auth primitives.
//!
//! The framework-agnostic core provides the admin browser-session store
//! ([`AdminSessionStore`]), the [`DiscoveryHandle`] and [`JwksCache`] from
//! `oidc-relying-party`, the [`BearerGate`] dispatcher (admin-token break-glass,
//! JWKS-verified JWTs with the discovery-advertised algorithms, RFC 7662
//! introspection, positive + revocation caches), and the [`AuthContext`]
//! identity vocabulary. It compiles with no axum dependency.
//!
//! The `axum` feature adds admin-token resolution ([`resolve_admin`]), a
//! borrowing cookie reader ([`read_cookie`]), and the MCP `Response` wrapper.

pub mod admin_token;
pub mod bearer;
pub mod context;

pub use admin_token::{AdminSessionStore, ADMIN_SESSION_TTL_SECS};
pub use bearer::{
    extract_bearer, BackendError, BearerAuthOutcome, BearerGate, BearerGateConfig, BearerGrant,
    GrantedScopes, JwtVerifierConfig, ProvisionResult, RevocationStore, TokenClientId,
    UserProvisioner,
};
pub use context::{AuthContext, AuthSource, PrincipalId};
pub use oidc_relying_party::discovery::DiscoveryHandle;
pub use oidc_relying_party::jwks::JwksCache;

#[cfg(test)]
pub(crate) mod test_util {
    use oidc_relying_party::algorithms::SigningAlgorithm;
    use oidc_relying_party::discovery::{ClientAuthMethod, Discovery, DiscoveryHandle};
    use oidc_relying_party::jwks::{JwksCache, JwksCacheConfig};

    pub fn primed_jwks(json: &str) -> JwksCache {
        let cache = JwksCache::new(
            reqwest::Client::new(),
            reqwest::Url::parse("http://127.0.0.1:0/jwks").unwrap(),
            JwksCacheConfig::default(),
        );
        cache.prime_raw(json).expect("test JWKS parses");
        cache
    }

    /// A handle over an in-memory document for `issuer` accepting `algs`.
    pub fn fixed_discovery(
        issuer: &str,
        algs: &[SigningAlgorithm],
        jwks: JwksCache,
    ) -> DiscoveryHandle {
        let url =
            |path: &str| reqwest::Url::parse(&format!("{issuer}{path}")).expect("stub URL valid");
        DiscoveryHandle::fixed(
            Discovery {
                issuer: issuer.to_string(),
                authorization_endpoint: url("/oauth2/auth"),
                token_endpoint: url("/oauth2/token"),
                jwks_uri: url("/.well-known/jwks.json"),
                userinfo_endpoint: None,
                end_session_endpoint: None,
                revocation_endpoint: None,
                introspection_endpoint: None,
                id_token_signing_alg_values_supported: Vec::new(),
                signing_algorithms: algs.to_vec(),
                token_endpoint_auth_method: ClientAuthMethod::Basic,
                authorization_response_iss_parameter_supported: false,
            },
            jwks,
        )
    }
}

#[cfg(feature = "axum")]
pub mod cookie;

#[cfg(feature = "axum")]
pub use cookie::read_cookie;

#[cfg(feature = "axum")]
pub mod axum_ext;

#[cfg(feature = "axum")]
pub use axum_ext::{
    mcp::{mcp_auth_middleware, McpAuthLayerState},
    resolve_admin,
};
