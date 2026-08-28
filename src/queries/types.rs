use serde::{Deserialize, Serialize};

use crate::domain::{
    Breadcrumb, ContextGroup, ExceptionData, IntegrationKind, IssueStatus, Measurement,
    ProjectStatus, RequestInfo, SummaryTag, Tag, UserInfo,
};

// cap offset so a huge value can't force an expensive scan-and-discard
const MAX_OFFSET: u64 = 1_000_000;

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

/// A second, independent pager for a page that already spends `Pagination` on
/// another table. The spans page shows Traces and "All spans" side by side; a
/// single `offset`/`limit` pair would make the two fight.
#[derive(Debug, Default, Deserialize)]
pub struct TracePagination {
    #[serde(default, deserialize_with = "opt_u64_from_str")]
    pub trace_limit: Option<u64>,
    #[serde(default, deserialize_with = "opt_u64_from_str")]
    pub trace_offset: Option<u64>,
}

impl TracePagination {
    pub fn page(&self) -> Page {
        Page::new(self.trace_offset, self.trace_limit)
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
            offset: offset.unwrap_or(0).min(MAX_OFFSET),
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
        self.offset.saturating_add(self.limit) < self.total
    }
    pub fn has_prev(&self) -> bool {
        self.offset > 0
    }
    pub fn next_offset(&self) -> u64 {
        self.offset.saturating_add(self.limit)
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
    pub environment: Option<String>,
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

#[derive(Debug, Serialize, Clone)]
pub struct ProjectSummary {
    pub project_id: u64,
    pub name: Option<String>,
    pub org_id: i64,
    /// Org display label (name, falling back to slug). Lets a cross-org listing show
    /// which org each project belongs to without a second query.
    pub org_name: String,
    pub archived: bool,
    pub event_count: u64,
    pub error_count: u64,
    pub transaction_count: u64,
    pub session_count: u64,
    pub other_count: u64,
    pub issue_count: u64,
    /// `None` for a project that has never received an event.
    pub first_seen: Option<i64>,
    /// `None` when the project received nothing inside the selected period.
    pub last_seen: Option<i64>,
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
    /// Guessed from the hostname; read it through [`effective_forge_type`](ProjectRepo::effective_forge_type).
    pub forge_type: String,
    /// Operator's correction for a host the heuristic can't name.
    pub forge_type_override: Option<String>,
    pub url_template: Option<String>,
    /// Frame filename prefix; once any repo in a project sets one, only prefix matching applies.
    pub stack_path_prefix: Option<String>,
}

impl ProjectRepo {
    /// The override if set, otherwise the detected guess.
    pub fn effective_forge_type(&self) -> &str {
        self.forge_type_override
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.forge_type)
    }

    /// True when the forge could not be named, which is not cosmetic: the row
    /// produces no source links and never matches a tracker integration. The
    /// operator has to set the override, so the UI has to say so.
    pub fn is_inert(&self) -> bool {
        self.effective_forge_type() == crate::forge::ForgeType::Unknown.as_str()
    }
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
    /// Routes for every project in the org unless excluded; tracker kinds resolve by repo and ignore it.
    pub is_global: bool,
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
            Some("smtp") => "SMTP",
            Some("postmark") => "Postmark",
            // Legacy rows predate provider selection -- those are Postmark.
            None | Some(_) => "Postmark",
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
    /// From the parent integration: if set, removing this row resumes org defaults rather than silencing.
    pub integration_is_global: bool,
}

impl ProjectIntegration {
    /// Recipient address for email integrations, extracted from the `config`
    /// JSON blob (`{"to": "..."}`). None for other kinds or when unset. Keeps the
    /// raw JSON out of the "To address" form field.
    pub fn to_address(&self) -> Option<String> {
        self.config
            .as_deref()
            .and_then(|c| serde_json::from_str::<serde_json::Value>(c).ok())
            .and_then(|v| v.get("to").and_then(|t| t.as_str()).map(String::from))
    }
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

/// One (op, description) group on the aggregated spans table, with exact
/// percentiles computed in Rust from the group's raw durations.
#[derive(Debug)]
pub struct SpanAggRow {
    pub op: Option<String>,
    pub description: Option<String>,
    pub count: u64,
    pub p50_ms: i64,
    pub p95_ms: i64,
    pub avg_ms: i64,
}

/// Aggregated spans grouped by (op, description). `truncated` is set when the
/// group cap dropped some groups from the tail.
#[derive(Debug, Default)]
pub struct SpanAggregation {
    pub groups: Vec<SpanAggRow>,
    pub truncated: bool,
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
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub depth: usize,
    pub op: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub duration_ms: Option<i64>,
    /// Start time relative to the trace start, in ms (not an absolute epoch).
    pub start_offset_ms: Option<i64>,
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

/// A compressed idle interval in a waterfall, positioned along the display axis.
#[derive(Debug, Clone)]
pub struct WaterfallGap {
    /// Center position along the (compressed) timeline, as a percentage.
    pub at_pct: f64,
    /// Real duration of the collapsed idle interval, in milliseconds.
    pub real_ms: i64,
}

#[derive(Debug, Default)]
pub struct Waterfall {
    pub rows: Vec<WaterfallRow>,
    pub total_ms: i64,
    pub span_count: usize,
    /// Width of the root transaction's own bar, on the same (possibly compressed)
    /// axis as the child rows. 100 when the root's duration is unknown or spans
    /// the whole trace; less when child spans extend past it.
    pub root_width_pct: f64,
    pub truncated: bool,
    /// Large idle gaps that were collapsed on the display axis.
    pub gaps: Vec<WaterfallGap>,
    /// True when at least one idle gap was compressed (axis is non-linear).
    pub compressed: bool,
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

/// One log2 duration bucket rendered on the transaction detail distribution.
/// `pct` is the bar width relative to the busiest bucket in the range.
#[derive(Debug, Serialize)]
pub struct DurationBucket {
    pub label: String,
    pub count: u64,
    pub pct: f64,
}

/// One point on a transaction's percentile-over-time trend. `bucket` is the
/// unix timestamp the point starts at; `label` is its pre-formatted caption.
/// `regressed` is set by the trailing-median heuristic, not by the alerting
/// subsystem — it is a visual marker on this page only.
#[derive(Debug, Serialize)]
pub struct TransactionTrendPoint {
    pub bucket: i64,
    pub label: String,
    pub count: u64,
    pub p50_ms: i64,
    pub p95_ms: i64,
    pub regressed: bool,
}

/// Duration histogram plus aggregate stats for a single transaction over the
/// selected period, powering the detail page header and distribution chart.
#[derive(Debug, Serialize)]
pub struct TransactionDistribution {
    pub summary: TransactionSummary,
    pub buckets: Vec<DurationBucket>,
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
    /// From `replay_metadata`, absent for replays stored before migration 022
    /// (forward-only: there is no backfill).
    pub duration_ms: Option<i64>,
    pub url: Option<String>,
    pub user_label: Option<String>,
    pub browser: Option<String>,
    pub os: Option<String>,
    pub error_count: i64,
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

/// Error event referenced by a replay's `error_ids`, resolved from the events
/// table. `fingerprint` links to the grouped issue when present; None falls
/// back to the single-event view.
#[derive(Debug)]
pub struct ReplayError {
    pub event_id: String,
    pub fingerprint: Option<String>,
    pub title: Option<String>,
    pub level: Option<String>,
    pub timestamp: i64,
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    fn email_row(config: Option<&str>) -> Integration {
        Integration {
            id: 1,
            name: "n".into(),
            kind: IntegrationKind::Email,
            url: None,
            secret: None,
            encrypted: false,
            config: config.map(String::from),
            created_at: 0,
            is_global: false,
        }
    }

