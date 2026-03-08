use anyhow::Result;

// ---------------------------------------------------------------------------
// Pool type aliases -- the concrete type depends on which backend is compiled.
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Pool creation
// ---------------------------------------------------------------------------

/// Create a reader pool from a database URL.
///
/// For SQLite this sets up WAL, busy timeout, and read-only PRAGMAs via
/// `after_connect`. For PostgreSQL it's a straightforward pool.
pub async fn create_read_pool(url: &str) -> Result<DbPool> {
    create_pool_inner(url, None, false).await
}

/// Create the writer pool. For SQLite, max_connections=1 and writer-specific
/// PRAGMAs are applied. For PostgreSQL, a standard pool is returned.
pub async fn create_write_pool(url: &str) -> Result<DbPool> {
    create_pool_inner(url, Some(1), true).await
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
        opts.foreign_keys(true)
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

// ---------------------------------------------------------------------------
// Convenience: run migrations against a pool
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Database URL resolution
// ---------------------------------------------------------------------------

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
