use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

use crate::models::{StorableAttachment, StorableEvent};
use crate::queries::types::{EventFilter, IssueFilter};
use crate::queries::IssueStatus;
use crate::stats::IngestStats;

use super::msg::WriteMsg;
use super::types::WriteReply;

type SendError = Box<tokio::sync::mpsc::error::TrySendError<WriteMsg>>;

/// Wires up a oneshot reply channel for a `WriteMsg` variant and sends it off.
macro_rules! writer_cmd {
    ($self:expr, $Variant:ident { $($field:ident),* $(,)? }) => {{
        let (tx, rx) = tokio::sync::oneshot::channel();
        $self.tx.try_send(WriteMsg::$Variant { $($field,)* reply: tx })
            .map_err(Box::new)?;
        Ok(rx)
    }};
}

/// The public face of the writer task. Wraps the channel sender so
/// callers can just call domain methods instead of constructing `WriteMsg`
/// variants by hand.
#[derive(Clone)]
pub struct WriterHandle {
    tx: Sender<WriteMsg>,
    ingest_stats: Arc<IngestStats>,
}

impl WriterHandle {
    pub fn new(tx: Sender<WriteMsg>, ingest_stats: Arc<IngestStats>) -> Self {
        Self { tx, ingest_stats }
    }

    #[cfg(test)]
    pub fn raw_sender(&self) -> &Sender<WriteMsg> {
        &self.tx
    }

    /// Queue depth and capacity -- exposed on the health endpoint.
    pub fn queue_used(&self) -> usize {
        self.tx.max_capacity() - self.tx.capacity()
    }

    pub fn queue_max(&self) -> usize {
        self.tx.max_capacity()
    }

    // -- Event ingestion (fire-and-forget) -----------------------------------

    pub fn send_event(&self, event: StorableEvent) -> Result<(), SendError> {
        self.warn_if_backpressure();
        match self.tx.try_send(WriteMsg::Event(event)) {
            Ok(()) => {
                self.ingest_stats
                    .events_accepted
                    .fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(e) => {
                self.ingest_stats
                    .events_rejected
                    .fetch_add(1, Ordering::Relaxed);
                Err(Box::new(e))
            }
        }
    }

    fn warn_if_backpressure(&self) {
        let capacity = self.tx.capacity();
        let max = self.tx.max_capacity();
        let used = max - capacity;
        let pct = (used * 100) / max;
        if pct >= 80 {
            tracing::warn!(
                "writer channel at {pct}% capacity ({used}/{max}) — ingestion may be backing up"
            );
        }
    }

    pub fn send_event_with_attachments(
        &self,
        event: StorableEvent,
        attachments: Vec<StorableAttachment>,
    ) -> Result<(), SendError> {
        self.warn_if_backpressure();
        let msg = if attachments.is_empty() {
            WriteMsg::Event(event)
        } else {
            WriteMsg::EventWithAttachments(event, attachments)
        };
        match self.tx.try_send(msg) {
            Ok(()) => {
                self.ingest_stats
                    .events_accepted
                    .fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(e) => {
                self.ingest_stats
                    .events_rejected
                    .fetch_add(1, Ordering::Relaxed);
                Err(Box::new(e))
            }
        }
    }

    // -- Issues --------------------------------------------------------------

    pub fn update_issue_status(
        &self,
        fingerprint: String,
        status: IssueStatus,
    ) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(
            self,
            UpdateIssueStatus {
                fingerprint,
                status
            }
        )
    }

    pub fn discard_fingerprint(
        &self,
        fingerprint: String,
        project_id: u64,
    ) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(
            self,
            DiscardFingerprint {
                fingerprint,
                project_id
            }
        )
    }

