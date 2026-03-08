use axum::extract::State;
use axum::middleware::Next;
use axum::response::IntoResponse;
use subtle::ConstantTimeEq;

fn extract_bearer_token(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn extract_auth_cookie(headers: &axum::http::HeaderMap) -> Option<String> {
    super::cookie::extract_cookie_value(headers, "stackpit_token")
}

fn wants_html(headers: &axum::http::HeaderMap) -> bool {
    headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.contains("text/html"))
}

fn login_form_response() -> axum::response::Response {
    axum::response::Redirect::to("/web/login").into_response()
}

fn json_unauthorized() -> axum::response::Response {
    (
        axum::http::StatusCode::UNAUTHORIZED,
        axum::Json(serde_json::json!({ "error": "unauthorized" })),
    )
        .into_response()
}

pub async fn admin_auth_middleware(
    State(token): State<Option<String>>,
    req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> axum::response::Response {
    let expected = match &token {
        Some(t) => t,
        None => return next.run(req).await,
    };

    // Let login and static assets through without auth
    let path = req.uri().path();
    if path == "/web/login" || path.starts_with("/web/_assets/") {
        return next.run(req).await;
    }

    // Try Bearer header first, then fall back to cookie
    let provided =
        extract_bearer_token(req.headers()).or_else(|| extract_auth_cookie(req.headers()));

    let is_valid = match &provided {
        Some(p) => !p.is_empty() && p.as_bytes().ct_eq(expected.as_bytes()).into(),
        None => false,
    };

    if is_valid {
        next.run(req).await
    } else if wants_html(req.headers()) {
        login_form_response()
    } else {
        json_unauthorized()
    }
}
