#[derive(Clone, Debug, Default)]
pub struct IntervalAgg {
    pub target: u64,
    pub scheduled: u64,
    pub sent: u64,
    pub ok: u64,
    pub s503: u64,
    pub errors: u64,
    pub timeouts: u64,
    pub dropped: u64,
    pub persisted: u64,
    pub lag_p99_ms: f64,
}

pub const ACCEPT_RATIO: f64 = 0.90;
pub const ERROR_RATE_LIMIT: f64 = 0.01;
pub const CLIENT_LAG_LIMIT_MS: f64 = 250.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Continue,
    Knee,
    ClientSaturated,
}

pub fn interval_failing(a: &IntervalAgg) -> bool {
    if a.scheduled == 0 {
        return true;
    }
    let accept_lag = (a.ok as f64) < ACCEPT_RATIO * a.scheduled as f64;
    let persist_lag = (a.persisted as f64) < ACCEPT_RATIO * a.ok as f64;
    let bad = (a.s503 + a.errors + a.timeouts + a.dropped) as f64;
    accept_lag || persist_lag || bad / a.scheduled as f64 > ERROR_RATE_LIMIT
}

pub fn evaluate(history: &[IntervalAgg]) -> Verdict {
    if history.len() < 2 {
        return Verdict::Continue;
    }
    let last2 = &history[history.len() - 2..];
    if last2.iter().all(|a| a.lag_p99_ms > CLIENT_LAG_LIMIT_MS) {
        return Verdict::ClientSaturated;
    }
    if last2.iter().all(interval_failing) {
        Verdict::Knee
    } else {
        Verdict::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy(target: u64) -> IntervalAgg {
        let scheduled = target * 20;
        IntervalAgg {
            target,
            scheduled,
            sent: scheduled,
            ok: scheduled,
            persisted: scheduled,
            lag_p99_ms: 2.0,
            ..Default::default()
        }
    }

    #[test]
    fn continues_when_healthy() {
        let h = vec![healthy(250), healthy(500), healthy(750)];
        assert!(matches!(evaluate(&h), Verdict::Continue));
    }

    #[test]
    fn continues_with_single_interval() {
        assert!(matches!(evaluate(&[healthy(250)]), Verdict::Continue));
    }

    #[test]
    fn knee_on_accept_lag_two_intervals() {
        let mut a = healthy(1000);
        a.ok = (a.scheduled as f64 * 0.8) as u64;
        let h = vec![healthy(750), a.clone(), a];
        assert!(matches!(evaluate(&h), Verdict::Knee));
    }

    #[test]
    fn no_knee_on_single_bad_interval() {
        let mut a = healthy(1000);
        a.ok = (a.scheduled as f64 * 0.8) as u64;
        let h = vec![healthy(750), a, healthy(1250)];
        assert!(matches!(evaluate(&h), Verdict::Continue));
    }

    #[test]
    fn knee_on_persisted_lag() {
        let mut a = healthy(1000);
        a.persisted = (a.ok as f64 * 0.5) as u64;
        let h = vec![a.clone(), a];
        assert!(matches!(evaluate(&h), Verdict::Knee));
    }

    #[test]
    fn knee_on_error_rate() {
        let mut a = healthy(1000);
        a.s503 = a.scheduled / 50; // 2%
        let h = vec![a.clone(), a];
        assert!(matches!(evaluate(&h), Verdict::Knee));
    }

    #[test]
    fn client_saturation_wins_over_knee() {
        let mut a = healthy(1000);
        a.lag_p99_ms = 400.0;
        a.ok = 0;
        let h = vec![a.clone(), a];
        assert!(matches!(evaluate(&h), Verdict::ClientSaturated));
    }

    #[test]
    fn interval_failing_matches_evaluate_criteria() {
        assert!(!interval_failing(&healthy(500)));
        let mut a = healthy(500);
        a.ok = (a.scheduled as f64 * 0.5) as u64;
        assert!(interval_failing(&a));
    }
}
