use crate::driver::Outcome;
use crate::knee::IntervalAgg;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
pub struct SecondBucket {
    pub phase: &'static str,
    pub target: u64,
    pub scheduled: u64,
    pub dropped: u64,
    pub ok: u64,
    pub s429: u64,
    pub s503: u64,
    pub errors: u64,
    pub timeouts: u64,
    pub latencies_ms: Vec<f64>,
    pub lags_ms: Vec<f64>,
    pub persisted: u64,
    pub db_bytes: u64,
    pub wal_bytes: u64,
}

impl SecondBucket {
    pub fn sent(&self) -> u64 {
        self.ok + self.s429 + self.s503 + self.errors + self.timeouts
    }
}

/// p in 0.0..=1.0 over an ascending-sorted slice; nearest-rank.
pub fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p).floor() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

pub struct Collector {
    buckets: Mutex<BTreeMap<u64, SecondBucket>>,
    got_429: AtomicBool,
}

impl Collector {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            buckets: Mutex::new(BTreeMap::new()),
            got_429: AtomicBool::new(false),
        })
    }

    pub fn note_tick(&self, second: u64, target: u64, phase: &'static str) {
        let mut b = self.buckets.lock().unwrap();
        let e = b.entry(second).or_default();
        e.scheduled += 1;
        e.target = target;
        e.phase = phase;
    }

    pub fn record(&self, second: u64, outcome: Outcome, latency_ms: f64, lag_ms: f64) {
        if outcome == Outcome::S429 {
            self.got_429.store(true, Ordering::Relaxed);
        }
        let mut b = self.buckets.lock().unwrap();
        let e = b.entry(second).or_default();
        match outcome {
            Outcome::Ok => e.ok += 1,
            Outcome::S429 => e.s429 += 1,
            Outcome::S503 => e.s503 += 1,
            Outcome::Error => e.errors += 1,
            Outcome::Timeout => e.timeouts += 1,
            Outcome::Dropped => e.dropped += 1,
        }
        if outcome != Outcome::Dropped {
            e.latencies_ms.push(latency_ms);
        }
        e.lags_ms.push(lag_ms);
    }

    pub fn record_db_sample(&self, second: u64, persisted: u64, db_bytes: u64, wal_bytes: u64) {
        let mut b = self.buckets.lock().unwrap();
        let e = b.entry(second).or_default();
        e.persisted += persisted;
        e.db_bytes = db_bytes;
        e.wal_bytes = wal_bytes;
    }

    pub fn saw_429(&self) -> bool {
        self.got_429.load(Ordering::Relaxed)
    }

    pub fn aggregate(&self, from_s: u64, to_s: u64, target: u64) -> IntervalAgg {
        let b = self.buckets.lock().unwrap();
        let mut agg = IntervalAgg {
            target,
            ..Default::default()
        };
        let mut lags: Vec<f64> = Vec::new();
        for (_, e) in b.range(from_s..to_s) {
            agg.scheduled += e.scheduled;
            agg.ok += e.ok;
            agg.s503 += e.s503;
            agg.errors += e.errors;
            agg.timeouts += e.timeouts;
            agg.dropped += e.dropped;
            agg.persisted += e.persisted;
            agg.sent += e.sent();
            lags.extend_from_slice(&e.lags_ms);
        }
        lags.sort_by(|a, b| a.partial_cmp(b).unwrap());
        agg.lag_p99_ms = percentile(&lags, 0.99);
        agg
    }

    pub fn snapshot(&self) -> BTreeMap<u64, SecondBucket> {
        self.buckets.lock().unwrap().clone()
    }
}

pub fn write_csv(path: &Path, buckets: &BTreeMap<u64, SecondBucket>) -> anyhow::Result<()> {
    let mut out = String::from(
        "second,phase,target,scheduled,sent,dropped,ok,s429,s503,errors,timeouts,p50_ms,p99_ms,persisted,db_bytes,wal_bytes\n",
    );
    for (s, e) in buckets {
        let mut lat = e.latencies_ms.clone();
        lat.sort_by(|a, b| a.partial_cmp(b).unwrap());
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{:.2},{:.2},{},{},{}\n",
            s,
            e.phase,
            e.target,
            e.scheduled,
            e.sent(),
            e.dropped,
            e.ok,
            e.s429,
            e.s503,
            e.errors,
            e.timeouts,
            percentile(&lat, 0.5),
            percentile(&lat, 0.99),
            e.persisted,
            e.db_bytes,
            e.wal_bytes
        ));
    }
    std::fs::write(path, out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_picks_expected_ranks() {
        let v: Vec<f64> = (1..=100).map(|i| i as f64).collect();
        assert_eq!(percentile(&v, 0.5), 50.0);
        assert_eq!(percentile(&v, 0.99), 99.0);
        assert_eq!(percentile(&[], 0.99), 0.0);
        assert_eq!(percentile(&[7.0], 0.5), 7.0);
    }

    #[test]
    fn collector_buckets_by_second_and_aggregates() {
        let c = Collector::new();
        c.note_tick(3, 1000, "ramp");
        c.note_tick(3, 1000, "ramp");
        c.record(3, crate::driver::Outcome::Ok, 4.0, 1.0);
        c.record(3, crate::driver::Outcome::S503, 2.0, 1.0);
        c.record_db_sample(3, 900, 4096, 1024);
        let agg = c.aggregate(0, 4, 1000);
        assert_eq!(agg.scheduled, 2);
        assert_eq!(agg.ok, 1);
        assert_eq!(agg.s503, 1);
        assert_eq!(agg.persisted, 900);
        assert_eq!(agg.target, 1000);
    }

    #[test]
    fn db_samples_same_second_sum_deltas_and_keep_latest_gauges() {
        let c = Collector::new();
        c.record_db_sample(7, 100, 4096, 1024);
        c.record_db_sample(7, 50, 8192, 2048);
        let buckets = c.snapshot();
        let b = &buckets[&7];
        assert_eq!(b.persisted, 150);
        assert_eq!(b.db_bytes, 8192);
        assert_eq!(b.wal_bytes, 2048);
    }

    #[test]
    fn collector_flags_429() {
        let c = Collector::new();
        assert!(!c.saw_429());
        c.record(0, crate::driver::Outcome::S429, 1.0, 0.0);
        assert!(c.saw_429());
    }

    #[test]
    fn csv_writes_header_and_rows() {
        let c = Collector::new();
        c.note_tick(0, 250, "ramp");
        c.record(0, crate::driver::Outcome::Ok, 3.0, 0.5);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run.csv");
        write_csv(&path, &c.snapshot()).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let mut lines = text.lines();
        assert_eq!(
            lines.next().unwrap(),
            "second,phase,target,scheduled,sent,dropped,ok,s429,s503,errors,timeouts,p50_ms,p99_ms,persisted,db_bytes,wal_bytes"
        );
        assert!(lines
            .next()
            .unwrap()
            .starts_with("0,ramp,250,1,1,0,1,0,0,0,0,"));
    }
}
