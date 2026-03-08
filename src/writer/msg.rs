use crate::models::{StorableAttachment, StorableEvent};
use crate::queries::types::{EventFilter, IssueFilter};
use crate::queries::IssueStatus;

use super::types::WriteError;

pub enum WriteMsg {
    Event(StorableEvent),
    EventWithAttachments(StorableEvent, Vec<StorableAttachment>),
    UpdateIssueStatus {
        fingerprint: String,
        status: IssueStatus,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    UpsertRelease {
        project_id: u64,
        version: String,
        commit_sha: Option<String>,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    UpsertProjectRepo {
        project_id: u64,
        repo_url: String,
        forge_type: String,
        url_template: Option<String>,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    DeleteProjectRepo {
        project_id: u64,
        repo_id: i64,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    SetProjectName {
        project_id: u64,
        name: String,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    CreateProject {
        name: String,
        platform: Option<String>,
        reply: tokio::sync::oneshot::Sender<Result<(u64, String), WriteError>>,
    },
    ArchiveProject {
        project_id: u64,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    UnarchiveProject {
        project_id: u64,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    DeleteProject {
        project_id: u64,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    EnsureProjectKey {
        project_id: u64,
        public_key: String,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    CreateProjectKey {
        project_id: u64,
        label: Option<String>,
        reply: tokio::sync::oneshot::Sender<Result<String, WriteError>>,
    },
    DeleteProjectKey {
        public_key: String,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    BulkDeleteEvents {
        ids: Option<Vec<String>>,
        filter: Option<EventFilter>,
        project_id: Option<u64>,
        reply: tokio::sync::oneshot::Sender<Result<u64, WriteError>>,
    },
    BulkDeleteIssues {
        fingerprints: Option<Vec<String>>,
        filter: Option<IssueFilter>,
        project_id: u64,
        since: Option<i64>,
        reply: tokio::sync::oneshot::Sender<Result<u64, WriteError>>,
    },
    BulkUpdateIssueStatus {
        fingerprints: Option<Vec<String>>,
        filter: Option<IssueFilter>,
        project_id: u64,
        since: Option<i64>,
        status: IssueStatus,
        reply: tokio::sync::oneshot::Sender<Result<u64, WriteError>>,
    },
    // Filters (Tier 1)
    DiscardFingerprint {
        fingerprint: String,
        project_id: u64,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    UndiscardFingerprint {
        fingerprint: String,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    SetInboundFilter {
        project_id: u64,
        filter_id: String,
        enabled: bool,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    CreateMessageFilter {
        project_id: u64,
        pattern: String,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    DeleteMessageFilter {
        id: i64,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    // Filters (Tier 2)
    SetRateLimit {
        project_id: u64,
        public_key: Option<String>,
        max_events_per_minute: u32,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    AddEnvironmentFilter {
        project_id: u64,
        environment: String,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    DeleteEnvironmentFilter {
        id: i64,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    AddReleaseFilter {
        project_id: u64,
        pattern: String,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    DeleteReleaseFilter {
        id: i64,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    AddUserAgentFilter {
        project_id: u64,
        pattern: String,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    DeleteUserAgentFilter {
        id: i64,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    // Filters (Tier 3)
    CreateFilterRule {
        project_id: u64,
        field: String,
        operator: String,
        value: String,
        action: String,
        sample_rate: Option<f64>,
        priority: i32,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    DeleteFilterRule {
        id: i64,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    AddIpBlock {
        project_id: u64,
        cidr: String,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    DeleteIpBlock {
        id: i64,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    // Integrations
    CreateIntegration {
        name: String,
        kind: String,
        url: String,
        secret: Option<String>,
        config: Option<String>,
        encrypted: bool,
        reply: tokio::sync::oneshot::Sender<Result<i64, WriteError>>,
    },
    DeleteIntegration {
        id: i64,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    // Per-project integrations
    ActivateProjectIntegration {
        project_id: u64,
        integration_id: i64,
        notify_new_issues: bool,
        notify_regressions: bool,
        min_level: Option<String>,
        environment_filter: Option<String>,
        config: Option<String>,
        notify_threshold: bool,
        notify_digests: bool,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    UpdateProjectIntegration {
        id: i64,
        notify_new_issues: bool,
        notify_regressions: bool,
        min_level: Option<String>,
        environment_filter: Option<String>,
        config: Option<String>,
        notify_threshold: bool,
        notify_digests: bool,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    DeactivateProjectIntegration {
        id: i64,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    // Alert rules
    CreateAlertRule {
        project_id: Option<u64>,
        fingerprint: Option<String>,
        trigger_kind: String,
        threshold_count: Option<i64>,
        window_secs: Option<i64>,
        cooldown_secs: i64,
        reply: tokio::sync::oneshot::Sender<Result<i64, WriteError>>,
    },
    UpdateAlertRule {
        id: i64,
        threshold_count: Option<i64>,
        window_secs: Option<i64>,
        cooldown_secs: i64,
        enabled: bool,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    DeleteAlertRule {
        id: i64,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    // Digest schedules
    CreateDigestSchedule {
        project_id: Option<u64>,
        interval_secs: i64,
        reply: tokio::sync::oneshot::Sender<Result<i64, WriteError>>,
    },
    UpdateDigestSchedule {
        id: i64,
        interval_secs: i64,
        enabled: bool,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    DeleteDigestSchedule {
        id: i64,
        reply: tokio::sync::oneshot::Sender<Result<(), WriteError>>,
    },
    Shutdown,
}
