mod admin_auth;
pub(crate) mod cookie;
mod csrf;
mod rate_limit;

pub use admin_auth::admin_auth_middleware;
pub use csrf::{csrf_middleware, CsrfConfig};
pub use rate_limit::{new_rate_limiter_state, rate_limit_middleware};

/// Derives a cookie-safe token from the admin token using SHA-256.
pub fn hash_token_for_cookie(token: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(token.as_bytes()))
}

pub async fn security_headers_middleware(
    req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let mut resp = next.run(req).await;
    let h = resp.headers_mut();
    h.insert("x-content-type-options", "nosniff".parse().unwrap());
    h.insert("x-frame-options", "DENY".parse().unwrap());
    h.insert(
        "referrer-policy",
        "strict-origin-when-cross-origin".parse().unwrap(),
    );
    h.insert(
        "content-security-policy",
        "default-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'; img-src 'self' data:; frame-ancestors 'none'"
            .parse()
            .unwrap(),
    );
    resp
}
