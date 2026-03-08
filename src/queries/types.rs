use serde::{Deserialize, Serialize};

use crate::event_data::{
    Breadcrumb, ContextGroup, ExceptionData, RequestInfo, SummaryTag, Tag, UserInfo,
};

/// Project/key status -- no more magic strings floating around.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub enum ProjectStatus {
    #[default]
    Active,
    Archived,
}

impl std::str::FromStr for ProjectStatus {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "archived" => Self::Archived,
            _ => Self::Active,
        })
    }
}

impl ProjectStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
        }
    }

    pub fn is_archived(&self) -> bool {
        matches!(self, Self::Archived)
    }
}

impl std::fmt::Display for ProjectStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Core project metadata -- name, status, source -- grabbed in one query.
pub struct ProjectInfo {
    pub name: Option<String>,
    pub status: ProjectStatus,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum IssueStatus {
    #[default]
    Unresolved,
    Resolved,
    Ignored,
}

impl std::str::FromStr for IssueStatus {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "resolved" => Self::Resolved,
            "ignored" => Self::Ignored,
            _ => Self::Unresolved,
        })
    }
}

impl IssueStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unresolved => "unresolved",
            Self::Resolved => "resolved",
            Self::Ignored => "ignored",
        }
    }
}

impl std::fmt::Display for IssueStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialEq<str> for IssueStatus {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for IssueStatus {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

#[derive(Debug, Clone)]
pub struct Page {
    pub offset: u64,
    pub limit: u64,
}

impl Page {
    pub fn new(offset: Option<u64>, limit: Option<u64>) -> Self {
        Self {
            offset: offset.unwrap_or(0),
            limit: limit.unwrap_or(25).min(100),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PagedResult<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub offset: u64,
    pub limit: u64,
}

impl<T> PagedResult<T> {
    pub fn has_next(&self) -> bool {
        self.offset + self.limit < self.total
    }
    pub fn has_prev(&self) -> bool {
        self.offset > 0
    }
    pub fn next_offset(&self) -> u64 {
        self.offset + self.limit
    }
    pub fn prev_offset(&self) -> u64 {
        self.offset.saturating_sub(self.limit)
    }
}

#[derive(Debug, Default)]
pub struct EventFilter {
    pub level: Option<String>,
    pub project_id: Option<u64>,
    pub query: Option<String>,
    pub sort: Option<String>,
    pub item_type: Option<String>,
}

#[derive(Debug, Default)]
pub struct IssueFilter {
    pub level: Option<String>,
    pub status: Option<String>,
    pub query: Option<String>,
    pub sort: Option<String>,
    pub item_type: Option<String>,
    pub release: Option<String>,
    pub tag: Option<(String, String)>,
}

#[derive(Debug, Default)]
pub struct LogFilter {
    pub level: Option<String>,
    pub query: Option<String>,
    pub trace_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LogEntry {
    pub id: i64,
    pub project_id: u64,
    pub timestamp: i64,
    pub level: Option<String>,
    pub body: Option<String>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub release: Option<String>,
    pub environment: Option<String>,
    pub attributes: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProjectSummary {
    pub project_id: u64,
    pub name: Option<String>,
    pub event_count: u64,
    pub error_count: u64,
    pub transaction_count: u64,
    pub session_count: u64,
    pub other_count: u64,
    pub issue_count: u64,
    pub first_seen: i64,
    pub last_seen: i64,
    pub platforms: String,
    pub latest_release: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct IssueSummary {
    pub fingerprint: String,
    pub project_id: u64,
    pub title: Option<String>,
    pub level: Option<String>,
    pub first_seen: i64,
    pub last_seen: i64,
    pub event_count: u64,
    pub status: IssueStatus,
    pub item_type: crate::models::ItemType,
    pub user_count: u64,
}

#[derive(Debug, Serialize)]
pub struct EventSummary {
    pub event_id: String,
    pub item_type: crate::models::ItemType,
    pub project_id: u64,
    pub fingerprint: Option<String>,
    pub timestamp: i64,
    pub level: Option<String>,
    pub title: Option<String>,
    pub platform: Option<String>,
    pub release: Option<String>,
    pub environment: Option<String>,
}

#[derive(Debug, Default)]
pub struct EventNav {
    pub prev_event_id: Option<String>,
    pub next_event_id: Option<String>,
    pub total: u64,
}

#[derive(Debug, Serialize)]
pub struct EventDetail {
    pub event_id: String,
    pub item_type: crate::models::ItemType,
    pub project_id: u64,
    pub fingerprint: Option<String>,
    pub timestamp: i64,
    pub level: Option<String>,
    pub title: Option<String>,
    pub platform: Option<String>,
    pub release: Option<String>,
    pub environment: Option<String>,
    pub server_name: Option<String>,
    pub transaction_name: Option<String>,
    pub sdk_name: Option<String>,
    pub sdk_version: Option<String>,
    pub received_at: i64,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectRepo {
    pub id: i64,
    pub project_id: u64,
    pub repo_url: String,
    pub forge_type: String,
    pub url_template: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReleaseHealth {
    pub release: String,
    pub total_sessions: u64,
    pub ok_count: u64,
    pub crashed_count: u64,
    pub errored_count: u64,
    pub crash_free_rate: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AttachmentInfo {
    pub id: i64,
    pub filename: String,
    pub content_type: Option<String>,
    pub size: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct TagFacet {
    pub key: String,
    pub top_values: Vec<TagFacetValue>,
    pub total_count: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TagFacetValue {
    pub value: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Release {
    pub id: i64,
    pub project_id: u64,
    pub version: String,
    pub commit_sha: Option<String>,
    pub date_released: Option<i64>,
    pub first_event: Option<i64>,
    pub last_event: Option<i64>,
    pub new_groups: u64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectKey {
    pub public_key: String,
    pub project_id: u64,
    pub status: ProjectStatus,
    pub label: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Integration {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub url: String,
    pub secret: Option<String>,
    pub encrypted: bool,
    pub config: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ProjectIntegration {
    pub id: i64,
    pub project_id: u64,
    pub integration_id: i64,
    pub integration_name: String,
    pub integration_kind: String,
    pub integration_url: String,
    pub integration_secret: Option<String>,
    pub integration_encrypted: bool,
    pub integration_config: Option<String>,
    pub notify_new_issues: bool,
    pub notify_regressions: bool,
    pub min_level: Option<String>,
    pub environment_filter: Option<String>,
    pub config: Option<String>,
    pub enabled: bool,
    pub notify_threshold: bool,
    pub notify_digests: bool,
}

#[derive(Debug, Default)]
pub struct ReleaseFilter {
    pub project_id: Option<u64>,
    pub query: Option<String>,
    pub sort: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReleaseSummary {
    pub version: String,
    pub project_id: u64,
    pub project_name: Option<String>,
    pub first_seen: i64,
    pub last_seen: i64,
    pub event_count: u64,
    pub issue_count: u64,
    pub adoption: f64,
}

#[derive(Debug, Serialize)]
pub struct MonitorSummary {
    pub monitor_slug: String,
    pub last_status: String,
    pub last_checkin: i64,
    pub checkin_count: u64,
}

/// Tail event entry -- includes `received_at` so the client can cursor through.
#[derive(Debug)]
pub struct TailEvent {
    pub item_type: String,
    pub project_id: u64,
    pub timestamp: i64,
    pub level: Option<String>,
    pub title: Option<String>,
    pub received_at: i64,
}

/// User report fields pulled out of the compressed payload.
#[derive(Debug)]
pub struct UserReportData {
    pub name: Option<String>,
    pub email: Option<String>,
    pub comments: Option<String>,
    pub timestamp: i64,
}

/// Nav badge counts for project sub-pages. Loaded in one shot so we don't
/// fire separate count queries from every HTML handler.
#[derive(Debug, Clone, Default)]
pub struct ProjectNavCounts {
    pub transaction_count: u64,
    pub monitor_count: u64,
    pub session_count: u64,
    pub user_report_count: u64,
    pub client_report_count: u64,
    pub log_count: u64,
    pub span_count: u64,
    pub metric_count: u64,
    pub profile_count: u64,
    pub replay_count: u64,
}

/// Raw row from `fetch_events_without_fingerprint` -- used during backfill.
pub struct BackfillRow {
    pub event_id: String,
    pub item_type_str: String,
    pub payload_blob: Vec<u8>,
    pub project_id: u64,
    pub timestamp: i64,
    pub title: Option<String>,
    pub level: Option<String>,
}

/// Everything the event detail page needs beyond the event itself.
/// Fetched in one call so we don't fall into N+1 territory.
#[derive(Default)]
pub struct EventSupplements {
    pub event_nav: EventNav,
    pub attachments: Vec<AttachmentInfo>,
    pub user_reports: Vec<UserReportData>,
    pub commit_sha: Option<String>,
    pub repos: Vec<ProjectRepo>,
}

/// The full picture for event/issue detail pages -- DB supplements merged
/// with parsed payload data, ready to hand off to templates.
pub struct ExtractedEventData {
    pub summary_tags: Vec<SummaryTag>,
    pub exceptions: Vec<ExceptionData>,
    pub breadcrumbs: Vec<Breadcrumb>,
    pub tags: Vec<Tag>,
    pub contexts: Vec<ContextGroup>,
    pub request: Option<RequestInfo>,
    pub user: UserInfo,
    pub event_nav: EventNav,
    pub attachments: Vec<AttachmentInfo>,
    pub user_reports: Vec<UserReportData>,
    pub raw_json: String,
}

/// Filter rule as it lives in the DB -- gets parsed into domain types later.
#[derive(Debug)]
pub struct RawFilterRule {
    pub id: i64,
    pub field: String,
    pub operator: String,
    pub value: String,
    pub action: String,
    pub sample_rate: Option<f64>,
    pub priority: i32,
}

#[derive(Debug)]
pub struct SpanSummary {
    pub span_id: String,
    pub trace_id: String,
    pub parent_span_id: Option<String>,
    pub timestamp: i64,
    pub op: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub duration_ms: Option<i64>,
}

pub type TraceSpan = SpanSummary;

#[derive(Debug)]
pub struct TraceSummary {
    pub trace_id: String,
    pub span_count: u64,
    pub first_timestamp: i64,
    pub last_timestamp: i64,
    pub root_op: Option<String>,
    pub root_description: Option<String>,
    pub total_duration_ms: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct MetricInfo {
    pub mri: String,
    pub metric_type: String,
    pub data_points: u64,
    pub first_seen: i64,
    pub last_seen: i64,
}

#[derive(Debug, Serialize)]
pub struct MetricBucket {
    pub timestamp: i64,
    pub count: u64,
    pub sum: f64,
    pub min: f64,
    pub max: f64,
    pub avg: f64,
}

#[derive(Debug, Serialize)]
pub struct ProfileSummary {
    pub event_id: String,
    pub project_id: u64,
    pub timestamp: i64,
    pub transaction_name: Option<String>,
    pub platform: Option<String>,
    pub release: Option<String>,
    pub environment: Option<String>,
}

#[derive(Debug)]
pub struct ProfileDetail {
    pub event_id: String,
    pub timestamp: i64,
    pub transaction_name: Option<String>,
    pub platform: Option<String>,
    pub release: Option<String>,
    pub environment: Option<String>,
    pub payload: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct ReplaySummary {
    pub event_id: String,
    pub project_id: u64,
    pub timestamp: i64,
    pub replay_type: String,
    pub release: Option<String>,
    pub environment: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReplayDetail {
    pub event_id: String,
    pub project_id: u64,
    pub timestamp: i64,
    pub replay_type: String,
    pub release: Option<String>,
    pub environment: Option<String>,
    pub payload: serde_json::Value,
}
