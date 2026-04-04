pub mod alerts;
pub mod api_keys;
pub mod backfill;
pub mod bulk;
pub mod event_supplements;
pub mod event_sync;
pub mod event_writes;
pub mod events;
pub mod filters;
pub mod integrations;
pub mod issues;
pub mod logs;
pub mod metrics;
pub mod monitors;
pub mod profiles;
pub mod projects;
pub mod releases;
pub mod replays;
pub mod retention;
pub mod spans;
pub mod types;

pub use types::*;

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
