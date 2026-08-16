pub mod alerts;
pub mod api_keys;
pub mod backfill;
pub mod bulk;
pub mod client_reports;
pub mod event_supplements;
pub mod event_sync;
pub mod event_writes;
pub mod events;
pub mod filters;
pub mod integration_exclusions;
pub mod integrations;
pub mod issue_links;
pub mod issues;
pub mod logs;
pub mod metrics;
pub mod monitors;
pub mod notify_queue;
pub mod orgs;
pub mod profiles;
pub mod projects;
pub mod releases;
pub mod replays;
pub mod retention;
pub mod spans;
pub mod transactions;
pub mod types;
pub mod users;

pub use types::*;

// Public functions here (e.g. `issues::update_issue_status`) take `IssueStatus`
// by value, so the type needs a reachable public path.
pub use crate::domain::IssueStatus;

/// Wrap a search term as a `%term%` LIKE pattern, escaping the LIKE
/// metacharacters with `\`. Callers must keep the matching `ESCAPE '\\'` clause.
pub(crate) fn like_contains(needle: &str) -> String {
    let escaped = needle
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

/// Canonical form of an org-id list: sorted and deduped, so two callers with the
/// same entitlements produce the same key and the same SQL.
pub(crate) fn canonical_org_ids(mut ids: Vec<i64>) -> Vec<i64> {
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// Push `<column> IN (SELECT project_id FROM projects WHERE org_id IN (...))`,
/// binding every id. The caller supplies its own `WHERE`/`AND`, and must have
/// short-circuited an empty list: `IN ()` is invalid SQL on both backends.
pub(crate) fn push_org_scope_predicate(
    qb: &mut sqlx::QueryBuilder<crate::db::Db>,
    project_column: &str,
    org_ids: &[i64],
) {
    debug_assert!(!org_ids.is_empty(), "an empty org list must short-circuit");
    qb.push(project_column);
    qb.push(" IN (SELECT project_id FROM projects WHERE org_id IN (");
    let mut sep = qb.separated(", ");
    for id in org_ids {
        sep.push_bind(*id);
    }
    qb.push("))");
}

#[cfg(test)]
pub(crate) mod test_helpers {
    use crate::db::{self, sql, DbPool};

    /// Spins up a throwaway test DB with the full schema applied.
    pub async fn open_test_db() -> DbPool {
        db::open_test_pool().await
    }

    /// Inserts a test event with a zstd-compressed payload.
    pub async fn insert_test_event(
        pool: &DbPool,
        event_id: &str,
        project_id: i64,
        timestamp: i64,
        fingerprint: Option<&str>,
        level: Option<&str>,
        title: Option<&str>,
    ) {
        let payload_json = serde_json::json!({
            "event_id": event_id,
            "message": title.unwrap_or("test event"),
        });
        let payload_bytes = serde_json::to_vec(&payload_json).unwrap();
        let compressed = zstd::encode_all(payload_bytes.as_slice(), 3).unwrap();

        sqlx::query(sql!(
            "INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp, level, title, platform, release, environment, server_name, transaction_name, sdk_name, sdk_version, received_at, fingerprint)
             VALUES (?1, 'event', ?2, ?3, 'testkey', ?4, ?5, ?6, 'rust', 'v1.0', 'production', 'server1', '/api/test', 'sentry.rust', '0.1.0', ?4, ?7)",
        ))
        .bind(event_id)
        .bind(&compressed)
        .bind(project_id)
        .bind(timestamp)
        .bind(level)
        .bind(title)
        .bind(fingerprint)
        .execute(pool)
        .await
        .unwrap();
    }

    /// Inserts a test issue row.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_test_issue(
        pool: &DbPool,
        fingerprint: &str,
        project_id: i64,
        title: Option<&str>,
        level: Option<&str>,
        first_seen: i64,
        last_seen: i64,
        event_count: i64,
        status: &str,
    ) {
        sqlx::query(sql!(
            "INSERT INTO issues (fingerprint, project_id, title, level, first_seen, last_seen, event_count, status, item_type)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'event')",
        ))
        .bind(fingerprint)
        .bind(project_id)
        .bind(title)
        .bind(level)
        .bind(first_seen)
        .bind(last_seen)
        .bind(event_count)
        .bind(status)
        .execute(pool)
        .await
        .unwrap();
    }
}
