use axum::extract::State;
use axum::middleware::Next;
use axum::response::IntoResponse;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

// 600 req/min comfortably covers a human browsing the UI (a single page load
// fans out into several asset requests) while still catching scripted abuse.
const ADMIN_RATE_LIMIT: u32 = 600;
const LOGIN_RATE_LIMIT: u32 = 10;
const ADMIN_RATE_WINDOW_SECS: u64 = 60;
// The limiter caps requests per IP but not the number of distinct IPs, so an
// address-rotating flood would otherwise grow `buckets` without bound.
const MAX_BUCKETS: usize = 100_000;

pub(crate) struct IpBucket {
    count: u32,
    window_start: u64,
}

struct RateLimiterInner {
    buckets: HashMap<String, IpBucket>,
    last_cleanup: u64,
}

pub struct RateLimiterState {
    inner: Mutex<RateLimiterInner>,
    trusted_proxies: Arc<crate::util::network::TrustedProxies>,
}

pub type SharedRateLimiter = Arc<RateLimiterState>;

pub fn new_rate_limiter_state(
    trusted_proxies: Arc<crate::util::network::TrustedProxies>,
) -> SharedRateLimiter {
    Arc::new(RateLimiterState {
        inner: Mutex::new(RateLimiterInner {
            buckets: HashMap::new(),
            last_cleanup: 0,
        }),
        trusted_proxies,
    })
}

/// Drops expired buckets first, then sheds arbitrary live ones until the map is
/// back under `cap`. Sheds a tenth of the cap on top so the O(n) sweep amortises
/// instead of running on every insert once the map is full.
fn evict_to_cap(buckets: &mut HashMap<String, IpBucket>, now: u64, cap: usize) {
    buckets.retain(|_, bucket| now.saturating_sub(bucket.window_start) < ADMIN_RATE_WINDOW_SECS);
    if buckets.len() < cap {
        return;
    }
    let excess = buckets.len() - cap + cap / 10 + 1;
    let victims: Vec<String> = buckets.keys().take(excess).cloned().collect();
    for key in victims {
        buckets.remove(&key);
    }
}

fn check_rate_limit(
    limiter: &SharedRateLimiter,
    req: &axum::http::Request<axum::body::Body>,
) -> bool {
    check_rate_limit_capped(limiter, req, MAX_BUCKETS)
}

fn check_rate_limit_capped(
    limiter: &SharedRateLimiter,
    req: &axum::http::Request<axum::body::Body>,
    cap: usize,
) -> bool {
    // Static assets are cheap and fan out per page load; don't count them against the bucket.
    if req.uri().path().starts_with("/web/_assets/") {
        return true;
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let peer_addr = req
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|ci| ci.0);

    let ip =
        crate::util::network::extract_client_ip(req.headers(), peer_addr, &limiter.trusted_proxies)
            .unwrap_or_else(|| "unknown".to_string());

    // `parking_lot::Mutex` doesn't poison: a panic inside can't fail-closed the admin surface.
    let mut inner = limiter.inner.lock();

    // Evict stale buckets once per window.
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

    if inner.buckets.len() >= cap && !inner.buckets.contains_key(&key) {
        evict_to_cap(&mut inner.buckets, now, cap);
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn limiter() -> SharedRateLimiter {
        new_rate_limiter_state(Arc::new(crate::util::network::TrustedProxies::default()))
    }

    fn login_request(peer: &str) -> axum::http::Request<axum::body::Body> {
        let mut req = axum::http::Request::builder()
            .method(axum::http::Method::POST)
            .uri("/web/login")
            .body(axum::body::Body::empty())
            .unwrap();
        let addr: std::net::SocketAddr = peer.parse().unwrap();
        req.extensions_mut()
            .insert(axum::extract::ConnectInfo(addr));
        req
    }

    #[test]
    fn login_limit_is_per_ip_not_global() {
        let limiter = limiter();
        for _ in 0..LOGIN_RATE_LIMIT {
            assert!(check_rate_limit(
                &limiter,
                &login_request("203.0.113.1:1000")
            ));
        }
        // First IP exhausted its bucket.
        assert!(!check_rate_limit(
            &limiter,
            &login_request("203.0.113.1:1000")
        ));
        // A different IP must still be allowed (no global lockout).
        assert!(check_rate_limit(
            &limiter,
            &login_request("203.0.113.2:1000")
        ));
    }

    #[test]
    fn missing_connect_info_shares_the_unknown_bucket() {
        let limiter = limiter();
        let req = axum::http::Request::builder()
            .method(axum::http::Method::POST)
            .uri("/web/login")
            .body(axum::body::Body::empty())
            .unwrap();
        for _ in 0..LOGIN_RATE_LIMIT {
            assert!(check_rate_limit(&limiter, &req));
        }
        assert!(!check_rate_limit(&limiter, &req));
    }

    // An address-rotating flood must not grow the bucket map without bound.
    #[test]
    fn distinct_ips_stay_under_the_cap() {
        let limiter = limiter();
        let cap = 20;
        for i in 0..500u32 {
            let req = login_request(&format!("198.51.100.{}:{}", i % 250, 1000 + i));
            assert!(check_rate_limit_capped(&limiter, &req, cap));
            assert!(limiter.inner.lock().buckets.len() <= cap);
        }
    }

    #[test]
    fn evict_to_cap_prefers_expired_buckets() {
        let mut buckets = HashMap::new();
        for i in 0..30 {
            buckets.insert(
                format!("ip{i}"),
                IpBucket {
                    count: 1,
                    window_start: if i < 25 { 0 } else { 1_000 },
                },
            );
        }
        evict_to_cap(&mut buckets, 1_000, 20);
        assert_eq!(buckets.len(), 5, "only the live buckets survive");
        assert!(buckets.contains_key("ip29"));
    }

    #[test]
    fn xff_from_loopback_peer_buckets_by_forwarded_ip() {
        let limiter = limiter();
        let mut req = login_request("127.0.0.1:1000");
        req.headers_mut()
            .insert("x-forwarded-for", "203.0.113.9".parse().unwrap());
        for _ in 0..LOGIN_RATE_LIMIT {
            assert!(check_rate_limit(&limiter, &req));
        }
        assert!(!check_rate_limit(&limiter, &req));
        // Loopback itself (no XFF) is a separate bucket.
        assert!(check_rate_limit(&limiter, &login_request("127.0.0.1:1000")));
    }
}
