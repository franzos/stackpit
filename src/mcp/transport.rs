//! Streamable HTTP transport for `/mcp`: origin gate, protocol-version
//! negotiation and JSON-RPC framing. Stateless: no `Mcp-Session-Id`.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Request, State};
use axum::http::header::{
    ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN,
    ACCESS_CONTROL_EXPOSE_HEADERS, ACCESS_CONTROL_MAX_AGE, ORIGIN, VARY,
};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::options;
use axum::{Extension, Json, Router};
use serde_json::{json, Value};
use stackpit_auth::axum_ext::mcp::render_rejection;
use stackpit_auth::{AuthContext, BearerAuthOutcome, BearerGate, GrantedScopes, TokenClientId};

use crate::config::url::url_origin;
use crate::mcp::principal::{scope_step_up, McpPrincipal, PrincipalError};
use crate::mcp::tools::{self, ToolError};
use crate::mcp::McpState;

pub(super) const MCP_PATH: &str = "/mcp";

/// Newest first. The `2026-07-28` "modern" era (stateless, per-request
/// `_meta`, `server/discover`) would slot in ahead of these once a client
/// speaks it; it needs more than a version string, so it is deliberately absent.
const SUPPORTED_PROTOCOL_VERSIONS: [&str; 4] =
    ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const LATEST_PROTOCOL_VERSION: &str = SUPPORTED_PROTOCOL_VERSIONS[0];

/// Transports §Protocol Version Header: assume this when the header is absent.
const ASSUMED_PROTOCOL_VERSION: &str = "2025-03-26";

/// JSON-RPC batching was removed in this revision.
const BATCHING_REMOVED_IN: &str = "2025-06-18";

const MCP_PROTOCOL_VERSION_HEADER: &str = "mcp-protocol-version";

const JSONRPC_PARSE_ERROR: i64 = -32700;
const JSONRPC_INVALID_REQUEST: i64 = -32600;
const JSONRPC_METHOD_NOT_FOUND: i64 = -32601;
const JSONRPC_INVALID_PARAMS: i64 = -32602;
const JSONRPC_INTERNAL_ERROR: i64 = -32603;

/// `authenticated` carries `POST /mcp` behind the bearer layer. The preflight
/// is merged outside it: a browser sends no credentials on `OPTIONS`, and the
/// origin gate must also decorate the layer's own 401 with CORS headers.
pub(super) fn routes<S>(policy: Arc<OriginPolicy>, authenticated: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::<S>::new()
        .route(MCP_PATH, options(preflight_handler))
        .merge(authenticated)
        .layer(axum::middleware::from_fn_with_state(
            policy,
            origin_middleware,
        ))
}

// Origin / CORS

/// Exact-match allow-list of browser origins permitted to reach `/mcp`.
#[derive(Debug, Default)]
pub struct OriginPolicy {
    allowed: Vec<String>,
}

impl OriginPolicy {
    pub fn new(external_url: Option<&str>, audience: &str, extra: &[String]) -> Self {
        let mut allowed: Vec<String> = external_url
            .into_iter()
            .chain(std::iter::once(audience))
            .chain(extra.iter().map(String::as_str))
            .filter_map(url_origin)
            .map(|o| o.to_ascii_lowercase())
            .collect();
        allowed.sort();
        allowed.dedup();
        Self { allowed }
    }

    fn allows(&self, origin: &str) -> bool {
        let origin = origin.trim();
        self.allowed.iter().any(|a| a.eq_ignore_ascii_case(origin))
    }
}

