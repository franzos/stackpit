use anyhow::Result;
use sqlx::Row;

use crate::db::{sql, DbPool};

pub struct ApiKeyInfo {
    pub project_id: u64,
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

/// Look up an API key by its SHA-256 hash and expected scope. Scope lives in
/// the SQL `WHERE` so no (non-constant-time) Rust-side scope compare is needed.
pub async fn get_api_key_by_hash(
    pool: &DbPool,
    key_hash: &str,
    scope: &str,
) -> Result<Option<ApiKeyInfo>> {
    let row = sqlx::query(sql!(
        "SELECT project_id FROM api_keys WHERE key_hash = ?1 AND scope = ?2"
    ))
    .bind(key_hash)
    .bind(scope)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| ApiKeyInfo {
        project_id: r.get::<i64, _>("project_id") as u64,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::test_helpers::open_test_db;
    use sha2::{Digest, Sha256};

    #[tokio::test]
    async fn lookup_matches_on_hash_and_scope() {
        let pool = open_test_db().await;
        let token = "sk_test_token_value";
        let hash = hex::encode(Sha256::digest(token.as_bytes()));
        create_api_key(&pool, 42, "events:write", &hash, "sk_test_")
            .await
            .unwrap();

        let info = get_api_key_by_hash(&pool, &hash, "events:write")
            .await
            .unwrap();
        assert!(info.is_some(), "expected a row for matching hash+scope");
        assert_eq!(info.unwrap().project_id, 42);
    }

    #[tokio::test]
    async fn lookup_rejects_wrong_scope() {
        let pool = open_test_db().await;
        let token = "sk_test_token_value";
        let hash = hex::encode(Sha256::digest(token.as_bytes()));
        create_api_key(&pool, 42, "events:write", &hash, "sk_test_")
            .await
            .unwrap();

        let info = get_api_key_by_hash(&pool, &hash, "sourcemap")
            .await
            .unwrap();
        assert!(info.is_none(), "wrong scope must not return a row");
    }

    #[tokio::test]
    async fn lookup_rejects_unknown_hash() {
        let pool = open_test_db().await;
        let token = "sk_test_token_value";
        let hash = hex::encode(Sha256::digest(token.as_bytes()));
        create_api_key(&pool, 42, "events:write", &hash, "sk_test_")
            .await
            .unwrap();

        let bogus = hex::encode(Sha256::digest(b"some-other-token"));
        let info = get_api_key_by_hash(&pool, &bogus, "events:write")
            .await
            .unwrap();
        assert!(info.is_none(), "unknown hash must not return a row");
    }
}
