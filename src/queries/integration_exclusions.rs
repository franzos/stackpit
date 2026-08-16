//! Which projects a global integration must skip.
//! Exclusion is the only opt-out - there's deliberately no include list.

use crate::db::{sql, DbPool};
use sqlx::Row;

/// Integrations excluded for this project.
pub async fn excluded_ids(pool: &DbPool, project_id: i64) -> anyhow::Result<Vec<i64>> {
    let rows = sqlx::query(sql!(
        "SELECT integration_id FROM integration_exclusions WHERE project_id = ?1"
    ))
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(|r| r.get("integration_id")).collect())
}

/// Exclude a project from a global integration. Idempotent.
pub async fn exclude(
    pool: &DbPool,
    org_id: i64,
    integration_id: i64,
    project_id: i64,
) -> anyhow::Result<()> {
    sqlx::query(sql!(
        "INSERT INTO integration_exclusions (org_id, integration_id, project_id) \
         VALUES (?1, ?2, ?3) ON CONFLICT (integration_id, project_id) DO NOTHING"
    ))
    .bind(org_id)
    .bind(integration_id)
    .bind(project_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Resume delivery for a project. Returns 0 if it wasn't excluded or is another org's.
pub async fn un_exclude(
    pool: &DbPool,
    org_id: i64,
    integration_id: i64,
    project_id: i64,
) -> anyhow::Result<u64> {
    let res = sqlx::query(sql!(
        "DELETE FROM integration_exclusions \
         WHERE integration_id = ?1 AND project_id = ?2 AND org_id = ?3"
    ))
    .bind(integration_id)
    .bind(project_id)
    .bind(org_id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

pub async fn is_excluded(
    pool: &DbPool,
    integration_id: i64,
    project_id: i64,
) -> anyhow::Result<bool> {
    let row = sqlx::query(sql!(
        "SELECT 1 FROM integration_exclusions WHERE integration_id = ?1 AND project_id = ?2"
    ))
    .bind(integration_id)
    .bind(project_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::test_helpers::open_test_db;

    const ORG: i64 = 5;
    const OTHER_ORG: i64 = 6;

    async fn integration(pool: &DbPool, org_id: i64, name: &str) -> i64 {
        sqlx::query(sql!(
            "INSERT INTO organizations (org_id, slug, name) VALUES (?1, ?2, ?2) \
             ON CONFLICT (org_id) DO NOTHING"
        ))
        .bind(org_id)
        .bind(format!("org-{org_id}"))
        .execute(pool)
        .await
        .unwrap();
        crate::queries::integrations::create_integration(
            pool,
            org_id,
            name,
            "slack",
            Some("https://hooks.test/x"),
            None,
            None,
            false,
            true,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn exclude_is_idempotent_and_reversible() {
        let pool = open_test_db().await;
        let id = integration(&pool, ORG, "ops").await;

        assert!(!is_excluded(&pool, id, 42).await.unwrap());
        exclude(&pool, ORG, id, 42).await.unwrap();
        exclude(&pool, ORG, id, 42).await.unwrap();
        assert!(is_excluded(&pool, id, 42).await.unwrap());
        assert_eq!(excluded_ids(&pool, 42).await.unwrap(), vec![id]);

        assert_eq!(un_exclude(&pool, ORG, id, 42).await.unwrap(), 1);
        assert!(!is_excluded(&pool, id, 42).await.unwrap());
        assert_eq!(un_exclude(&pool, ORG, id, 42).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn un_exclude_is_org_scoped() {
        let pool = open_test_db().await;
        let id = integration(&pool, ORG, "ops-scoped").await;
        exclude(&pool, ORG, id, 42).await.unwrap();

        assert_eq!(un_exclude(&pool, OTHER_ORG, id, 42).await.unwrap(), 0);
        assert!(is_excluded(&pool, id, 42).await.unwrap());
    }

    #[tokio::test]
    async fn exclusions_go_with_their_integration() {
        let pool = open_test_db().await;
        let id = integration(&pool, ORG, "ops-gone").await;
        exclude(&pool, ORG, id, 42).await.unwrap();

        crate::queries::integrations::delete_integration(&pool, id, ORG)
            .await
            .unwrap();
        assert!(excluded_ids(&pool, 42).await.unwrap().is_empty());
    }
}
