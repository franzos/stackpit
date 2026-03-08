use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::models::StorableEvent;

use super::cidr::CidrBlock;

use super::data::FilterData;
use super::glob::glob_match_any;
use super::rate_limit::SlidingWindow;
use super::rules::{FilterAction, FilterRule};
use super::verdict::{DropReason, FilterVerdict};
use super::{contains_ignore_ascii_case, starts_with_ignore_ascii_case};

/// Why a pre-filter check said "no" -- before we even parse the body.
#[derive(Debug)]
pub enum PreFilterReject {
    RateLimited(u32),
    DroppedUserAgent,
    DroppedIp,
}

/// Immutable lookup data -- gets swapped atomically on reload so readers
/// never see a half-updated state.
struct FilterSnapshot {
    inbound_filters: HashMap<u64, HashSet<String>>,
    message_filters: HashMap<u64, Vec<String>>,
    rate_limits_by_key: HashMap<String, u32>,
    rate_limits_by_project: HashMap<u64, u32>,
    excluded_environments: HashMap<u64, HashSet<String>>,
    release_filters: HashMap<u64, Vec<String>>,
    ua_filters: HashMap<u64, Vec<String>>,
    filter_rules: HashMap<u64, Vec<FilterRule>>,
    ip_blocklist: HashMap<u64, Vec<CidrBlock>>,
}

impl FilterSnapshot {
    fn from_data(data: FilterData) -> Self {
        let mut by_key = HashMap::new();
        let mut by_project = HashMap::new();
        for (compound_key, limit) in data.rate_limits {
            if let Some(key) = compound_key.strip_prefix("key:") {
                by_key.insert(key.to_string(), limit);
            } else if let Some(pid_str) = compound_key.strip_prefix("project:") {
                if let Ok(pid) = pid_str.parse::<u64>() {
                    by_project.insert(pid, limit);
                }
            }
        }

        // load_filter_data lowercases patterns, but tests might not --
        // so we do it here too, just to be safe.
        let lowercase_patterns = |m: HashMap<u64, Vec<String>>| -> HashMap<u64, Vec<String>> {
            m.into_iter()
                .map(|(k, v)| (k, v.into_iter().map(|s| s.to_lowercase()).collect()))
                .collect()
        };

        Self {
            inbound_filters: data.inbound_filters,
            message_filters: lowercase_patterns(data.message_filters),
            rate_limits_by_key: by_key,
            rate_limits_by_project: by_project,
            excluded_environments: data.excluded_environments,
            release_filters: lowercase_patterns(data.release_filters),
            ua_filters: lowercase_patterns(data.ua_filters),
            filter_rules: data.filter_rules,
            ip_blocklist: data.ip_blocklist,
        }
    }
}

/// The thing that decides whether an event lives or dies.
///
/// I've organized checks into tiers -- roughly by cost and specificity:
/// - Tier 1: fingerprint discard, inbound filters (browser extensions, localhost), message patterns
/// - Tier 2: rate limits, user-agent blocking, environment/release exclusions
/// - Tier 3: custom filter rules, IP blocklists
///
/// The read-only filter data sits behind a single `RwLock` + `Arc` swap.
/// Discarded fingerprints get their own lock because they're mutated
/// independently of full reloads.
pub struct FilterEngine {
    snapshot: RwLock<Arc<FilterSnapshot>>,
    /// Separate from snapshot -- fingerprints get added/removed on the fly.
    discarded: RwLock<HashSet<String>>,
    /// Per-key sliding windows. DashMap gives us lock-free concurrent access.
    rate_windows: dashmap::DashMap<String, SlidingWindow>,
    global_rate_limit: u32,
    global_excluded_environments: Vec<String>,
    /// Pre-lowercased for case-insensitive glob matching.
    global_blocked_user_agents: Vec<String>,
    /// Epoch seconds -- tracks when we last cleaned up stale rate windows.
    last_eviction: std::sync::atomic::AtomicU64,
}

