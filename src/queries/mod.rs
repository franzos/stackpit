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

/// Wrap a search term as a `term%` LIKE pattern, escaping the LIKE
/// metacharacters with `\`. Callers must keep the matching `ESCAPE '\\'` clause.
pub(crate) fn like_prefix(needle: &str) -> String {
    let escaped = needle
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("{escaped}%")
}

/// Length of a W3C/Sentry trace id in hex characters.
pub const TRACE_ID_LEN: usize = 32;

/// Shortest prefix accepted as a trace-id lookup. Below this a hex string is
/// far likelier to be a word someone meant to search for.
const TRACE_ID_MIN_PREFIX: usize = 8;

/// Read a string as a full or partial trace id, normalised for comparison.
/// `None` when it cannot be one, which is what keeps an ordinary search term
/// out of the trace path.
pub fn trace_id_candidate(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if !(TRACE_ID_MIN_PREFIX..=TRACE_ID_LEN).contains(&trimmed.len())
        || !trimmed.chars().all(|c| c.is_ascii_hexdigit())
    {
        return None;
    }
    // Stored ids are lowercase hex, and both `=` and PostgreSQL's `LIKE` are
    // case-sensitive.
    Some(trimmed.to_ascii_lowercase())
}

/// Push a trace-id match on `column`. A full id compares with `=` so it rides
/// the trace index; a shorter one falls back to a prefix `LIKE`, which does not.
/// `column` is `&'static str` so no caller can route input into the SQL text.
pub(crate) fn push_trace_id_predicate(
    qb: &mut sqlx::QueryBuilder<crate::db::Db>,
    column: &'static str,
    trace_id: &str,
) {
    qb.push(column);
    if trace_id.len() >= TRACE_ID_LEN {
        qb.push(" = ");
        qb.push_bind(trace_id.to_string());
    } else {
        qb.push(" LIKE ");
        qb.push_bind(like_prefix(trace_id));
        qb.push(" ESCAPE '\\'");
    }
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
/// `project_column` is `&'static str` so no caller can route input into the SQL text.
pub(crate) fn push_org_scope_predicate(
    qb: &mut sqlx::QueryBuilder<crate::db::Db>,
    project_column: &'static str,
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

#[cfg(test)]
mod tests {
    use super::*;

    // The sniff decides whether a search term is treated as a trace id at all,
    // so it has to stay narrow enough that ordinary words fall through to the
    // title search.
    #[test]
    fn only_trace_shaped_terms_are_read_as_trace_ids() {
        let full = "0123456789abcdef0123456789abcdef";
        assert_eq!(trace_id_candidate(full).as_deref(), Some(full));
        assert_eq!(
            trace_id_candidate(&format!("  {} ", full.to_uppercase())).as_deref(),
            Some(full),
            "pasted ids arrive padded and in either case"
        );
        assert_eq!(
            trace_id_candidate("0123456789ab").as_deref(),
            Some("0123456789ab")
        );

        assert!(trace_id_candidate("deadbee").is_none(), "too short");
        assert!(
            trace_id_candidate(&format!("{full}0")).is_none(),
            "too long"
        );
        assert!(trace_id_candidate("TypeError").is_none());
        assert!(trace_id_candidate("timeout!").is_none());
        assert!(trace_id_candidate("").is_none());
    }

    #[test]
    fn like_patterns_escape_their_metacharacters() {
        assert_eq!(like_contains("100%_x"), "%100\\%\\_x%");
        assert_eq!(like_prefix("a\\b"), "a\\\\b%");
    }
}
