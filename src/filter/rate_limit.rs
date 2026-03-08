/// A sliding window over 60 one-second buckets. Pretty simple -- each bucket
/// holds the count for that second, and we zero out stale ones on advance.
pub(super) struct SlidingWindow {
    counts: [u32; 60],
    pub(super) current_second: u64,
}

impl SlidingWindow {
    pub fn new() -> Self {
        Self {
            counts: [0; 60],
            current_second: 0,
        }
    }

    pub fn advance(&mut self, now_secs: u64) {
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

    pub fn count(&self) -> u32 {
        self.counts.iter().sum()
    }

    pub fn increment(&mut self, now_secs: u64) {
        let idx = (now_secs % 60) as usize;
        self.counts[idx] = self.counts[idx].saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sliding_window_basic() {
        let mut w = SlidingWindow::new();
        w.advance(1000);
        w.increment(1000);
        w.increment(1000);
        assert_eq!(w.count(), 2);
    }

    #[test]
    fn sliding_window_clears_on_advance() {
        let mut w = SlidingWindow::new();
        w.advance(1000);
        w.increment(1000);
        // 60+ seconds later, everything should've been zeroed out
        w.advance(1061);
        assert_eq!(w.count(), 0);
    }

    #[test]
    fn sliding_window_independent_seconds() {
        let mut w = SlidingWindow::new();
        w.advance(100);
        w.increment(100);
        w.advance(101);
        w.increment(101);
        w.increment(101);
        assert_eq!(w.count(), 3);
    }
}
