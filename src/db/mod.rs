#[cfg(not(any(feature = "sqlite", feature = "postgres")))]
compile_error!("At least one database backend feature must be enabled: `sqlite` or `postgres`");

pub mod pool;

use anyhow::Result;

pub use pool::{run_migrations, Db, DbPool, DbRow};

// ---------------------------------------------------------------------------
// SQL dialect helpers -- write queries with SQLite-style ?N placeholders;
// they're translated to PostgreSQL $N when compiled with the postgres feature.
// ---------------------------------------------------------------------------

/// Translate a static SQL string's ?N placeholders to $N for PostgreSQL.
/// For SQLite this is a zero-cost pass-through.
///
/// Usage: `sqlx::query(sql!("SELECT ?1, ?2")).bind(a).bind(b)`
#[cfg(feature = "sqlite")]
macro_rules! sql {
    ($s:literal $(,)?) => {
        $s
    };
}

#[cfg(not(feature = "sqlite"))]
macro_rules! sql {
    ($s:literal $(,)?) => {{
        static __SQL: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
            let mut result = String::with_capacity(($s).len());
            let bytes = ($s).as_bytes();
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'?' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
                    result.push('$');
                    i += 1;
                } else {
                    result.push(bytes[i] as char);
                    i += 1;
                }
            }
            result
        });
        __SQL.as_str()
    }};
}

pub(crate) use sql;

/// Runtime variant of `sql!` for dynamically built SQL strings (e.g. from format!).
#[cfg(feature = "sqlite")]
#[inline(always)]
pub fn translate_sql(s: &str) -> std::borrow::Cow<'_, str> {
    std::borrow::Cow::Borrowed(s)
}

#[cfg(not(feature = "sqlite"))]
pub fn translate_sql(s: &str) -> std::borrow::Cow<'_, str> {
    let bytes = s.as_bytes();
    let needs_translate = bytes
        .windows(2)
        .any(|w| w[0] == b'?' && w[1].is_ascii_digit());
    if needs_translate {
        let mut result = String::with_capacity(s.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'?' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
                result.push('$');
                i += 1;
            } else {
                result.push(bytes[i] as char);
                i += 1;
            }
        }
        std::borrow::Cow::Owned(result)
    } else {
        std::borrow::Cow::Borrowed(s)
    }
}

/// Create the read pool and return it. Does NOT run migrations.
pub async fn create_pool(url: &str) -> Result<DbPool> {
    pool::create_read_pool(url).await
}

/// Create the writer pool (max_connections=1 for SQLite, standard for PG).
/// Does NOT run migrations.
pub async fn create_writer_pool(url: &str) -> Result<DbPool> {
    pool::create_write_pool(url).await
}

/// Run a PRAGMA on a SQLite pool. No-op for PostgreSQL.
#[cfg(feature = "sqlite")]
pub async fn sqlite_pragma(pool: &DbPool, pragma: &str) -> Result<()> {
    #[cfg(not(feature = "postgres"))]
    {
        sqlx::query(pragma).execute(pool).await?;
        Ok(())
    }

    #[cfg(feature = "postgres")]
    {
        match pool {
            pool::DbPool::Sqlite(p) => {
                sqlx::query(pragma).execute(p).await?;
            }
            pool::DbPool::Postgres(_) => {} // no-op
        }
        Ok(())
    }
}

/// Fetch a single raw event row by ID. Test-only helper.
#[cfg(test)]
pub(crate) async fn get_event(pool: &DbPool, event_id: &str) -> Result<Option<EventRow>> {
    use sqlx::Row;
    let row = sqlx::query(sql!(
        "SELECT event_id, item_type, project_id, timestamp, level, title, release, environment, received_at, payload
         FROM events WHERE event_id = ?1",
    ))
    .bind(event_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| EventRow {
        event_id: row.get("event_id"),
        item_type: row.get("item_type"),
        project_id: row.get::<i64, _>("project_id") as u64,
        timestamp: row.get("timestamp"),
        level: row.get("level"),
        title: row.get("title"),
        release: row.get("release"),
        environment: row.get("environment"),
        received_at: row.get("received_at"),
        payload: row.get("payload"),
    }))
}

