use std::sync::atomic::{AtomicU64, Ordering};

/// Once-per-interval gate over a shared timestamp. `allow` returns `true` for
/// exactly one caller per `interval_secs` window, claiming the slot atomically.
#[derive(Default)]
pub struct Throttle(AtomicU64);

impl Throttle {
    pub fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    pub fn with_last(last_secs: u64) -> Self {
        Self(AtomicU64::new(last_secs))
    }

    /// Returns `true` if at least `interval_secs` have passed since the last
    /// granted call and this call wins the race to claim the new slot.
    pub fn allow(&self, now_secs: u64, interval_secs: u64) -> bool {
        let last = self.0.load(Ordering::Relaxed);
        now_secs.saturating_sub(last) >= interval_secs
            && self
                .0
                .compare_exchange(last, now_secs, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
    }
}
