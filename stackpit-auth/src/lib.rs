//! Reusable OIDC/OAuth auth primitives.
//!
//! The framework-agnostic core provides admin-token cookie hashing
//! ([`hash_token_for_cookie`]), a TTL-bounded [`JwksCache`], the [`BearerGate`]
//! dispatcher (admin-token break-glass, RS256/JWKS and RFC 7662 introspection
//! arms, positive + revocation caches), and the [`AuthContext`] identity
//! vocabulary. It compiles with no axum dependency.
//!
//! The `axum` feature adds admin-token resolution ([`resolve_admin`]), a
//! borrowing cookie reader ([`read_cookie`]), and the MCP `Response` wrapper.

pub mod admin_token;
pub mod bearer;
pub mod context;
pub mod jwks;

pub use admin_token::hash_token_for_cookie;
pub use bearer::{
    extract_bearer, BackendError, BearerAuthOutcome, BearerGate, BearerGateConfig,
    JwtVerifierConfig, ProvisionResult, RevocationStore, UserProvisioner,
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
    resolve_admin,
};
