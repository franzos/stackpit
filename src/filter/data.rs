use std::collections::{HashMap, HashSet};

use super::cidr::CidrBlock;
use super::rules::FilterRule;

/// Everything the filter engine needs, decoupled from the DB schema. Built by
/// `queries::filters::load_filter_data()`, consumed by `FilterEngine::apply_data()`.
#[derive(Default)]
pub struct FilterData {
    pub discarded: HashSet<String>,
    pub inbound_filters: HashMap<u64, HashSet<String>>,
    pub message_filters: HashMap<u64, Vec<String>>,
    pub rate_limits: HashMap<String, u32>,
    pub excluded_environments: HashMap<u64, HashSet<String>>,
    pub release_filters: HashMap<u64, Vec<String>>,
    pub ua_filters: HashMap<u64, Vec<String>>,
    pub filter_rules: HashMap<u64, Vec<FilterRule>>,
    pub ip_blocklist: HashMap<u64, Vec<CidrBlock>>,
}
