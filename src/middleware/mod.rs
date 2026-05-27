mod csrf;
mod rate_limit;
mod web_auth;

pub(crate) use stackpit_auth::cookie;
pub use stackpit_auth::hash_token_for_cookie;

pub use csrf::{csrf_middleware, derive_admin_csrf_token, CsrfConfig, CsrfToken};
pub use rate_limit::{new_rate_limiter_state, rate_limit_middleware};
pub use web_auth::web_auth_middleware;

pub async fn security_headers_middleware(
    req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::HeaderValue;
    let is_web = req.uri().path().starts_with("/web/");
    let mut resp = next.run(req).await;
    let h = resp.headers_mut();
    h.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    h.insert("x-frame-options", HeaderValue::from_static("DENY"));
    h.insert(
        "referrer-policy",
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    // `style-src 'unsafe-inline'` remains pending an inline-style extraction pass.
    h.insert(
        "content-security-policy",
        HeaderValue::from_static(
            "default-src 'self'; \
             style-src 'self' 'unsafe-inline'; \
             script-src 'self'; \
             img-src 'self' data:; \
             frame-ancestors 'none'; \
             object-src 'none'; \
             base-uri 'self'; \
             form-action 'self'",
        ),
    );
    if is_web {
        h.insert(
            "cache-control",
            HeaderValue::from_static("no-store, private"),
        );
    }
    resp
}