async fn origin_middleware(
    State(policy): State<Arc<OriginPolicy>>,
    req: Request,
    next: Next,
) -> Response {
    // Non-browser clients (Claude Code CLI) send no Origin at all; only a
    // present one can carry a DNS-rebinding attack, so absent is allowed.
    let origin = req
        .headers()
        .get(ORIGIN)
        .map(|v| v.to_str().unwrap_or("").to_string());

    let allowed_origin = match origin {
        None => None,
        Some(o) if policy.allows(&o) => Some(o),
        Some(_) => return forbidden_origin(),
    };

    let mut resp = next.run(req).await;
    let headers = resp.headers_mut();
    // Without this a browser client cannot read the 401 challenge that
    // bootstraps OAuth discovery.
    headers.insert(
        ACCESS_CONTROL_EXPOSE_HEADERS,
        HeaderValue::from_static("WWW-Authenticate"),
    );
    if let Some(origin) = allowed_origin {
        if let Ok(value) = HeaderValue::from_str(&origin) {
            headers.insert(ACCESS_CONTROL_ALLOW_ORIGIN, value);
            headers.insert(VARY, HeaderValue::from_static("Origin"));
        }
    }
    resp
}

async fn preflight_handler() -> Response {
    (
        StatusCode::NO_CONTENT,
        [
            (ACCESS_CONTROL_ALLOW_METHODS, "POST, OPTIONS"),
            (
                ACCESS_CONTROL_ALLOW_HEADERS,
                "authorization, content-type, accept, mcp-protocol-version, last-event-id",
            ),
            (ACCESS_CONTROL_MAX_AGE, "600"),
        ],
    )
        .into_response()
}

// JSON-RPC

/// Identity for one request, plus what it takes to resolve the principal. The
/// resolution is lazy: the handshake must not pay an IdP round-trip.
struct RequestContext<'a> {
    state: &'a McpState,
    ctx: &'a AuthContext,
    scopes: &'a GrantedScopes,
    client_id: Option<&'a str>,
    /// The presented bearer, re-read from the header for the userinfo call.
    token: Option<&'a str>,
}

impl RequestContext<'_> {
    async fn principal(&self) -> Result<Arc<McpPrincipal>, PrincipalError> {
        let token = self.token.ok_or(PrincipalError::Unavailable)?;
        self.state
            .principal(token, self.ctx, self.scopes, self.client_id)
            .await
    }
}

/// What one JSON-RPC message produced.
enum Dispatched {
    Reply(Value),
    /// Notification or response: 202 with no body.
    Accepted,
    /// Escapes the JSON-RPC envelope. RFC 6750 rejections carry their meaning
    /// in the status and `WWW-Authenticate`, which a JSON-RPC error would hide.
    Http(Response),
}

pub(super) async fn post_handler(
    State(state): State<McpState>,
    Extension(auth): Extension<AuthContext>,
    Extension(scopes): Extension<GrantedScopes>,
    Extension(client_id): Extension<TokenClientId>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let ctx = RequestContext {
        state: &state,
        ctx: &auth,
        scopes: &scopes,
        client_id: client_id.0.as_deref(),
        token: stackpit_auth::extract_bearer(&headers),
    };

    let message: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return jsonrpc_response(error_object(
                Value::Null,
                JSONRPC_PARSE_ERROR,
                "Parse error",
            ))
        }
    };

    if is_initialize(&message) {
        // The header describes an already-negotiated version, so it carries no
        // meaning on the request that performs the negotiation.
        return finish(dispatch(&ctx, &message).await);
    }

    let version = match negotiated_version(&headers) {
        Ok(v) => v,
        Err(requested) => return unsupported_protocol_version(requested),
    };

    match message {
        Value::Array(items) => {
            if !supports_batching(version) || items.is_empty() {
                return jsonrpc_response(error_object(
                    Value::Null,
                    JSONRPC_INVALID_REQUEST,
                    "Invalid Request",
                ));
            }
            let mut replies = Vec::new();
            for item in &items {
                match dispatch(&ctx, item).await {
                    Dispatched::Reply(reply) => replies.push(reply),
                    Dispatched::Accepted => {}
                    // One HTTP-level rejection answers the whole batch; there is
                    // no way to carry it per element.
                    Dispatched::Http(resp) => return resp,
                }
            }
            if replies.is_empty() {
                StatusCode::ACCEPTED.into_response()
            } else {
                jsonrpc_response(Value::Array(replies))
            }
        }
        single => finish(dispatch(&ctx, &single).await),
    }
}

