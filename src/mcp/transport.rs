//! Streamable HTTP transport for `/mcp`: the origin gate, the bearer gate and
//! rmcp's `StreamableHttpService`. Stateless: no `Mcp-Session-Id`.

use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::extract::{FromRef, Request, State};
use axum::http::header::{
    ACCEPT, ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS,
    ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_EXPOSE_HEADERS, ACCESS_CONTROL_MAX_AGE,
    AUTHORIZATION, CONTENT_TYPE, ORIGIN, VARY,
};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{options, post, MethodRouter};
use axum::{Json, Router};
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use serde_json::{json, Value};
use stackpit_auth::axum_ext::mcp::{mcp_auth_middleware, render_rejection, McpAuthLayerState};
use stackpit_auth::{AuthContext, BearerAuthOutcome, BearerGate, GrantedScopes, TokenClientId};

use crate::config::url::url_origin;
use crate::mcp::handler::McpServer;
use crate::mcp::principal::{scope_step_up, PrincipalError};
use crate::mcp::tools;
use crate::mcp::{McpRuntime, McpState};

pub(super) const MCP_PATH: &str = "/mcp";

/// Matches rmcp's own default; declared here because the body is buffered on
/// the way in, before rmcp gets to enforce it.
const MAX_REQUEST_BODY_BYTES: usize = 4 * 1024 * 1024;

const JSONRPC_INVALID_REQUEST: i64 = -32600;
const JSONRPC_INVALID_PARAMS: i64 = -32602;
const JSONRPC_PARSE_ERROR: i64 = -32700;

type McpService = StreamableHttpService<McpServer, NeverSessionManager>;

pub(super) fn routes<S>(runtime: &McpRuntime) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    McpState: FromRef<S>,
{
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
    let authenticated = Router::<S>::new()
        .route(MCP_PATH, post_route(runtime))
        .layer(auth);
    with_origin_gate(runtime.origins.clone(), authenticated)
}

/// The preflight is merged outside the bearer layer: a browser sends no
/// credentials on `OPTIONS`, and the origin gate must also decorate the bearer
/// layer's own 401 with CORS headers. Split from [`routes`] so the tests can
/// substitute that layer.
fn with_origin_gate<S>(policy: Arc<OriginPolicy>, authenticated: Router<S>) -> Router<S>
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

    fn origins(&self) -> &[String] {
        &self.allowed
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

// rmcp service

fn post_route<S>(runtime: &McpRuntime) -> MethodRouter<S>
where
    S: Clone + Send + Sync + 'static,
    McpState: FromRef<S>,
{
    let service = Arc::new(streamable_service(&runtime.origins));
    post(move |State(state): State<McpState>, req: Request| {
        let service = service.clone();
        async move { serve(&service, state, req).await }
    })
}

fn streamable_service(origins: &OriginPolicy) -> McpService {
    // Origin is the control the transports spec mandates, and [`origin_middleware`]
    // already enforces it more strictly. rmcp's `Host` check is additive, 403s
    // behind a proxy that rewrites `Host`, and fails open on an empty list.
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .disable_allowed_hosts()
        .with_allowed_origins(origins.origins().to_vec())
        .with_max_request_body_bytes(MAX_REQUEST_BODY_BYTES);
    let server = McpServer::new();
    StreamableHttpService::new(
        move || Ok(server.clone()),
        Arc::new(NeverSessionManager::default()),
        config,
    )
}

/// Everything rmcp cannot express happens here, in front of it: the two
/// refusals that must escape the JSON-RPC envelope, and the request state the
/// tool handlers read back out of [`Parts`].
async fn serve(service: &McpService, state: McpState, req: Request) -> Response {
    let (mut parts, body) = req.into_parts();
    let Ok(mut bytes) = axum::body::to_bytes(body, MAX_REQUEST_BODY_BYTES).await else {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    };

    let Ok(mut message) = serde_json::from_slice::<Value>(&bytes) else {
        return jsonrpc_error(
            StatusCode::BAD_REQUEST,
            Value::Null,
            JSONRPC_PARSE_ERROR,
            "Parse error",
        );
    };
    if let Some(name) = tool_call_name(&message) {
        if let Some(rejection) = preflight(&state, &mut parts, &name).await {
            return rejection;
        }
    }
    if message.get("method").and_then(Value::as_str) == Some("initialize") {
        if let Some(rejection) = prepare_initialize(&mut message) {
            return rejection;
        }
        bytes = Bytes::from(message.to_string());
    }

    parts.extensions.insert(state);
    // MCP authorization: the client's token is ours to check, never to relay.
    parts.headers.remove(AUTHORIZATION);
    negotiate_headers(&mut parts.headers);
    service
        .handle(Request::from_parts(parts, Body::from(bytes)))
        .await
        .into_response()
}

fn jsonrpc_error(status: StatusCode, id: Value, code: i64, message: &str) -> Response {
    (
        status,
        Json(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": message },
        })),
    )
        .into_response()
}