    pub fn undiscard_fingerprint(&self, fingerprint: String) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(self, UndiscardFingerprint { fingerprint })
    }

    // -- Bulk operations -----------------------------------------------------

    pub fn bulk_delete_events(
        &self,
        ids: Option<Vec<String>>,
        filter: Option<EventFilter>,
        project_id: Option<u64>,
    ) -> Result<WriteReply<u64>, SendError> {
        writer_cmd!(
            self,
            BulkDeleteEvents {
                ids,
                filter,
                project_id
            }
        )
    }

    pub fn bulk_delete_issues(
        &self,
        fingerprints: Option<Vec<String>>,
        filter: Option<IssueFilter>,
        project_id: u64,
        since: Option<i64>,
    ) -> Result<WriteReply<u64>, SendError> {
        writer_cmd!(
            self,
            BulkDeleteIssues {
                fingerprints,
                filter,
                project_id,
                since
            }
        )
    }

    pub fn bulk_update_issue_status(
        &self,
        fingerprints: Option<Vec<String>>,
        filter: Option<IssueFilter>,
        project_id: u64,
        since: Option<i64>,
        status: IssueStatus,
    ) -> Result<WriteReply<u64>, SendError> {
        writer_cmd!(
            self,
            BulkUpdateIssueStatus {
                fingerprints,
                filter,
                project_id,
                since,
                status
            }
        )
    }

    // -- Releases ------------------------------------------------------------

    pub fn upsert_release(
        &self,
        project_id: u64,
        version: String,
        commit_sha: Option<String>,
    ) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(
            self,
            UpsertRelease {
                project_id,
                version,
                commit_sha
            }
        )
    }

    // -- Projects ------------------------------------------------------------

    pub fn set_project_name(
        &self,
        project_id: u64,
        name: String,
    ) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(self, SetProjectName { project_id, name })
    }

    pub fn create_project(
        &self,
        name: String,
        platform: Option<String>,
    ) -> Result<WriteReply<(u64, String)>, SendError> {
        writer_cmd!(self, CreateProject { name, platform })
    }

    pub fn archive_project(&self, project_id: u64) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(self, ArchiveProject { project_id })
    }

    pub fn unarchive_project(&self, project_id: u64) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(self, UnarchiveProject { project_id })
    }

    pub fn delete_project(&self, project_id: u64) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(self, DeleteProject { project_id })
    }

    pub fn upsert_project_repo(
        &self,
        project_id: u64,
        repo_url: String,
        forge_type: String,
        url_template: Option<String>,
    ) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(
            self,
            UpsertProjectRepo {
                project_id,
                repo_url,
                forge_type,
                url_template
            }
        )
    }

    pub fn delete_project_repo(
        &self,
        project_id: u64,
        repo_id: i64,
    ) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(
            self,
            DeleteProjectRepo {
                project_id,
                repo_id
            }
        )
    }

    // -- Project keys --------------------------------------------------------

    pub fn ensure_project_key(
        &self,
        project_id: u64,
        public_key: String,
    ) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(
            self,
            EnsureProjectKey {
                project_id,
                public_key
            }
        )
    }

    pub fn create_project_key(
        &self,
        project_id: u64,
        label: Option<String>,
    ) -> Result<WriteReply<String>, SendError> {
        writer_cmd!(self, CreateProjectKey { project_id, label })
    }

    pub fn delete_project_key(&self, public_key: String) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(self, DeleteProjectKey { public_key })
    }

    // -- Filters (Tier 1) ----------------------------------------------------

    pub fn set_inbound_filter(
        &self,
        project_id: u64,
        filter_id: String,
        enabled: bool,
    ) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(
            self,
            SetInboundFilter {
                project_id,
                filter_id,
                enabled
            }
        )
    }

    pub fn create_message_filter(
        &self,
        project_id: u64,
        pattern: String,
    ) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(
            self,
            CreateMessageFilter {
                project_id,
                pattern
            }
        )
    }

    pub fn delete_message_filter(&self, id: i64) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(self, DeleteMessageFilter { id })
    }

    // -- Filters (Tier 2) ----------------------------------------------------

    pub fn set_rate_limit(
        &self,
        project_id: u64,
        public_key: Option<String>,
        max_events_per_minute: u32,
    ) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(
            self,
            SetRateLimit {
                project_id,
                public_key,
                max_events_per_minute
            }
        )
    }

    pub fn add_environment_filter(
        &self,
        project_id: u64,
        environment: String,
    ) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(
            self,
            AddEnvironmentFilter {
                project_id,
                environment
            }
        )
    }

    pub fn delete_environment_filter(&self, id: i64) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(self, DeleteEnvironmentFilter { id })
    }

    pub fn add_release_filter(
        &self,
        project_id: u64,
        pattern: String,
    ) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(
            self,
            AddReleaseFilter {
                project_id,
                pattern
            }
        )
    }

    pub fn delete_release_filter(&self, id: i64) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(self, DeleteReleaseFilter { id })
    }

    pub fn add_user_agent_filter(
        &self,
        project_id: u64,
        pattern: String,
    ) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(
            self,
            AddUserAgentFilter {
                project_id,
                pattern
            }
        )
    }

    pub fn delete_user_agent_filter(&self, id: i64) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(self, DeleteUserAgentFilter { id })
    }

    // -- Filters (Tier 3) ----------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub fn create_filter_rule(
        &self,
        project_id: u64,
        field: String,
        operator: String,
        value: String,
        action: String,
        sample_rate: Option<f64>,
        priority: i32,
    ) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(
            self,
            CreateFilterRule {
                project_id,
                field,
                operator,
                value,
                action,
                sample_rate,
                priority
            }
        )
    }

    pub fn delete_filter_rule(&self, id: i64) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(self, DeleteFilterRule { id })
    }

    pub fn add_ip_block(&self, project_id: u64, cidr: String) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(self, AddIpBlock { project_id, cidr })
    }

    pub fn delete_ip_block(&self, id: i64) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(self, DeleteIpBlock { id })
    }

    // -- Integrations --------------------------------------------------------

    pub fn create_integration(
        &self,
        name: String,
        kind: String,
        url: String,
        secret: Option<String>,
        config: Option<String>,
        encrypted: bool,
    ) -> Result<WriteReply<i64>, SendError> {
        writer_cmd!(
            self,
            CreateIntegration {
                name,
                kind,
                url,
                secret,
                config,
                encrypted
            }
        )
    }

    pub fn delete_integration(&self, id: i64) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(self, DeleteIntegration { id })
    }

    // -- Project integrations ------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub fn activate_project_integration(
        &self,
        project_id: u64,
        integration_id: i64,
        notify_new_issues: bool,
        notify_regressions: bool,
        min_level: Option<String>,
        environment_filter: Option<String>,
        config: Option<String>,
        notify_threshold: bool,
        notify_digests: bool,
    ) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(
            self,
            ActivateProjectIntegration {
                project_id,
                integration_id,
                notify_new_issues,
                notify_regressions,
                min_level,
                environment_filter,
                config,
                notify_threshold,
                notify_digests
            }
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_project_integration(
        &self,
        id: i64,
        notify_new_issues: bool,
        notify_regressions: bool,
        min_level: Option<String>,
        environment_filter: Option<String>,
        config: Option<String>,
        notify_threshold: bool,
        notify_digests: bool,
    ) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(
            self,
            UpdateProjectIntegration {
                id,
                notify_new_issues,
                notify_regressions,
                min_level,
                environment_filter,
                config,
                notify_threshold,
                notify_digests
            }
        )
    }

    pub fn deactivate_project_integration(&self, id: i64) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(self, DeactivateProjectIntegration { id })
    }

    // -- Alert rules ---------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub fn create_alert_rule(
        &self,
        project_id: Option<u64>,
        fingerprint: Option<String>,
        trigger_kind: String,
        threshold_count: Option<i64>,
        window_secs: Option<i64>,
        cooldown_secs: i64,
    ) -> Result<WriteReply<i64>, SendError> {
        writer_cmd!(
            self,
            CreateAlertRule {
                project_id,
                fingerprint,
                trigger_kind,
                threshold_count,
                window_secs,
                cooldown_secs
            }
        )
    }

    pub fn update_alert_rule(
        &self,
        id: i64,
        threshold_count: Option<i64>,
        window_secs: Option<i64>,
        cooldown_secs: i64,
        enabled: bool,
    ) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(
            self,
            UpdateAlertRule {
                id,
                threshold_count,
                window_secs,
                cooldown_secs,
                enabled
            }
        )
    }

    pub fn delete_alert_rule(&self, id: i64) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(self, DeleteAlertRule { id })
    }

    // -- Digest schedules ----------------------------------------------------

    pub fn create_digest_schedule(
        &self,
        project_id: Option<u64>,
        interval_secs: i64,
    ) -> Result<WriteReply<i64>, SendError> {
        writer_cmd!(
            self,
            CreateDigestSchedule {
                project_id,
                interval_secs
            }
        )
    }

    pub fn update_digest_schedule(
        &self,
        id: i64,
        interval_secs: i64,
        enabled: bool,
    ) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(
            self,
            UpdateDigestSchedule {
                id,
                interval_secs,
                enabled
            }
        )
    }

    pub fn delete_digest_schedule(&self, id: i64) -> Result<WriteReply<()>, SendError> {
        writer_cmd!(self, DeleteDigestSchedule { id })
    }

    // -- Lifecycle -----------------------------------------------------------

    pub fn shutdown(&self) -> Result<(), Box<tokio::sync::mpsc::error::TrySendError<WriteMsg>>> {
        self.tx.try_send(WriteMsg::Shutdown).map_err(Box::new)
    }
}
