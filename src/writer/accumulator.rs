use crate::ingest::models::{ItemType, Level, StorableEvent, MAX_TAGS_PER_EVENT};
use simple_hll::HyperLogLog;
use std::collections::HashMap;
use std::time::Instant;

/// Test default; production batch size comes from `storage.ingest_batch_size`.
#[cfg(test)]
pub(super) const BATCH_SIZE: usize = 2000;
pub(super) const AGGREGATION_FLUSH_INTERVAL_MS: u128 = 1000;
pub(super) const AGGREGATION_FLUSH_FINGERPRINT_THRESHOLD: usize = 1000;
pub(super) const AGGREGATION_FLUSH_TAG_THRESHOLD: usize = 10_000;

/// Number of log2 duration histogram buckets per transaction rollup row.
pub(super) const TXN_DURATION_BUCKETS: usize = 24;

/// Map a duration in milliseconds to its log2 histogram bucket, clamped to
/// `0..=23`. Bucket b covers `[2^b, 2^(b+1))` ms.
pub(super) fn duration_bucket(ms: i64) -> usize {
    let ms = ms.max(1) as u64;
    ((63 - ms.leading_zeros()) as usize).min(TXN_DURATION_BUCKETS - 1)
}

/// Floor a unix-seconds timestamp to UTC midnight.
pub(super) fn day_bucket(ts: i64) -> i64 {
    (ts / 86400) * 86400
}

/// A transaction is "failed" when its trace status is set and not one of the
/// healthy terminal states. A missing status is treated as success.
pub(super) fn is_failed(status: Option<&str>) -> bool {
    match status {
        Some(s) => !matches!(s, "ok" | "cancelled" | "unknown"),
        None => false,
    }
}

/// Tracks accumulated changes for a single fingerprint between flushes.
#[derive(Clone)]
pub(super) struct IssueDelta {
    pub project_id: u64,
    pub event_count: u64,
    pub first_seen: i64,
    pub last_seen: i64,
    pub title: Option<String>,
    pub level: Option<Level>,
    pub item_type: String,
    pub hll: HyperLogLog<12>,
    pub has_hll_data: bool,
}

/// Tracks accumulated session rollup for a (project, release, environment).
#[derive(Clone)]
pub(super) struct SessionDelta {
    pub total: u64,
    pub crashed: u64,
    pub errored: u64,
    pub abnormal: u64,
    pub first_seen: i64,
    pub last_seen: i64,
    pub users_hll: HyperLogLog<12>,
    pub users_crashed_hll: HyperLogLog<12>,
    pub has_user_data: bool,
    pub has_aggregate: bool,
}

impl SessionDelta {
    fn new() -> Self {
        Self {
            total: 0,
            crashed: 0,
            errored: 0,
            abnormal: 0,
            first_seen: i64::MAX,
            last_seen: i64::MIN,
            users_hll: HyperLogLog::new(),
            users_crashed_hll: HyperLogLog::new(),
            has_user_data: false,
            has_aggregate: false,
        }
    }
}

/// Tracks accumulated transaction performance rollup for a
/// (project, transaction_name, hour_bucket).
#[derive(Clone)]
pub(super) struct TxnDelta {
    pub count: u64,
    pub sum_duration_ms: u64,
    pub failed_count: u64,
    pub buckets: [u64; TXN_DURATION_BUCKETS],
    pub users_hll: HyperLogLog<12>,
    pub has_user_data: bool,
    pub first_seen: i64,
    pub last_seen: i64,
}

impl TxnDelta {
    fn new() -> Self {
        Self {
            count: 0,
            sum_duration_ms: 0,
            failed_count: 0,
            buckets: [0; TXN_DURATION_BUCKETS],
            users_hll: HyperLogLog::new(),
            has_user_data: false,
            first_seen: i64::MAX,
            last_seen: i64::MIN,
        }
    }
}

