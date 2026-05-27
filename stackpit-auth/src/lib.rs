//! Reusable OIDC/OAuth auth primitives.
//!
//! The framework-agnostic core provides admin-token cookie hashing
//! ([`hash_token_for_cookie`]), a TTL-bounded [`JwksCache`] with an RS256
//! verify primitive ([`JwksCache::verify_rs256`]), the [`BearerGate`]
//! dispatcher (admin-token break-glass, RS256/JWKS JWT arm, RFC 7662
//! introspection arm, positive + revocation caches), and the [`AuthContext`]
//! identity vocabulary every authenticated path resolves to. It compiles with
//! no axum dependency.
//!
//! The `axum` feature -- on by default for now, since Stackpit is the only
//! consumer -- layers in typed extractors ([`RequireAdmin`], [`RequireUser`]),
//! the admin-token resolution middleware, a borrowing cookie reader
//! ([`read_cookie`]), and the MCP `Response`-rendering wrapper.

pub mod admin_token;
pub mod bearer;
pub mod context;
pub mod jwks;

pub use admin_token::hash_token_for_cookie;
pub use bearer::{
    extract_bearer, BearerAuthOutcome, BearerGate, BearerGateConfig, JwtVerifierConfig,
    ProvisionError, ProvisionResult, RevocationError, RevocationStore, UserProvisioner,
};
pub use context::{AuthContext, AuthSource, PrincipalId};
pub use jwks::{JwksCache, JwksError, VerifyError};

#[cfg(feature = "axum")]
pub mod cookie;

#[cfg(feature = "axum")]
pub use cookie::read_cookie;

#[cfg(feature = "axum")]
pub mod axum_ext;

#[cfg(feature = "axum")]
pub use axum_ext::{
    mcp::{mcp_auth_middleware, McpAuthLayerState},
    middleware::{resolve_auth_context, AdminToken},
    resolve_admin, RequireAdmin, RequireUser,
};
