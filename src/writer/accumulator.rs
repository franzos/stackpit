use crate::models::{StorableEvent, MAX_TAGS_PER_EVENT};
use simple_hll::HyperLogLog;
use std::collections::HashMap;
use std::time::Instant;

pub(super) const BATCH_SIZE: usize = 2000;
pub(super) const AGGREGATION_FLUSH_INTERVAL_MS: u128 = 1000;
pub(super) const AGGREGATION_FLUSH_FINGERPRINT_THRESHOLD: usize = 1000;
pub(super) const AGGREGATION_FLUSH_TAG_THRESHOLD: usize = 10_000;

/// Tracks accumulated changes for a single fingerprint between flushes.
pub(super) struct IssueDelta {
    pub project_id: u64,
    pub event_count: u64,
    pub first_seen: i64,
    pub last_seen: i64,
    pub title: Option<String>,
    pub level: Option<String>,
    pub item_type: String,
    pub hll: HyperLogLog<12>,
    pub has_hll_data: bool,
}

/// Holds in-memory issue and tag deltas until we're ready to flush them to SQLite.
pub(super) struct Accumulators {
    pub issues: HashMap<String, IssueDelta>,
    /// Tag counts -- keyed by (fingerprint, tag_key, tag_value)
    pub tags: HashMap<(String, String, String), u64>,
    pub last_flush: Instant,
}

impl Accumulators {
    pub fn new() -> Self {
        Self {
            issues: HashMap::new(),
            tags: HashMap::new(),
            last_flush: Instant::now(),
        }
    }

    pub fn accumulate(&mut self, event: &StorableEvent) {
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

    pub fn should_flush(&self) -> bool {
        self.last_flush.elapsed().as_millis() >= AGGREGATION_FLUSH_INTERVAL_MS
            || self.issues.len() >= AGGREGATION_FLUSH_FINGERPRINT_THRESHOLD
            || self.tags.len() >= AGGREGATION_FLUSH_TAG_THRESHOLD
    }

    pub fn is_empty(&self) -> bool {
        self.issues.is_empty() && self.tags.is_empty()
    }
}
