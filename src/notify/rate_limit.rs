use crate::sliding_window::SlidingWindow;
use crate::throttle::Throttle;
use dashmap::DashMap;
use parking_lot::Mutex;

/// In-memory rate limiter (per-project + global tiers, 60s windows; 0 = unlimited).
pub struct NotifyRateLimiter {
    project_windows: DashMap<u64, SlidingWindow>,
    global_window: Mutex<SlidingWindow>,
    project_limit: u32,
    global_limit: u32,
    cleanup_throttle: Throttle,
}

impl NotifyRateLimiter {
    pub fn new(project_limit: u32, global_limit: u32) -> Self {
        Self {
            project_windows: DashMap::new(),
            global_window: Mutex::new(SlidingWindow::new()),
            project_limit,
            global_limit,
            cleanup_throttle: Throttle::new(),
        }
    }

    /// Returns `true` if the notification is allowed, `false` if rate-limited.
    pub fn check_and_record(&self, project_id: u64, now_secs: u64) -> bool {
        // Evict stale project entries every 2 minutes.
        if self.cleanup_throttle.allow(now_secs, 120) {
            self.project_windows
                .retain(|_, w| now_secs.saturating_sub(w.current_second) < 120);
        }

        // Check per-project limit but don't increment yet (global check may still reject).
        if self.project_limit > 0 {
            let mut entry = self
                .project_windows
                .entry(project_id)
                .or_insert_with(SlidingWindow::new);
            let window = entry.value_mut();
            window.advance(now_secs);
            if window.count() >= self.project_limit {
                return false;
            }
        }

        if self.global_limit > 0 {
            let mut global = self.global_window.lock();
            global.advance(now_secs);
            if global.count() >= self.global_limit {
                return false;
            }
            global.increment(now_secs);
        }

        // Both limits passed: now safe to increment per-project.
        if self.project_limit > 0 {
            if let Some(mut entry) = self.project_windows.get_mut(&project_id) {
                entry.value_mut().increment(now_secs);
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_under_limit() {
        let limiter = NotifyRateLimiter::new(5, 10);
        for i in 0..5 {
            assert!(limiter.check_and_record(1, 1000 + i));
        }
    }

    #[test]
    fn blocks_over_project_limit() {
        let limiter = NotifyRateLimiter::new(3, 100);
        assert!(limiter.check_and_record(1, 1000));
        assert!(limiter.check_and_record(1, 1000));
        assert!(limiter.check_and_record(1, 1000));
        assert!(!limiter.check_and_record(1, 1000));
    }

    #[test]
    fn blocks_over_global_limit() {
        let limiter = NotifyRateLimiter::new(0, 3);
        assert!(limiter.check_and_record(1, 1000));
        assert!(limiter.check_and_record(2, 1000));
        assert!(limiter.check_and_record(3, 1000));
        assert!(!limiter.check_and_record(4, 1000));
    }

    #[test]
    fn project_limit_independent() {
        let limiter = NotifyRateLimiter::new(2, 0);
        assert!(limiter.check_and_record(1, 1000));
        assert!(limiter.check_and_record(1, 1000));
        assert!(!limiter.check_and_record(1, 1000));
        // Different project still allowed
        assert!(limiter.check_and_record(2, 1000));
        assert!(limiter.check_and_record(2, 1000));
        assert!(!limiter.check_and_record(2, 1000));
    }

    #[test]
    fn window_resets_after_60s() {
        let limiter = NotifyRateLimiter::new(2, 0);
        assert!(limiter.check_and_record(1, 1000));
        assert!(limiter.check_and_record(1, 1000));
        assert!(!limiter.check_and_record(1, 1000));
        // 60 seconds later, window has cleared
        assert!(limiter.check_and_record(1, 1061));
    }

    #[test]
    fn zero_limit_means_unlimited() {
        let limiter = NotifyRateLimiter::new(0, 0);
        for i in 0..1000 {
            assert!(limiter.check_and_record(1, 1000 + i));
        }
    }
}
