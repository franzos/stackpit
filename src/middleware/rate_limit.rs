use axum::extract::State;
use axum::middleware::Next;
use axum::response::IntoResponse;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

const ADMIN_RATE_LIMIT: u32 = 120;
const LOGIN_RATE_LIMIT: u32 = 10;
const ADMIN_RATE_WINDOW_SECS: u64 = 60;

pub(crate) struct IpBucket {
    count: u32,
    window_start: u64,
}

struct RateLimiterInner {
    buckets: HashMap<String, IpBucket>,
    last_cleanup: u64,
}

pub struct RateLimiterState(Mutex<RateLimiterInner>);

pub type SharedRateLimiter = Arc<RateLimiterState>;

pub fn new_rate_limiter_state() -> SharedRateLimiter {
    Arc::new(RateLimiterState(Mutex::new(RateLimiterInner {
        buckets: HashMap::new(),
        last_cleanup: 0,
    })))
}

fn check_rate_limit(
    limiter: &SharedRateLimiter,
    req: &axum::http::Request<axum::body::Body>,
) -> bool {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let peer_addr = req
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|ci| ci.0);

    let ip = crate::network::extract_client_ip(req.headers(), peer_addr)
        .unwrap_or_else(|| "unknown".to_string());

    let mut inner = match limiter.0.lock() {
        Ok(m) => m,
        Err(_) => {
            tracing::error!("rate limiter mutex poisoned, failing closed");
            return false;
        }
    };

    // Periodic cleanup: evict stale entries once per window
    if now.saturating_sub(inner.last_cleanup) >= ADMIN_RATE_WINDOW_SECS {
        inner
            .buckets
            .retain(|_, bucket| now.saturating_sub(bucket.window_start) < ADMIN_RATE_WINDOW_SECS);
        inner.last_cleanup = now;
    }

    let is_login_post =
        req.uri().path() == "/web/login" && req.method() == axum::http::Method::POST;
    let (key, limit) = if is_login_post {
        (format!("{ip}:login"), LOGIN_RATE_LIMIT)
    } else {
        (ip, ADMIN_RATE_LIMIT)
    };

    let bucket = inner.buckets.entry(key).or_insert(IpBucket {
        count: 0,
        window_start: now,
    });

    if now.saturating_sub(bucket.window_start) >= ADMIN_RATE_WINDOW_SECS {
        bucket.count = 0;
        bucket.window_start = now;
    }

    if bucket.count >= limit {
        false
    } else {
        bucket.count += 1;
        true
    }
}

pub async fn rate_limit_middleware(
    State(limiter): State<SharedRateLimiter>,
    req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> axum::response::Response {
    if check_rate_limit(&limiter, &req) {
        next.run(req).await
    } else {
        (
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            [("Retry-After", "60")],
            "rate limit exceeded",
        )
            .into_response()
    }
}
