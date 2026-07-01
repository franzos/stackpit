use anyhow::Result;
use sqlx::Row;
use std::str::FromStr;

use crate::db::sql;
use crate::db::DbPool;

use crate::domain::IntegrationKind;

use super::types::{Integration, ProjectIntegration};

// --- Read queries ---

fn row_to_integration(row: &crate::db::DbRow) -> Result<Integration> {
    Ok(Integration {
        id: row.get(0),
        name: row.get(1),
        kind: IntegrationKind::from_str(&row.get::<String, _>(2))?,
        url: row.get(3),
        secret: row.get(4),
        encrypted: row.get::<bool, _>(5),
        config: row.get(6),
        created_at: row.get(7),
    })
}

/// All configured integrations (webhooks, Slack, email, etc.).
/// Pass `Some(org_id)` to scope to one org; `None` returns all (superuser only).
pub async fn list_integrations(pool: &DbPool, org_id: Option<i64>) -> Result<Vec<Integration>> {
    let rows = if let Some(oid) = org_id {
        sqlx::query(sql!(
            "SELECT id, name, kind, url, secret, encrypted, config, created_at
             FROM integrations WHERE org_id = ?1 ORDER BY name"
        ))
        .bind(oid)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(sql!(
            "SELECT id, name, kind, url, secret, encrypted, config, created_at
             FROM integrations ORDER BY name"
        ))
        .fetch_all(pool)
        .await?
    };
    rows.iter().map(row_to_integration).collect()
}

/// Fetch a single integration by ID.
/// Pass `Some(org_id)` to restrict to the caller's org (prevents cross-org reads).
pub async fn get_integration(pool: &DbPool, id: i64, org_id: Option<i64>) -> Result<Option<Integration>> {
    let row = if let Some(oid) = org_id {
        sqlx::query(sql!(
            "SELECT id, name, kind, url, secret, encrypted, config, created_at
             FROM integrations WHERE id = ?1 AND org_id = ?2"
        ))
        .bind(id)
        .bind(oid)
        .fetch_optional(pool)
        .await?
    } else {
        sqlx::query(sql!(
            "SELECT id, name, kind, url, secret, encrypted, config, created_at
             FROM integrations WHERE id = ?1"
        ))
        .bind(id)
        .fetch_optional(pool)
        .await?
    };
    row.as_ref().map(row_to_integration).transpose()
}

fn row_to_project_integration(row: &crate::db::DbRow) -> Result<ProjectIntegration> {
    Ok(ProjectIntegration {
        id: row.get(0),
        project_id: row.get::<i64, _>(1) as u64,
        integration_id: row.get(2),
        integration_name: row.get(3),
        integration_kind: IntegrationKind::from_str(&row.get::<String, _>(4))?,
        integration_url: row.get(5),
        integration_secret: row.get(6),
        integration_encrypted: row.get::<bool, _>(7),
        integration_config: row.get(8),
        notify_new_issues: row.get::<bool, _>(9),
        notify_regressions: row.get::<bool, _>(10),
        min_level: row.get(11),
        environment_filter: row.get(12),
        config: row.get(13),
        enabled: row.get::<bool, _>(14),
        notify_threshold: row.get::<bool, _>(15),
        notify_digests: row.get::<bool, _>(16),
    })
}

const PROJECT_INTEGRATION_SELECT: &str = "SELECT pi.id, pi.project_id, pi.integration_id,
            i.name, i.kind, i.url, i.secret, i.encrypted, i.config,
            pi.notify_new_issues, pi.notify_regressions,
            pi.min_level, pi.environment_filter, pi.config, pi.enabled,
            pi.notify_threshold, pi.notify_digests
     FROM project_integrations pi
     JOIN integrations i ON i.id = pi.integration_id";