impl FilterEngine {
    pub fn new(
        initial_data: FilterData,
        global_rate_limit: u32,
        global_excluded_environments: Vec<String>,
        global_blocked_user_agents: Vec<String>,
    ) -> Self {
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let discarded = initial_data.discarded.clone();
        Self {
            snapshot: RwLock::new(Arc::new(FilterSnapshot::from_data(initial_data))),
            discarded: RwLock::new(discarded),
            rate_windows: dashmap::DashMap::new(),
            global_rate_limit,
            global_excluded_environments,
            global_blocked_user_agents: global_blocked_user_agents
                .into_iter()
                .map(|s| s.to_lowercase())
                .collect(),
            last_eviction: std::sync::atomic::AtomicU64::new(now_secs),
        }
    }

    /// Swap in new filter data. Doesn't care where it came from -- DB, config,
    /// tests, whatever.
    pub fn apply_data(&self, data: FilterData) {
        *self.discarded.write() = data.discarded.clone();
        let new_snapshot = Arc::new(FilterSnapshot::from_data(data));
        *self.snapshot.write() = new_snapshot;
    }

    /// Grab an Arc clone of the current snapshot -- one quick read lock,
    /// then all checks run lock-free against the immutable data.
    fn snapshot(&self) -> Arc<FilterSnapshot> {
        self.snapshot.read().clone()
    }

    // ---
    // Tier 1 -- event-level check (after parse, before writer)
    // ---

    /// Run an event through all filter tiers. One snapshot clone, then
    /// everything's lock-free from there.
    pub fn check(&self, event: &StorableEvent) -> FilterVerdict {
        // Fingerprint discard has its own lock -- separate from the snapshot.
        if let Some(ref fp) = event.fingerprint {
            if self.discarded.read().contains(fp) {
                return FilterVerdict::Drop {
                    reason: DropReason::DiscardedFingerprint,
                };
            }
        }

        let snap = self.snapshot();

        // Built-in inbound filters
        if let Some(filters) = snap.inbound_filters.get(&event.project_id) {
            if filters.contains("browser_extensions") {
                if let Some(ref title) = event.title {
                    if contains_ignore_ascii_case(title, "chrome-extension://")
                        || contains_ignore_ascii_case(title, "moz-extension://")
                        || contains_ignore_ascii_case(title, "safari-extension://")
                    {
                        return FilterVerdict::Drop {
                            reason: DropReason::BrowserExtension,
                        };
                    }
                }
            }

            if filters.contains("localhost") {
                if let Some(ref sn) = event.server_name {
                    if sn.eq_ignore_ascii_case("localhost")
                        || sn == "127.0.0.1"
                        || sn == "::1"
                        || starts_with_ignore_ascii_case(sn, "192.168.")
                        || starts_with_ignore_ascii_case(sn, "10.")
                    {
                        return FilterVerdict::Drop {
                            reason: DropReason::Localhost,
                        };
                    }
                }
            }
        }

        // Message glob patterns
        if let Some(ref title) = event.title {
            if let Some(patterns) = snap.message_filters.get(&event.project_id) {
                if glob_match_any(patterns, title) {
                    return FilterVerdict::Drop {
                        reason: DropReason::MessageFilter,
                    };
                }
            }
        }

        // Excluded environments -- global config first, then per-project.
        if let Some(ref env) = event.environment {
            for excluded in &self.global_excluded_environments {
                if excluded.eq_ignore_ascii_case(env) {
                    return FilterVerdict::Drop {
                        reason: DropReason::ExcludedEnvironment,
                    };
                }
            }
            // Per-project excludes from the DB
            if let Some(set) = snap.excluded_environments.get(&event.project_id) {
                if set.contains(env) {
                    return FilterVerdict::Drop {
                        reason: DropReason::ExcludedEnvironment,
                    };
                }
            }
        }

        // Release filters
        if let Some(ref release) = event.release {
            if let Some(patterns) = snap.release_filters.get(&event.project_id) {
                if glob_match_any(patterns, release) {
                    return FilterVerdict::Drop {
                        reason: DropReason::ReleaseFilter,
                    };
                }
            }
        }

        // Custom filter rules -- the user-defined ones.
        if let Some(rules) = snap.filter_rules.get(&event.project_id) {
            for rule in rules {
                if rule.matches(event) {
                    match rule.action {
                        FilterAction::Drop => {
                            return FilterVerdict::Drop {
                                reason: DropReason::FilterRule,
                            };
                        }
                        FilterAction::Sample => {
                            if let Some(rate) = rule.sample_rate {
                                let hash = crate::fingerprint::fnv1a_64(event.event_id.as_bytes());
                                let normalized = (hash as f64) / (u64::MAX as f64);
                                if normalized > rate {
                                    return FilterVerdict::Drop {
                                        reason: DropReason::Sampled,
                                    };
                                }
                            }
                        }
                    }
                }
            }
        }

        FilterVerdict::Accept
    }

