//! SQLite-backed [`RevocationStore`] for [`BearerGate`].
//!
//! - `oidc_revocations`: TTL'd `(iss, kind, value)` rows; written by
//!   back-channel logout, read on every authed request.
//! - `oidc_logout_jti`: replay defense for back-channel logout tokens.
//!
//! Fail CLOSED on DB error: a transient 401 beats serving a possibly-revoked
//! session.

use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use stackpit_auth::{RevocationError, RevocationStore};

use crate::db::{sql, DbPool};

/// Cheap to clone; pool is Arc-shared internally.
#[derive(Clone)]
pub struct SqliteRevocationStore {
    pool: DbPool,
}

impl SqliteRevocationStore {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub fn into_arc(self) -> Arc<dyn RevocationStore> {
        Arc::new(self)
    }
}

#[async_trait]
impl RevocationStore for SqliteRevocationStore {
    async fn is_revoked(
        &self,
        iss: &str,
        sub: &str,
        sid: Option<&str>,
    ) -> Result<bool, RevocationError> {
        // Surface the error so the bearer gate's fail-closed log carries
        // the underlying sqlx/context message instead of a bare bool.
        check_revoked(&self.pool, iss, sub, sid)
            .await
            .map_err(|e| RevocationError::Backend(format!("{e:#}")))
    }
}

async fn check_revoked(pool: &DbPool, iss: &str, sub: &str, sid: Option<&str>) -> Result<bool> {
    let now = chrono::Utc::now().timestamp();

    // sub-scope: blocks every device for this user.
    let sub_hit: Option<(i64,)> = sqlx::query_as(sql!(
        "SELECT 1 FROM oidc_revocations \
         WHERE iss = ?1 AND kind = 'sub' AND value = ?2 AND expires_at > ?3 LIMIT 1"
    ))
    .bind(iss)
    .bind(sub)
    .bind(now)
    .fetch_optional(pool)
    .await
    .context("checking sub-scoped revocation")?;
    if sub_hit.is_some() {
        return Ok(true);
    }

    // sid-scope: only when the token carries a sid claim.
    if let Some(sid) = sid {
        let sid_hit: Option<(i64,)> = sqlx::query_as(sql!(
            "SELECT 1 FROM oidc_revocations \
             WHERE iss = ?1 AND kind = 'sid' AND value = ?2 AND expires_at > ?3 LIMIT 1"
        ))
        .bind(iss)
        .bind(sid)
        .bind(now)
        .fetch_optional(pool)
        .await
        .context("checking sid-scoped revocation")?;
        if sid_hit.is_some() {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Insert a sid-scoped revocation marker. `expires_at` is unix seconds.
pub async fn insert_sid(pool: &DbPool, iss: &str, sid: &str, expires_at: i64) -> Result<()> {
    sqlx::query(sql!(
        "INSERT OR REPLACE INTO oidc_revocations (iss, kind, value, expires_at) \
         VALUES (?1, 'sid', ?2, ?3)"
    ))
    .bind(iss)
    .bind(sid)
    .bind(expires_at)
    .execute(pool)
    .await
    .context("inserting sid revocation")?;
    Ok(())
}

/// Insert a sub-scoped revocation marker (whole-user logout).
pub async fn insert_sub(pool: &DbPool, iss: &str, sub: &str, expires_at: i64) -> Result<()> {
    sqlx::query(sql!(
        "INSERT OR REPLACE INTO oidc_revocations (iss, kind, value, expires_at) \
         VALUES (?1, 'sub', ?2, ?3)"
    ))
    .bind(iss)
    .bind(sub)
    .bind(expires_at)
    .execute(pool)
    .await
    .context("inserting sub revocation")?;
    Ok(())
}

/// Purge expired revocation markers and JTI dedupe rows.
pub async fn purge_expired(pool: &DbPool, now_secs: i64) -> Result<u64> {
    let revs = sqlx::query(sql!("DELETE FROM oidc_revocations WHERE expires_at <= ?1"))
        .bind(now_secs)
        .execute(pool)
        .await
        .context("purging expired revocations")?
        .rows_affected();
    let jtis = sqlx::query(sql!("DELETE FROM oidc_logout_jti WHERE expires_at <= ?1"))
        .bind(now_secs)
        .execute(pool)
        .await
        .context("purging expired JTI dedupe")?
        .rows_affected();
    Ok(revs + jtis)
}

/// Atomically remember a back-channel logout JTI. Returns `true` if seen
/// before -- caller MUST reject the request as a replay.
pub async fn jti_seen_or_remember(pool: &DbPool, jti: &str, expires_at: i64) -> Result<bool> {
    let res = sqlx::query(sql!(
        "INSERT INTO oidc_logout_jti (jti, expires_at) VALUES (?1, ?2)"
    ))
    .bind(jti)
    .bind(expires_at)
    .execute(pool)
    .await;

    match res {
        Ok(_) => Ok(false),
        Err(sqlx::Error::Database(db_err)) => {
            // 1555/2067 = SQLite UNIQUE / PK; 23505 = Postgres unique violation.
            let code = db_err.code();
            let code = code.as_deref();
            if matches!(code, Some("1555") | Some("2067") | Some("23505"))
                || db_err.message().contains("UNIQUE")
                || db_err.message().contains("duplicate")
            {
                Ok(true)
            } else {
                Err(anyhow::Error::from(sqlx::Error::Database(db_err)))
                    .context("inserting JTI dedupe row")
            }
        }
        Err(e) => Err(e).context("inserting JTI dedupe row"),
    }
}

/// Grant count for `(iss, sid)`. Logout handler uses this to report how
/// many sessions are being torn down.
#[allow(dead_code)] // future settings UI / audit logging
pub async fn count_grants_for_sid(pool: &DbPool, iss: &str, sid: &str) -> Result<i64> {
    let row: (i64,) = sqlx::query_as(sql!(
        "SELECT COUNT(*) FROM oidc_grants WHERE iss = ?1 AND sid = ?2"
    ))
    .bind(iss)
    .bind(sid)
    .fetch_one(pool)
    .await
    .context("counting grants by sid")?;
    Ok(row.0)
}
