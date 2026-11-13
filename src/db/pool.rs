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

/// Begin a transaction that reads before it writes; SQLite can't upgrade a deferred one once another writer commits.
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub(crate) async fn begin_write(pool: &DbPool) -> sqlx::Result<sqlx::Transaction<'static, Db>> {
    pool.begin_with("BEGIN IMMEDIATE").await
}

/// Begin a transaction that reads before it writes; Postgres needs no up-front lock.
#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
pub(crate) async fn begin_write(pool: &DbPool) -> sqlx::Result<sqlx::Transaction<'static, Db>> {
    pool.begin().await
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

    let path = opts.get_filename().to_path_buf();
    let pool = SqlitePoolOptions::new()
        .max_connections(max_connections.unwrap_or(4))
        .connect_with(opts)
        .await?;
    secure_sqlite_files(&path);

    Ok(pool)
}

/// Owner-only mode on the database file and its WAL siblings. In-memory
/// databases carry a `file:sqlx-in-memory-N` name, hence the `is_file` guard.
#[cfg(all(feature = "sqlite", unix))]
fn secure_sqlite_files(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;

    if !path.is_file() {
        return;
    }
    let mut files = vec![path.to_path_buf()];
    for suffix in ["-wal", "-shm"] {
        let mut sibling = path.as_os_str().to_owned();
        sibling.push(suffix);
        files.push(sibling.into());
    }
    for file in files.iter().filter(|f| f.is_file()) {
        // Best effort: a file we cannot chmod still opens.
        let _ = std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600));
    }
}

#[cfg(all(feature = "sqlite", not(unix)))]
fn secure_sqlite_files(_path: &std::path::Path) {}

#[cfg(feature = "postgres")]
async fn create_pg_pool(url: &str, max_connections: Option<u32>) -> Result<sqlx::PgPool> {
    use sqlx::postgres::PgPoolOptions;

    let pool = PgPoolOptions::new()
        .max_connections(max_connections.unwrap_or(10))
        .connect(url)
        .await?;

    Ok(pool)
}

/// Single-connection pool for `stackpit migrate`.
///
/// SQLite: `foreign_keys` off, since the pragma is a no-op inside the transaction each migration runs in, and `journal_mode=DELETE` so a table rebuild doesn't grow the WAL to the size of the table. [`restore_journal_mode`] switches back to WAL.
pub async fn create_migration_pool(url: &str) -> Result<DbPool> {
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
        use std::str::FromStr;

        let opts = SqliteConnectOptions::from_str(url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Delete)
            .foreign_keys(false)
            .busy_timeout(std::time::Duration::from_secs(5))
            .pragma("auto_vacuum", "INCREMENTAL");
        let path = opts.get_filename().to_path_buf();
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await?;
        secure_sqlite_files(&path);
        Ok(pool)
    }

    #[cfg(all(feature = "postgres", not(feature = "sqlite")))]
    {
        create_pg_pool(url, Some(1)).await
    }
}

/// Put a migration pool's database back into WAL mode. No-op on Postgres.
pub async fn restore_journal_mode(pool: &DbPool) -> Result<()> {
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query("PRAGMA journal_mode = WAL")
        .execute(pool)
        .await?;
    #[cfg(all(feature = "postgres", not(feature = "sqlite")))]
    let _ = pool;
    Ok(())
}

// Run migrations

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("migrations/sqlite");
#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("migrations/postgres");

/// Migrations too slow to run at boot on a populated database; `stackpit migrate` applies them.
const OFFLINE_MIGRATIONS: &[i64] = &[34, 35, 36];

/// Run embedded migrations, refusing when an offline migration is pending on a database that holds events.
pub async fn run_migrations(pool: &DbPool) -> Result<()> {
    refuse_offline_pending(pool, OFFLINE_MIGRATIONS).await?;
    MIGRATOR.run(pool).await?;
    Ok(())
}

async fn refuse_offline_pending(pool: &DbPool, offline: &[i64]) -> Result<()> {
    let pending = pending_offline(pool, offline).await?;
    if pending.is_empty() || !events_present(pool).await? {
        return Ok(());
    }
    let list = pending
        .iter()
        .map(|v| format!("{v:03}"))
        .collect::<Vec<_>>()
        .join(", ");
    anyhow::bail!(
        "database needs offline migrations ({list}) that are too slow to run at startup; stop stackpit and run `stackpit migrate`"
    )
}

/// Versions in `offline` that are embedded but not applied. A fresh database has none.
async fn pending_offline(pool: &DbPool, offline: &[i64]) -> Result<Vec<i64>> {
    let Some(applied) = applied_versions(pool).await? else {
        return Ok(Vec::new());
    };
    let embedded: Vec<i64> = up_migrations().map(|m| m.version).collect();
    Ok(offline_pending(offline, &embedded, &applied))
}

fn offline_pending(offline: &[i64], embedded: &[i64], applied: &[i64]) -> Vec<i64> {
    offline
        .iter()
        .copied()
        .filter(|v| embedded.contains(v) && !applied.contains(v))
        .collect()
}

fn up_migrations() -> impl Iterator<Item = &'static sqlx::migrate::Migration> {
    MIGRATOR
        .iter()
        .filter(|m| !m.migration_type.is_down_migration())
}