/// Holds in-memory issue and tag deltas until we're ready to flush them to SQLite.
#[derive(Clone)]
pub(super) struct Accumulators {
    pub issues: HashMap<String, IssueDelta>,
    /// Tag counts -- keyed by (fingerprint, tag_key, tag_value)
    pub tags: HashMap<(String, String, String), u64>,
    /// Session rollups -- keyed by (project_id, release, environment, day_bucket)
    pub session_aggregates: HashMap<(u64, String, String, i64), SessionDelta>,
    /// Transaction perf rollups -- keyed by (project_id, transaction_name, hour_bucket)
    pub transaction_metrics: HashMap<(u64, String, i64), TxnDelta>,
    pub last_flush: Instant,
}

impl Accumulators {
    pub fn new() -> Self {
        Self {
            issues: HashMap::new(),
            tags: HashMap::new(),
            session_aggregates: HashMap::new(),
            transaction_metrics: HashMap::new(),
            last_flush: Instant::now(),
        }
    }

    pub fn accumulate(&mut self, event: &StorableEvent) {
        // Sessions carry no fingerprint, so roll them up before the early-return.
        self.accumulate_sessions(event);
        // Transactions carry no fingerprint either -- roll up before returning.
        self.accumulate_transactions(event);

        let fp = match event.fingerprint.as_ref() {
            Some(fp) => fp,
            None => return,
        };

        let delta = self.issues.entry(fp.clone()).or_insert_with(|| IssueDelta {
            project_id: event.project_id,
            event_count: 0,
            first_seen: i64::MAX,
            last_seen: i64::MIN,
            title: None,
            level: None,
            item_type: event.item_type.as_str().to_string(),
            hll: HyperLogLog::new(),
            has_hll_data: false,
        });

        delta.event_count += 1;
        // Skip bogus timestamps -- negative or more than a year in the future
        let ts = event.timestamp;
        if ts > 0 && ts < chrono::Utc::now().timestamp() + 86400 * 365 {
            delta.first_seen = delta.first_seen.min(ts);
            delta.last_seen = delta.last_seen.max(ts);
        }
        if event.title.is_some() {
            delta.title.clone_from(&event.title);
        }
        if event.level.is_some() {
            delta.level.clone_from(&event.level);
        }

        if let Some(ref user_id) = event.user_identifier {
            delta.hll.add_object(user_id);
            delta.has_hll_data = true;
        }

        for (key, value) in event.tags.iter().take(MAX_TAGS_PER_EVENT) {
            *self
                .tags
                .entry((fp.clone(), key.clone(), value.clone()))
                .or_insert(0) += 1;
        }
    }

    fn accumulate_sessions(&mut self, event: &StorableEvent) {
        if event.session_buckets.is_empty() {
            return;
        }
        for bucket in &event.session_buckets {
            // `started_ts` carries the session's own start (parsed from rfc3339
            // in the envelope; the aggregate's bucket start otherwise), falling
            // back to the event timestamp. Bucketing by it keeps the daily trend
            // honest instead of collapsing every session onto the ingest day.
            let seen = bucket.started_ts;
            let key = (
                event.project_id,
                bucket.release.clone(),
                bucket.environment.clone(),
                day_bucket(seen),
            );
            let delta = self
                .session_aggregates
                .entry(key)
                .or_insert_with(SessionDelta::new);

            // A session in a terminal state still counts toward the total. An
            // individual `crashed`/`errored`/`abnormal` update with init=false
            // carries total=0, so floor the total at the terminal count to keep
            // crashed+errored+abnormal <= total. Aggregates already satisfy this.
            let terminal = bucket.crashed + bucket.errored + bucket.abnormal;
            delta.total += bucket.total.max(terminal);
            delta.crashed += bucket.crashed;
            delta.errored += bucket.errored;
            delta.abnormal += bucket.abnormal;

            if seen > 0 {
                delta.first_seen = delta.first_seen.min(seen);
                delta.last_seen = delta.last_seen.max(seen);
            }

            if let Some(ref did) = bucket.did {
                delta.users_hll.add_object(did);
                delta.has_user_data = true;
                if bucket.crashed > 0 {
                    delta.users_crashed_hll.add_object(did);
                }
            }
            if bucket.is_aggregate {
                delta.has_aggregate = true;
            }
        }
    }