/// All integrations linked to a project (active and inactive).
pub async fn list_project_integrations(
    pool: &DbPool,
    project_id: u64,
) -> Result<Vec<ProjectIntegration>> {
    let sql = format!("{PROJECT_INTEGRATION_SELECT} WHERE pi.project_id = ?1 ORDER BY i.name");
    let sql = crate::db::translate_sql(&sql);
    let rows = sqlx::query(&sql)
        .bind(project_id as i64)
        .fetch_all(pool)
        .await?;
    rows.iter().map(row_to_project_integration).collect()
}

/// Enabled integrations for a project, used by the notification dispatcher.
pub async fn get_active_for_project(
    pool: &DbPool,
    project_id: u64,
) -> Result<Vec<ProjectIntegration>> {
    let sql = format!(
        "{PROJECT_INTEGRATION_SELECT} WHERE pi.project_id = ?1 AND pi.enabled = TRUE ORDER BY i.name"
    );
    let sql = crate::db::translate_sql(&sql);
    let rows = sqlx::query(&sql)
        .bind(project_id as i64)
        .fetch_all(pool)
        .await?;
    rows.iter().map(row_to_project_integration).collect()
}

/// Integrations not yet linked to a project (candidates for the "add" dropdown).
/// Scoped to `org_id` so only same-org integrations are offered.
pub async fn list_available_for_project(
    pool: &DbPool,
    project_id: u64,
    org_id: i64,
) -> Result<Vec<Integration>> {
    let rows = sqlx::query(sql!(
        "SELECT id, name, kind, url, secret, encrypted, config, created_at
         FROM integrations
         WHERE org_id = ?2
           AND id NOT IN (
               SELECT integration_id FROM project_integrations WHERE project_id = ?1
           )
         ORDER BY name"
    ))
    .bind(project_id as i64)
    .bind(org_id)
    .fetch_all(pool)
    .await?;
    rows.iter().map(row_to_integration).collect()
}

// --- Write operations ---

/// Create a new integration. Returns its row ID.
pub async fn create_integration(
    pool: &DbPool,
    org_id: i64,
    name: &str,
    kind: &str,
    url: Option<&str>,
    secret: Option<&str>,
    config: Option<&str>,
    encrypted: bool,
) -> Result<i64> {
    #[cfg(feature = "sqlite")]
    {
        let result = sqlx::query(sql!(
            "INSERT INTO integrations (org_id, name, kind, url, secret, encrypted, config)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
        ))
        .bind(org_id)
        .bind(name)
        .bind(kind)
        .bind(url)
        .bind(secret)
        .bind(encrypted)
        .bind(config)
        .execute(pool)
        .await?;
        Ok(result.last_insert_rowid())
    }
    #[cfg(not(feature = "sqlite"))]
    {
        let row = sqlx::query(sql!(
            "INSERT INTO integrations (org_id, name, kind, url, secret, encrypted, config)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) RETURNING id"
        ))
        .bind(org_id)
        .bind(name)
        .bind(kind)
        .bind(url)
        .bind(secret)
        .bind(encrypted)
        .bind(config)
        .fetch_one(pool)
        .await?;
        Ok(row.get::<i64, _>("id"))
    }
}