async fn table_exists(pool: &DbPool, table: &str) -> Result<bool> {
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
    )
    .bind(table)
    .fetch_one(pool)
    .await?
        > 0;
    #[cfg(all(feature = "postgres", not(feature = "sqlite")))]
    let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
        .bind(table)
        .fetch_one(pool)
        .await?;
    Ok(exists)
}

/// Versions recorded in `_sqlx_migrations`, or `None` when the table doesn't exist yet.
async fn applied_versions(pool: &DbPool) -> Result<Option<Vec<i64>>> {
    if !table_exists(pool, "_sqlx_migrations").await? {
        return Ok(None);
    }
    Ok(Some(
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success")
            .fetch_all(pool)
            .await?,
    ))
}

async fn events_present(pool: &DbPool) -> Result<bool> {
    if !table_exists(pool, "events").await? {
        return Ok(false);
    }
    Ok(sqlx::query_scalar::<_, i32>("SELECT 1 FROM events LIMIT 1")
        .fetch_optional(pool)
        .await?
        .is_some())
}

/// Apply every pending migration, offline-listed or not, one file at a time with progress on stdout.
pub async fn run_migrations_unchecked(pool: &DbPool) -> Result<()> {
    use sqlx::migrate::Migrator;
    use std::borrow::Cow;
    use std::time::{Duration, Instant};

    const HEARTBEAT: Duration = Duration::from_secs(10);

    let applied = applied_versions(pool).await?.unwrap_or_default();
    let pending: Vec<_> = up_migrations()
        .filter(|m| !applied.contains(&m.version))
        .collect();
    if pending.is_empty() {
        println!("no pending migrations");
        return Ok(());
    }

    for migration in pending {
        println!(
            "applying {:03} {} ...",
            migration.version, migration.description
        );
        // ignore_missing: every already-applied version is absent from a one-migration set.
        let single = Migrator {
            migrations: Cow::Owned(vec![migration.clone()]),
            ignore_missing: true,
            ..Migrator::DEFAULT
        };

        let start = Instant::now();
        let run = single.run(pool);
        tokio::pin!(run);
        let mut heartbeat =
            tokio::time::interval_at(tokio::time::Instant::now() + HEARTBEAT, HEARTBEAT);
        loop {
            tokio::select! {
                result = &mut run => {
                    result?;
                    break;
                }
                _ = heartbeat.tick() => {
                    println!("  {:.0?} elapsed{}", start.elapsed(), size_report(pool));
                }
            }
        }
        println!("  done in {:.1?}", start.elapsed());
    }
    Ok(())
}

/// `, db X MiB, journal Y MiB` for a file-backed SQLite database; empty otherwise.
fn size_report(pool: &DbPool) -> String {
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let mib = |p: &std::path::Path| {
            std::fs::metadata(p)
                .map(|m| format!("{:.1} MiB", m.len() as f64 / (1024.0 * 1024.0)))
                .ok()
        };
        let path = pool.connect_options().get_filename().to_path_buf();
        let mut journal = path.as_os_str().to_owned();
        journal.push("-journal");
        let mut out = String::new();
        if let Some(db) = mib(&path) {
            out.push_str(&format!(", db {db}"));
        }
        if let Some(j) = mib(std::path::Path::new(&journal)) {
            out.push_str(&format!(", journal {j}"));
        }
        out
    }
    #[cfg(all(feature = "postgres", not(feature = "sqlite")))]
    {
        let _ = pool;
        String::new()
    }
}

