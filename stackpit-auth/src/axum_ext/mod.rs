//! Axum integration: admin-token resolution and the MCP wrapper.

pub mod mcp;
pub mod middleware;

pub use mcp::{mcp_auth_middleware, McpAuthLayerState};
pub use middleware::{json_unauthorized, resolve_admin, wants_html};
