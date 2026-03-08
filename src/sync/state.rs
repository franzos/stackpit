use anyhow::Result;
use sqlx::Row;

use crate::db::{sql, DbPool};

/// Reads a checkpoint value back as an i64 timestamp.
pub async fn get_checkpoint(pool: &DbPool, key: &str) -> Result<Option<i64>> {
    let result: Option<String> = sqlx::query(sql!("SELECT value FROM sync_state WHERE key = ?1",))
        .bind(key)
        .fetch_optional(pool)
        .await?
        .map(|row| row.get("value"));

    Ok(result.and_then(|v| v.parse::<i64>().ok()))
}

/// Reads a checkpoint value as a raw string (for cursors and other non-numeric values).
pub async fn get_checkpoint_str(pool: &DbPool, key: &str) -> Result<Option<String>> {
    let result = sqlx::query(sql!("SELECT value FROM sync_state WHERE key = ?1",))
        .bind(key)
        .fetch_optional(pool)
        .await?
        .map(|row| row.get("value"));

    Ok(result)
}

/// Persists a checkpoint value. Uses upsert so it's safe to call repeatedly.
pub async fn set_checkpoint(pool: &DbPool, key: &str, value: &str) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query(sql!(
        "INSERT INTO sync_state (key, value, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    ))
    .bind(key)
    .bind(value)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Removes a checkpoint entry -- typically after a sync run completes successfully.
pub async fn clear_checkpoint(pool: &DbPool, key: &str) -> Result<()> {
    sqlx::query(sql!("DELETE FROM sync_state WHERE key = ?1"))
        .bind(key)
        .execute(pool)
        .await?;
    Ok(())
}
