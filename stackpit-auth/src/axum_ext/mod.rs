//! Axum integration: extractors and the resolution middleware.

pub mod extractors;
pub mod mcp;
pub mod middleware;

pub use extractors::{RequireAdmin, RequireUser};
pub use mcp::{mcp_auth_middleware, McpAuthLayerState};
pub use middleware::{
    json_unauthorized, login_form_response, resolve_admin, resolve_auth_context, wants_html,
    AdminToken,
};