/// Run migrations up to and including `version`, so a test can migrate, seed and migrate again.
///
/// On Postgres only against a scratch database: the shared test database's migration state is global.
#[cfg(test)]
pub async fn run_migrations_to(pool: &DbPool, version: i64) -> Result<()> {
    use sqlx::migrate::Migrator;
    use std::borrow::Cow;

    let subset: Vec<_> = MIGRATOR
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

    #[cfg(unix)]
    #[tokio::test]
    async fn sqlite_database_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let path = std::env::temp_dir().join(format!("stackpit-perm-{}.db", std::process::id()));
        let pool = create_write_pool(&format!("sqlite://{}", path.display()))
            .await
            .unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        pool.close().await;
        for suffix in ["", "-wal", "-shm"] {
            let mut file = path.as_os_str().to_owned();
            file.push(suffix);
            let _ = std::fs::remove_file(file);
        }
        assert_eq!(mode, 0o600);
    }

    #[tokio::test]
    async fn in_memory_database_creates_no_file() {
        let pool = create_read_pool("sqlite::memory:").await.unwrap();
        pool.close().await;
        assert!(!std::path::Path::new("file:sqlx-in-memory-0").exists());
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

    #[tokio::test]
    async fn migration_028_widens_the_link_key_without_losing_rows_or_ids() {
        let pool = seeded_at_022().await;
        run_migrations_to(&pool, 28).await.unwrap();

        // The existing row survives, keeping its id: the unlink route takes
        // `link_id` straight off the rendered page.
        let (id, url): (i64, String) =
            sqlx::query_as("SELECT id, external_url FROM issue_external_links")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(id, 5, "028 must preserve link ids");
        assert_eq!(url, "https://github.com/franzos/throwaway/issues/1");

        // Two repositories of one integration, filed from one issue.
        sqlx::query(
            "INSERT INTO issue_external_links (project_id, fingerprint, integration_id, integration_name, integration_kind, external_id, external_url, created_at)
             VALUES (900, 'fp-abc', 2, 'gh', 'github', 'acme/frontend#9', 'https://github.com/acme/frontend/issues/9', 1700000003)",
        )
        .execute(&pool)
        .await
        .expect("a second repository on the same integration is a second link");

        // A true duplicate of the whole triple is still rejected.
        sqlx::query(
            "INSERT INTO issue_external_links (project_id, fingerprint, integration_id, integration_name, integration_kind, external_id, external_url, created_at)
             VALUES (900, 'fp-abc', 2, 'gh', 'github', 'acme/frontend#9', 'https://github.com/acme/frontend/issues/9', 1700000004)",
        )
        .execute(&pool)
        .await
        .expect_err("the same (fingerprint, integration, external_id) must still collide");
    }

    /// The rebuild must not re-add the foreign key. A plain `NULL` insert would
    /// prove nothing — NULL satisfies any FK — so this deletes the integration
    /// and asserts the link outlives it, which is what the cascade would break.
    /// `foreign_keys(true)` is set on the writer pool, so the check is live.
    #[tokio::test]
    async fn migration_028_keeps_integration_id_nullable_and_free_of_a_foreign_key() {
        let pool = seeded_at_022().await;
        run_migrations_to(&pool, 28).await.unwrap();

        sqlx::query(
            "INSERT INTO issue_external_links (project_id, fingerprint, integration_id, integration_name, integration_kind, external_id, external_url, created_at)
             VALUES (900, 'fp-orphan', 4242, 'gone', 'github', '1', 'https://x/1', 1700000005)",
        )
        .execute(&pool)
        .await
        .expect("a non-existent integration_id must be accepted: there is no FK");

        sqlx::query("DELETE FROM integrations WHERE id = 2")
            .execute(&pool)
            .await
            .unwrap();
        let surviving: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM issue_external_links WHERE id = 5")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            surviving, 1,
            "028 re-added the cascade and ate a filed link"
        );

        // Orphans stay mutually distinct: no NULLS NOT DISTINCT.
        for fp in ["fp-a", "fp-a"] {
            sqlx::query(
                "INSERT INTO issue_external_links (project_id, fingerprint, integration_id, integration_name, integration_kind, external_id, external_url, created_at)
                 VALUES (900, ?1, NULL, 'gone', 'github', '1', 'https://x/1', 1700000006)",
            )
            .bind(fp)
            .execute(&pool)
            .await
            .expect("NULL integration_ids must not collide with each other");
        }
    }

    /// SQLite drops indexes with the table, so the rebuild has to recreate both.
    #[tokio::test]
    async fn migration_028_recreates_both_indexes() {
        let pool = seeded_at_022().await;
        run_migrations_to(&pool, 28).await.unwrap();

        let names: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = 'issue_external_links' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            names,
            vec![
                "idx_issue_external_links_fingerprint".to_string(),
                "idx_issue_external_links_integration".to_string(),
            ]
        );
    }

    /// Pool stopped at 028 with one issue, its discard, one tag row and one
    /// orphaned tag row, all under fingerprint `fp-shared` in project 900, plus
    /// a second org with project 901 that 029 must let reuse the fingerprint.
    async fn seeded_at_028() -> DbPool {
        let pool = seeded_at_022().await;
        run_migrations_to(&pool, 28).await.unwrap();

        sqlx::query("INSERT INTO organizations (org_id, slug, name) VALUES (4, 'beta', 'Beta')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO projects (project_id, status, source, org_id) VALUES (901, 'active', 'manual', 4)",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO issues (fingerprint, project_id, title, first_seen, last_seen, event_count)
             VALUES ('fp-shared', 900, 'Shared', 1700000000, 1700000001, 3)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO discarded_fingerprints (fingerprint, project_id) VALUES ('fp-shared', 900)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO issue_tag_values (fingerprint, tag_key, tag_value, count)
             VALUES ('fp-shared', 'browser', 'chrome', 2), ('fp-orphan', 'browser', 'chrome', 1)",
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn migration_029_backfills_tag_values_and_drops_orphans() {
        let pool = seeded_at_028().await;
        run_migrations_to(&pool, 29).await.unwrap();

        let rows: Vec<(i64, String, String, String, i64)> = sqlx::query_as(
            "SELECT project_id, fingerprint, tag_key, tag_value, count FROM issue_tag_values ORDER BY fingerprint",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            rows,
            vec![(
                900,
                "fp-shared".into(),
                "browser".into(),
                "chrome".into(),
                2
            )],
            "029 must backfill project_id from issues and drop the orphan"
        );

        let (pid, count): (i64, i64) = sqlx::query_as(
            "SELECT project_id, event_count FROM issues WHERE fingerprint = 'fp-shared'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((pid, count), (900, 3));

        let discards: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM discarded_fingerprints WHERE project_id = 900 AND fingerprint = 'fp-shared'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(discards, 1);
    }

    #[tokio::test]
    async fn migration_029_composite_key_allows_the_same_fingerprint_in_two_projects() {
        let pool = seeded_at_028().await;
        run_migrations_to(&pool, 29).await.unwrap();

        sqlx::query(
            "INSERT INTO issues (fingerprint, project_id, title, first_seen, last_seen, event_count)
             VALUES ('fp-shared', 901, 'Other', 1700000005, 1700000006, 1)",
        )
        .execute(&pool)
        .await
        .expect("the same fingerprint in a second project is a distinct issue");
        sqlx::query(
            "INSERT INTO discarded_fingerprints (fingerprint, project_id) VALUES ('fp-shared', 901)",
        )
        .execute(&pool)
        .await
        .expect("a discard is scoped to its project");
        sqlx::query(
            "INSERT INTO issue_tag_values (project_id, fingerprint, tag_key, tag_value, count)
             VALUES (901, 'fp-shared', 'browser', 'chrome', 7)",
        )
        .execute(&pool)
        .await
        .expect("tag counts are scoped to their project");

        // A true duplicate within one project still collides.
        sqlx::query(
            "INSERT INTO issues (fingerprint, project_id, title, first_seen, last_seen)
             VALUES ('fp-shared', 900, 'Dup', 1, 2)",
        )
        .execute(&pool)
        .await
        .expect_err("(project_id, fingerprint) must stay unique");

        let (title, count): (String, i64) = sqlx::query_as(
            "SELECT title, event_count FROM issues WHERE project_id = 900 AND fingerprint = 'fp-shared'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            (title.as_str(), count),
            ("Shared", 3),
            "project 900's row must be untouched"
        );
        let tag_count: i64 = sqlx::query_scalar(
            "SELECT count FROM issue_tag_values WHERE project_id = 900 AND fingerprint = 'fp-shared'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(tag_count, 2);
    }

    /// 030 is additive and deliberately does not backfill: a transaction
    /// ingested before it must keep reading NULL rather than fail the migration.
    #[tokio::test]
    async fn migration_030_adds_nullable_trace_columns_and_leaves_old_rows_null() {
        let pool = seeded_at_028().await;
        run_migrations_to(&pool, 29).await.unwrap();

        sqlx::query(
            "INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp, trace_id)
             VALUES ('e1', 'transaction', x'00', 900, 'pk', 1700000000, 'aabbccddeeff00112233445566778899')",
        )
        .execute(&pool)
        .await
        .unwrap();

        run_migrations_to(&pool, 30).await.unwrap();

        let (span_id, parent_span_id, start_ms): (Option<String>, Option<String>, Option<i64>) =
            sqlx::query_as(
                "SELECT span_id, parent_span_id, start_ms FROM events WHERE event_id = 'e1'",
            )
            .fetch_one(&pool)
            .await
            .expect("030 must add all three columns");
        assert_eq!((span_id, parent_span_id, start_ms), (None, None, None));

        // Nullable, so a row can be written without them.
        sqlx::query(
            "INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp)
             VALUES ('e2', 'error', x'00', 900, 'pk', 1700000001)",
        )
        .execute(&pool)
        .await
        .expect("the three new columns must be nullable");
    }

    #[tokio::test]
    async fn migration_033_backfills_platform_from_first_key_then_recent_events() {
        let pool = create_write_pool("sqlite::memory:").await.unwrap();
        run_migrations_to(&pool, 32).await.unwrap();

        sqlx::query(
            "INSERT INTO projects (project_id, source) VALUES
                (1, 'manual'), (2, 'manual'), (3, 'auto'), (4, 'auto'), (5, 'auto')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO project_keys (public_key, project_id, label, created_at) VALUES
                ('k1a', 1, 'rust', 100), ('k1b', 1, 'python', 200), ('k2', 2, NULL, 100)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp, platform) VALUES
                ('e1', 'error', x'00', 1, 'k1a', 10, 'native'),
                ('e2a', 'error', x'00', 2, 'k2', 10, 'python'),
                ('e2b', 'error', x'00', 2, 'k2', 20, 'native'),
                ('e2c', 'error', x'00', 2, 'k2', 30, NULL),
                ('e3', 'error', x'00', 3, 'x', 10, 'javascript')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 300)
             INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp, platform)
             SELECT 'e4-' || i, 'error', x'00', 4, 'x', 1000 + i, NULL FROM n",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp, platform)
             VALUES ('e4-old', 'error', x'00', 4, 'x', 1, 'python')",
        )
        .execute(&pool)
        .await
        .unwrap();

        run_migrations_to(&pool, 33).await.unwrap();

        let rows: Vec<(i64, Option<String>)> =
            sqlx::query_as("SELECT project_id, platform FROM projects ORDER BY project_id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            rows,
            vec![
                (1, Some("rust".into())),
                (2, Some("native".into())),
                (3, Some("javascript".into())),
                (4, None),
                (5, None),
            ]
        );
    }

    /// SQLite drops indexes with the table, so the rebuild has to recreate every one.
    #[tokio::test]
    async fn migration_029_recreates_every_index() {
        let pool = seeded_at_028().await;
        run_migrations_to(&pool, 29).await.unwrap();

        async fn index_names(pool: &DbPool, table: &str) -> Vec<String> {
            sqlx::query_scalar(
                "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = ?1 AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .bind(table)
            .fetch_all(pool)
            .await
            .unwrap()
        }

        assert_eq!(
            index_names(&pool, "issues").await,
            vec![
                "idx_issues_fingerprint".to_string(),
                "idx_issues_project_status".to_string(),
                "idx_issues_project_time".to_string(),
                "idx_issues_project_type".to_string(),
                "idx_issues_sentry_group_id".to_string(),
            ]
        );
        assert_eq!(
            index_names(&pool, "discarded_fingerprints").await,
            vec!["idx_discarded_fp_project".to_string()]
        );
        assert_eq!(
            index_names(&pool, "issue_tag_values").await,
            vec!["idx_issue_tag_values_project_fp_key_count".to_string()]
        );
    }
}

#[cfg(test)]
mod offline_tests {
    use super::*;

    #[test]
    fn listed_versions_missing_from_the_binary_are_not_pending() {
        let embedded: Vec<i64> = (1..=33).collect();
        assert!(offline_pending(&[34, 35, 36], &embedded, &embedded).is_empty());
    }

    #[test]
    fn listed_versions_embedded_but_unapplied_are_pending() {
        let embedded: Vec<i64> = (1..=36).collect();
        let applied: Vec<i64> = (1..=34).collect();
        assert_eq!(
            offline_pending(&[34, 35, 36], &embedded, &applied),
            vec![35, 36]
        );
    }

    #[test]
    fn an_unlisted_migration_after_the_offline_ones_does_not_gate_boot() {
        let embedded: Vec<i64> = (1..=37).collect();
        let applied: Vec<i64> = (1..=36).collect();
        assert!(offline_pending(&[34, 35, 36], &embedded, &applied).is_empty());
    }

    #[cfg(all(feature = "postgres", not(feature = "sqlite")))]
    #[tokio::test]
    async fn migrate_reruns_cleanly_on_a_migrated_postgres_database() {
        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://stackpit:stackpit@localhost:5432/stackpit_test".into());
        let pool = crate::db::open_test_pool().await;

        let applied = applied_versions(&pool).await.unwrap().unwrap();
        assert!(applied.contains(&33));
        assert!(pending_offline(&pool, &[33]).await.unwrap().is_empty());

        crate::cli::migrate::run(&url).await.unwrap();
        run_migrations(&pool).await.unwrap();
    }
}

/// File-backed test database, removed on drop; `sqlite::memory:` is per connection.
#[cfg(all(test, feature = "sqlite"))]
pub(crate) struct TempDb(std::path::PathBuf);

#[cfg(all(test, feature = "sqlite"))]
impl TempDb {
    pub(crate) fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("stackpit-{name}-{}.db", std::process::id()));
        let db = Self(path);
        db.remove();
        db
    }

    pub(crate) fn url(&self) -> String {
        format!("sqlite://{}?mode=rwc", self.0.display())
    }

    fn remove(&self) {
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let mut file = self.0.as_os_str().to_owned();
            file.push(suffix);
            let _ = std::fs::remove_file(file);
        }
    }
}