fn finish(dispatched: Dispatched) -> Response {
    match dispatched {
        Dispatched::Reply(reply) => jsonrpc_response(reply),
        // Transports §Sending Messages: a notification or response gets
        // 202 with no body, never a JSON-RPC reply.
        Dispatched::Accepted => StatusCode::ACCEPTED.into_response(),
        Dispatched::Http(resp) => resp,
    }
}

fn is_initialize(message: &Value) -> bool {
    message.get("method").and_then(Value::as_str) == Some("initialize")
}

fn supports_batching(version: &str) -> bool {
    version < BATCHING_REMOVED_IN
}

/// `Err` carries the rejected header value.
fn negotiated_version(headers: &HeaderMap) -> Result<&'static str, &str> {
    let Some(raw) = headers.get(MCP_PROTOCOL_VERSION_HEADER) else {
        return Ok(ASSUMED_PROTOCOL_VERSION);
    };
    let requested = raw.to_str().unwrap_or_default();
    SUPPORTED_PROTOCOL_VERSIONS
        .iter()
        .copied()
        .find(|v| *v == requested)
        .ok_or(requested)
}

fn unsupported_protocol_version(requested: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "error": "unsupported_protocol_version",
            "requested": requested,
            "supported": SUPPORTED_PROTOCOL_VERSIONS,
        })),
    )
        .into_response()
}