    fn accumulate_transactions(&mut self, event: &StorableEvent) {
        if event.item_type != ItemType::Transaction {
            return;
        }
        let Some(duration) = event.duration_ms else {
            return;
        };
        if event.transaction_name.is_none() {
            return;
        }
        let name = event.transaction_name.clone().unwrap();

        let ts = event.timestamp;
        let hour_bucket = (ts / 3600) * 3600;
        let key = (event.project_id, name, hour_bucket);
        let delta = self
            .transaction_metrics
            .entry(key)
            .or_insert_with(TxnDelta::new);

        delta.count += 1;
        delta.sum_duration_ms += duration.max(0) as u64;
        delta.buckets[duration_bucket(duration)] += 1;
        if is_failed(event.trace_status.as_deref()) {
            delta.failed_count += 1;
        }
        delta.first_seen = delta.first_seen.min(ts);
        delta.last_seen = delta.last_seen.max(ts);

        if let Some(ref user_id) = event.user_identifier {
            delta.users_hll.add_object(user_id);
            delta.has_user_data = true;
        }
    }

    /// Fold `other` (a batch-local scratch) into `self` with the same combination semantics as `accumulate`, iterating only `other`'s batch-bounded entries and leaving `last_flush` untouched.
    pub fn merge(&mut self, other: &Accumulators) {
        for (fp, src) in &other.issues {
            match self.issues.get_mut(fp) {
                Some(dst) => {
                    dst.event_count += src.event_count;
                    dst.first_seen = dst.first_seen.min(src.first_seen);
                    dst.last_seen = dst.last_seen.max(src.last_seen);
                    if src.title.is_some() {
                        dst.title.clone_from(&src.title);
                    }
                    if src.level.is_some() {
                        dst.level = src.level;
                    }
                    dst.hll.merge(&src.hll);
                    dst.has_hll_data |= src.has_hll_data;
                }
                None => {
                    self.issues.insert(fp.clone(), src.clone());
                }
            }
        }

        for (key, count) in &other.tags {
            *self.tags.entry(key.clone()).or_insert(0) += *count;
        }

        for (key, src) in &other.session_aggregates {
            match self.session_aggregates.get_mut(key) {
                Some(dst) => {
                    dst.total += src.total;
                    dst.crashed += src.crashed;
                    dst.errored += src.errored;
                    dst.abnormal += src.abnormal;
                    dst.first_seen = dst.first_seen.min(src.first_seen);
                    dst.last_seen = dst.last_seen.max(src.last_seen);
                    dst.users_hll.merge(&src.users_hll);
                    dst.users_crashed_hll.merge(&src.users_crashed_hll);
                    dst.has_user_data |= src.has_user_data;
                    dst.has_aggregate |= src.has_aggregate;
                }
                None => {
                    self.session_aggregates.insert(key.clone(), src.clone());
                }
            }
        }

        for (key, src) in &other.transaction_metrics {
            match self.transaction_metrics.get_mut(key) {
                Some(dst) => {
                    dst.count += src.count;
                    dst.sum_duration_ms += src.sum_duration_ms;
                    dst.failed_count += src.failed_count;
                    for (d, s) in dst.buckets.iter_mut().zip(src.buckets.iter()) {
                        *d += *s;
                    }
                    dst.users_hll.merge(&src.users_hll);
                    dst.has_user_data |= src.has_user_data;
                    dst.first_seen = dst.first_seen.min(src.first_seen);
                    dst.last_seen = dst.last_seen.max(src.last_seen);
                }
                None => {
                    self.transaction_metrics.insert(key.clone(), src.clone());
                }
            }
        }
    }

