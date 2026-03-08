use crate::db::DbPool;
use crate::filter::FilterEngine;
use crate::queries;
use crate::queries::filters::load_filter_data;

use super::msg::WriteMsg;
use super::types::WriteError;

fn to_write<T>(result: anyhow::Result<T>) -> Result<T, WriteError> {
    result.map_err(|e| WriteError::Internal(e.to_string()))
}

/// Tracks consecutive filter reload failures so we can escalate logging.
static FILTER_RELOAD_FAILURES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

async fn reload_filter(pool: &DbPool, filter_engine: Option<&FilterEngine>) {
    if let Some(fe) = filter_engine {
        match load_filter_data(pool).await {
            Ok(data) => {
                let prev = FILTER_RELOAD_FAILURES.swap(0, std::sync::atomic::Ordering::Relaxed);
                if prev > 0 {
                    tracing::info!(
                        "filter engine recovered after {prev} consecutive reload failures"
                    );
                }
                fe.apply_data(data);
            }
            Err(e) => {
                let count =
                    FILTER_RELOAD_FAILURES.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                tracing::error!(
                    consecutive_failures = count,
                    "filter engine reload failed: {e} — filter rules may be stale"
                );
            }
        }
    }
}

fn rows_or_not_found(rows: anyhow::Result<u64>, label: &str) -> Result<(), WriteError> {
    match to_write(rows) {
        Ok(0) => Err(WriteError::NotFound(label.to_string())),
        Ok(_) => Ok(()),
        Err(e) => Err(e),
    }
}

/// Async dispatch macros for writer commands.
macro_rules! dispatch_write {
    ($reply:expr, $expr:expr) => {{
        let result = to_write($expr);
        let _ = $reply.send(result);
    }};
    (reload: $pool:expr, $fe:expr, $reply:expr, $expr:expr) => {{
        let result = to_write($expr);
        if result.is_ok() {
            reload_filter($pool, $fe).await;
        }
        let _ = $reply.send(result);
    }};
    (rows: $label:expr, $reply:expr, $expr:expr) => {{
        let result = rows_or_not_found($expr, $label);
        let _ = $reply.send(result);
    }};
    (rows_reload: $pool:expr, $fe:expr, $label:expr, $reply:expr, $expr:expr) => {{
        let result = rows_or_not_found($expr, $label);
        if result.is_ok() {
            reload_filter($pool, $fe).await;
        }
        let _ = $reply.send(result);
    }};
}

