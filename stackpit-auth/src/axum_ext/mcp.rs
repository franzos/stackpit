//! Axum wrapper around the framework-free [`BearerGate`] in [`crate::bearer`].
//!
//! MCP transport: extracts bearer from `Authorization`, gate validates,
//! rejection becomes 401/403 with RFC 6750 / RFC 9728 `WWW-Authenticate` and
//! a JSON body that mirrors the challenge (so MCP clients that don't surface
//! response headers can still parse the error class).

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderValue, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use crate::bearer::{BearerAuthOutcome, BearerGate};

/// JSON body returned alongside RFC 6750 401/403 responses. Field names follow
/// the `WWW-Authenticate` challenge vocabulary so a client can compare both
/// surfaces. `resource_metadata` points at the RFC 9728 well-known doc so the
/// caller can re-discover the resource server's expected audience and scopes.
#[derive(Serialize)]
struct BearerErrorBody<'a> {
    error: &'static str,
    error_description: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<&'a str>,
    #[serde(skip_serializing_if = "str::is_empty")]
    resource_metadata: &'a str,
}

/// State for [`mcp_auth_middleware`]. The required scope is layered: one
/// route, one scope; layer twice if you need different scopes on different
/// routes.
#[derive(Clone)]
pub struct McpAuthLayerState {
    pub gate: BearerGate,
    pub required_scope: String,
}