async fn dispatch(ctx: &RequestContext<'_>, message: &Value) -> Dispatched {
    let Some(obj) = message.as_object() else {
        return Dispatched::Reply(error_object(
            Value::Null,
            JSONRPC_INVALID_REQUEST,
            "Invalid Request",
        ));
    };

    let Some(method) = obj.get("method").and_then(Value::as_str) else {
        return if obj.contains_key("result") || obj.contains_key("error") {
            Dispatched::Accepted
        } else {
            Dispatched::Reply(error_object(
                Value::Null,
                JSONRPC_INVALID_REQUEST,
                "Invalid Request",
            ))
        };
    };

    // A request carries an `id`; anything under `notifications/` never does.
    if !obj.contains_key("id") || method.starts_with("notifications/") {
        return Dispatched::Accepted;
    }
    let id = obj.get("id").cloned().unwrap_or(Value::Null);

    if obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Dispatched::Reply(error_object(id, JSONRPC_INVALID_REQUEST, "Invalid Request"));
    }

    let result = match method {
        "initialize" => initialize_result(obj.get("params")),
        "ping" => json!({}),
        // The spec permits the advertised set to vary by authorization.
        "tools/list" => tools::list_result(ctx.scopes),
        "tools/call" => return tools_call(ctx, id, obj.get("params")).await,
        _ => {
            return Dispatched::Reply(error_object(
                id,
                JSONRPC_METHOD_NOT_FOUND,
                "Method not found",
            ))
        }
    };
    Dispatched::Reply(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

/// Identity resolves before arguments: a token the IdP has since revoked must be
/// answered with 401, not with a tool-level error about a missing field.
async fn tools_call(ctx: &RequestContext<'_>, id: Value, params: Option<&Value>) -> Dispatched {
    let principal = match ctx.principal().await {
        Ok(p) => p,
        Err(err) => {
            let gate = ctx.state.runtime.as_ref().map(|rt| &rt.gate);
            return principal_rejection(gate, id, err);
        }
    };

    let name = params
        .and_then(|p| p.get("name"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    tracing::debug!(
        auth_source = "mcp",
        sub = %principal.sub,
        client_id = principal.client_id.as_deref().unwrap_or("-"),
        tool = name,
        "mcp tool call",
    );

    // An unknown tool is a protocol-level mistake; a bad argument is not.
    let Some(tool) = tools::find(name) else {
        return Dispatched::Reply(error_object(id, JSONRPC_INVALID_PARAMS, "Unknown tool"));
    };
    let args = params
        .and_then(|p| p.get("arguments"))
        .cloned()
        .unwrap_or_else(|| json!({}));

    match tools::invoke(tool, ctx.state, principal, &args).await {
        Ok(structured) => Dispatched::Reply(
            json!({ "jsonrpc": "2.0", "id": id, "result": tools::success(structured) }),
        ),
        // The challenge header is the client's only cue to ask for the scope, so
        // this one refusal leaves the envelope.
        Err(ToolError::Scope { required }) => match ctx.state.runtime.as_ref() {
            Some(rt) => Dispatched::Http(scope_step_up(&rt.gate, required)),
            None => Dispatched::Reply(error_object(id, JSONRPC_INTERNAL_ERROR, "not configured")),
        },
        Err(err) => {
            Dispatched::Reply(json!({ "jsonrpc": "2.0", "id": id, "result": tools::failure(&err) }))
        }
    }
}

fn principal_rejection(gate: Option<&BearerGate>, id: Value, err: PrincipalError) -> Dispatched {
    match (err, gate) {
        // The one recoverable case: tell the client to get a fresh token.
        (PrincipalError::TokenRejected, Some(gate)) => Dispatched::Http(
            render_rejection(gate, BearerAuthOutcome::InvalidToken)
                .unwrap_or_else(|| StatusCode::UNAUTHORIZED.into_response()),
        ),
        _ => Dispatched::Reply(error_object(id, JSONRPC_INTERNAL_ERROR, err.message())),
    }
}

/// Lifecycle §Version Negotiation: echo the client's version when we speak it,
/// otherwise answer with our latest and let the client decide.
fn initialize_result(params: Option<&Value>) -> Value {
    let requested = params
        .and_then(|p| p.get("protocolVersion"))
        .and_then(Value::as_str);
    let negotiated = requested
        .filter(|v| SUPPORTED_PROTOCOL_VERSIONS.contains(v))
        .unwrap_or(LATEST_PROTOCOL_VERSION);

    json!({
        "protocolVersion": negotiated,
        "serverInfo": {
            "name": "stackpit",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "capabilities": {
            "tools": {}
        }
    })
}

fn error_object(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
}

fn jsonrpc_response(payload: Value) -> Response {
    (StatusCode::OK, Json(payload)).into_response()
}

fn forbidden_origin() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({
            "jsonrpc": "2.0",
            "error": { "code": JSONRPC_INVALID_REQUEST, "message": "Origin not allowed" },
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::test_support::state;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use axum::routing::post;
    use stackpit_auth::PrincipalId;
    use tower::ServiceExt;

    fn policy() -> Arc<OriginPolicy> {
        Arc::new(OriginPolicy::new(
            Some("https://stackpit.example.com/"),
            "https://stackpit.example.com/mcp",
            &["https://claude.ai".to_string()],
        ))
    }

    /// Stands in for `mcp_auth_middleware`: inserts the same three extensions.
    async fn allow_all(mut req: Request, next: Next) -> Response {
        let ext = req.extensions_mut();
        ext.insert(AuthContext::User {
            iss: "https://idp.test".to_string(),
            sub: "alice".to_string(),
            principal_id: PrincipalId::Request(uuid::Uuid::nil()),
        });
        ext.insert(GrantedScopes::parse(Some("stackpit:events:read")));
        ext.insert(TokenClientId(Some("mcp-client".to_string())));
        next.run(req).await
    }

    async fn deny_all(_req: Request, _next: Next) -> Response {
        let mut resp = StatusCode::UNAUTHORIZED.into_response();
        resp.headers_mut().insert(
            axum::http::header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Bearer realm=\"stackpit\""),
        );
        resp
    }

    /// Same wiring as `mcp::routes`, with the bearer layer stubbed out.
    async fn app() -> Router {
        let authenticated = Router::new()
            .route(MCP_PATH, post(post_handler))
            .layer(axum::middleware::from_fn(allow_all));
        routes(policy(), authenticated).with_state(state().await)
    }

    async fn app_denying_auth() -> Router {
        let authenticated = Router::new()
            .route(MCP_PATH, post(post_handler))
            .layer(axum::middleware::from_fn(deny_all));
        routes(policy(), authenticated).with_state(state().await)
    }

    fn post_request(body: Value) -> HttpRequest<Body> {
        HttpRequest::builder()
            .method("POST")
            .uri(MCP_PATH)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    async fn body_bytes(resp: Response) -> Bytes {
        axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap()
    }

    async fn json_body(resp: Response) -> Value {
        serde_json::from_slice(&body_bytes(resp).await).unwrap()
    }

    #[tokio::test]
    async fn initialize_echoes_a_supported_client_version() {
        let resp = app()
            .await
            .oneshot(post_request(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": { "protocolVersion": "2025-06-18" },
            })))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = json_body(resp).await;
        assert_eq!(body["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(body["result"]["serverInfo"]["name"], "stackpit");
        assert!(body["result"]["capabilities"]["tools"].is_object());
    }

    #[tokio::test]
    async fn initialize_answers_with_latest_for_an_unknown_version() {
        let resp = app()
            .await
            .oneshot(post_request(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": { "protocolVersion": "2026-07-28" },
            })))
            .await
            .unwrap();
        let body = json_body(resp).await;
        assert_eq!(body["result"]["protocolVersion"], LATEST_PROTOCOL_VERSION);
    }

    // The handshake breaks outright if this replies with a JSON-RPC error.
    #[tokio::test]
    async fn handshake_sequence_initialize_notify_list() {
        let app = app().await;

        let resp = app
            .clone()
            .oneshot(post_request(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": { "protocolVersion": "2025-11-25" },
            })))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            json_body(resp).await["result"]["protocolVersion"],
            "2025-11-25"
        );

        let resp = app
            .clone()
            .oneshot(
                HttpRequest::builder()
                    .method("POST")
                    .uri(MCP_PATH)
                    .header("content-type", "application/json")
                    .header(MCP_PROTOCOL_VERSION_HEADER, "2025-11-25")
                    .body(Body::from(
                        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        assert!(body_bytes(resp).await.is_empty(), "202 carries no body");

        let resp = app
            .oneshot(
                HttpRequest::builder()
                    .method("POST")
                    .uri(MCP_PATH)
                    .header("content-type", "application/json")
                    .header(MCP_PROTOCOL_VERSION_HEADER, "2025-11-25")
                    .body(Body::from(
                        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        // Every tool, whatever the token carries; the ungranted ones say which
        // scope they need so the client can step up.
        let names: Vec<String> = json_body(resp).await["result"]["tools"]
            .as_array()
            .expect("tools is an array")
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            names,
            vec![
                "whoami",
                "list_projects",
                "get_project",
                "list_issues",
                "get_issue",
                "get_latest_event",
                "get_event",
                "search_events",
                "list_releases",
                "get_release_health",
                "list_traces",
                "get_trace",
                "update_issue_status",
                "set_project_name",
                "create_tracker_issue",
                "create_project",
                "archive_project"
            ]
        );
    }

    // Identity resolution precedes argument handling, so a credential the IdP
    // has since refused is answered with a challenge, not a tool error.
    #[test]
    fn a_token_the_idp_rejected_becomes_a_401_challenge() {
        let gate = crate::mcp::test_support::gate();
        let Dispatched::Http(resp) = principal_rejection(
            Some(&gate),
            json!(1),
            crate::mcp::principal::PrincipalError::TokenRejected,
        ) else {
            panic!("expected an HTTP-level rejection");
        };
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let www = resp
            .headers()
            .get(axum::http::header::WWW_AUTHENTICATE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(www.contains("error=\"invalid_token\""), "got {www}");
        assert!(www.contains("resource_metadata="), "got {www}");
    }

    // Everything else fails closed inside the envelope: a transient IdP outage
    // must not send the client round the re-authorization loop.
    #[tokio::test]
    async fn tools_call_without_a_resolvable_identity_stays_a_jsonrpc_error() {
        let resp = app()
            .await
            .oneshot(post_request(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": { "name": "whoami" },
            })))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            json_body(resp).await["error"]["code"],
            JSONRPC_INTERNAL_ERROR
        );
    }

    #[tokio::test]
    async fn jsonrpc_response_body_is_accepted() {
        let resp = app()
            .await
            .oneshot(post_request(
                json!({ "jsonrpc": "2.0", "id": 9, "result": {} }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        assert!(body_bytes(resp).await.is_empty());
    }

    #[tokio::test]
    async fn ping_returns_an_empty_result() {
        let resp = app()
            .await
            .oneshot(post_request(
                json!({ "jsonrpc": "2.0", "id": 3, "method": "ping" }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(json_body(resp).await["result"], json!({}));
    }

    #[tokio::test]
    async fn unknown_method_is_method_not_found() {
        let resp = app()
            .await
            .oneshot(post_request(
                json!({ "jsonrpc": "2.0", "id": 4, "method": "resources/list" }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            json_body(resp).await["error"]["code"],
            JSONRPC_METHOD_NOT_FOUND
        );
    }

    #[tokio::test]
    async fn malformed_json_is_a_parse_error() {
        let resp = app()
            .await
            .oneshot(
                HttpRequest::builder()
                    .method("POST")
                    .uri(MCP_PATH)
                    .header("content-type", "application/json")
                    .body(Body::from("{not json"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(json_body(resp).await["error"]["code"], JSONRPC_PARSE_ERROR);
    }

    #[tokio::test]
    async fn unsupported_protocol_version_header_is_bad_request() {
        let resp = app()
            .await
            .oneshot(
                HttpRequest::builder()
                    .method("POST")
                    .uri(MCP_PATH)
                    .header("content-type", "application/json")
                    .header(MCP_PROTOCOL_VERSION_HEADER, "2026-07-28")
                    .body(Body::from(
                        json!({ "jsonrpc": "2.0", "id": 5, "method": "ping" }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn absent_protocol_version_header_assumes_2025_03_26() {
        let version = negotiated_version(&HeaderMap::new()).unwrap_or("unset");
        assert_eq!(version, "2025-03-26");
    }

    #[tokio::test]
    async fn batch_is_accepted_before_batching_was_removed() {
        let resp = app()
            .await
            .oneshot(
                HttpRequest::builder()
                    .method("POST")
                    .uri(MCP_PATH)
                    .header("content-type", "application/json")
                    .header(MCP_PROTOCOL_VERSION_HEADER, "2025-03-26")
                    .body(Body::from(
                        json!([
                            { "jsonrpc": "2.0", "id": 6, "method": "ping" },
                            { "jsonrpc": "2.0", "method": "notifications/initialized" },
                        ])
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = json_body(resp).await;
        let replies = body.as_array().expect("batch reply is an array");
        assert_eq!(replies.len(), 1, "the notification gets no reply");
        assert_eq!(replies[0]["id"], 6);
    }

    #[tokio::test]
    async fn batch_of_only_notifications_is_accepted_with_no_body() {
        let resp = app()
            .await
            .oneshot(
                HttpRequest::builder()
                    .method("POST")
                    .uri(MCP_PATH)
                    .header("content-type", "application/json")
                    .header(MCP_PROTOCOL_VERSION_HEADER, "2024-11-05")
                    .body(Body::from(
                        json!([{ "jsonrpc": "2.0", "method": "notifications/initialized" }])
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        assert!(body_bytes(resp).await.is_empty());
    }

    #[tokio::test]
    async fn batch_is_invalid_from_2025_06_18_on() {
        let resp = app()
            .await
            .oneshot(
                HttpRequest::builder()
                    .method("POST")
                    .uri(MCP_PATH)
                    .header("content-type", "application/json")
                    .header(MCP_PROTOCOL_VERSION_HEADER, "2025-06-18")
                    .body(Body::from(
                        json!([{ "jsonrpc": "2.0", "id": 7, "method": "ping" }]).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            json_body(resp).await["error"]["code"],
            JSONRPC_INVALID_REQUEST
        );
    }

    #[tokio::test]
    async fn get_and_delete_are_method_not_allowed() {
        for method in ["GET", "DELETE"] {
            let resp = app()
                .await
                .oneshot(
                    HttpRequest::builder()
                        .method(method)
                        .uri(MCP_PATH)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::METHOD_NOT_ALLOWED,
                "{method} /mcp"
            );
        }
    }

    #[tokio::test]
    async fn missing_origin_is_allowed() {
        let resp = app()
            .await
            .oneshot(post_request(
                json!({ "jsonrpc": "2.0", "id": 8, "method": "ping" }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp.headers().get(ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
    }

    #[tokio::test]
    async fn allowed_origin_is_echoed_back() {
        for origin in ["https://stackpit.example.com", "https://claude.ai"] {
            let resp = app()
                .await
                .oneshot(
                    HttpRequest::builder()
                        .method("POST")
                        .uri(MCP_PATH)
                        .header("content-type", "application/json")
                        .header(ORIGIN, origin)
                        .body(Body::from(
                            json!({ "jsonrpc": "2.0", "id": 8, "method": "ping" }).to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::OK, "{origin}");
            assert_eq!(
                resp.headers().get(ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(),
                origin
            );
        }
    }

    #[tokio::test]
    async fn foreign_origin_is_forbidden() {
        for origin in ["https://evil.example.com", "null"] {
            let resp = app()
                .await
                .oneshot(
                    HttpRequest::builder()
                        .method("POST")
                        .uri(MCP_PATH)
                        .header("content-type", "application/json")
                        .header(ORIGIN, origin)
                        .body(Body::from(
                            json!({ "jsonrpc": "2.0", "id": 8, "method": "ping" }).to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::FORBIDDEN, "{origin}");
        }
    }

    #[tokio::test]
    async fn preflight_needs_no_bearer() {
        let resp = app_denying_auth()
            .await
            .oneshot(
                HttpRequest::builder()
                    .method("OPTIONS")
                    .uri(MCP_PATH)
                    .header(ORIGIN, "https://claude.ai")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            resp.headers().get(ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(),
            "https://claude.ai"
        );
        assert!(resp.headers().get(ACCESS_CONTROL_ALLOW_METHODS).is_some());
    }

    // The challenge is unreadable from a browser without the expose header.
    #[tokio::test]
    async fn auth_rejection_exposes_the_challenge_header() {
        let resp = app_denying_auth()
            .await
            .oneshot(
                HttpRequest::builder()
                    .method("POST")
                    .uri(MCP_PATH)
                    .header("content-type", "application/json")
                    .header(ORIGIN, "https://claude.ai")
                    .body(Body::from(
                        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            resp.headers().get(ACCESS_CONTROL_EXPOSE_HEADERS).unwrap(),
            "WWW-Authenticate"
        );
        assert_eq!(
            resp.headers().get(ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(),
            "https://claude.ai"
        );
    }

    #[test]
    fn origin_policy_derives_from_external_url_and_audience() {
        let policy = OriginPolicy::new(
            Some("https://app.example.com/base/"),
            "https://mcp.example.com/mcp",
            &["https://Claude.ai".to_string(), "not a url".to_string()],
        );
        assert!(policy.allows("https://app.example.com"));
        assert!(policy.allows("https://mcp.example.com"));
        assert!(policy.allows("https://claude.ai"));
        assert!(!policy.allows("http://app.example.com"));
        assert!(!policy.allows("https://app.example.com:8443"));
        assert!(!policy.allows("null"));
    }
}
