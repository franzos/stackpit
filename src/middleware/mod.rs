mod csrf;
mod rate_limit;
mod web_auth;

pub(crate) use stackpit_auth::cookie;

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
    // /web/ defaults to no-store so a new page can't be cached by accident. A
    // handler that set its own value meant it -- the assets are served immutable
    // under version-stamped URLs and must survive this.
    if is_web && !h.contains_key("cache-control") {
        h.insert(
            "cache-control",
            HeaderValue::from_static("no-store, private"),
        );
    }
    resp
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    async fn cache_control_for(path: &'static str, handler_value: Option<&'static str>) -> String {
        let app = Router::new()
            .route(
                path,
                get(move || async move {
                    match handler_value {
                        Some(v) => {
                            ([(axum::http::header::CACHE_CONTROL, v)], "body").into_response()
                        }
                        None => "body".into_response(),
                    }
                }),
            )
            .layer(axum::middleware::from_fn(
                super::security_headers_middleware,
            ));

        let resp = app
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        resp.headers()
            .get("cache-control")
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default()
    }

    #[tokio::test]
    async fn web_pages_default_to_no_store() {
        assert_eq!(
            cache_control_for("/web/projects/", None).await,
            "no-store, private"
        );
    }

    // The header the asset handler sets is the whole caching story; overwriting
    // it made every font, stylesheet and script refetch on each page load.
    #[tokio::test]
    async fn assets_keep_their_own_cache_control() {
        assert_eq!(
            cache_control_for(
                "/web/_assets/style.css",
                Some("public, max-age=31536000, immutable")
            )
            .await,
            "public, max-age=31536000, immutable"
        );
    }
}