    // ---
    // Tier 2 -- rate limiting (before body parse)
    // ---

    /// Returns `Err(retry_after_secs)` if the key's over its limit.
    #[allow(dead_code)] // used by tests; production callers use pre_filter_check
    pub fn check_rate_limit(&self, public_key: &str, project_id: u64) -> Result<(), u32> {
        let snap = self.snapshot();
        self.check_rate_limit_with_snap(public_key, project_id, &snap)
    }

    /// Same check, but takes a pre-acquired snapshot so `pre_filter_check`
    /// doesn't grab the lock twice.
    fn check_rate_limit_with_snap(
        &self,
        public_key: &str,
        project_id: u64,
        snap: &FilterSnapshot,
    ) -> Result<(), u32> {
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Priority: per-key > per-project > global. Split maps keep lookups
        // zero-alloc.
        let limit = snap
            .rate_limits_by_key
            .get(public_key)
            .copied()
            .or_else(|| snap.rate_limits_by_project.get(&project_id).copied())
            .unwrap_or(self.global_rate_limit);

        if limit == 0 {
            return Ok(());
        }

        // The "key:" prefix avoids collisions between public_key strings and
        // project_id strings in the same DashMap.
        let window_key = format!("key:{public_key}");
        let mut window = self
            .rate_windows
            .entry(window_key)
            .or_insert_with(SlidingWindow::new);
        window.advance(now_secs);

        if window.count() >= limit {
            let retry_after = 60u32;
            return Err(retry_after);
        }

        // Still holding the entry guard -- check+increment must be atomic.
        window.increment(now_secs);

        self.evict_stale_rate_windows(now_secs);

        Ok(())
    }

    /// Cleans up stale rate windows every ~60s. It turns out rotating keys can
    /// cause unbounded growth, so we also hard-cap at 100k entries.
    fn evict_stale_rate_windows(&self, now_secs: u64) {
        let last = self
            .last_eviction
            .load(std::sync::atomic::Ordering::Relaxed);
        if now_secs.saturating_sub(last) < 60 {
            return;
        }
        if self
            .last_eviction
            .compare_exchange(
                last,
                now_secs,
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
            )
            .is_err()
        {
            return;
        }

        self.rate_windows
            .retain(|_, window| now_secs.saturating_sub(window.current_second) < 120);

        // Still too many? Get aggressive with the cutoff.
        if self.rate_windows.len() > 100_000 {
            tracing::warn!(
                "rate_windows has {} entries after time eviction, pruning oldest",
                self.rate_windows.len()
            );
            self.rate_windows
                .retain(|_, window| now_secs.saturating_sub(window.current_second) < 30);
        }
    }

    // ---
    // Tier 2 -- user-agent check (before body parse)
    // ---

    /// Drops health-check bots and any user-agents matching project or global patterns.
    #[allow(dead_code)] // used by tests; production callers use pre_filter_check
    pub fn check_user_agent(&self, user_agent: &str, project_id: u64) -> FilterVerdict {
        let snap = self.snapshot();
        self.check_user_agent_with_snap(user_agent, project_id, &snap)
    }

    fn check_user_agent_with_snap(
        &self,
        user_agent: &str,
        project_id: u64,
        snap: &FilterSnapshot,
    ) -> FilterVerdict {
        // Health check probes -- always dropped, no config needed.
        if starts_with_ignore_ascii_case(user_agent, "kube-probe/")
            || starts_with_ignore_ascii_case(user_agent, "elb-healthchecker/")
            || user_agent.eq_ignore_ascii_case("consul health check")
        {
            return FilterVerdict::Drop {
                reason: DropReason::HealthCheckUserAgent,
            };
        }

        // Global blocked user-agents from config
        if glob_match_any(&self.global_blocked_user_agents, user_agent) {
            return FilterVerdict::Drop {
                reason: DropReason::BlockedUserAgent,
            };
        }

        // Per-project patterns
        if let Some(patterns) = snap.ua_filters.get(&project_id) {
            if glob_match_any(patterns, user_agent) {
                return FilterVerdict::Drop {
                    reason: DropReason::BlockedUserAgent,
                };
            }
        }

        FilterVerdict::Accept
    }

