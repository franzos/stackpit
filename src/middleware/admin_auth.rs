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

    // Bearer header: compare raw token. Cookie: compare SHA-256 hash so
    // the raw admin token is never stored in the browser cookie jar.
    let is_valid = if let Some(ref bearer) = extract_bearer_token(req.headers()) {
        !bearer.is_empty() && bearer.as_bytes().ct_eq(expected.as_bytes()).into()
    } else if let Some(ref cookie_val) = extract_auth_cookie(req.headers()) {
        let expected_hash = super::hash_token_for_cookie(expected);
        !cookie_val.is_empty() && cookie_val.as_bytes().ct_eq(expected_hash.as_bytes()).into()
    } else {
        false
    };

    if is_valid {
        next.run(req).await
    } else if wants_html(req.headers()) {
        login_form_response()
    } else {
        json_unauthorized()
    }
}
