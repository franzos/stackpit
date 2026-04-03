use axum::extract::State;
use axum::middleware::Next;
use axum::response::IntoResponse;
use subtle::ConstantTimeEq;

use crate::encoding::percent_decode;

const CSRF_COOKIE_NAME: &str = "csrf_token";
const CSRF_TOKEN_LEN: usize = 16; // 128-bit hex token

#[derive(Clone)]
pub struct CsrfConfig {
    pub use_secure_cookies: bool,
    pub max_body_size: usize,
}

fn generate_csrf_token() -> String {
    let mut buf = [0u8; CSRF_TOKEN_LEN];
    rand::fill(&mut buf);
    hex::encode(buf)
}

fn extract_csrf_cookie(headers: &axum::http::HeaderMap) -> Option<String> {
    super::cookie::extract_cookie_value(headers, CSRF_COOKIE_NAME)
}

fn extract_csrf_field(body: &[u8]) -> Option<String> {
    let body_str = std::str::from_utf8(body).ok()?;
    for pair in body_str.split('&') {
        if let Some(val) = pair.strip_prefix("csrf_token=") {
            return Some(percent_decode(val.trim()));
        }
    }
    None
}

pub async fn csrf_middleware(
    State(config): State<CsrfConfig>,
    req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> axum::response::Response {
    let is_post = req.method() == axum::http::Method::POST;
    let path = req.uri().path();
    let is_web = path.starts_with("/web/");
    let is_login = path == "/web/login";
    let cookie_token = extract_csrf_cookie(req.headers());

    if is_post && is_web && !is_login {
        // Reject early if the CSRF cookie is missing -- no need to read the body.
        let cookie_val = match &cookie_token {
            Some(c) if !c.is_empty() => c.clone(),
            _ => {
                return (axum::http::StatusCode::FORBIDDEN, "CSRF token mismatch").into_response();
            }
        };

        let (parts, body) = req.into_parts();
        let bytes = match axum::body::to_bytes(body, config.max_body_size).await {
            Ok(b) => b,
            Err(_) => {
                return (axum::http::StatusCode::BAD_REQUEST, "request too large").into_response();
            }
        };

        let valid = match extract_csrf_field(&bytes) {
            Some(field) if !field.is_empty() => {
                cookie_val.as_bytes().ct_eq(field.as_bytes()).into()
            }
            _ => false,
        };

        if !valid {
            return (axum::http::StatusCode::FORBIDDEN, "CSRF token mismatch").into_response();
        }

        let req = axum::http::Request::from_parts(parts, axum::body::Body::from(bytes));
        next.run(req).await
    } else {
        let needs_cookie = is_web && cookie_token.is_none();
        let mut resp = next.run(req).await;
        if needs_cookie {
            let token = generate_csrf_token();
            let secure_flag = if config.use_secure_cookies {
                "; Secure"
            } else {
                ""
            };
            if let Ok(val) =
                format!("{CSRF_COOKIE_NAME}={token}; Path=/web; SameSite=Strict{secure_flag}")
                    .parse()
            {
                resp.headers_mut().append("set-cookie", val);
            }
        }
        resp
    }
}
