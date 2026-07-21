use crate::db::{sql, DbPool};
use sqlx::Row;

/// Standalone per-project tracker target override (D2): kept in its own table
/// so it never surfaces in the notify dispatcher or the Alerts Hub.
pub async fn set_override(
    pool: &DbPool,
    project_id: i64,
    integration_id: i64,
    target: &serde_json::Value,
) -> anyhow::Result<()> {
    sqlx::query(sql!(
        "INSERT INTO project_tracker_targets (project_id, integration_id, target) \
         VALUES (?1, ?2, ?3) \
         ON CONFLICT (project_id, integration_id) DO UPDATE SET target = excluded.target"
    ))
    .bind(project_id)
    .bind(integration_id)
    .bind(target.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_override(
    pool: &DbPool,
    project_id: i64,
    integration_id: i64,
) -> anyhow::Result<()> {
    sqlx::query(sql!(
        "DELETE FROM project_tracker_targets WHERE project_id = ?1 AND integration_id = ?2"
    ))
    .bind(project_id)
    .bind(integration_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_override(
    pool: &DbPool,
    project_id: i64,
    integration_id: i64,
) -> anyhow::Result<Option<serde_json::Value>> {
    let row = sqlx::query(sql!(
        "SELECT target FROM project_tracker_targets WHERE project_id = ?1 AND integration_id = ?2"
    ))
    .bind(project_id)
    .bind(integration_id)
    .fetch_optional(pool)
    .await?;
    row.map(|r| {
        let target: String = r.get("target");
        Ok(serde_json::from_str(&target)?)
    })
    .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::integrations::create_integration;
    use crate::queries::test_helpers::open_test_db;

    async fn seed_github_integration(pool: &DbPool) -> i64 {
        create_integration(pool, 1, "gh-intg", "github", None, None, None, false)
            .await
            .unwrap();
        sqlx::query(sql!("SELECT id FROM integrations WHERE name = 'gh-intg'"))
            .fetch_one(pool)
            .await
            .unwrap()
            .get(0)
    }

    #[tokio::test]
    async fn override_roundtrips() {
        let pool = open_test_db().await;
        let integration_id = seed_github_integration(&pool).await;

        let t = serde_json::json!({ "owner": "acme", "repo": "frontend" });
        set_override(&pool, 1, integration_id, &t).await.unwrap();
        assert_eq!(
            get_override(&pool, 1, integration_id).await.unwrap(),
            Some(t)
        );
        assert_eq!(get_override(&pool, 1, 999).await.unwrap(), None);
    }

    #[tokio::test]
    async fn set_override_on_conflict_overwrites_target() {
        let pool = open_test_db().await;
        let integration_id = seed_github_integration(&pool).await;

        let first = serde_json::json!({ "owner": "acme", "repo": "frontend" });
        set_override(&pool, 1, integration_id, &first)
            .await
            .unwrap();

        let second = serde_json::json!({ "owner": "acme", "repo": "backend" });
        set_override(&pool, 1, integration_id, &second)
            .await
            .unwrap();

        assert_eq!(
            get_override(&pool, 1, integration_id).await.unwrap(),
            Some(second)
        );
    }

    #[tokio::test]
    async fn get_override_returns_none_for_absent_pair() {
        let pool = open_test_db().await;
        let integration_id = seed_github_integration(&pool).await;

        assert_eq!(get_override(&pool, 1, integration_id).await.unwrap(), None);
    }

    #[tokio::test]
    async fn delete_override_removes_row() {
        let pool = open_test_db().await;
        let integration_id = seed_github_integration(&pool).await;

        let t = serde_json::json!({ "owner": "acme", "repo": "frontend" });
        set_override(&pool, 1, integration_id, &t).await.unwrap();
        assert_eq!(
            get_override(&pool, 1, integration_id).await.unwrap(),
            Some(t)
        );

        delete_override(&pool, 1, integration_id).await.unwrap();
        assert_eq!(get_override(&pool, 1, integration_id).await.unwrap(), None);
    }
}