pub(super) async fn handle_immediate(
    pool: &DbPool,
    msg: WriteMsg,
    filter_engine: Option<&FilterEngine>,
) {
    match msg {
        // -- Issues ----------------------------------------------------------
        WriteMsg::UpdateIssueStatus {
            fingerprint,
            status,
            reply,
        } => {
            dispatch_write!(rows: &format!("issue: {fingerprint}"), reply,
                queries::issues::update_issue_status(pool, &fingerprint, status).await);
        }

        // -- Releases --------------------------------------------------------
        WriteMsg::UpsertRelease {
            project_id,
            version,
            commit_sha,
            reply,
        } => {
            let info = queries::releases::ReleaseUpsert {
                version: &version,
                commit_sha: commit_sha.as_deref(),
                date_released: None,
                first_event: None,
                last_event: None,
                new_groups: 0,
            };
            dispatch_write!(
                reply,
                queries::releases::upsert_release(pool, project_id, &info).await
            );
        }

        // -- Projects --------------------------------------------------------
        WriteMsg::SetProjectName {
            project_id,
            name,
            reply,
        } => {
            dispatch_write!(
                reply,
                queries::projects::set_project_name(pool, project_id, &name).await
            );
        }
        WriteMsg::CreateProject {
            name,
            platform,
            reply,
        } => {
            dispatch_write!(
                reply,
                queries::projects::create_project(pool, &name, platform.as_deref()).await
            );
        }
        WriteMsg::ArchiveProject { project_id, reply } => {
            dispatch_write!(rows: &format!("project: {project_id}"), reply,
                queries::projects::archive_project(pool, project_id).await);
        }
        WriteMsg::UnarchiveProject { project_id, reply } => {
            dispatch_write!(rows: &format!("project: {project_id}"), reply,
                queries::projects::unarchive_project(pool, project_id).await);
        }
        WriteMsg::DeleteProject { project_id, reply } => {
            dispatch_write!(
                reply,
                queries::projects::delete_project(pool, project_id).await
            );
        }
        WriteMsg::EnsureProjectKey {
            project_id,
            public_key,
            reply,
        } => {
            dispatch_write!(
                reply,
                queries::projects::ensure_project_key(pool, project_id, &public_key).await
            );
        }
        WriteMsg::CreateProjectKey {
            project_id,
            label,
            reply,
        } => {
            dispatch_write!(
                reply,
                queries::projects::create_project_key(pool, project_id, label.as_deref()).await
            );
        }
        WriteMsg::DeleteProjectKey { public_key, reply } => {
            dispatch_write!(rows: &format!("key: {public_key}"), reply,
                queries::projects::delete_project_key(pool, &public_key).await);
        }
        WriteMsg::UpsertProjectRepo {
            project_id,
            repo_url,
            forge_type,
            url_template,
            reply,
        } => {
            dispatch_write!(
                reply,
                queries::projects::upsert_project_repo(
                    pool,
                    project_id,
                    &repo_url,
                    &forge_type,
                    url_template.as_deref()
                )
                .await
            );
        }
        WriteMsg::DeleteProjectRepo {
            project_id,
            repo_id,
            reply,
        } => {
            dispatch_write!(rows: &format!("repo: {repo_id}"), reply,
                queries::projects::delete_project_repo(pool, project_id, repo_id).await);
        }

        // -- Bulk operations -------------------------------------------------
        WriteMsg::BulkDeleteEvents {
            ids,
            filter,
            project_id,
            reply,
        } => {
            dispatch_write!(
                reply,
                queries::bulk::bulk_delete_events(
                    pool,
                    ids.as_deref(),
                    filter.as_ref(),
                    project_id
                )
                .await
            );
        }
        WriteMsg::BulkDeleteIssues {
            fingerprints,
            filter,
            project_id,
            since,
            reply,
        } => {
            dispatch_write!(
                reply,
                queries::bulk::bulk_delete_issues(
                    pool,
                    fingerprints.as_deref(),
                    filter.as_ref(),
                    project_id,
                    since
                )
                .await
            );
        }
        WriteMsg::BulkUpdateIssueStatus {
            fingerprints,
            filter,
            project_id,
            since,
            status,
            reply,
        } => {
            dispatch_write!(
                reply,
                queries::bulk::bulk_update_issue_status(
                    pool,
                    fingerprints.as_deref(),
                    filter.as_ref(),
                    project_id,
                    since,
                    status
                )
                .await
            );
        }

        // -- Filters (fingerprint discard with in-memory cache) ---------------
        WriteMsg::DiscardFingerprint {
            fingerprint,
            project_id,
            reply,
        } => {
            let result = if let Some(fe) = filter_engine {
                // persist_discarded_fingerprint takes a sync closure but we need async.
                // Pre-execute the DB query, then call persist with a no-op closure.
                let db_result =
                    queries::filters::discard_fingerprint(pool, &fingerprint, project_id).await;
                match db_result {
                    Ok(()) => to_write(fe.persist_discarded_fingerprint(&fingerprint, || Ok(()))),
                    Err(e) => Err(WriteError::Internal(e.to_string())),
                }
            } else {
                to_write(
                    queries::filters::discard_fingerprint(pool, &fingerprint, project_id).await,
                )
            };
            let _ = reply.send(result);
        }
        WriteMsg::UndiscardFingerprint { fingerprint, reply } => {
            let result = if let Some(fe) = filter_engine {
                let db_result = queries::filters::undiscard_fingerprint(pool, &fingerprint).await;
                match db_result {
                    Ok(()) => to_write(fe.persist_undiscarded_fingerprint(&fingerprint, || Ok(()))),
                    Err(e) => Err(WriteError::Internal(e.to_string())),
                }
            } else {
                to_write(queries::filters::undiscard_fingerprint(pool, &fingerprint).await)
            };
            let _ = reply.send(result);
        }

        // -- Filters (with engine reload) ------------------------------------
        WriteMsg::SetInboundFilter {
            project_id,
            filter_id,
            enabled,
            reply,
        } => {
            dispatch_write!(reload: pool, filter_engine, reply,
                queries::filters::set_inbound_filter(pool, project_id, &filter_id, enabled).await);
        }
        WriteMsg::CreateMessageFilter {
            project_id,
            pattern,
            reply,
        } => {
            dispatch_write!(reload: pool, filter_engine, reply,
                queries::filters::create_message_filter(pool, project_id, &pattern).await);
        }
        WriteMsg::DeleteMessageFilter { id, reply } => {
            dispatch_write!(rows_reload: pool, filter_engine, "message filter", reply,
                queries::filters::delete_message_filter(pool, id).await);
        }
        WriteMsg::SetRateLimit {
            project_id,
            public_key,
            max_events_per_minute,
            reply,
        } => {
            dispatch_write!(reload: pool, filter_engine, reply,
                queries::filters::set_rate_limit(pool, project_id, public_key.as_deref(), max_events_per_minute).await);
        }
        WriteMsg::AddEnvironmentFilter {
            project_id,
            environment,
            reply,
        } => {
            dispatch_write!(reload: pool, filter_engine, reply,
                queries::filters::add_environment_filter(pool, project_id, &environment).await);
        }
        WriteMsg::DeleteEnvironmentFilter { id, reply } => {
            dispatch_write!(rows_reload: pool, filter_engine, "environment filter", reply,
                queries::filters::delete_environment_filter(pool, id).await);
        }
        WriteMsg::AddReleaseFilter {
            project_id,
            pattern,
            reply,
        } => {
            dispatch_write!(reload: pool, filter_engine, reply,
                queries::filters::add_release_filter(pool, project_id, &pattern).await);
        }
        WriteMsg::DeleteReleaseFilter { id, reply } => {
            dispatch_write!(rows_reload: pool, filter_engine, "release filter", reply,
                queries::filters::delete_release_filter(pool, id).await);
        }
        WriteMsg::AddUserAgentFilter {
            project_id,
            pattern,
            reply,
        } => {
            dispatch_write!(reload: pool, filter_engine, reply,
                queries::filters::add_user_agent_filter(pool, project_id, &pattern).await);
        }
        WriteMsg::DeleteUserAgentFilter { id, reply } => {
            dispatch_write!(rows_reload: pool, filter_engine, "user-agent filter", reply,
                queries::filters::delete_user_agent_filter(pool, id).await);
        }
        WriteMsg::CreateFilterRule {
            project_id,
            field,
            operator,
            value,
            action,
            sample_rate,
            priority,
            reply,
        } => {
            dispatch_write!(reload: pool, filter_engine, reply,
                queries::filters::create_filter_rule(pool, project_id, &field, &operator, &value, &action, sample_rate, priority).await);
        }
        WriteMsg::DeleteFilterRule { id, reply } => {
            dispatch_write!(rows_reload: pool, filter_engine, "filter rule", reply,
                queries::filters::delete_filter_rule(pool, id).await);
        }
        WriteMsg::AddIpBlock {
            project_id,
            cidr,
            reply,
        } => {
            dispatch_write!(reload: pool, filter_engine, reply,
                queries::filters::add_ip_block(pool, project_id, &cidr).await);
        }
        WriteMsg::DeleteIpBlock { id, reply } => {
            dispatch_write!(rows_reload: pool, filter_engine, "IP block", reply,
                queries::filters::delete_ip_block(pool, id).await);
        }

        // -- Integrations ----------------------------------------------------
        WriteMsg::CreateIntegration {
            name,
            kind,
            url,
            secret,
            config,
            encrypted,
            reply,
        } => {
            dispatch_write!(
                reply,
                queries::integrations::create_integration(
                    pool,
                    &name,
                    &kind,
                    &url,
                    secret.as_deref(),
                    config.as_deref(),
                    encrypted
                )
                .await
            );
        }
        WriteMsg::DeleteIntegration { id, reply } => {
            dispatch_write!(rows: &format!("integration: {id}"), reply,
                queries::integrations::delete_integration(pool, id).await);
        }
        WriteMsg::ActivateProjectIntegration {
            project_id,
            integration_id,
            notify_new_issues,
            notify_regressions,
            min_level,
            environment_filter,
            config,
            notify_threshold,
            notify_digests,
            reply,
        } => {
            dispatch_write!(
                reply,
                queries::integrations::activate_project_integration(
                    pool,
                    project_id,
                    integration_id,
                    notify_new_issues,
                    notify_regressions,
                    min_level.as_deref(),
                    environment_filter.as_deref(),
                    config.as_deref(),
                    notify_threshold,
                    notify_digests
                )
                .await
            );
        }
        WriteMsg::UpdateProjectIntegration {
            id,
            notify_new_issues,
            notify_regressions,
            min_level,
            environment_filter,
            config,
            notify_threshold,
            notify_digests,
            reply,
        } => {
            dispatch_write!(rows: &format!("project integration: {id}"), reply,
                queries::integrations::update_project_integration(
                    pool, id, notify_new_issues, notify_regressions,
                    min_level.as_deref(), environment_filter.as_deref(), config.as_deref(),
                    notify_threshold, notify_digests).await);
        }
        WriteMsg::DeactivateProjectIntegration { id, reply } => {
            dispatch_write!(rows: &format!("project integration: {id}"), reply,
                queries::integrations::deactivate_project_integration(pool, id).await);
        }

        // -- Alert rules ---------------------------------------------------------
        WriteMsg::CreateAlertRule {
            project_id,
            fingerprint,
            trigger_kind,
            threshold_count,
            window_secs,
            cooldown_secs,
            reply,
        } => {
            dispatch_write!(
                reply,
                queries::alerts::create_alert_rule(
                    pool,
                    project_id,
                    fingerprint.as_deref(),
                    &trigger_kind,
                    threshold_count,
                    window_secs,
                    cooldown_secs
                )
                .await
            );
        }
        WriteMsg::UpdateAlertRule {
            id,
            threshold_count,
            window_secs,
            cooldown_secs,
            enabled,
            reply,
        } => {
            dispatch_write!(rows: &format!("alert rule: {id}"), reply,
                queries::alerts::update_alert_rule(pool, id, threshold_count, window_secs, cooldown_secs, enabled).await);
        }
        WriteMsg::DeleteAlertRule { id, reply } => {
            dispatch_write!(rows: &format!("alert rule: {id}"), reply,
                queries::alerts::delete_alert_rule(pool, id).await);
        }

        // -- Digest schedules ----------------------------------------------------
        WriteMsg::CreateDigestSchedule {
            project_id,
            interval_secs,
            reply,
        } => {
            dispatch_write!(
                reply,
                queries::alerts::create_digest_schedule(pool, project_id, interval_secs).await
            );
        }
        WriteMsg::UpdateDigestSchedule {
            id,
            interval_secs,
            enabled,
            reply,
        } => {
            dispatch_write!(rows: &format!("digest schedule: {id}"), reply,
                queries::alerts::update_digest_schedule(pool, id, interval_secs, enabled).await);
        }
        WriteMsg::DeleteDigestSchedule { id, reply } => {
            dispatch_write!(rows: &format!("digest schedule: {id}"), reply,
                queries::alerts::delete_digest_schedule(pool, id).await);
        }

        WriteMsg::Event(_) | WriteMsg::EventWithAttachments(_, _) | WriteMsg::Shutdown => {
            tracing::warn!("handle_immediate: received batch/shutdown message variant");
        }
    }
}