#[cfg(all(test, feature = "sqlite"))]
impl Drop for TempDb {
    fn drop(&mut self) {
        self.remove();
    }
}

/// Two migrated writer pools on one database file, as two processes would see it.
#[cfg(all(test, feature = "sqlite"))]
pub(crate) async fn two_writer_pools(name: &str) -> (TempDb, DbPool, DbPool) {
    let db = TempDb::new(name);
    let a = create_write_pool(&db.url()).await.unwrap();
    run_migrations(&a).await.unwrap();
    let b = create_write_pool(&db.url()).await.unwrap();
    (db, a, b)
}

/// Run `write` in an immediate transaction on `pool` and return once it has
/// executed; the transaction commits `hold` later on the returned task.
#[cfg(all(test, feature = "sqlite"))]
pub(crate) async fn hold_write_lock(
    pool: &DbPool,
    write: &'static str,
    hold: std::time::Duration,
) -> tokio::task::JoinHandle<()> {
    let pool = pool.clone();
    let (written_tx, written_rx) = tokio::sync::oneshot::channel();
    let handle = tokio::spawn(async move {
        let mut tx = begin_write(&pool).await.unwrap();
        sqlx::query(write).execute(&mut *tx).await.unwrap();
        written_tx.send(()).unwrap();
        tokio::time::sleep(hold).await;
        tx.commit().await.unwrap();
    });
    written_rx.await.unwrap();
    handle
}

