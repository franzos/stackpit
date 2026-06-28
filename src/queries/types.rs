use serde::{Deserialize, Serialize};

use crate::domain::{
    Breadcrumb, ContextGroup, ExceptionData, IntegrationKind, IssueStatus, Measurement,
    ProjectStatus, RequestInfo, SummaryTag, Tag, UserInfo,
};

/// Shared `limit`/`offset` query params. Embed via `#[serde(flatten)]` in
/// per-page param structs, then call `.page()` to get the clamped [`Page`].
///
/// Fields parse through a string so `flatten` works under serde_urlencoded
/// (its flatten buffer hands every value to the field as a string, so a plain
/// `Option<u64>` would fail even on `"50"`). A present non-numeric value still
/// errors, matching a bare `Option<u64>`; an absent key defaults to `None`.
#[derive(Debug, Default, Deserialize)]
pub struct Pagination {
    #[serde(default, deserialize_with = "opt_u64_from_str")]
    pub limit: Option<u64>,
    #[serde(default, deserialize_with = "opt_u64_from_str")]
    pub offset: Option<u64>,
}

impl Pagination {
    pub fn page(&self) -> Page {
        Page::new(self.offset, self.limit)
    }
}

/// Parses an optional unsigned int from a (possibly flatten-buffered) string.
/// A present value must parse; an absent one yields `None`.
fn opt_u64_from_str<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    match s {
        None => Ok(None),
        Some(v) => v.parse().map(Some).map_err(serde::de::Error::custom),
    }
}

/// Core project metadata (name, status, source).
pub struct ProjectInfo {
    pub name: Option<String>,
    pub status: ProjectStatus,
    pub source: Option<String>,
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
    /// Build from a `COUNT(*)` total (DB-native `i64`) and the request page.
    pub fn from_page(items: Vec<T>, total: i64, page: &Page) -> Self {
        Self {
            items,
            total: total as u64,
            offset: page.offset,
            limit: page.limit,
        }
    }

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
    pub item_type: crate::ingest::models::ItemType,
    pub user_count: u64,
}