fn tool_call_name(message: &Value) -> Option<String> {
    if message.get("method").and_then(Value::as_str)? != "tools/call" {
        return None;
    }
    let name = message.get("params")?.get("name")?.as_str()?;
    Some(name.to_string())
}

/// MCP Lifecycle makes `params`, `protocolVersion`, `capabilities` and
/// `clientInfo` all required on `initialize`. The last two are defaulted here --
/// "supports nothing", under an anonymous name, is a safe reading of silence.
/// A missing version is not: synthesising one contradicts the client's own
/// `MCP-Protocol-Version` header, and letting rmcp refuse the message instead
/// yields `-32601`, which sends 2025-03-26 clients off probing the deprecated
/// HTTP+SSE transport.
fn prepare_initialize(message: &mut Value) -> Option<Response> {
    let id = message.get("id").cloned().unwrap_or(Value::Null);
    let invalid = |member: &str| {
        Some(jsonrpc_error(
            StatusCode::BAD_REQUEST,
            id.clone(),
            JSONRPC_INVALID_PARAMS,
            &format!("Invalid params: initialize requires {member}"),
        ))
    };
    let Some(params) = message.get_mut("params").and_then(Value::as_object_mut) else {
        return invalid("an object `params`");
    };
    if !params.get("protocolVersion").is_some_and(Value::is_string) {
        return invalid("`params.protocolVersion`");
    }
    let defaults = [
        ("capabilities", json!({})),
        ("clientInfo", json!({ "name": "unknown", "version": "0" })),
    ];
    for (key, value) in defaults {
        params.entry(key).or_insert(value);
    }
    None
}

/// Resolves the caller and settles the scope, both of which have answers that
/// RFC 6750 carries in the status and `WWW-Authenticate` -- a JSON-RPC error
/// object would hide them, and an rmcp handler can produce nothing else.
///
/// The resolved principal is handed to the tool handler through the request
/// extensions, so it is never resolved twice.
async fn preflight(state: &McpState, parts: &mut Parts, name: &str) -> Option<Response> {
    // Ahead of the identity round-trip: an unknown name must not cost an IdP call.
    let tool = tools::find(name)?;
    parts.extensions.insert(PreflightTool(tool.name));
    let runtime = state.runtime.clone()?;
    let ctx = parts.extensions.get::<AuthContext>()?.clone();
    let scopes = parts
        .extensions
        .get::<GrantedScopes>()
        .cloned()
        .unwrap_or_default();
    let client_id = parts
        .extensions
        .get::<TokenClientId>()
        .and_then(|c| c.0.clone());

    // Identity resolves before the scope: a token the IdP has since revoked
    // must be answered with a challenge, not with a step-up for a scope the
    // caller can no longer be granted.
    let resolved = match stackpit_auth::extract_bearer(&parts.headers) {
        Some(token) => {
            state
                .principal(token, &ctx, &scopes, client_id.as_deref())
                .await
        }
        None => Err(PrincipalError::Unavailable),
    };
    match resolved {
        Ok(principal) => {
            parts.extensions.insert(principal);
        }
        Err(err) => {
            if let Some(rejection) = principal_rejection(&runtime.gate, err) {
                return Some(rejection);
            }
            // Everything else fails closed inside the envelope: a transient IdP
            // outage must not send the client round the re-authorization loop.
            parts.extensions.insert(err);
        }
    }

    let required = tool.permission.scope;
    if required.is_empty() || scopes.has(required) {
        return None;
    }
    // The refusal leaves no other trace: it never reaches the tool layer that
    // logs outcomes, so without this a client that mishandles the challenge
    // looks identical to one that never called.
    tracing::info!(
        auth_source = "mcp",
        tool = tool.name,
        required_scope = required,
        granted = %scopes.as_slice().join(" "),
        "mcp scope step-up",
    );
    Some(scope_step_up(&runtime.gate, required))
}