#[cfg(all(test, feature = "sqlite", not(feature = "postgres")))]
mod migrate_tests {
    use super::*;

    async fn applied_count(pool: &DbPool) -> usize {
        applied_versions(pool).await.unwrap().unwrap().len()
    }

    #[tokio::test]
    async fn gate_refuses_a_populated_database_until_migrate_runs() {
        let db = TempDb::new("gate-populated");
        let pool = create_write_pool(&db.url()).await.unwrap();
        run_migrations_to(&pool, 32).await.unwrap();
        sqlx::query("INSERT INTO projects (project_id, source) VALUES (1, 'manual')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp)
             VALUES ('e1', 'error', x'00', 1, 'k', 10)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let err = refuse_offline_pending(&pool, &[33])
            .await
            .expect_err("a pending offline migration on a populated database must refuse");
        let msg = err.to_string();
        assert!(msg.contains("033"), "{msg}");
        assert!(msg.contains("stackpit migrate"), "{msg}");
        pool.close().await;

        crate::cli::migrate::run(&db.url()).await.unwrap();

        let pool = create_write_pool(&db.url()).await.unwrap();
        assert_eq!(applied_count(&pool).await, up_migrations().count());
        refuse_offline_pending(&pool, &[33]).await.unwrap();
        // A full run validates every row the one-migration runs recorded.
        run_migrations(&pool).await.unwrap();

        let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(mode, "wal");
        pool.close().await;

        crate::cli::migrate::run(&db.url())
            .await
            .expect("migrate must be idempotent");
        let pool = create_write_pool(&db.url()).await.unwrap();
        let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(events, 1);
        pool.close().await;
    }

    #[tokio::test]
    async fn gate_lets_an_empty_database_through() {
        let pool = create_write_pool("sqlite::memory:").await.unwrap();
        assert!(applied_versions(&pool).await.unwrap().is_none());
        refuse_offline_pending(&pool, &[33]).await.unwrap();

        run_migrations_to(&pool, 32).await.unwrap();
        refuse_offline_pending(&pool, &[33]).await.unwrap();

        run_migrations(&pool).await.unwrap();
        assert_eq!(applied_count(&pool).await, up_migrations().count());
    }

    #[tokio::test]
    async fn gate_lets_an_interrupted_first_boot_through() {
        let pool = create_write_pool("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE _sqlx_migrations (version BIGINT PRIMARY KEY, description TEXT NOT NULL,
             installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP, success BOOLEAN NOT NULL,
             checksum BLOB NOT NULL, execution_time BIGINT NOT NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        refuse_offline_pending(&pool, &[33]).await.unwrap();
    }

    #[tokio::test]
    async fn migrate_takes_a_fresh_database_straight_through() {
        let db = TempDb::new("migrate-fresh");
        crate::cli::migrate::run(&db.url()).await.unwrap();

        let pool = create_write_pool(&db.url()).await.unwrap();
        assert_eq!(applied_count(&pool).await, up_migrations().count());
        run_migrations(&pool).await.unwrap();
        pool.close().await;
    }

    #[tokio::test]
    async fn migrate_creates_an_incremental_auto_vacuum_database() {
        let db = TempDb::new("migrate-auto-vacuum");
        crate::cli::migrate::run(&db.url()).await.unwrap();

        // A plain connection, so the pools' own auto_vacuum pragma can't mask the file header.
        let pool = sqlx::SqlitePool::connect(&db.url()).await.unwrap();
        let mode: i64 = sqlx::query_scalar("PRAGMA auto_vacuum")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(mode, 2);
        pool.close().await;
    }
}

#[cfg(test)]
mod payload_split_tests {
    use super::*;
    use crate::db::sql;

    const EVENT_COLUMNS: &[&str] = &[
        "event_id",
        "project_id",
        "item_type",
        "timestamp",
        "received_at",
        "trace_id",
        "duration_ms",
        "start_ms",
        "span_id",
        "parent_span_id",
        "trace_op",
        "trace_status",
        "level",
        "fingerprint",
        "release",
        "environment",
        "transaction_name",
        "title",
        "monitor_slug",
        "session_status",
        "parent_event_id",
        "platform",
        "server_name",
        "sdk_name",
        "sdk_version",
        "public_key",
    ];

    const EVENT_INDEXES: &[&str] = &[
        "idx_events_fingerprint",
        "idx_events_level",
        "idx_events_monitor",
        "idx_events_parent",
        "idx_events_project_release",
        "idx_events_project_time",
        "idx_events_project_txn",
        "idx_events_project_type_time",
        "idx_events_received",
        "idx_events_trace",
        "idx_events_trace_list",
        "idx_events_txn_duration",
        "idx_events_type_time",
    ];

    const SPAN_INDEXES: &[&str] = &[
        "idx_spans_project_time",
        "idx_spans_project_trace",
        "idx_spans_received",
        "idx_spans_root",
        "idx_spans_trace",
    ];

    const TAG_INDEXES: &[&str] = &[
        "idx_issue_tag_values_project_fp_key_count",
        "idx_itv_key_value",
    ];

    fn zstd_json(value: &serde_json::Value) -> Vec<u8> {
        zstd::encode_all(serde_json::to_vec(value).unwrap().as_slice(), 3).unwrap()
    }

    fn txn_payload(n: u32) -> serde_json::Value {
        serde_json::json!({"transaction": format!("/t{n}"), "contexts": {"trace": {"op": "http.server"}}})
    }

    /// Seeds a database at 033: two transactions, an error with an attachment, a
    /// replay with metadata, a client report, and spans whose sequence sits above MAX(id).
    async fn seed_at_033(pool: &DbPool) {
        sqlx::query("INSERT INTO projects (project_id, source) VALUES (1, 'manual')")
            .execute(pool)
            .await
            .unwrap();
        let rows: [(&str, &str, Vec<u8>); 5] = [
            ("t1", "transaction", zstd_json(&txn_payload(1))),
            ("t2", "transaction", zstd_json(&txn_payload(2))),
            (
                "err",
                "event",
                zstd_json(&serde_json::json!({"message": "boom"})),
            ),
            (
                "rep",
                "replay_event",
                zstd_json(&serde_json::json!({"urls": []})),
            ),
            (
                "cr",
                "client_report",
                zstd_json(&serde_json::json!({
                    "discarded_events": [{"category": "error", "reason": "sample_rate", "quantity": 4}]
                })),
            ),
        ];
        for (event_id, item_type, payload) in &rows {
            sqlx::query(sql!(
                "INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp, received_at, trace_id, duration_ms)
                 VALUES (?1, ?2, ?3, 1, 'pk', 1700000000, 1700000000, 'tr', 12)"
            ))
            .bind(event_id)
            .bind(item_type)
            .bind(payload)
            .execute(pool)
            .await
            .unwrap();
        }
        sqlx::query(sql!(
            "INSERT INTO attachments (event_id, filename, data) VALUES ('err', 'a.txt', ?1)"
        ))
        .bind(&b"blob"[..])
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO replay_metadata (event_id, project_id, error_count) VALUES ('rep', 1, 2)",
        )
        .execute(pool)
        .await
        .unwrap();
        for span_id in ["s1", "s2", "s3"] {
            sqlx::query(sql!(
                "INSERT INTO spans (span_id, payload, project_id, public_key, timestamp, trace_id)
                 VALUES (?1, ?2, 1, 'pk', 1700000000, 'tr')"
            ))
            .bind(span_id)
            .bind(&b"{}"[..])
            .execute(pool)
            .await
            .unwrap();
        }
        sqlx::query("DELETE FROM spans WHERE span_id = 's3'")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE events SET fingerprint = 'fp', release = '1.0',
                 environment = CASE event_id WHEN 'err' THEN 'prod' ELSE '' END
             WHERE event_id IN ('err', 'rep')",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO issue_tag_values (project_id, fingerprint, tag_key, tag_value, count) VALUES
                 (1, 'fp', 'environment', 'sdk-env', 7),
                 (1, 'fp', 'environment', 'prod', 3),
                 (1, 'fp', 'browser', 'chrome', 2)",
        )
        .execute(pool)
        .await
        .unwrap();
    }

