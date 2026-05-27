//! Framework-agnostic auth identity.
//!
//! `AuthContext` is the single enum every authenticated path resolves to,
//! regardless of how the principal proved themselves (admin token, OAuth
//! session, project API key, ...). It lives outside the `axum` feature so
//! non-axum consumers -- CLI tools, future MCP shims -- can speak the same
//! vocabulary.

use serde::Serialize;
use uuid::Uuid;

/// Identifier carried on every authenticated `User` context. Two flavors
/// exist because the web and MCP paths produce different stability
/// guarantees, and conflating them breaks audit-log correlation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrincipalId {
    /// Stable per-device handle (browser SSO grant). Safe to use as an audit
    /// or join key: the same browser session yields the same UUID across
    /// requests until the user logs out.
    Session(Uuid),
    /// Per-request correlation ID (MCP bearer auth). Fresh on every request;
    /// useful for tracing a single call but MUST NOT be used as a session or
    /// join key.
    Request(Uuid),
}

impl PrincipalId {
    /// The underlying UUID, regardless of stability. Use this for log fields
    /// where the distinction is communicated separately.
    pub fn uuid(&self) -> Uuid {
        match self {
            PrincipalId::Session(u) | PrincipalId::Request(u) => *u,
        }
    }

    /// True when this id is stable across requests (i.e. session-bound).
    pub fn is_stable(&self) -> bool {
        matches!(self, PrincipalId::Session(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthContext {
    /// `admin_token` presented via Bearer or cookie. Full access, break-glass.
    Admin,
    /// OAuth-authenticated principal, either via session cookie (UI) or
    /// bearer token (MCP). `iss` is the OIDC issuer URL the principal was
    /// authenticated against -- paired with `sub` it forms the stable user
    /// key in the `users` table. `principal_id` is either a stable session
    /// handle (web) or a per-request correlation ID (MCP); see
    /// [`PrincipalId`].
    User {
        iss: String,
        sub: String,
        principal_id: PrincipalId,
    },
}

impl AuthContext {
    pub fn is_admin(&self) -> bool {
        matches!(self, AuthContext::Admin)
    }

    pub fn is_user(&self) -> bool {
        matches!(self, AuthContext::User { .. })
    }

    /// Which auth flow produced this context. Audit logs and tracing spans
    /// tag identity events with this so a "User { sub: alice }" event from a
    /// cookie session is distinguishable from the same `sub` coming in via
    /// MCP bearer auth -- two different surfaces, two different blast radii.
    pub fn source(&self) -> AuthSource {
        match self {
            AuthContext::Admin => AuthSource::Admin,
            AuthContext::User {
                principal_id: PrincipalId::Session(_),
                ..
            } => AuthSource::Web,
            AuthContext::User {
                principal_id: PrincipalId::Request(_),
                ..
            } => AuthSource::Mcp,
        }
    }
}

/// Coarse label for the surface that produced an [`AuthContext`]. Stable
/// across releases; safe to embed in audit logs and metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthSource {
    /// Browser session: cookie-resolved grant; principal is a `PrincipalId::Session`.
    Web,
    /// MCP bearer auth: per-request correlation only; principal is a
    /// `PrincipalId::Request`.
    Mcp,
    /// `admin_token` break-glass (header or cookie).
    Admin,
}

impl std::fmt::Display for AuthSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            AuthSource::Web => "web",
            AuthSource::Mcp => "mcp",
            AuthSource::Admin => "admin",
        };
        f.write_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_source() {
        assert_eq!(AuthContext::Admin.source(), AuthSource::Admin);
    }

    #[test]
    fn session_user_is_web() {
        let ctx = AuthContext::User {
            iss: "https://idp.example".to_string(),
            sub: "alice".to_string(),
            principal_id: PrincipalId::Session(Uuid::nil()),
        };
        assert_eq!(ctx.source(), AuthSource::Web);
    }

    #[test]
    fn request_user_is_mcp() {
        let ctx = AuthContext::User {
            iss: "https://idp.example".to_string(),
            sub: "alice".to_string(),
            principal_id: PrincipalId::Request(Uuid::nil()),
        };
        assert_eq!(ctx.source(), AuthSource::Mcp);
    }

    #[test]
    fn display_is_lowercase() {
        assert_eq!(AuthSource::Web.to_string(), "web");
        assert_eq!(AuthSource::Mcp.to_string(), "mcp");
        assert_eq!(AuthSource::Admin.to_string(), "admin");
    }
}