    pub fn should_flush(&self) -> bool {
        self.last_flush.elapsed().as_millis() >= AGGREGATION_FLUSH_INTERVAL_MS
            || self.issues.len() >= AGGREGATION_FLUSH_FINGERPRINT_THRESHOLD
            || self.tags.len() >= AGGREGATION_FLUSH_TAG_THRESHOLD
            || self.session_aggregates.len() >= AGGREGATION_FLUSH_FINGERPRINT_THRESHOLD
            || self.transaction_metrics.len() >= AGGREGATION_FLUSH_FINGERPRINT_THRESHOLD
    }

    pub fn is_empty(&self) -> bool {
        self.issues.is_empty()
            && self.tags.is_empty()
            && self.session_aggregates.is_empty()
            && self.transaction_metrics.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_bucket_boundaries() {
        assert_eq!(duration_bucket(1), 0);
        assert_eq!(duration_bucket(2), 1);
        assert_eq!(duration_bucket(3), 1);
        assert_eq!(duration_bucket(4), 2);
        assert_eq!(duration_bucket(1024), 10);
        assert_eq!(duration_bucket(0), 0);
        assert_eq!(duration_bucket(-5), 0);
        assert_eq!(duration_bucket(i64::MAX), 23);
        assert_eq!(duration_bucket(1 << 23), 23);
        assert_eq!(duration_bucket(1 << 24), 23);
    }

    #[test]
    fn day_bucket_floors_to_utc_midnight() {
        assert_eq!(day_bucket(0), 0);
        assert_eq!(day_bucket(86399), 0);
        assert_eq!(day_bucket(86400), 86400);
        assert_eq!(day_bucket(86401), 86400);
        // 2021-01-01 12:00:00 UTC -> floor to 2021-01-01 00:00:00 UTC
        assert_eq!(day_bucket(1_609_502_400), 1_609_459_200);
    }

    use crate::ingest::models::SessionBucket;

    fn session_event(event_id: &str, ts: i64, bucket: SessionBucket) -> StorableEvent {
        let mut e = StorableEvent::new(
            event_id.to_string(),
            ItemType::Session,
            vec![0],
            1,
            "k".to_string(),
        );
        e.timestamp = ts;
        e.release = Some(bucket.release.clone());
        e.session_buckets = vec![bucket];
        e
    }

    fn sess_bucket(ts: i64, total: u64, crashed: u64) -> SessionBucket {
        SessionBucket {
            release: "app@1.0".to_string(),
            environment: "prod".to_string(),
            started_ts: ts,
            total,
            crashed,
            errored: 0,
            abnormal: 0,
            did: None,
            is_aggregate: false,
        }
    }

    #[test]
    fn sessions_on_two_days_produce_two_keys() {
        let mut acc = Accumulators::new();
        let day1 = 1_609_459_200; // 2021-01-01 00:00:00 UTC
        let day2 = day1 + 86400; // 2021-01-02

        acc.accumulate(&session_event(
            "d1",
            day1 + 100,
            sess_bucket(day1 + 100, 5, 1),
        ));
        acc.accumulate(&session_event(
            "d1b",
            day1 + 200,
            sess_bucket(day1 + 200, 3, 0),
        ));
        acc.accumulate(&session_event(
            "d2",
            day2 + 100,
            sess_bucket(day2 + 100, 7, 2),
        ));

        assert_eq!(acc.session_aggregates.len(), 2);
        let k1 = (1u64, "app@1.0".to_string(), "prod".to_string(), day1);
        let k2 = (1u64, "app@1.0".to_string(), "prod".to_string(), day2);
        assert_eq!(acc.session_aggregates[&k1].total, 8);
        assert_eq!(acc.session_aggregates[&k1].crashed, 1);
        assert_eq!(acc.session_aggregates[&k2].total, 7);
        assert_eq!(acc.session_aggregates[&k2].crashed, 2);
    }

    #[test]
    fn crashed_init_false_session_still_counts_toward_total() {
        let mut acc = Accumulators::new();
        let ts = 1_609_459_200; // 2021-01-01 00:00:00 UTC
                                // Mirrors the envelope output for a `crashed` session with init=false:
                                // total=0 but crashed=1. The accumulator must floor total at crashed.
        acc.accumulate(&session_event("s", ts, sess_bucket(ts, 0, 1)));

        let key = (1u64, "app@1.0".to_string(), "prod".to_string(), ts);
        let d = &acc.session_aggregates[&key];
        assert!(
            d.crashed <= d.total,
            "crashed {} must not exceed total {}",
            d.crashed,
            d.total
        );
        assert_eq!(d.total, 1);
        assert_eq!(d.crashed, 1);
    }

    #[test]
    fn is_failed_classification() {
        assert!(!is_failed(None));
        assert!(!is_failed(Some("ok")));
        assert!(!is_failed(Some("cancelled")));
        assert!(!is_failed(Some("unknown")));
        assert!(is_failed(Some("internal_error")));
        assert!(is_failed(Some("deadline_exceeded")));
    }

    fn txn_event(name: &str, ts: i64, duration: i64, status: Option<&str>) -> StorableEvent {
        let mut e = StorableEvent::new(
            format!("evt-{ts}-{duration}"),
            ItemType::Transaction,
            vec![0],
            7,
            "k".to_string(),
        );
        e.timestamp = ts;
        e.transaction_name = Some(name.to_string());
        e.duration_ms = Some(duration);
        e.trace_status = status.map(str::to_string);
        e
    }

    #[test]
    fn accumulates_transaction_rollup() {
        let mut acc = Accumulators::new();
        acc.accumulate(&txn_event("/api/x", 1000, 100, Some("ok")));
        acc.accumulate(&txn_event("/api/x", 1500, 200, Some("internal_error")));

        let key = (7u64, "/api/x".to_string(), 0i64);
        let delta = acc.transaction_metrics.get(&key).unwrap();
        assert_eq!(delta.count, 2);
        assert_eq!(delta.sum_duration_ms, 300);
        assert_eq!(delta.failed_count, 1);
        assert_eq!(delta.buckets[duration_bucket(100)], 1);
        assert_eq!(delta.buckets[duration_bucket(200)], 1);
        assert_eq!(delta.first_seen, 1000);
        assert_eq!(delta.last_seen, 1500);
    }

    #[test]
    fn separate_hour_buckets() {
        let mut acc = Accumulators::new();
        acc.accumulate(&txn_event("/api/x", 100, 50, Some("ok")));
        acc.accumulate(&txn_event("/api/x", 7300, 50, Some("ok")));
        assert_eq!(acc.transaction_metrics.len(), 2);
    }

    #[test]
    fn skips_transaction_without_duration() {
        let mut acc = Accumulators::new();
        let mut e = txn_event("/api/x", 100, 0, Some("ok"));
        e.duration_ms = None;
        acc.accumulate(&e);
        assert!(acc.transaction_metrics.is_empty());
    }

    fn issue_event(id: &str, fp: &str, ts: i64, user: Option<&str>) -> StorableEvent {
        let mut e =
            StorableEvent::new(id.to_string(), ItemType::Event, vec![0], 1, "k".to_string());
        e.fingerprint = Some(fp.to_string());
        e.timestamp = ts;
        e.title = Some(format!("title-{fp}"));
        e.level = Some(Level::Error);
        e.user_identifier = user.map(String::from);
        e
    }

    /// Pins `merge` to `accumulate`: folding a scratch accumulator into an empty one must yield the same rollups as accumulating the events directly.
    #[test]
    fn merge_is_equivalent_to_accumulate() {
        let sess = |id: &str, ts: i64, total: u64, crashed: u64, did: Option<&str>| {
            let mut b = sess_bucket(ts, total, crashed);
            b.did = did.map(String::from);
            session_event(id, ts, b)
        };

        let build = |acc: &mut Accumulators| {
            let mut e1 = issue_event("m1", "fp-a", 100, Some("u1"));
            e1.tags = vec![("browser".into(), "chrome".into())];
            let mut e2 = issue_event("m2", "fp-a", 200, Some("u2"));
            e2.tags = vec![
                ("browser".into(), "chrome".into()),
                ("os".into(), "linux".into()),
            ];
            let e3 = issue_event("m3", "fp-b", 150, Some("u1"));
            acc.accumulate(&e1);
            acc.accumulate(&e2);
            acc.accumulate(&e3);
            acc.accumulate(&sess("s1", 1000, 5, 1, Some("du1")));
            acc.accumulate(&sess("s2", 1000, 3, 0, Some("du2")));
            acc.accumulate(&txn_event("/api/x", 1000, 100, Some("ok")));
            acc.accumulate(&txn_event("/api/x", 1500, 200, Some("internal_error")));
        };

        // Direct accumulate.
        let mut direct = Accumulators::new();
        build(&mut direct);

        // Scratch, then merge into an empty accumulator.
        let mut scratch = Accumulators::new();
        build(&mut scratch);
        let mut merged = Accumulators::new();
        merged.merge(&scratch);

        // Issues.
        assert_eq!(direct.issues.len(), merged.issues.len());
        for (fp, d) in &direct.issues {
            let m = merged.issues.get(fp).expect("fingerprint present");
            assert_eq!(d.event_count, m.event_count, "event_count {fp}");
            assert_eq!(d.first_seen, m.first_seen, "first_seen {fp}");
            assert_eq!(d.last_seen, m.last_seen, "last_seen {fp}");
            assert_eq!(d.title, m.title, "title {fp}");
            assert_eq!(d.level, m.level, "level {fp}");
            assert_eq!(d.has_hll_data, m.has_hll_data, "has_hll_data {fp}");
            assert_eq!(d.hll.count(), m.hll.count(), "hll {fp}");
        }

        // Tags.
        assert_eq!(direct.tags, merged.tags);

        // Sessions.
        assert_eq!(
            direct.session_aggregates.len(),
            merged.session_aggregates.len()
        );
        for (k, d) in &direct.session_aggregates {
            let m = merged
                .session_aggregates
                .get(k)
                .expect("session key present");
            assert_eq!(d.total, m.total);
            assert_eq!(d.crashed, m.crashed);
            assert_eq!(d.errored, m.errored);
            assert_eq!(d.abnormal, m.abnormal);
            assert_eq!(d.first_seen, m.first_seen);
            assert_eq!(d.last_seen, m.last_seen);
            assert_eq!(d.has_user_data, m.has_user_data);
            assert_eq!(d.has_aggregate, m.has_aggregate);
            assert_eq!(d.users_hll.count(), m.users_hll.count());
            assert_eq!(d.users_crashed_hll.count(), m.users_crashed_hll.count());
        }

        // Transactions.
        assert_eq!(
            direct.transaction_metrics.len(),
            merged.transaction_metrics.len()
        );
        for (k, d) in &direct.transaction_metrics {
            let m = merged.transaction_metrics.get(k).expect("txn key present");
            assert_eq!(d.count, m.count);
            assert_eq!(d.sum_duration_ms, m.sum_duration_ms);
            assert_eq!(d.failed_count, m.failed_count);
            assert_eq!(d.buckets, m.buckets);
            assert_eq!(d.first_seen, m.first_seen);
            assert_eq!(d.last_seen, m.last_seen);
            assert_eq!(d.has_user_data, m.has_user_data);
            assert_eq!(d.users_hll.count(), m.users_hll.count());
        }
    }

    /// Merging into an occupied accumulator must combine deltas, matching what accumulating every event into one accumulator would produce.
    #[test]
    fn merge_into_occupied_combines_deltas() {
        let mut base = Accumulators::new();
        base.accumulate(&issue_event("a", "fp", 100, Some("u1")));

        let mut scratch = Accumulators::new();
        scratch.accumulate(&issue_event("b", "fp", 300, Some("u2")));
        base.merge(&scratch);

        let d = base.issues.get("fp").unwrap();
        assert_eq!(d.event_count, 2);
        assert_eq!(d.first_seen, 100);
        assert_eq!(d.last_seen, 300);
        assert_eq!(d.hll.count(), 2);
    }
}
