//! Per-client-IP fixed-window limiter that counts ingest AUTH FAILURES only; successful (authenticated) ingest never touches it, so a valid key is never throttled and only IPs flooding rejected keys accrue a budget and get cut off before the next DB lookup.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

const FAILURE_WINDOW_SECS: u64 = 60;
// A misconfigured SDK repeats the same bad key (absorbed by the negative cache), so 100 failures/min/IP leaves headroom for a few broken apps behind one NAT while capping a unique-key DB flood.
const FAILURE_BUDGET: u32 = 100;

struct IpBucket {
    count: u32,
    window_start: u64,
}

struct Inner {
    buckets: HashMap<String, IpBucket>,
    last_cleanup: u64,
}

pub struct FailureLimiter {
    inner: Mutex<Inner>,
    window_secs: u64,
    budget: u32,
}

pub type SharedFailureLimiter = Arc<FailureLimiter>;

pub fn new_failure_limiter() -> SharedFailureLimiter {
    Arc::new(FailureLimiter::new(FAILURE_WINDOW_SECS, FAILURE_BUDGET))
}

impl FailureLimiter {
    fn new(window_secs: u64, budget: u32) -> Self {
        Self {
            inner: Mutex::new(Inner {
                buckets: HashMap::new(),
                last_cleanup: 0,
            }),
            window_secs,
            budget,
        }
    }

    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    /// True when this IP has already spent its failure budget in the current window; read-only, never increments, so a well-behaved IP stays at 0.
    pub fn is_over_budget(&self, ip: &str) -> bool {
        self.is_over_budget_at(ip, Self::now())
    }

    /// Records one auth failure for this IP.
    pub fn record_failure(&self, ip: &str) {
        self.record_failure_at(ip, Self::now());
    }

    fn is_over_budget_at(&self, ip: &str, now: u64) -> bool {
        let mut inner = self.inner.lock();
        self.cleanup(&mut inner, now);
        match inner.buckets.get_mut(ip) {
            Some(bucket) => {
                if now.saturating_sub(bucket.window_start) >= self.window_secs {
                    bucket.count = 0;
                    bucket.window_start = now;
                }
                bucket.count >= self.budget
            }
            None => false,
        }
    }

    fn record_failure_at(&self, ip: &str, now: u64) {
        let mut inner = self.inner.lock();
        let bucket = inner.buckets.entry(ip.to_owned()).or_insert(IpBucket {
            count: 0,
            window_start: now,
        });
        if now.saturating_sub(bucket.window_start) >= self.window_secs {
            bucket.count = 0;
            bucket.window_start = now;
        }
        bucket.count = bucket.count.saturating_add(1);
    }

    fn cleanup(&self, inner: &mut Inner, now: u64) {
        if now.saturating_sub(inner.last_cleanup) >= self.window_secs {
            let window = self.window_secs;
            inner
                .buckets
                .retain(|_, bucket| now.saturating_sub(bucket.window_start) < window);
            inner.last_cleanup = now;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_behaved_ip_is_never_over_budget() {
        let limiter = FailureLimiter::new(60, 100);
        for t in 0..10_000 {
            assert!(!limiter.is_over_budget_at("1.2.3.4", t));
        }
    }

    #[test]
    fn budget_trips_only_after_enough_failures() {
        let limiter = FailureLimiter::new(60, 3);
        assert!(!limiter.is_over_budget_at("ip", 0));
        limiter.record_failure_at("ip", 0);
        limiter.record_failure_at("ip", 0);
        assert!(!limiter.is_over_budget_at("ip", 0));
        limiter.record_failure_at("ip", 0);
        assert!(limiter.is_over_budget_at("ip", 0));
    }

    #[test]
    fn window_resets_after_expiry() {
        let limiter = FailureLimiter::new(60, 2);
        limiter.record_failure_at("ip", 0);
        limiter.record_failure_at("ip", 0);
        assert!(limiter.is_over_budget_at("ip", 0));
        assert!(!limiter.is_over_budget_at("ip", 60));
    }

    #[test]
    fn failures_are_scoped_per_ip() {
        let limiter = FailureLimiter::new(60, 1);
        limiter.record_failure_at("a", 0);
        assert!(limiter.is_over_budget_at("a", 0));
        assert!(!limiter.is_over_budget_at("b", 0));
    }
}