pub async fn mcp_auth_middleware(
    State(state): State<McpAuthLayerState>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    let outcome = state
        .gate
        .authorize_headers(req.headers(), &state.required_scope)
        .await;
    match outcome {
        BearerAuthOutcome::Ok(ctx) => {
            // Tag the audit/tracing surface with which flow produced the
            // identity so MCP bearer auth is distinguishable from a web
            // session for the same `sub`.
            tracing::debug!(
                target: "stackpit::auth",
                auth_source = %ctx.source(),
                "mcp bearer authenticated",
            );
            req.extensions_mut().insert(ctx);
            next.run(req).await
        }
        other => render_rejection(&state.gate, other)
            .unwrap_or_else(|| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
    }
}

/// Convert an outcome into a `Response`. Caller does this when running
/// the gate directly from a handler. Body shape mirrors RFC 6750 §3 and
/// RFC 9728: status + `WWW-Authenticate` header + JSON body containing the
/// same error class plus the protected-resource-metadata URL.
pub fn render_rejection(gate: &BearerGate, outcome: BearerAuthOutcome) -> Option<Response> {
    match outcome {
        BearerAuthOutcome::Ok(_) => None,
        BearerAuthOutcome::MissingToken => Some(json_error(
            gate,
            StatusCode::UNAUTHORIZED,
            None,
            BearerErrorBody {
                error: "missing_token",
                error_description: "Authorization header required",
                scope: None,
                resource_metadata: gate.resource_metadata_url(),
            },
        )),
        BearerAuthOutcome::InvalidToken => Some(json_error(
            gate,
            StatusCode::UNAUTHORIZED,
            Some("invalid_token"),
            BearerErrorBody {
                error: "invalid_token",
                error_description: "Authorization header required",
                scope: None,
                resource_metadata: gate.resource_metadata_url(),
            },
        )),
        BearerAuthOutcome::InsufficientScope { required } => {
            // Custom challenge for the scope variant: RFC 6750 §3 says the
            // `scope` parameter belongs on the challenge when the gate knows
            // which scope was insufficient. The body echoes it for clients
            // that don't read response headers.
            let challenge = format!(
                "Bearer realm=\"{}\", error=\"insufficient_scope\", scope=\"{}\", resource_metadata=\"{}\"",
                gate.realm(),
                required,
                gate.resource_metadata_url(),
            );
            let body = BearerErrorBody {
                error: "insufficient_scope",
                error_description: "The access token does not carry the required scope",
                scope: Some(required.as_str()),
                resource_metadata: gate.resource_metadata_url(),
            };
            let mut resp = (StatusCode::FORBIDDEN, Json(body)).into_response();
            resp.headers_mut().insert(
                axum::http::header::WWW_AUTHENTICATE,
                HeaderValue::from_str(&challenge)
                    .unwrap_or_else(|_| HeaderValue::from_static("Bearer")),
            );
            Some(resp)
        }
    }
}

fn json_error(
    gate: &BearerGate,
    status: StatusCode,
    challenge_error: Option<&str>,
    body: BearerErrorBody<'_>,
) -> Response {
    let mut resp = (status, Json(body)).into_response();
    resp.headers_mut().insert(
        axum::http::header::WWW_AUTHENTICATE,
        challenge_value(gate, challenge_error),
    );
    resp
}

fn challenge_value(gate: &BearerGate, error: Option<&str>) -> HeaderValue {
    // Header values are guaranteed ASCII here (URL + ASCII identifiers),
    // so the parse never fails in practice.
    HeaderValue::from_str(&gate.challenge_header(error))
        .unwrap_or_else(|_| HeaderValue::from_static("Bearer"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bearer::{BearerGateConfig, JwtVerifierConfig};
    use crate::jwks::JwksCache;

    fn build_gate() -> BearerGate {
        BearerGate::new(BearerGateConfig {
            introspection_url: None,
            audience: "https://mcp.example.com".to_string(),
            resource_metadata_url:
                "https://stackpit.example.com/.well-known/oauth-protected-resource".to_string(),
            realm: "stackpit".to_string(),
            expected_issuer: Some("https://hydra.example.com".to_string()),
            client_id: "stackpit-mcp".to_string(),
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

    async fn body_json(resp: Response) -> serde_json::Value {
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body collects");
        serde_json::from_slice(&body).expect("response body is JSON")
    }

    #[tokio::test]
    async fn missing_token_renders_json_with_metadata() {
        let gate = build_gate();
        let resp = render_rejection(&gate, BearerAuthOutcome::MissingToken).unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let www = resp
            .headers()
            .get(axum::http::header::WWW_AUTHENTICATE)
            .expect("WWW-Authenticate header set")
            .to_str()
            .unwrap()
            .to_string();
        assert!(www.starts_with("Bearer "), "got {www}");
        assert!(www.contains("realm=\"stackpit\""), "got {www}");
        assert!(www.contains("resource_metadata="), "got {www}");
        let json = body_json(resp).await;
        assert_eq!(json["error"], "missing_token");
        assert_eq!(json["error_description"], "Authorization header required");
        assert_eq!(
            json["resource_metadata"],
            "https://stackpit.example.com/.well-known/oauth-protected-resource"
        );
    }

    #[tokio::test]
    async fn invalid_token_renders_json_with_metadata() {
        let gate = build_gate();
        let resp = render_rejection(&gate, BearerAuthOutcome::InvalidToken).unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let www = resp
            .headers()
            .get(axum::http::header::WWW_AUTHENTICATE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(www.contains("error=\"invalid_token\""), "got {www}");
        let json = body_json(resp).await;
        assert_eq!(json["error"], "invalid_token");
        assert_eq!(
            json["resource_metadata"],
            "https://stackpit.example.com/.well-known/oauth-protected-resource"
        );
    }

    #[tokio::test]
    async fn insufficient_scope_renders_403_with_scope() {
        let gate = build_gate();
        let resp = render_rejection(
            &gate,
            BearerAuthOutcome::InsufficientScope {
                required: "stackpit:events:read".to_string(),
            },
        )
        .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let www = resp
            .headers()
            .get(axum::http::header::WWW_AUTHENTICATE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(www.contains("error=\"insufficient_scope\""), "got {www}");
        assert!(www.contains("scope=\"stackpit:events:read\""), "got {www}");
        let json = body_json(resp).await;
        assert_eq!(json["error"], "insufficient_scope");
        assert_eq!(json["scope"], "stackpit:events:read");
        assert_eq!(
            json["resource_metadata"],
            "https://stackpit.example.com/.well-known/oauth-protected-resource"
        );
    }
}
