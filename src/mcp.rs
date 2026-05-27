//! MCP endpoint: JSON-RPC subset, bearer-only auth.
//! Currently `initialize` + empty `tools/list`; tools TBD.

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::State;
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
use crate::server::AppState;

/// `2025-06-18` is the current basic-authorization revision.
const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

/// Minimum scope for `/mcp`. Per-tool scopes layer on top later.
pub const MCP_BASE_SCOPE: &str = "stackpit:events:read";

/// Shared via `AppState` so handlers can pull metadata + the auth gate.
#[derive(Clone)]
pub struct McpRuntime {
    pub metadata: Arc<ResourceMetadata>,
    pub gate: BearerGate,
}

/// Well-known is public; `/mcp` requires introspection.
pub fn routes(runtime: &McpRuntime) -> Router<AppState> {
    let auth_state = McpAuthLayerState {
        gate: runtime.gate.clone(),
        required_scope: MCP_BASE_SCOPE.to_string(),
    };

    let protected = Router::<AppState>::new()
        .route("/mcp", post(mcp_handler))
        .layer(axum::middleware::from_fn_with_state(
            auth_state,
            mcp_auth_middleware,
        ));

    Router::<AppState>::new()
        .route(
            "/.well-known/oauth-protected-resource",
            get(well_known_handler),
        )
        .merge(protected)
}

// — RFC 9728 resource-metadata document

/// Built once at startup; Arc-shared so handlers don't re-serialize per request.
#[derive(Debug, Clone)]
pub struct ResourceMetadata {
    body: Value,
}

impl ResourceMetadata {
    /// Claude Code reads `scopes_supported` for DCR; mirror any new scope here.
    pub fn new(audience: &str, authorization_server: &str) -> Self {
        // `offline_access` (OIDC standard) -- NOT Hydra's `offline` alias.
        // Publishing `offline` breaks the DCR client's refresh-token grant.
        let body = json!({
            "resource": audience,
            "authorization_servers": [authorization_server],
            "scopes_supported": [
                "stackpit:events:read",
                "stackpit:projects:read",
                "stackpit:projects:write",
                "stackpit:admin",
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

async fn well_known_handler(State(state): State<AppState>) -> impl IntoResponse {
    let body = match &state.mcp {
        Some(rt) => rt.metadata.body().clone(),
        // Defensive -- route is only mounted when MCP is configured.
        None => return (StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))),
    };
    (StatusCode::OK, Json(body))
}

// — JSON-RPC handler

const JSONRPC_PARSE_ERROR: i64 = -32700;
const JSONRPC_INVALID_REQUEST: i64 = -32600;
const JSONRPC_METHOD_NOT_FOUND: i64 = -32601;

async fn mcp_handler(body: axum::body::Bytes) -> axum::response::Response {
    // JSON-RPC ids are string|number|null; typing all variants is overkill here.
    let envelope: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return jsonrpc_error(Value::Null, JSONRPC_PARSE_ERROR, "Parse error"),
    };

    let id = envelope.get("id").cloned().unwrap_or(Value::Null);

    if envelope.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return jsonrpc_error(id, JSONRPC_INVALID_REQUEST, "Invalid Request");
    }

    let method = match envelope.get("method").and_then(Value::as_str) {
        Some(m) => m,
        None => return jsonrpc_error(id, JSONRPC_INVALID_REQUEST, "Invalid Request"),
    };

    match method {
        "initialize" => jsonrpc_result(
            id,
            json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "serverInfo": {
                    "name": "stackpit",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "capabilities": {
                    "tools": {}
                }
            }),
        ),
        "tools/list" => jsonrpc_result(id, json!({ "tools": [] })),
        _ => jsonrpc_error(id, JSONRPC_METHOD_NOT_FOUND, "Method not found"),
    }
}

fn jsonrpc_result(id: Value, result: Value) -> axum::response::Response {
    (
        StatusCode::OK,
        Json(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        })),
    )
        .into_response()
}

// — JIT user provisioner (LRU dampens upserts during token refresh cycles)

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

        match crate::queries::users::upsert_from_oidc(&self.pool, iss, sub, None, None).await {
            Ok(_) => {
                self.seen
                    .lock()
                    .put((iss.to_string(), sub.to_string()), Instant::now());
                Ok(())
            }
            Err(e) => {
                // Don't touch the LRU -- next request retries the upsert.
                // Caller skips the introspection cache on Err.
                Err(stackpit_auth::ProvisionError::Backend(format!("{e:#}")))
            }
        }
    }
}

fn jsonrpc_error(id: Value, code: i64, message: &str) -> axum::response::Response {
    (
        StatusCode::OK,
        Json(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": code,
                "message": message,
            }
        })),
    )
        .into_response()
}
