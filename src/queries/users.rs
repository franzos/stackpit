//! User row helpers: composite key (iss, sub) for OIDC identity.
//! Email/name refreshed on every login (JIT); admin_token is a separate privilege path.

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
/// provisioned. Both halves of the key must come from verified id_token
/// claims; never pass client-supplied values here.
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

/// Upsert by `(iss, sub)`. Pass `Some(addr)` only when `email_verified=true`;
/// `None` never downgrades a stored verified email to NULL. Email is unique
/// when non-NULL; conflicts bubble up so the caller refuses the login.
pub async fn upsert_from_oidc(
    pool: &DbPool,
    iss: &str,
    sub: &str,
    verified_email: Option<&str>,
    name: Option<&str>,
) -> Result<UserRow> {
    let now = chrono::Utc::now().timestamp();

    // Atomic upsert: collapses the find-then-insert race on concurrent first logins.
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

    // Round-trip the user_id to avoid SQLite/Postgres LAST_INSERT_ROWID divergence.
    let row = find_by_iss_sub(pool, iss, sub)
        .await?
        .ok_or_else(|| anyhow::anyhow!("user disappeared between upsert and read"))?;
    Ok(row)
}

/// Persist the resolved OIDC `locale` claim. `None` writes NULL (clears any
/// stale value). Dialect handling is via `sql!`, same as the rest of this file.
pub async fn set_preferred_language(
    pool: &DbPool,
    user_id: i64,
    value: Option<&str>,
) -> Result<()> {
    sqlx::query(sql!(
        "UPDATE users SET preferred_language = ?1 WHERE user_id = ?2"
    ))
    .bind(value)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Seed `preferred_language` from an OIDC `locale` claim, first-write-wins: the
/// `WHERE ... preferred_language IS NULL` guard makes this a no-op once the user
/// has an explicit choice, so a claim sent on every login never clobbers it.
/// Atomic (no read-modify-write race).
pub async fn set_preferred_language_if_unset(
    pool: &DbPool,
    user_id: i64,
    value: &str,
) -> Result<()> {
    sqlx::query(sql!(
        "UPDATE users SET preferred_language = ?1 WHERE user_id = ?2 AND preferred_language IS NULL"
    ))
    .bind(value)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Read the persisted `preferred_language` (nullable). `None` when the user is
/// absent or the column is NULL. Used by the locale ladder's persisted step.
pub async fn get_preferred_language(pool: &DbPool, user_id: i64) -> Result<Option<String>> {
    let row = sqlx::query(sql!(
        "SELECT preferred_language FROM users WHERE user_id = ?1"
    ))
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|r| r.get::<Option<String>, _>("preferred_language")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn preferred_language_round_trips() {
        let pool = crate::db::open_test_pool().await;
        let user = upsert_from_oidc(&pool, "https://idp", "sub-lang", None, None)
            .await
            .unwrap();

        // Fresh row defaults to NULL.
        assert_eq!(
            get_preferred_language(&pool, user.user_id).await.unwrap(),
            None
        );

        set_preferred_language(&pool, user.user_id, Some("de"))
            .await
            .unwrap();
        assert_eq!(
            get_preferred_language(&pool, user.user_id).await.unwrap(),
            Some("de".to_string())
        );

        // None clears the stored value back to NULL.
        set_preferred_language(&pool, user.user_id, None)
            .await
            .unwrap();
        assert_eq!(
            get_preferred_language(&pool, user.user_id).await.unwrap(),
            None
        );

        // Absent user reads as None, not an error.
        assert_eq!(get_preferred_language(&pool, 999_999).await.unwrap(), None);
    }

    #[tokio::test]
    async fn set_preferred_language_if_unset_is_first_write_wins() {
        let pool = crate::db::open_test_pool().await;
        let user = upsert_from_oidc(&pool, "https://idp", "sub-seed", None, None)
            .await
            .unwrap();

        // NULL pref: the claim seeds it.
        set_preferred_language_if_unset(&pool, user.user_id, "de")
            .await
            .unwrap();
        assert_eq!(
            get_preferred_language(&pool, user.user_id).await.unwrap(),
            Some("de".to_string())
        );

        // Non-NULL pref: a later claim must not overwrite the user's choice.
        set_preferred_language_if_unset(&pool, user.user_id, "en")
            .await
            .unwrap();
        assert_eq!(
            get_preferred_language(&pool, user.user_id).await.unwrap(),
            Some("de".to_string())
        );
    }
}
