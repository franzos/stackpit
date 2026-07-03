use anyhow::{ensure, Context, Result};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{PgPool, SqlitePool};
use std::path::PathBuf;
use std::str::FromStr;

pub struct DbSample {
    pub persisted_delta: u64,
    pub db_bytes: u64,
    pub wal_bytes: u64,
}

pub fn is_postgres_url(db: &str) -> bool {
    db.starts_with("postgres://") || db.starts_with("postgresql://")
}

enum Backend {
    Sqlite {
        pool: SqlitePool,
        db_path: PathBuf,
        last_rowid: i64,
    },
    Pg {
        pool: PgPool,
        last_inserts: i64,
    },
}

pub struct Sampler {
    backend: Backend,
}

impl Sampler {
    pub async fn connect(db: &str) -> Result<Self> {
        if is_postgres_url(db) {
            let opts = PgConnectOptions::from_str(db).context("parse postgres URL")?;
            let pool = PgPoolOptions::new()
                .max_connections(1)
                .connect_with(opts)
                .await
                .context("open sampling connection")?;
            let last_inserts = pg_inserts(&pool).await?;
            Ok(Self {
                backend: Backend::Pg { pool, last_inserts },
            })
        } else {
            let db_path = PathBuf::from(db);
            ensure!(
                db_path.is_file(),
                "database file not found: {}",
                db_path.display()
            );
            let opts = SqliteConnectOptions::new()
                .filename(&db_path)
                .read_only(true)
                .pragma("query_only", "ON");
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(opts)
                .await
                .context("open read-only sampling connection")?;
            let last_rowid = max_rowid(&pool).await?;
            Ok(Self {
                backend: Backend::Sqlite {
                    pool,
                    db_path,
                    last_rowid,
                },
            })
        }
    }

    pub async fn assert_fresh(&self) -> Result<()> {
        let n: i64 = match &self.backend {
            Backend::Sqlite { pool, .. } => {
                sqlx::query_scalar("SELECT COUNT(*) FROM events")
                    .fetch_one(pool)
                    .await?
            }
            Backend::Pg { pool, .. } => {
                sqlx::query_scalar("SELECT COUNT(*) FROM events")
                    .fetch_one(pool)
                    .await?
            }
        };
        ensure!(
            n == 0,
            "events table has {n} rows; the bench needs a fresh database"
        );
        Ok(())
    }

    pub async fn sample(&mut self) -> Result<DbSample> {
        match &mut self.backend {
            Backend::Sqlite {
                pool,
                db_path,
                last_rowid,
            } => {
                let max = max_rowid(pool).await?;
                let delta = (max - *last_rowid).max(0) as u64;
                *last_rowid = max;
                let db_bytes = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
                let wal = db_path.with_extension("db-wal");
                let wal_bytes = std::fs::metadata(&wal).map(|m| m.len()).unwrap_or(0);
                Ok(DbSample {
                    persisted_delta: delta,
                    db_bytes,
                    wal_bytes,
                })
            }
            Backend::Pg { pool, last_inserts } => {
                let inserts = pg_inserts(pool).await?;
                let delta = (inserts - *last_inserts).max(0) as u64;
                *last_inserts = inserts;
                let db_bytes: i64 =
                    sqlx::query_scalar("SELECT pg_database_size(current_database())")
                        .fetch_one(&*pool)
                        .await?;
                Ok(DbSample {
                    persisted_delta: delta,
                    db_bytes: db_bytes.max(0) as u64,
                    wal_bytes: 0,
                })
            }
        }
    }
}

async fn max_rowid(pool: &SqlitePool) -> Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT COALESCE(MAX(rowid), 0) FROM events")
            .fetch_one(pool)
            .await?,
    )
}

// Cumulative inserts counter from pg's stats collector; missing row means the table doesn't exist yet.
async fn pg_inserts(pool: &PgPool) -> Result<i64> {
    let n: Option<i64> =
        sqlx::query_scalar("SELECT n_tup_ins FROM pg_stat_user_tables WHERE relname = 'events'")
            .fetch_optional(pool)
            .await?;
    Ok(n.unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_postgres_urls() {
        assert!(is_postgres_url("postgres://user:pass@localhost/db"));
        assert!(is_postgres_url("postgresql://localhost/db"));
        assert!(!is_postgres_url("/var/lib/stackpit/stackpit.db"));
        assert!(!is_postgres_url("relative/path.db"));
    }

    async fn seed_db(dir: &std::path::Path) -> std::path::PathBuf {
        let path = dir.join("test.db");
        let opts = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let pool = sqlx::SqlitePool::connect_with(opts).await.unwrap();
        sqlx::query(
            "CREATE TABLE events (event_id TEXT PRIMARY KEY, item_type TEXT NOT NULL, \
             payload BLOB NOT NULL, project_id INTEGER NOT NULL, public_key TEXT NOT NULL, \
             timestamp INTEGER NOT NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;
        path
    }

    async fn insert_events(path: &std::path::Path, from: i64, n: i64) {
        let opts = sqlx::sqlite::SqliteConnectOptions::new().filename(path);
        let pool = sqlx::SqlitePool::connect_with(opts).await.unwrap();
        for i in from..from + n {
            sqlx::query("INSERT INTO events VALUES (?1, 'event', x'00', 1, 'k', 0)")
                .bind(format!("ev{i:028}"))
                .execute(&pool)
                .await
                .unwrap();
        }
        pool.close().await;
    }

    #[tokio::test]
    async fn fresh_db_passes_assert_and_deltas_track_inserts() {
        let dir = tempfile::tempdir().unwrap();
        let path = seed_db(dir.path()).await;
        let mut s = Sampler::connect(path.to_str().unwrap()).await.unwrap();
        s.assert_fresh().await.unwrap();
        assert_eq!(s.sample().await.unwrap().persisted_delta, 0);
        insert_events(&path, 0, 5).await;
        assert_eq!(s.sample().await.unwrap().persisted_delta, 5);
        insert_events(&path, 5, 3).await;
        let d = s.sample().await.unwrap();
        assert_eq!(d.persisted_delta, 3);
        assert!(d.db_bytes > 0);
    }

    #[tokio::test]
    async fn non_empty_db_fails_assert_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let path = seed_db(dir.path()).await;
        insert_events(&path, 0, 1).await;
        let s = Sampler::connect(path.to_str().unwrap()).await.unwrap();
        assert!(s.assert_fresh().await.is_err());
    }

    #[tokio::test]
    async fn missing_file_fails_connect() {
        assert!(Sampler::connect("/nonexistent/x.db").await.is_err());
    }
}
