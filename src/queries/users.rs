//! User row helpers: composite key (iss, sub) for OIDC identity.
//! Email/name refreshed on every login (JIT); no privilege split (admin_token separate).

use anyhow::Result;
use sqlx::Row;

use crate::db::{sql, DbPool};

#[derive(Debug, Clone)]
pub struct UserRow {
    pub user_id: i64,
    #[allow(dead_code)]
    pub iss: String,
    #[allow(dead_code)]
    pub sub: String,
    #[allow(dead_code)]
    pub email: Option<String>,
    #[allow(dead_code)]
    pub name: Option<String>,
}

/// Look up a user by their OIDC `(iss, sub)` pair. Returns `None` if not
/// provisioned. Both halves of the key come straight from the verified
/// id_token claims -- never trust client-supplied values here.
pub async fn find_by_iss_sub(pool: &DbPool, iss: &str, sub: &str) -> Result<Option<UserRow>> {
    let row = sqlx::query(sql!(
        "SELECT user_id, iss, sub, email, name FROM users WHERE iss = ?1 AND sub = ?2"
    ))
    .bind(iss)
    .bind(sub)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| UserRow {
        user_id: r.get("user_id"),
        iss: r.get("iss"),
        sub: r.get("sub"),
        email: r.get("email"),
        name: r.get("name"),
    }))
}

/// Upsert by `(iss, sub)`. Pass `Some(addr)` only when `email_verified=true`; `None` never downgrades a stored verified email to NULL. Unique non-NULL email; conflicts bubble up so the caller refuses the login.
pub async fn upsert_from_oidc(
    pool: &DbPool,
    iss: &str,
    sub: &str,
    verified_email: Option<&str>,
    name: Option<&str>,
) -> Result<UserRow> {
    let now = chrono::Utc::now().timestamp();

    // Atomic upsert -- collapses the find-then-insert race on concurrent first logins.
    sqlx::query(sql!(
        "INSERT INTO users (iss, sub, email, name, last_seen) \
         VALUES (?1, ?2, ?3, ?4, ?5) \
         ON CONFLICT (iss, sub) DO UPDATE SET \
             email = COALESCE(excluded.email, users.email), \
             name = excluded.name, \
             last_seen = excluded.last_seen"
    ))
    .bind(iss)
    .bind(sub)
    .bind(verified_email)
    .bind(name)
    .bind(now)
    .execute(pool)
    .await?;

    // Round-trip for the user_id; avoids SQLite/Postgres LAST_INSERT_ROWID divergence.
    let row = find_by_iss_sub(pool, iss, sub)
        .await?
        .ok_or_else(|| anyhow::anyhow!("user disappeared between upsert and read"))?;
    Ok(row)
}