#[derive(Debug, Serialize)]
pub struct EventSummary {
    pub event_id: String,
    pub item_type: crate::ingest::models::ItemType,
    pub project_id: u64,
    /// Set only by the cross-project firehose query; None for project-scoped lists.
    pub project_name: Option<String>,
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
    pub item_type: crate::ingest::models::ItemType,
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
    /// None when an identity-less aggregate contributed (users can't be counted).
    pub crash_free_users: Option<f64>,
    pub total_users: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailySessions {
    pub day: i64,
    pub total: u64,
    pub crashed: u64,
    pub errored: u64,
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
    pub kind: IntegrationKind,
    pub url: Option<String>,
    pub secret: Option<String>,
    pub encrypted: bool,
    pub config: Option<String>,
    pub created_at: i64,
}

impl Integration {
    /// Pretty provider label for email rows; `None` for non-email kinds.
    pub fn provider_label(&self) -> Option<&'static str> {
        if self.kind != IntegrationKind::Email {
            return None;
        }
        let provider = self
            .config
            .as_deref()
            .and_then(|c| serde_json::from_str::<serde_json::Value>(c).ok())
            .and_then(|v| v.get("provider").and_then(|p| p.as_str()).map(String::from));
        Some(match provider.as_deref() {
            Some("lettermint") => "Lettermint",
            Some("sendgrid") => "SendGrid",
            // Legacy rows predate provider selection -- those are Postmark.
            _ => "Postmark",
        })
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ProjectIntegration {
    pub id: i64,
    pub project_id: u64,
    pub integration_id: i64,
    pub integration_name: String,
    pub integration_kind: IntegrationKind,
    pub integration_url: Option<String>,
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

/// A user report rendered as its own event: the feedback fields plus a link
/// back to the error event it references.
#[derive(Debug)]
pub struct UserFeedback {
    pub name: Option<String>,
    pub email: Option<String>,
    pub comments: Option<String>,
    pub event_id: Option<String>,
}

impl UserFeedback {
    pub fn has_any(&self) -> bool {
        self.name.is_some()
            || self.email.is_some()
            || self.comments.is_some()
            || self.event_id.is_some()
    }
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
    /// Human label for the project: stored `name` if set, else `Project {id}`.
    /// Lives on `ProjectNavCounts` because every per-project template already
    /// renders this struct for tab badges.
    pub label: String,
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

/// Event detail supplements, fetched in one call to avoid N+1 queries.
#[derive(Default)]
pub struct EventSupplements {
    pub event_nav: EventNav,
    pub attachments: Vec<AttachmentInfo>,
    pub user_reports: Vec<UserReportData>,
    pub commit_sha: Option<String>,
    pub repos: Vec<ProjectRepo>,
}

/// Event/issue detail data: DB supplements merged with parsed payload data.
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
    /// Present when the event being viewed is itself a user report.
    pub own_feedback: Option<UserFeedback>,
    /// Web vitals / measurements pulled from a transaction payload.
    pub measurements: Vec<Measurement>,
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
    pub timestamp: i64,
    pub op: Option<String>,
    pub description: Option<String>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct TraceSpan {
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub op: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub duration_ms: Option<i64>,
    pub start_ms: Option<i64>,
}

/// One rendered row of a span waterfall. Geometry is pre-computed as
/// percentages so the template only emits inline `margin-left`/`width`.
#[derive(Debug, Clone)]
pub struct WaterfallRow {
    // Carried for test assertions on row ordering; not rendered.
    #[allow(dead_code)]
    pub span_id: String,
    pub depth: usize,
    pub op: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub duration_ms: Option<i64>,
    pub offset_pct: f64,
    pub width_pct: f64,
}

impl WaterfallRow {
    /// Bar color bucket: green = ok, red = failed, neutral otherwise.
    pub fn bar_color(&self) -> &'static str {
        match self.status.as_deref() {
            Some("ok") => "#16a34a",
            None | Some("cancelled" | "unknown") => "#9ca3af",
            Some(_) => "#dc2626",
        }
    }

    /// True when the span carries a non-ok, non-neutral status.
    pub fn is_error(&self) -> bool {
        matches!(self.status.as_deref(), Some(s) if !matches!(s, "ok" | "cancelled" | "unknown"))
    }
}

/// The transaction a trace belongs to, used as the waterfall's root row.
#[derive(Debug)]
pub struct TraceRoot {
    pub name: Option<String>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Default)]
pub struct Waterfall {
    pub rows: Vec<WaterfallRow>,
    pub total_ms: i64,
    pub span_count: usize,
    pub truncated: bool,
}

/// Error event correlated to a trace via shared `trace_id`.
#[derive(Debug)]
pub struct TraceError {
    pub event_id: String,
    pub title: Option<String>,
    pub level: Option<String>,
    pub timestamp: i64,
}

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
pub struct TransactionSummary {
    pub name: String,
    pub tpm: f64,
    pub throughput: String,
    pub p50_ms: i64,
    pub p75_ms: i64,
    pub p95_ms: i64,
    pub failure_rate: f64,
    pub count: u64,
    pub users: u64,
    pub avg_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct TransactionInstance {
    pub event_id: String,
    pub trace_id: Option<String>,
    pub duration_ms: Option<i64>,
    pub timestamp: i64,
    pub op: Option<String>,
    pub status: Option<String>,
}

impl TransactionInstance {
    /// True when the trace status is set and not a healthy terminal state.
    pub fn is_failed(&self) -> bool {
        matches!(self.status.as_deref(), Some(s) if !matches!(s, "ok" | "cancelled" | "unknown"))
    }
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

#[cfg(test)]
mod pagination_tests {
    use super::*;

    #[derive(Debug, Deserialize)]
    struct Probe {
        query: Option<String>,
        #[serde(flatten)]
        page: Pagination,
    }

    fn parse<T: serde::de::DeserializeOwned>(qs: &str) -> Result<T, ()> {
        let uri: axum::http::Uri = format!("/x?{qs}").parse().unwrap();
        axum::extract::Query::try_from_uri(&uri)
            .map(|axum::extract::Query(v)| v)
            .map_err(|_| ())
    }

    #[test]
    fn flatten_works_with_serde_urlencoded() {
        let p: Probe = parse("query=foo&limit=50&offset=10").unwrap();
        assert_eq!(p.query.as_deref(), Some("foo"));
        assert_eq!(p.page.limit, Some(50));
        assert_eq!(p.page.offset, Some(10));
    }

    #[test]
    fn flatten_absent_pagination_is_none() {
        let p: Probe = parse("query=foo").unwrap();
        assert_eq!(p.page.limit, None);
        assert_eq!(p.page.offset, None);
    }

    #[test]
    fn present_non_numeric_still_rejects() {
        // Matches a bare `Option<u64>`: a present unparseable value is a 400.
        assert!(parse::<Probe>("limit=abc").is_err());
        assert!(parse::<Probe>("limit=").is_err());
    }

    #[test]
    fn direct_query_pagination() {
        let p: Pagination = parse("limit=5&offset=2").unwrap();
        assert_eq!(p.page().limit, 5);
        assert_eq!(p.page().offset, 2);
    }
}
