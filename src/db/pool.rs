use anyhow::Result;

// Pool type aliases (backend-dependent)

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub type DbPool = sqlx::SqlitePool;
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub type Db = sqlx::Sqlite;
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub type DbRow = sqlx::sqlite::SqliteRow;

#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
pub type DbPool = sqlx::PgPool;
#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
pub type Db = sqlx::Postgres;
#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
pub type DbRow = sqlx::postgres::PgRow;

// Pool creation

/// Create a reader pool from a database URL.
///
/// SQLite sets up WAL, busy timeout, and `query_only=ON` so a stray write on the read pool fails loudly instead of drifting onto it; PostgreSQL is a straightforward pool with no per-connection query_only.
pub async fn create_read_pool(url: &str) -> Result<DbPool> {
    create_pool_inner(url, None, false).await
}

/// Create the writer pool. For SQLite, max_connections=1 and writer-specific
/// PRAGMAs are applied. For PostgreSQL, a standard pool is returned.
pub async fn create_write_pool(url: &str) -> Result<DbPool> {
    create_pool_inner(url, Some(1), true).await
}

/// Create a small background-writer pool for PostgreSQL (low connection ceiling
/// since background tasks are infrequent). SQLite uses per-subsystem writer
/// pools instead, so this is Postgres-only.
#[cfg(feature = "postgres")]
pub async fn create_bg_pool(url: &str) -> Result<DbPool> {
    create_pool_inner(url, Some(2), true).await
}

/// Create the ingest writer pool sized to the configured number of concurrent
/// writer tasks. Postgres-only; SQLite is single-writer by nature.
#[cfg(feature = "postgres")]
pub async fn create_ingest_pool(url: &str, max_connections: u32) -> Result<DbPool> {
    create_pool_inner(url, Some(max_connections), true).await
}

async fn create_pool_inner(
    url: &str,
    max_connections: Option<u32>,
    writer: bool,
) -> Result<DbPool> {
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        create_sqlite_pool(url, max_connections, writer).await
    }

    #[cfg(all(feature = "postgres", not(feature = "sqlite")))]
    {
        let _ = writer; // PG doesn't need writer-specific config
        create_pg_pool(url, max_connections).await
    }
}

#[cfg(feature = "sqlite")]
async fn create_sqlite_pool(
    url: &str,
    max_connections: Option<u32>,
    writer: bool,
) -> Result<sqlx::SqlitePool> {
    use sqlx::sqlite::{
        SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous,
    };
    use std::str::FromStr;

    let opts = SqliteConnectOptions::from_str(url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(std::time::Duration::from_secs(5))
        .pragma("cache_size", "-64000")
        .pragma("auto_vacuum", "INCREMENTAL");

    let opts = if writer {
        opts.foreign_keys(true)
            .pragma("temp_store", "MEMORY")
            .pragma("mmap_size", "268435456")
            .pragma("wal_autocheckpoint", "1000")
    } else {
        // Enforce read-only: any write on the read pool errors instead of drifting.
        opts.foreign_keys(true).pragma("query_only", "ON")
    };

    let pool = SqlitePoolOptions::new()
        .max_connections(max_connections.unwrap_or(4))
        .connect_with(opts)
        .await?;

    Ok(pool)
}

#[cfg(feature = "postgres")]
async fn create_pg_pool(url: &str, max_connections: Option<u32>) -> Result<sqlx::PgPool> {
    use sqlx::postgres::PgPoolOptions;

    let pool = PgPoolOptions::new()
        .max_connections(max_connections.unwrap_or(10))
        .connect(url)
        .await?;

    Ok(pool)
}

// Run migrations

/// Run embedded migrations. For SQLite, runs the sqlite migrations directory.
/// For PostgreSQL, runs the postgres migrations directory.
pub async fn run_migrations(pool: &DbPool) -> Result<()> {
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        sqlx::migrate!("migrations/sqlite").run(pool).await?;
    }

    #[cfg(all(feature = "postgres", not(feature = "sqlite")))]
    {
        sqlx::migrate!("migrations/postgres").run(pool).await?;
    }

    Ok(())
}

/// Run migrations up to and including `version`, so a test can migrate, seed and migrate again.
///
/// SQLite only: Postgres tests share one database whose migration state is global.
#[cfg(all(test, feature = "sqlite", not(feature = "postgres")))]
pub async fn run_migrations_to(pool: &DbPool, version: i64) -> Result<()> {
    use sqlx::migrate::Migrator;
    use std::borrow::Cow;

    let full = sqlx::migrate!("migrations/sqlite");
    let subset: Vec<_> = full
        .migrations
        .iter()
        .filter(|m| m.version <= version)
        .cloned()
        .collect();

    Migrator {
        migrations: Cow::Owned(subset),
        ..Migrator::DEFAULT
    }
    .run(pool)
    .await?;

    Ok(())
}

// Database URL resolution

/// Resolve a database URL from config. If `database_url` is set, use it.
/// Otherwise, convert a SQLite file path to a `sqlite:` URL.
pub fn resolve_database_url(database_url: Option<&str>, path: &str) -> String {
    if let Some(url) = database_url {
        url.to_string()
    } else if path.starts_with("sqlite:")
        || path.starts_with("postgres://")
        || path.starts_with("postgresql://")
    {
        path.to_string()
    } else {
        format!("sqlite:{path}?mode=rwc")
    }
}

#[cfg(all(test, feature = "sqlite", not(feature = "postgres")))]
mod tests {
    use super::*;