    #[test]
    fn provider_label_covers_all_providers() {
        let label = |p: &str| email_row(Some(&format!(r#"{{"provider":"{p}"}}"#))).provider_label();
        assert_eq!(label("lettermint"), Some("Lettermint"));
        assert_eq!(label("postmark"), Some("Postmark"));
        assert_eq!(label("sendgrid"), Some("SendGrid"));
        // Regression: smtp must not fall through to the legacy-Postmark arm.
        assert_eq!(label("smtp"), Some("SMTP"));
        // Legacy rows (no provider key) and unknown tags stay Postmark.
        assert_eq!(email_row(None).provider_label(), Some("Postmark"));
        assert_eq!(label("mailgun"), Some("Postmark"));
    }

    #[test]
    fn provider_label_none_for_non_email() {
        let mut row = email_row(None);
        row.kind = IntegrationKind::Webhook;
        assert_eq!(row.provider_label(), None);
    }
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

    #[test]
    fn offset_is_clamped() {
        assert_eq!(Page::new(Some(5_000_000), Some(10)).offset, MAX_OFFSET);
    }

    #[test]
    fn paged_result_arithmetic_does_not_overflow() {
        let r = PagedResult {
            items: Vec::<u8>::new(),
            total: 0,
            offset: u64::MAX,
            limit: 100,
        };
        assert_eq!(r.next_offset(), u64::MAX);
        assert!(!r.has_next());
    }
}