    // ---
    // Tier 3 -- IP check
    // ---

    /// Match client IP against per-project CIDR blocklists. Unparseable IPs fail open.
    #[allow(dead_code)] // used by tests; production callers use pre_filter_check
    pub fn check_ip(&self, ip_str: &str, project_id: u64) -> FilterVerdict {
        let snap = self.snapshot();
        self.check_ip_with_snap(ip_str, project_id, &snap)
    }

    fn check_ip_with_snap(
        &self,
        ip_str: &str,
        project_id: u64,
        snap: &FilterSnapshot,
    ) -> FilterVerdict {
        let ip: IpAddr = match ip_str.parse() {
            Ok(ip) => ip,
            Err(_) => return FilterVerdict::Accept,
        };

        if let Some(blocks) = snap.ip_blocklist.get(&project_id) {
            for block in blocks {
                if block.contains_addr(ip) {
                    return FilterVerdict::Drop {
                        reason: DropReason::IpBlocked,
                    };
                }
            }
        }

        FilterVerdict::Accept
    }

    // ---
    // Combined pre-body-parse filter (rate limit + UA + IP, one snapshot)
    // ---

    /// The cheap checks we can do before parsing the body -- rate limit, UA,
    /// and IP. Grabs the snapshot once for all three.
    pub fn pre_filter_check(
        &self,
        public_key: &str,
        project_id: u64,
        user_agent: Option<&str>,
        client_ip: Option<&str>,
    ) -> Result<(), PreFilterReject> {
        // One snapshot for all three checks
        let snap = self.snapshot();

        if let Err(retry_after) = self.check_rate_limit_with_snap(public_key, project_id, &snap) {
            return Err(PreFilterReject::RateLimited(retry_after));
        }

        if let Some(ua) = user_agent {
            if self
                .check_user_agent_with_snap(ua, project_id, &snap)
                .is_drop()
            {
                return Err(PreFilterReject::DroppedUserAgent);
            }
        }

        if let Some(ip_str) = client_ip {
            if self.check_ip_with_snap(ip_str, project_id, &snap).is_drop() {
                return Err(PreFilterReject::DroppedIp);
            }
        }

        Ok(())
    }

    // ---
    // Cache mutation helpers (called from the writer thread)
    // ---

    /// Mark a fingerprint as discarded -- future events with it get dropped.
    pub fn add_discarded_fingerprint(&self, fingerprint: &str) {
        self.discarded.write().insert(fingerprint.to_string());
    }

    /// Un-discard a fingerprint, letting events through again.
    pub fn remove_discarded_fingerprint(&self, fingerprint: &str) {
        self.discarded.write().remove(fingerprint);
    }

    /// Optimistic update -- add to cache first, run the DB op, roll back if
    /// it fails. Keeps the UI snappy.
    pub fn persist_discarded_fingerprint<F>(
        &self,
        fingerprint: &str,
        db_op: F,
    ) -> anyhow::Result<()>
    where
        F: FnOnce() -> anyhow::Result<()>,
    {
        self.add_discarded_fingerprint(fingerprint);
        if let Err(e) = db_op() {
            self.remove_discarded_fingerprint(fingerprint);
            return Err(e);
        }
        Ok(())
    }