    // Proves query_only is live: a write on the read pool must error, not drift on.
    #[tokio::test]
    async fn read_pool_rejects_writes() {
        let pool = create_read_pool("sqlite::memory:").await.unwrap();
        let err = sqlx::query("CREATE TABLE t (x INTEGER)")
            .execute(&pool)
            .await
            .expect_err("write on read pool must fail under query_only");
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("readonly") || msg.contains("read-only") || msg.contains("read only"),
            "expected a read-only error, got: {msg}"
        );
    }

    /// Pool stopped at 022, the pre-rebuild state the tests below assume.
    async fn seeded_at_022() -> DbPool {
        let pool = create_write_pool("sqlite::memory:").await.unwrap();
        run_migrations_to(&pool, 22).await.unwrap();

        // org 1 is the system org, seeded by the migrations themselves.
        sqlx::query("INSERT INTO organizations (org_id, slug, name) VALUES (3, 'acme', 'Acme')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO projects (project_id, status, source, org_id) VALUES (900, 'active', 'manual', 3)",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO integrations (id, org_id, name, kind, url, secret, encrypted, config, created_at)
             VALUES (1, 3, 'ops-slack', 'slack', 'https://hooks.slack.test/x', 'shh', 0, '{\"a\":1}', 1700000000),
                    (2, 3, 'gh', 'github', 'https://github.com', 'tok', 0, NULL, 1700000001)",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO project_integrations (id, project_id, integration_id, min_level, environment_filter, enabled)
             VALUES (10, 900, 1, 'error', 'prod', 1)",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO issue_external_links (id, project_id, fingerprint, integration_id, external_id, external_url, external_state, created_at)
             VALUES (5, 900, 'fp-abc', 2, '1', 'https://github.com/franzos/throwaway/issues/1', NULL, 1700000002)",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO project_tracker_targets (project_id, integration_id, target)
             VALUES (900, 2, '{\"owner\":\"franzos\"}')",
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn rebuild_023_preserves_child_rows() {
        let pool = seeded_at_022().await;
        run_migrations_to(&pool, 23).await.unwrap();

        let pi: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM project_integrations WHERE integration_id = 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(pi, 1, "023 destroyed project_integrations rows");

        let level: String =
            sqlx::query_scalar("SELECT min_level FROM project_integrations WHERE id = 10")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(level, "error", "023 lost per-project column values");

        let links: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issue_external_links")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(links, 1, "023 destroyed issue_external_links rows");

        let targets: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_tracker_targets")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(targets, 1, "023 destroyed project_tracker_targets rows");
    }

    #[tokio::test]
    async fn rebuild_023_preserves_integrations_and_defaults_is_global_off() {
        let pool = seeded_at_022().await;
        run_migrations_to(&pool, 23).await.unwrap();

        let rows: Vec<(i64, i64, String, String, i64)> = sqlx::query_as(
            "SELECT id, org_id, name, kind, is_global FROM integrations ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], (1, 3, "ops-slack".into(), "slack".into(), 0));
        assert_eq!(rows[1], (2, 3, "gh".into(), "github".into(), 0));

        let secret: String = sqlx::query_scalar("SELECT secret FROM integrations WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(secret, "shh", "023 lost the credential");
    }

    #[tokio::test]
    async fn rebuild_023_scopes_name_uniqueness_to_the_org() {
        let pool = seeded_at_022().await;
        run_migrations_to(&pool, 23).await.unwrap();

        sqlx::query(
            "INSERT INTO integrations (org_id, name, kind, url) VALUES (1, 'ops-slack', 'slack', 'https://x.test')",
        )
        .execute(&pool)
        .await
        .expect("the same name in a different org must be allowed after 023");

        sqlx::query(
            "INSERT INTO integrations (org_id, name, kind, url) VALUES (3, 'ops-slack', 'slack', 'https://y.test')",
        )
        .execute(&pool)
        .await
        .expect_err("a duplicate name within one org must still be rejected");
    }

    #[tokio::test]
    async fn migration_025_backfills_link_context_and_drops_the_cascade() {
        let pool = seeded_at_022().await;
        run_migrations_to(&pool, 25).await.unwrap();

        let (name, kind): (String, String) = sqlx::query_as(
            "SELECT integration_name, integration_kind FROM issue_external_links WHERE id = 5",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(name, "gh");
        assert_eq!(kind, "github");

        sqlx::query("DELETE FROM integrations WHERE id = 2")
            .execute(&pool)
            .await
            .unwrap();

        let (url, name): (String, String) = sqlx::query_as(
            "SELECT external_url, integration_name FROM issue_external_links WHERE id = 5",
        )
        .fetch_one(&pool)
        .await
        .expect("link must survive its integration");
        assert_eq!(url, "https://github.com/franzos/throwaway/issues/1");
        assert_eq!(name, "gh", "denormalised context must still read");
    }

    #[tokio::test]
    async fn migration_026_tables_exist_and_exclusions_are_unique_per_pair() {
        let pool = seeded_at_022().await;
        run_migrations_to(&pool, 26).await.unwrap();

        sqlx::query(
            "INSERT INTO integration_exclusions (org_id, integration_id, project_id) VALUES (3, 1, 900)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO integration_exclusions (org_id, integration_id, project_id) VALUES (3, 1, 900)",
        )
        .execute(&pool)
        .await
        .expect_err("one exclusion per (integration, project)");

        sqlx::query(
            "INSERT INTO notification_delivery_queue (org_id, project_id, integration_id, payload, next_attempt_at)
             VALUES (3, 900, 1, '{}', 1700000000)",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query("DELETE FROM integrations WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();
        let queued: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notification_delivery_queue")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(queued, 0);
    }
}
