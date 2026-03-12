use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// A sliding window over 60 one-second buckets.
struct SlidingWindow {
    counts: [u32; 60],
    current_second: u64,
}

impl SlidingWindow {
    fn new() -> Self {
        Self {
            counts: [0; 60],
            current_second: 0,
        }
    }

    fn advance(&mut self, now_secs: u64) {
        if self.current_second == 0 {
            self.current_second = now_secs;
            return;
        }

        let elapsed = now_secs.saturating_sub(self.current_second);
        if elapsed == 0 {
            return;
        }

        if elapsed >= 60 {
            self.counts = [0; 60];
        } else {
            for i in 0..elapsed.min(60) {
                let idx = ((self.current_second + i + 1) % 60) as usize;
                self.counts[idx] = 0;
            }
        }
        self.current_second = now_secs;
    }

    fn count(&self) -> u32 {
        self.counts.iter().sum()
    }

    fn increment(&mut self, now_secs: u64) {
        let idx = (now_secs % 60) as usize;
        self.counts[idx] = self.counts[idx].saturating_add(1);
    }
}

/// In-memory rate limiter for outbound notifications.
///
/// Two tiers: per-project and global, both using 60-second sliding windows.
/// A limit of 0 means unlimited.
pub struct NotifyRateLimiter {
    project_windows: DashMap<u64, SlidingWindow>,
    global_window: Mutex<SlidingWindow>,
    project_limit: u32,
    global_limit: u32,
    last_cleanup: AtomicU64,
}

impl NotifyRateLimiter {
    pub fn new(project_limit: u32, global_limit: u32) -> Self {
        Self {
            project_windows: DashMap::new(),
            global_window: Mutex::new(SlidingWindow::new()),
            project_limit,
            global_limit,
            last_cleanup: AtomicU64::new(0),
        }
    }

    /// Returns `true` if the notification is allowed, `false` if rate-limited.
    pub fn check_and_record(&self, project_id: u64, now_secs: u64) -> bool {
        // Periodic cleanup: evict stale project entries every 2 minutes
        let last = self.last_cleanup.load(Ordering::Relaxed);
        if now_secs.saturating_sub(last) >= 120
            && self
                .last_cleanup
                .compare_exchange(last, now_secs, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            self.project_windows
                .retain(|_, w| now_secs.saturating_sub(w.current_second) < 120);
        }

        // Check per-project limit (don't increment yet -- wait for global check)
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

        // Check global limit
        if self.global_limit > 0 {
            let mut global = self.global_window.lock().unwrap_or_else(|e| e.into_inner());
            global.advance(now_secs);
            if global.count() >= self.global_limit {
                return false;
            }
            global.increment(now_secs);
        }

        // Both limits passed -- now safe to increment per-project
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