/// The tool the scope pre-check resolved, carried to the handler so rmcp's own
/// parse of the same body cannot end up dispatching a different one.
#[derive(Clone, Copy)]
pub(super) struct PreflightTool(pub(super) &'static str);

/// The one recoverable principal failure: tell the client to get a fresh token.
fn principal_rejection(gate: &BearerGate, err: PrincipalError) -> Option<Response> {
    matches!(err, PrincipalError::TokenRejected).then(|| {
        render_rejection(gate, BearerAuthOutcome::InvalidToken)
            .unwrap_or_else(|| StatusCode::UNAUTHORIZED.into_response())
    })
}

/// rmcp answers 406 unless `Accept` names both media types and 415 unless the
/// body is declared JSON. An absent header states no preference (RFC 9110
/// §12.5.1), so it is filled in; a stated one is never overwritten, because a
/// client that named `application/json` alone is refusing SSE, not omitting it
/// -- `json_response` serves it JSON either way.
fn negotiate_headers(headers: &mut HeaderMap) {
    match headers.get(ACCEPT).and_then(|v| v.to_str().ok()) {
        // `*/*` states the same absence of preference as no header at all.
        None | Some("*/*") => {
            headers.insert(
                ACCEPT,
                HeaderValue::from_static("application/json, text/event-stream"),
            );
        }
        Some(accept)
            if accept.contains("application/json") && !accept.contains("text/event-stream") =>
        {
            let widened = format!("{accept}, text/event-stream");
            if let Ok(value) = HeaderValue::from_str(&widened) {
                headers.insert(ACCEPT, value);
            }
        }
        Some(_) => {}
    }
    if !headers.contains_key(CONTENT_TYPE) {
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::test_support::state;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use stackpit_auth::PrincipalId;
    use std::future::Future;
    use tower::ServiceExt;

    const TEST_HOST: &str = "stackpit.example.com";
    const MCP_PROTOCOL_VERSION_HEADER: &str = "mcp-protocol-version";

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

    /// Also hands the tool handler a resolved principal, the way [`preflight`]
    /// does when the IdP answers.
    async fn allow_all_resolved(mut req: Request, next: Next) -> Response {
        req.extensions_mut()
            .insert(Arc::new(crate::mcp::principal::McpPrincipal::for_test(
                "stackpit:events:read",
                Vec::new(),
            )));
        allow_all(req, next).await
    }

    async fn deny_all(_req: Request, _next: Next) -> Response {
        let mut resp = StatusCode::UNAUTHORIZED.into_response();
        resp.headers_mut().insert(
            axum::http::header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Bearer realm=\"stackpit\""),
        );
        resp
    }

    /// Same wiring as `routes`, with the bearer layer stubbed out.
    async fn app_with<F>(auth: F) -> Router
    where
        F: Fn(Request, Next) -> std::pin::Pin<Box<dyn Future<Output = Response> + Send>>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        let state = state().await;
        let runtime = state.runtime.clone().expect("test runtime");
        let authenticated = Router::new()
            .route(MCP_PATH, post_route(&runtime))
            .layer(axum::middleware::from_fn(auth));
        with_origin_gate(runtime.origins.clone(), authenticated).with_state(state)
    }

    async fn app() -> Router {
        app_with(|req, next| Box::pin(allow_all(req, next))).await
    }

    async fn app_denying_auth() -> Router {
        app_with(|req, next| Box::pin(deny_all(req, next))).await
    }

    fn builder() -> axum::http::request::Builder {
        HttpRequest::builder()
            .method("POST")
            .uri(MCP_PATH)
            .header("host", TEST_HOST)
            .header("content-type", "application/json")
    }

    fn post_request(body: Value) -> HttpRequest<Body> {
        builder().body(Body::from(body.to_string())).unwrap()
    }

    fn post_versioned(version: &str, body: Value) -> HttpRequest<Body> {
        builder()
            .header(MCP_PROTOCOL_VERSION_HEADER, version)
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
    async fn initialize_echoes_every_supported_client_version() {
        for version in ["2025-06-18", "2025-11-25"] {
            let resp = app()
                .await
                .oneshot(post_request(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "initialize",
                    "params": { "protocolVersion": version },
                })))
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::OK, "{version}");
            let body = json_body(resp).await;
            assert_eq!(body["result"]["protocolVersion"], version);
            assert_eq!(body["result"]["serverInfo"]["name"], "stackpit");
            assert!(body["result"]["capabilities"]["tools"].is_object());
        }
    }

    // Lifecycle: a version the server does not speak is answered with its
    // latest, not refused -- the client decides whether it can live with it.
    // `2026-07-28` needs more than a version string; the two batching-era
    // revisions below it make batching a MUST, which rmcp does not accept.
    #[tokio::test]
    async fn initialize_answers_with_latest_for_an_unsupported_version() {
        for version in ["2026-07-28", "2025-03-26", "2024-11-05"] {
            let resp = app()
                .await
                .oneshot(post_request(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "initialize",
                    "params": { "protocolVersion": version },
                })))
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::OK, "{version}");
            let body = json_body(resp).await;
            assert_eq!(body["result"]["protocolVersion"], "2025-11-25", "{version}");
        }
    }

    // Transports §Protocol Version Header: a request without the header is
    // assumed to be `2025-03-26`, which stays a served version even though it
    // is no longer one `initialize` will agree to.
    #[tokio::test]
    async fn a_request_without_the_version_header_still_serves() {
        let resp = app()
            .await
            .oneshot(post_request(
                json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = json_body(resp).await;
        assert!(body.get("error").is_none(), "got {body}");
        assert_eq!(
            body["result"]["tools"]
                .as_array()
                .expect("tools is an array")
                .len(),
            tools::TOOLS.len()
        );
    }

    // The header names a revision `initialize` no longer negotiates; the
    // transport still has to serve it.
    #[tokio::test]
    async fn a_dropped_version_in_the_header_still_serves() {
        for version in ["2025-03-26", "2024-11-05"] {
            let resp = app()
                .await
                .oneshot(post_versioned(
                    version,
                    json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }),
                ))
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::OK, "{version}");
            let body = json_body(resp).await;
            assert!(body.get("error").is_none(), "{version}: {body}");
        }
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
                "params": {
                    "protocolVersion": "2025-11-25",
                    "capabilities": {},
                    "clientInfo": { "name": "claude-code", "version": "1.0" },
                },
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
            .oneshot(post_versioned(
                "2025-11-25",
                json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        assert!(body_bytes(resp).await.is_empty(), "202 carries no body");

        let resp = app
            .oneshot(post_versioned(
                "2025-11-25",
                json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
            ))
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

    // The challenge header is the client's only cue to ask for the scope, so
    // this one refusal leaves the JSON-RPC envelope.
    #[tokio::test]
    async fn an_ungranted_tool_is_a_403_step_up() {
        let resp = app()
            .await
            .oneshot(post_request(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": { "name": "create_project", "arguments": { "org_id": 1, "name": "x" } },
            })))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let www = resp
            .headers()
            .get(axum::http::header::WWW_AUTHENTICATE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(www.contains("error=\"insufficient_scope\""), "got {www}");
        assert!(www.contains("scope=\"stackpit:admin\""), "got {www}");
        assert!(www.contains("resource_metadata="), "got {www}");
        assert_eq!(json_body(resp).await["error"], "insufficient_scope");
    }

    // Identity resolution precedes argument handling, so a credential the IdP
    // has since refused is answered with a challenge, not a tool error.
    #[test]
    fn a_token_the_idp_rejected_becomes_a_401_challenge() {
        let gate = crate::mcp::test_support::gate();
        let resp = principal_rejection(&gate, PrincipalError::TokenRejected)
            .expect("expected an HTTP-level rejection");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let www = resp
            .headers()
            .get(axum::http::header::WWW_AUTHENTICATE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(www.contains("error=\"invalid_token\""), "got {www}");
        assert!(www.contains("resource_metadata="), "got {www}");
        assert!(principal_rejection(&gate, PrincipalError::Unavailable).is_none());
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
        assert_eq!(json_body(resp).await["error"]["code"], -32603);
    }

    #[tokio::test]
    async fn an_unknown_tool_is_invalid_params() {
        let resp = app()
            .await
            .oneshot(post_request(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": { "name": "drop_database" },
            })))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = json_body(resp).await;
        assert_eq!(body["error"]["code"], -32602);
        assert_eq!(body["error"]["message"], "Unknown tool");
    }

    // A bad argument is a tool-level mistake the model can correct, not a
    // protocol error.
    #[tokio::test]
    async fn a_non_object_arguments_value_is_a_tool_error() {
        let resp = app()
            .await
            .oneshot(post_request(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": { "name": "whoami", "arguments": [1, 2] },
            })))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = json_body(resp).await;
        assert_eq!(body["result"]["isError"], true);
        assert_eq!(
            body["result"]["content"][0]["text"],
            "arguments must be a JSON object"
        );
    }

    // A client that predates structured output renders the text copy; a modern
    // one reads `structuredContent`. Both ship on every result.
    #[tokio::test]
    async fn a_tool_result_carries_text_and_structured_content() {
        let resp = app_with(|req, next| Box::pin(allow_all_resolved(req, next)))
            .await
            .oneshot(post_request(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": { "name": "whoami", "arguments": {} },
            })))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let result = &json_body(resp).await["result"];
        assert_eq!(result["isError"], false);
        assert_eq!(result["structuredContent"]["sub"], "alice");
        assert_eq!(result["content"][0]["type"], "text");
        assert!(result["content"][0]["text"]
            .as_str()
            .is_some_and(|t| t.contains("\"sub\"")));
        assert!(
            result.get("resultType").is_none(),
            "legacy peers never see the SEP-2322 discriminator"
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

    // Stackpit exposes tools and nothing else.
    #[tokio::test]
    async fn unimplemented_methods_are_method_not_found() {
        for method in ["resources/list", "prompts/list", "stackpit/nope"] {
            let resp = app()
                .await
                .oneshot(post_request(
                    json!({ "jsonrpc": "2.0", "id": 4, "method": method }),
                ))
                .await
                .unwrap();
            assert_eq!(json_body(resp).await["error"]["code"], -32601, "{method}");
        }
    }

    #[tokio::test]
    async fn malformed_json_is_a_parse_error() {
        let resp = app()
            .await
            .oneshot(builder().body(Body::from("{not json")).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            resp.headers().get(CONTENT_TYPE).unwrap(),
            "application/json"
        );
        assert_eq!(
            json_body(resp).await,
            json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": { "code": -32700, "message": "Parse error" },
            })
        );
    }

    #[tokio::test]
    async fn unsupported_protocol_version_header_is_bad_request() {
        let resp = app()
            .await
            .oneshot(post_versioned(
                "1999-01-01",
                json!({ "jsonrpc": "2.0", "id": 5, "method": "ping" }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            String::from_utf8(body_bytes(resp).await.to_vec()).unwrap(),
            "Bad Request: Unsupported MCP-Protocol-Version: 1999-01-01"
        );
    }

    // MCP Lifecycle requires all three members; only the last two are safe to
    // default, and rmcp would answer a missing one with -32601.
    #[tokio::test]
    async fn an_incomplete_initialize_is_invalid_params() {
        for params in [None, Some(json!([])), Some(json!({}))] {
            let mut message = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" });
            if let Some(params) = params.clone() {
                message["params"] = params;
            }
            let resp = app().await.oneshot(post_request(message)).await.unwrap();
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{params:?}");
            let body = json_body(resp).await;
            assert_eq!(body["id"], 1, "{params:?}");
            assert_eq!(body["error"]["code"], -32602, "{params:?}");
            assert!(
                body["error"]["message"]
                    .as_str()
                    .is_some_and(|m| m.starts_with("Invalid params: initialize requires")),
                "{params:?}: {body}"
            );
        }
    }

    // The version is never synthesised: doing so used to collide with rmcp's
    // header/body consistency check and name a version the client never sent.
    #[tokio::test]
    async fn an_initialize_without_a_version_never_contradicts_the_header() {
        let resp = app()
            .await
            .oneshot(post_versioned(
                "2025-06-18",
                json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let message = json_body(resp).await["error"]["message"].to_string();
        assert!(!message.contains("2025-11-25"), "got {message}");
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
                        .header("host", TEST_HOST)
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
                    builder()
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
                    builder()
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
                    .header("host", TEST_HOST)
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
                builder()
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

    fn negotiated(headers: [(&str, &str); 1]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in headers {
            map.insert(
                axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                HeaderValue::from_str(value).unwrap(),
            );
        }
        negotiate_headers(&mut map);
        map
    }

    #[test]
    fn an_absent_accept_is_filled_in() {
        let mut headers = HeaderMap::new();
        negotiate_headers(&mut headers);
        assert_eq!(headers[ACCEPT], "application/json, text/event-stream");
        assert_eq!(headers[CONTENT_TYPE], "application/json");
    }

    // RFC 9110 §12.5.1: `*/*` accepts anything, so curl and bare HTTP clients
    // must not fare worse than one that sent no header at all.
    #[test]
    fn a_wildcard_accept_is_treated_as_absent() {
        let headers = negotiated([("accept", "*/*")]);
        assert_eq!(headers[ACCEPT], "application/json, text/event-stream");
    }

    // Serving JSON to a client that asked for JSON is not a 406.
    #[test]
    fn a_json_only_accept_is_widened_not_replaced() {
        let headers = negotiated([("accept", "application/json")]);
        assert_eq!(headers[ACCEPT], "application/json, text/event-stream");
    }

    #[test]
    fn an_accept_that_refuses_json_is_left_for_rmcp_to_refuse() {
        let headers = negotiated([("accept", "text/plain")]);
        assert_eq!(headers[ACCEPT], "text/plain");
    }

    #[test]
    fn a_stated_content_type_is_left_for_rmcp_to_refuse() {
        let headers = negotiated([("content-type", "text/plain")]);
        assert_eq!(headers[CONTENT_TYPE], "text/plain");
    }
}