    /// Assertions shared by both backends on a database `stackpit migrate` took past 036.
    async fn assert_migrated(pool: &DbPool) {
        for (table, expected) in [
            ("events", 5),
            ("event_payloads", 5),
            ("attachments", 1),
            ("replay_metadata", 1),
            ("spans", 2),
            ("client_report_outcomes", 1),
        ] {
            let n: i64 =
                sqlx::query_scalar(crate::db::dyn_sql(&format!("SELECT COUNT(*) FROM {table}")))
                    .fetch_one(pool)
                    .await
                    .unwrap();
            assert_eq!(n, expected, "{table}");
        }

        let detail = crate::queries::events::get_event_detail(pool, "t2")
            .await
            .unwrap()
            .expect("t2 must survive with its payload");
        assert_eq!(detail.payload, txn_payload(2));

        let trace: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT event_id, trace_op, trace_status FROM events
             WHERE event_id IN ('t1', 't2', 'err') ORDER BY event_id",
        )
        .fetch_all(pool)
        .await
        .unwrap();
        let s = |v: &str| Some(v.to_string());
        assert_eq!(
            trace,
            vec![
                ("err".to_string(), None, None),
                ("t1".to_string(), s("http.server"), s("")),
                ("t2".to_string(), s("http.server"), s("")),
            ]
        );
        let rerun = crate::cli::migrate::backfill_trace_op_and_status(pool, 5000)
            .await
            .unwrap();
        assert_eq!(rerun, 0);

