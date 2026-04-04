use anyhow::Result;
use sqlx::Row;

use crate::db::{sql, DbPool};

pub struct ApiKeyInfo {
    pub project_id: u64,
    pub scope: String,
}

pub struct ApiKeyDisplay {
    pub key_prefix: String,
    pub created_at: i64,
}

/// Create an API key for a project+scope, replacing any existing one.
pub async fn create_api_key(
    pool: &DbPool,
    project_id: u64,
    scope: &str,
    key_hash: &str,
    key_prefix: &str,
) -> Result<()> {
    // One key per project per scope
    sqlx::query(sql!(
        "DELETE FROM api_keys WHERE project_id = ?1 AND scope = ?2"
    ))
    .bind(project_id as i64)
    .bind(scope)
    .execute(pool)
    .await?;

    sqlx::query(sql!(
        "INSERT INTO api_keys (key_hash, key_prefix, project_id, scope) VALUES (?1, ?2, ?3, ?4)"
    ))
    .bind(key_hash)
    .bind(key_prefix)
    .bind(project_id as i64)
    .bind(scope)
    .execute(pool)
    .await?;

    Ok(())
}

/// Look up an API key by its SHA-256 hash. Used for auth validation.
pub async fn get_api_key_by_hash(pool: &DbPool, key_hash: &str) -> Result<Option<ApiKeyInfo>> {
    let row = sqlx::query(sql!(
        "SELECT project_id, scope FROM api_keys WHERE key_hash = ?1"
    ))
    .bind(key_hash)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| ApiKeyInfo {
        project_id: r.get::<i64, _>("project_id") as u64,
        scope: r.get("scope"),
    }))
}

/// Get the display info for a project's API key in a given scope.
pub async fn get_api_key_for_project(
    pool: &DbPool,
    project_id: u64,
    scope: &str,
) -> Result<Option<ApiKeyDisplay>> {
    let row = sqlx::query(sql!(
        "SELECT key_prefix, created_at FROM api_keys WHERE project_id = ?1 AND scope = ?2"
    ))
    .bind(project_id as i64)
    .bind(scope)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| ApiKeyDisplay {
        key_prefix: r.get("key_prefix"),
        created_at: r.get("created_at"),
    }))
}

/// Delete the API key for a project+scope.
#[allow(dead_code)]
pub async fn delete_api_key_for_project(
    pool: &DbPool,
    project_id: u64,
    scope: &str,
) -> Result<u64> {
    let result = sqlx::query(sql!(
        "DELETE FROM api_keys WHERE project_id = ?1 AND scope = ?2"
    ))
    .bind(project_id as i64)
    .bind(scope)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}