    /// Same optimistic approach, but for un-discarding. Roll back on DB failure.
    pub fn persist_undiscarded_fingerprint<F>(
        &self,
        fingerprint: &str,
        db_op: F,
    ) -> anyhow::Result<()>
    where
        F: FnOnce() -> anyhow::Result<()>,
    {
        self.remove_discarded_fingerprint(fingerprint);
        if let Err(e) = db_op() {
            self.add_discarded_fingerprint(fingerprint);
            return Err(e);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::StorableEvent;

    fn make_test_event() -> StorableEvent {
        StorableEvent {
            public_key: "testkey".to_string(),
            timestamp: 0,
            title: Some("test error".to_string()),
            payload: vec![],
            ..StorableEvent::test_default("test-event-id")
        }
    }

    fn empty_engine() -> FilterEngine {
        FilterEngine::new(FilterData::default(), 0, vec![], vec![])
    }

    #[test]
    fn check_discarded_fingerprint() {
        let mut data = FilterData::default();
        data.discarded.insert("fp123".to_string());
        let engine = FilterEngine::new(data, 0, vec![], vec![]);

        let mut event = make_test_event();
        event.fingerprint = Some("fp123".to_string());
        assert!(engine.check(&event).is_drop());

        event.fingerprint = Some("other".to_string());
        assert!(!engine.check(&event).is_drop());
    }

    #[test]
    fn check_browser_extension_filter() {
        let mut data = FilterData::default();
        data.inbound_filters
            .entry(1)
            .or_default()
            .insert("browser_extensions".to_string());
        let engine = FilterEngine::new(data, 0, vec![], vec![]);

        let mut event = make_test_event();
        event.title = Some("Error in chrome-extension://abc123/content.js".to_string());
        assert!(engine.check(&event).is_drop());

        event.title = Some("Normal error".to_string());
        assert!(!engine.check(&event).is_drop());
    }

    #[test]
    fn check_localhost_filter() {
        let mut data = FilterData::default();
        data.inbound_filters
            .entry(1)
            .or_default()
            .insert("localhost".to_string());
        let engine = FilterEngine::new(data, 0, vec![], vec![]);

        let mut event = make_test_event();
        event.server_name = Some("localhost".to_string());
        assert!(engine.check(&event).is_drop());

        event.server_name = Some("192.168.1.50".to_string());
        assert!(engine.check(&event).is_drop());

        event.server_name = Some("prod-web-01".to_string());
        assert!(!engine.check(&event).is_drop());
    }

    #[test]
    fn check_message_filter() {
        let mut data = FilterData::default();
        data.message_filters
            .entry(1)
            .or_default()
            .push("*timeout*".to_string());
        let engine = FilterEngine::new(data, 0, vec![], vec![]);

        let mut event = make_test_event();
        event.title = Some("Connection timeout error".to_string());
        assert!(engine.check(&event).is_drop());

        event.title = Some("NullPointerException".to_string());
        assert!(!engine.check(&event).is_drop());
    }

    #[test]
    fn check_user_agent_builtin() {
        let engine = empty_engine();

        assert!(engine.check_user_agent("kube-probe/1.27", 1).is_drop());
        assert!(engine
            .check_user_agent("ELB-HealthChecker/2.0", 1)
            .is_drop());
        assert!(engine.check_user_agent("Consul Health Check", 1).is_drop());
        assert!(!engine.check_user_agent("Mozilla/5.0", 1).is_drop());
    }

    #[test]
    fn rate_limiter_allows_under_limit() {
        let engine = FilterEngine::new(FilterData::default(), 100, vec![], vec![]);

        for _ in 0..99 {
            assert!(engine.check_rate_limit("testkey", 1).is_ok());
        }
    }

    #[test]
    fn rate_limiter_blocks_over_limit() {
        let engine = FilterEngine::new(FilterData::default(), 5, vec![], vec![]);

        for _ in 0..5 {
            assert!(engine.check_rate_limit("testkey", 1).is_ok());
        }
        assert!(engine.check_rate_limit("testkey", 1).is_err());
    }

    #[test]
    fn rate_limiter_zero_means_unlimited() {
        let engine = empty_engine();

        for _ in 0..1000 {
            assert!(engine.check_rate_limit("testkey", 1).is_ok());
        }
    }

    #[test]
    fn check_release_filter() {
        let mut data = FilterData::default();
        data.release_filters
            .entry(1)
            .or_default()
            .push("1.0.*".to_string());
        let engine = FilterEngine::new(data, 0, vec![], vec![]);

        let mut event = make_test_event();
        event.release = Some("1.0.3".to_string());
        assert!(engine.check(&event).is_drop());

        event.release = Some("2.0.0".to_string());
        assert!(!engine.check(&event).is_drop());

        event.release = None;
        assert!(!engine.check(&event).is_drop());
    }

    #[test]
    fn check_excluded_environment_global() {
        let engine = FilterEngine::new(
            FilterData::default(),
            0,
            vec!["development".to_string(), "test".to_string()],
            vec![],
        );

        let mut event = make_test_event();
        event.environment = Some("development".to_string());
        assert!(engine.check(&event).is_drop());

        event.environment = Some("DEVELOPMENT".to_string());
        assert!(engine.check(&event).is_drop());

        event.environment = Some("production".to_string());
        assert!(!engine.check(&event).is_drop());
    }

    #[test]
    fn check_excluded_environment_per_project() {
        let mut data = FilterData::default();
        data.excluded_environments
            .entry(1)
            .or_default()
            .insert("staging".to_string());
        let engine = FilterEngine::new(data, 0, vec![], vec![]);

        let mut event = make_test_event();
        event.environment = Some("staging".to_string());
        assert!(engine.check(&event).is_drop());

        event.environment = Some("production".to_string());
        assert!(!engine.check(&event).is_drop());
    }

    #[test]
    fn check_ip_blocklist() {
        let mut data = FilterData::default();
        data.ip_blocklist
            .entry(1)
            .or_default()
            .push(CidrBlock::parse("10.0.0.0/8").unwrap());
        let engine = FilterEngine::new(data, 0, vec![], vec![]);

        assert!(engine.check_ip("10.1.2.3", 1).is_drop());
        assert!(!engine.check_ip("192.168.1.1", 1).is_drop());
        // Fail-open on address family mismatch
        assert!(!engine.check_ip("::1", 1).is_drop());
        // Different project -- shouldn't match
        assert!(!engine.check_ip("10.1.2.3", 999).is_drop());
    }

    #[test]
    fn check_global_blocked_user_agent() {
        let engine = FilterEngine::new(
            FilterData::default(),
            0,
            vec![],
            vec!["*Bot*".to_string(), "curl/*".to_string()],
        );

        assert!(engine.check_user_agent("Googlebot/2.1", 1).is_drop());
        assert!(engine.check_user_agent("curl/7.68", 1).is_drop());
        assert!(!engine.check_user_agent("Mozilla/5.0", 1).is_drop());
    }

    #[test]
    fn check_per_project_user_agent() {
        let mut data = FilterData::default();
        data.ua_filters
            .entry(1)
            .or_default()
            .push("*Scrapy*".to_string());
        let engine = FilterEngine::new(data, 0, vec![], vec![]);

        assert!(engine.check_user_agent("Scrapy/2.5", 1).is_drop());
        // Different project -- no match
        assert!(!engine.check_user_agent("Scrapy/2.5", 2).is_drop());
    }

    #[test]
    fn apply_data_replaces_snapshot() {
        let engine = empty_engine();

        let mut event = make_test_event();
        event.fingerprint = Some("fp-test".to_string());
        assert!(!engine.check(&event).is_drop());

        // Now discard the fingerprint and swap in new data
        let mut data = FilterData::default();
        data.discarded.insert("fp-test".to_string());
        engine.apply_data(data);

        assert!(engine.check(&event).is_drop());
    }

    #[test]
    fn add_remove_discarded_fingerprint() {
        let engine = empty_engine();

        let mut event = make_test_event();
        event.fingerprint = Some("dynamic-fp".to_string());

        assert!(!engine.check(&event).is_drop());

        engine.add_discarded_fingerprint("dynamic-fp");
        assert!(engine.check(&event).is_drop());

        engine.remove_discarded_fingerprint("dynamic-fp");
        assert!(!engine.check(&event).is_drop());
    }

    #[test]
    fn check_custom_filter_rule_drop() {
        use super::super::rules::{FilterAction, FilterField, FilterOperator, FilterRule};

        let mut data = FilterData::default();
        data.filter_rules.entry(1).or_default().push(FilterRule {
            field: FilterField::Level,
            operator: FilterOperator::Equals,
            value: "debug".to_string(),
            action: FilterAction::Drop,
            sample_rate: None,
        });
        let engine = FilterEngine::new(data, 0, vec![], vec![]);

        let mut event = make_test_event();
        event.level = Some("debug".to_string());
        assert!(engine.check(&event).is_drop());

        event.level = Some("error".to_string());
        assert!(!engine.check(&event).is_drop());
    }
}