        let outcomes: Vec<(i64, i64, String, String, i64)> = sqlx::query_as(
            "SELECT project_id, hour_bucket, category, reason, quantity FROM client_report_outcomes",
        )
        .fetch_all(pool)
        .await
        .unwrap();
        let outcome = (
            1,
            1699999200,
            "error".to_string(),
            "sample_rate".to_string(),
            4,
        );
        assert_eq!(outcomes, vec![outcome]);
        let rerun = crate::cli::migrate::backfill_client_report_outcomes(pool, 5000)
            .await
            .unwrap();
        assert_eq!(rerun, None, "a completed backfill is not repeated");

        let (data, errors): (Vec<u8>, i64) = sqlx::query_as(
            "SELECT a.data, m.error_count FROM attachments a, replay_metadata m
             WHERE a.event_id = 'err' AND m.event_id = 'rep'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!((data.as_slice(), errors), (&b"blob"[..], 2));

        let id: i64 = sqlx::query_scalar(
            "INSERT INTO spans (span_id, project_id, public_key, timestamp) VALUES ('s4', 1, 'pk', 0) RETURNING id",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(id, 4, "the span id sequence must carry over");

        let tags: Vec<(String, String, i64)> = sqlx::query_as(
            "SELECT tag_key, tag_value, count FROM issue_tag_values
             WHERE project_id = 1 AND fingerprint = 'fp' ORDER BY tag_key, tag_value",
        )
        .fetch_all(pool)
        .await
        .unwrap();
        let t = |k: &str, v: &str, n| (k.to_string(), v.to_string(), n);
        assert_eq!(
            tags,
            vec![
                t("browser", "chrome", 2),
                t("environment", "prod", 1),
                t("release", "1.0", 2),
            ]
        );
    }

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    #[tokio::test]
    async fn migrate_splits_payloads_and_rebuilds_events_and_spans() {
        let path =
            std::env::temp_dir().join(format!("stackpit-payload-split-{}.db", std::process::id()));
        let remove = || {
            for suffix in ["", "-wal", "-shm", "-journal"] {
                let mut file = path.as_os_str().to_owned();
                file.push(suffix);
                let _ = std::fs::remove_file(file);
            }
        };
        remove();
        let url = format!("sqlite://{}?mode=rwc", path.display());

        let pool = create_write_pool(&url).await.unwrap();
        run_migrations_to(&pool, 33).await.unwrap();
        seed_at_033(&pool).await;
        pool.close().await;

        crate::cli::migrate::run(&url).await.unwrap();

        let pool = create_write_pool(&url).await.unwrap();
        assert_migrated(&pool).await;

        let columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('events') ORDER BY cid")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(columns, EVENT_COLUMNS);
        let span_columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('spans')")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert!(!span_columns.iter().any(|c| c == "payload"));

        for child in ["attachments", "replay_metadata", "event_payloads"] {
            let parents: Vec<String> =
                sqlx::query_scalar("SELECT \"table\" FROM pragma_foreign_key_list(?1)")
                    .bind(child)
                    .fetch_all(&pool)
                    .await
                    .unwrap();
            assert_eq!(parents, vec!["events"], "{child}");
        }
        let violations = sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert!(violations.is_empty());
        let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(integrity, "ok");
        let seq_names: Vec<String> =
            sqlx::query_scalar("SELECT name FROM sqlite_sequence WHERE name LIKE 'spans%'")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(seq_names, vec!["spans"]);

        for (table, expected) in [
            ("events", EVENT_INDEXES),
            ("spans", SPAN_INDEXES),
            ("issue_tag_values", TAG_INDEXES),
        ] {
            let names: Vec<String> = sqlx::query_scalar(
                "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = ?1
                 AND name NOT LIKE 'sqlite_autoindex_%' ORDER BY name",
            )
            .bind(table)
            .fetch_all(&pool)
            .await
            .unwrap();
            assert_eq!(names, expected, "{table}");
        }

        sqlx::query("DELETE FROM events WHERE event_id = 'err'")
            .execute(&pool)
            .await
            .unwrap();
        let left: i64 = sqlx::query_scalar(
            "SELECT (SELECT COUNT(*) FROM event_payloads WHERE event_id = 'err')
                  + (SELECT COUNT(*) FROM attachments WHERE event_id = 'err')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            left, 0,
            "children must cascade from the rebuilt events table"
        );
        pool.close().await;

        crate::cli::migrate::run(&url).await.unwrap();
        let pool = create_write_pool(&url).await.unwrap();
        let quantity: i64 = sqlx::query_scalar("SELECT SUM(quantity) FROM client_report_outcomes")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(quantity, 4, "a second migrate must not add to the rollup");

        pool.close().await;
        remove();
    }

    #[cfg(all(feature = "postgres", not(feature = "sqlite")))]
    #[tokio::test]
    async fn migrate_splits_payloads_and_drops_the_columns() {
        let base = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://stackpit:stackpit@localhost:5432/stackpit_test".into());
        let name = format!("stackpit_payload_split_{}", std::process::id());
        let (prefix, _) = base.rsplit_once('/').unwrap();
        let url = format!("{prefix}/{name}");

        let admin = create_pg_pool(&base, Some(1)).await.unwrap();
        let drop_db = format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)");
        sqlx::query(crate::db::dyn_sql(&drop_db))
            .execute(&admin)
            .await
            .unwrap();
        sqlx::query(crate::db::dyn_sql(&format!("CREATE DATABASE {name}")))
            .execute(&admin)
            .await
            .unwrap();

        let pool = create_pg_pool(&url, Some(2)).await.unwrap();
        run_migrations_to(&pool, 33).await.unwrap();
        seed_at_033(&pool).await;
        pool.close().await;

        crate::cli::migrate::run(&url).await.unwrap();

        let pool = create_pg_pool(&url, Some(2)).await.unwrap();
        assert_migrated(&pool).await;

        let mut columns: Vec<String> = sqlx::query_scalar(
            "SELECT column_name::text FROM information_schema.columns WHERE table_name = 'events'",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        columns.sort();
        let mut expected: Vec<&str> = EVENT_COLUMNS.to_vec();
        expected.sort_unstable();
        assert_eq!(columns, expected);
        let span_payload: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.columns WHERE table_name = 'spans' AND column_name = 'payload'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(span_payload, 0);

        let parents: Vec<String> = sqlx::query_scalar(
            "SELECT confrelid::regclass::text FROM pg_constraint
             WHERE conrelid = 'event_payloads'::regclass AND contype = 'f'",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(parents, vec!["events"]);

        for (table, expected) in [
            ("events", EVENT_INDEXES),
            ("spans", SPAN_INDEXES),
            ("issue_tag_values", TAG_INDEXES),
        ] {
            let names: Vec<String> = sqlx::query_scalar(
                "SELECT indexname::text FROM pg_indexes WHERE tablename = $1
                 AND indexname LIKE 'idx\\_%' ORDER BY indexname",
            )
            .bind(table)
            .fetch_all(&pool)
            .await
            .unwrap();
            assert_eq!(names, expected, "{table}");
        }

        sqlx::query("DELETE FROM events WHERE event_id = 'err'")
            .execute(&pool)
            .await
            .unwrap();
        let left: i64 = sqlx::query_scalar(
            "SELECT (SELECT COUNT(*) FROM event_payloads WHERE event_id = 'err')
                  + (SELECT COUNT(*) FROM attachments WHERE event_id = 'err')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(left, 0);

        pool.close().await;
        sqlx::query(crate::db::dyn_sql(&drop_db))
            .execute(&admin)
            .await
            .unwrap();
    }
}