#[cfg(test)]
pub(crate) struct EventRow {
    pub event_id: String,
    #[allow(dead_code)]
    pub item_type: String,
    pub project_id: u64,
    #[allow(dead_code)]
    pub timestamp: i64,
    pub level: Option<String>,
    pub title: Option<String>,
    #[allow(dead_code)]
    pub release: Option<String>,
    #[allow(dead_code)]
    pub environment: Option<String>,
    #[allow(dead_code)]
    pub received_at: i64,
    #[allow(dead_code)]
    pub payload: Vec<u8>,
}

#[cfg(test)]
pub(crate) async fn open_test_pool() -> DbPool {
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let url = "sqlite::memory:";

    #[cfg(all(feature = "postgres", not(feature = "sqlite")))]
    let url_owned = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://stackpit:stackpit@localhost:5432/stackpit_test".into());
    #[cfg(all(feature = "postgres", not(feature = "sqlite")))]
    let url = url_owned.as_str();

    let pool = pool::create_write_pool(url).await.unwrap();
    pool::run_migrations(&pool).await.unwrap();

    // Postgres tests share a real database -- clean all data between tests.
    // TRUNCATE CASCADE handles foreign key ordering for us.
    #[cfg(all(feature = "postgres", not(feature = "sqlite")))]
    {
        sqlx::query(
            "TRUNCATE events, issues, logs, spans, metrics, attachments, \
             issue_tag_values, integrations, project_integrations, \
             alert_rules, alert_state, digest_schedules, \
             project_keys, projects, releases, sourcemaps, upload_chunks, \
             discard_stats, discarded_fingerprints, inbound_filters, \
             message_filters, rate_limits, environment_filters, \
             release_filters, user_agent_filters, filter_rules, \
             ip_blocklist, project_repos, sync_state \
             CASCADE",
        )
        .execute(&pool)
        .await
        .unwrap();
    }

    pool
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::retention::delete_old_events;

    async fn setup() -> DbPool {
        open_test_pool().await
    }

    async fn insert_test_event(pool: &DbPool, event_id: &str, project_id: i64, timestamp: i64) {
        sqlx::query(sql!(
            "INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp, level, title, received_at, monitor_slug, session_status, parent_event_id)
             VALUES (?1, 'event', ?2, ?3, 'testkey', ?4, 'error', 'test title', ?4, NULL, NULL, NULL)",
        ))
        .bind(event_id)
        .bind(&[0u8] as &[u8])
        .bind(project_id)
        .bind(timestamp)
        .execute(pool)
        .await
        .unwrap();
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn open_creates_schema() {
        let pool = setup().await;
        let row: (i32,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN (
                'events','logs','attachments','issues','project_repos','releases','projects','sync_state','project_keys',
                'discarded_fingerprints','inbound_filters','message_filters',
                'rate_limits','environment_filters','release_filters','user_agent_filters',
                'filter_rules','ip_blocklist','discard_stats',
                'issue_tag_values',
                'integrations','project_integrations',
                'alert_rules','alert_state','digest_schedules',
                'organizations',
                'sourcemaps','upload_chunks'
            )",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, 28);
    }

    #[tokio::test]
    async fn get_event_found() {
        let pool = setup().await;
        insert_test_event(&pool, "abc", 1, 100).await;

        let event = get_event(&pool, "abc").await.unwrap().unwrap();
        assert_eq!(event.event_id, "abc");
        assert_eq!(event.project_id, 1);
        assert_eq!(event.title.as_deref(), Some("test title"));
    }

    #[tokio::test]
    async fn get_event_not_found() {
        let pool = setup().await;
        assert!(get_event(&pool, "nope").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_old_events_removes_expired() {
        let pool = setup().await;
        let now = chrono::Utc::now().timestamp();
        let old = now - 100 * 86400;
        insert_test_event(&pool, "old", 1, old).await;
        sqlx::query(sql!(
            "UPDATE events SET received_at = ?1 WHERE event_id = 'old'"
        ))
        .bind(old)
        .execute(&pool)
        .await
        .unwrap();
        insert_test_event(&pool, "new", 1, now).await;

        let deleted = delete_old_events(&pool, 90).await.unwrap();
        assert_eq!(deleted, 1);

        assert!(get_event(&pool, "old").await.unwrap().is_none());
        assert!(get_event(&pool, "new").await.unwrap().is_some());
    }

    async fn insert_event_with_fingerprint(
        pool: &DbPool,
        event_id: &str,
        project_id: i64,
        fingerprint: &str,
        received_at: i64,
    ) {
        sqlx::query(sql!(
            "INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp, level, title, received_at, fingerprint)
             VALUES (?1, 'event', ?2, ?3, 'testkey', ?4, 'error', 'test title', ?4, ?5)",
        ))
        .bind(event_id)
        .bind(&[0u8] as &[u8])
        .bind(project_id)
        .bind(received_at)
        .bind(fingerprint)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn insert_issue(
        pool: &DbPool,
        fingerprint: &str,
        project_id: i64,
        event_count: i64,
        first_seen: i64,
        last_seen: i64,
    ) {
        sqlx::query(sql!(
            "INSERT INTO issues (fingerprint, project_id, title, level, first_seen, last_seen, event_count, status)
             VALUES (?1, ?2, 'test issue', 'error', ?3, ?4, ?5, 'unresolved')",
        ))
        .bind(fingerprint)
        .bind(project_id)
        .bind(first_seen)
        .bind(last_seen)
        .bind(event_count)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn retention_reconciles_issue_counts() {
        let pool = setup().await;
        let now = chrono::Utc::now().timestamp();
        let old = now - 100 * 86400;

        insert_event_with_fingerprint(&pool, "e1", 1, "fp-a", old).await;
        insert_event_with_fingerprint(&pool, "e2", 1, "fp-a", old).await;
        insert_event_with_fingerprint(&pool, "e3", 1, "fp-a", now).await;
        insert_issue(&pool, "fp-a", 1, 3, old, now).await;

        let deleted = delete_old_events(&pool, 90).await.unwrap();
        assert_eq!(deleted, 2);

        let row: (i64,) =
            sqlx::query_as("SELECT event_count FROM issues WHERE fingerprint = 'fp-a'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.0, 1);
    }

    #[tokio::test]
    async fn retention_removes_orphaned_issues() {
        let pool = setup().await;
        let now = chrono::Utc::now().timestamp();
        let old = now - 100 * 86400;

        insert_event_with_fingerprint(&pool, "e1", 1, "fp-orphan", old).await;
        insert_issue(&pool, "fp-orphan", 1, 1, old, old).await;

        let deleted = delete_old_events(&pool, 90).await.unwrap();
        assert_eq!(deleted, 1);

        assert!(get_event(&pool, "e1").await.unwrap().is_none());
        let row: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM issues WHERE fingerprint = 'fp-orphan'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.0, 0);
    }

    #[tokio::test]
    async fn retention_leaves_healthy_issues_alone() {
        let pool = setup().await;
        let now = chrono::Utc::now().timestamp();
        let old = now - 100 * 86400;

        insert_event_with_fingerprint(&pool, "a1", 1, "fp-old", old).await;
        insert_event_with_fingerprint(&pool, "a2", 1, "fp-old", old).await;
        insert_issue(&pool, "fp-old", 1, 2, old, old).await;

        insert_event_with_fingerprint(&pool, "b1", 1, "fp-fresh", now).await;
        insert_event_with_fingerprint(&pool, "b2", 1, "fp-fresh", now).await;
        insert_issue(&pool, "fp-fresh", 1, 2, now, now).await;

        let deleted = delete_old_events(&pool, 90).await.unwrap();
        assert_eq!(deleted, 2);

        let row: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM issues WHERE fingerprint = 'fp-old'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.0, 0);

        let row: (i64,) =
            sqlx::query_as("SELECT event_count FROM issues WHERE fingerprint = 'fp-fresh'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.0, 2);
    }
}