/// Delete an integration in the given org. Returns 0 if not found or wrong org.
pub async fn delete_integration(pool: &DbPool, id: i64, org_id: i64) -> Result<u64> {
    let result = sqlx::query(sql!("DELETE FROM integrations WHERE id = ?1 AND org_id = ?2"))
        .bind(id)
        .bind(org_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Wire up an integration to a project (or re-activate if it was removed).
#[allow(clippy::too_many_arguments)]
pub async fn activate_project_integration(
    pool: &DbPool,
    project_id: u64,
    integration_id: i64,
    notify_new_issues: bool,
    notify_regressions: bool,
    min_level: Option<&str>,
    environment_filter: Option<&str>,
    config: Option<&str>,
    notify_threshold: bool,
    notify_digests: bool,
) -> Result<()> {
    sqlx::query(sql!(
        "INSERT INTO project_integrations (project_id, integration_id, notify_new_issues, notify_regressions, min_level, environment_filter, config, notify_threshold, notify_digests)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(project_id, integration_id) DO UPDATE SET
             notify_new_issues = excluded.notify_new_issues,
             notify_regressions = excluded.notify_regressions,
             min_level = excluded.min_level,
             environment_filter = excluded.environment_filter,
             config = excluded.config,
             notify_threshold = excluded.notify_threshold,
             notify_digests = excluded.notify_digests,
             enabled = TRUE"
    ))
    .bind(project_id as i64)
    .bind(integration_id)
    .bind(notify_new_issues)
    .bind(notify_regressions)
    .bind(min_level)
    .bind(environment_filter)
    .bind(config)
    .bind(notify_threshold)
    .bind(notify_digests)
    .execute(pool)
    .await?;
    Ok(())
}

/// Update notification settings on a project integration.
#[allow(clippy::too_many_arguments)]
pub async fn update_project_integration(
    pool: &DbPool,
    project_id: i64,
    id: i64,
    notify_new_issues: bool,
    notify_regressions: bool,
    min_level: Option<&str>,
    environment_filter: Option<&str>,
    config: Option<&str>,
    notify_threshold: bool,
    notify_digests: bool,
) -> Result<u64> {
    let result = sqlx::query(sql!(
        "UPDATE project_integrations SET
             notify_new_issues = ?1, notify_regressions = ?2,
             min_level = ?3, environment_filter = ?4, config = ?5,
             notify_threshold = ?6, notify_digests = ?7
         WHERE id = ?8 AND project_id = ?9"
    ))
    .bind(notify_new_issues)
    .bind(notify_regressions)
    .bind(min_level)
    .bind(environment_filter)
    .bind(config)
    .bind(notify_threshold)
    .bind(notify_digests)
    .bind(id)
    .bind(project_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Remove a project integration link. Returns 0 if it wasn't found.
pub async fn deactivate_project_integration(pool: &DbPool, project_id: i64, id: i64) -> Result<u64> {
    let result = sqlx::query(sql!(
        "DELETE FROM project_integrations WHERE id = ?1 AND project_id = ?2"
    ))
    .bind(id)
    .bind(project_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::test_helpers::open_test_db;
    use sqlx::Row;

    async fn seed_project_integration(pool: &DbPool, project_id: u64) -> i64 {
        create_integration(pool, 1, "test-intg", "webhook", Some("https://example.com"), None, None, false)
            .await
            .unwrap();
        let integration_id: i64 = sqlx::query(sql!(
            "SELECT id FROM integrations WHERE name = 'test-intg'"
        ))
        .fetch_one(pool)
        .await
        .unwrap()
        .get(0);
        activate_project_integration(pool, project_id, integration_id, false, false, None, None, None, false, false)
            .await
            .unwrap();
        sqlx::query(sql!(
            "SELECT id FROM project_integrations WHERE project_id = ?1 AND integration_id = ?2"
        ))
        .bind(project_id as i64)
        .bind(integration_id)
        .fetch_one(pool)
        .await
        .unwrap()
        .get(0)
    }

    #[tokio::test]
    async fn update_project_integration_cross_project_affects_zero_rows() {
        let pool = open_test_db().await;
        let pi_id = seed_project_integration(&pool, 1).await;
        let rows = update_project_integration(&pool, 2, pi_id, false, false, None, None, None, false, false)
            .await
            .unwrap();
        assert_eq!(rows, 0, "cross-project update must affect 0 rows");
        let rows = update_project_integration(&pool, 1, pi_id, true, false, None, None, None, false, false)
            .await
            .unwrap();
        assert_eq!(rows, 1);
    }

    #[tokio::test]
    async fn deactivate_project_integration_cross_project_affects_zero_rows() {
        let pool = open_test_db().await;
        let pi_id = seed_project_integration(&pool, 1).await;
        let rows = deactivate_project_integration(&pool, 2, pi_id).await.unwrap();
        assert_eq!(rows, 0, "cross-project delete must affect 0 rows");
        let rows = deactivate_project_integration(&pool, 1, pi_id).await.unwrap();
        assert_eq!(rows, 1);
    }

    #[tokio::test]
    async fn list_integrations_scoped_excludes_other_org() {
        let pool = open_test_db().await;
        create_integration(&pool, 1, "intg-org1", "webhook", Some("https://a.example"), None, None, false)
            .await
            .unwrap();
        create_integration(&pool, 2, "intg-org2", "webhook", Some("https://b.example"), None, None, false)
            .await
            .unwrap();
        let org1 = list_integrations(&pool, Some(1)).await.unwrap();
        assert!(org1.iter().any(|i| i.name == "intg-org1"));
        assert!(!org1.iter().any(|i| i.name == "intg-org2"));
        let org2 = list_integrations(&pool, Some(2)).await.unwrap();
        assert!(org2.iter().any(|i| i.name == "intg-org2"));
        assert!(!org2.iter().any(|i| i.name == "intg-org1"));
        // superuser (None) sees all
        let all = list_integrations(&pool, None).await.unwrap();
        assert!(all.iter().any(|i| i.name == "intg-org1"));
        assert!(all.iter().any(|i| i.name == "intg-org2"));
    }

    #[tokio::test]
    async fn get_integration_cross_org_returns_none() {
        let pool = open_test_db().await;
        let id = create_integration(&pool, 1, "cross-get", "webhook", Some("https://x.example"), None, None, false)
            .await
            .unwrap();
        // correct org -> found
        assert!(get_integration(&pool, id, Some(1)).await.unwrap().is_some());
        // wrong org -> None
        assert!(get_integration(&pool, id, Some(2)).await.unwrap().is_none());
        // superuser (None) -> found
        assert!(get_integration(&pool, id, None).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn delete_integration_cross_org_affects_zero_rows() {
        let pool = open_test_db().await;
        let id = create_integration(&pool, 1, "cross-del", "webhook", Some("https://y.example"), None, None, false)
            .await
            .unwrap();
        let rows = delete_integration(&pool, id, 2).await.unwrap();
        assert_eq!(rows, 0, "cross-org delete must affect 0 rows");
        // still exists for correct org
        assert!(get_integration(&pool, id, Some(1)).await.unwrap().is_some());
        let rows = delete_integration(&pool, id, 1).await.unwrap();
        assert_eq!(rows, 1);
    }

    #[tokio::test]
    async fn list_available_for_project_excludes_other_org_integrations() {
        let pool = open_test_db().await;
        create_integration(&pool, 1, "org1-intg", "webhook", Some("https://a.example"), None, None, false)
            .await
            .unwrap();
        create_integration(&pool, 2, "org2-intg", "webhook", Some("https://b.example"), None, None, false)
            .await
            .unwrap();
        // project 99 belongs to org 1 (no links yet)
        let available = list_available_for_project(&pool, 99, 1).await.unwrap();
        assert!(available.iter().any(|i| i.name == "org1-intg"), "org1 integration must be offered");
        assert!(!available.iter().any(|i| i.name == "org2-intg"), "org2 integration must be excluded");
    }

    #[tokio::test]
    async fn activate_cross_org_integration_guard_rejects() {
        let pool = open_test_db().await;
        // Integration belongs to org 2; project owner is in org 1.
        let foreign_id = create_integration(&pool, 2, "foreign-intg", "webhook", Some("https://c.example"), None, None, false)
            .await
            .unwrap();
        // The activate handler guards by calling get_integration with the project's org_id.
        // Confirm it returns None for the wrong org so the handler correctly rejects.
        assert!(
            get_integration(&pool, foreign_id, Some(1)).await.unwrap().is_none(),
            "cross-org integration must not be visible to org 1, so activation is rejected"
        );
    }
}
