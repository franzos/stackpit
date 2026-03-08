use anyhow::Result;
use sqlx::Row;

use crate::db::sql;
use crate::db::DbPool;

use super::types::{Integration, ProjectIntegration};

// --- Read queries ---

fn row_to_integration(row: &crate::db::DbRow) -> Integration {
    Integration {
        id: row.get(0),
        name: row.get(1),
        kind: row.get(2),
        url: row.get(3),
        secret: row.get(4),
        encrypted: row.get::<bool, _>(5),
        config: row.get(6),
        created_at: row.get(7),
    }
}

/// All configured integrations -- webhooks, Slack, email, etc.
pub async fn list_integrations(pool: &DbPool) -> Result<Vec<Integration>> {
    let rows = sqlx::query(sql!(
        "SELECT id, name, kind, url, secret, encrypted, config, created_at
         FROM integrations ORDER BY name"
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(row_to_integration).collect())
}

/// Fetch a single integration by ID.
pub async fn get_integration(pool: &DbPool, id: i64) -> Result<Option<Integration>> {
    let row = sqlx::query(sql!(
        "SELECT id, name, kind, url, secret, encrypted, config, created_at
         FROM integrations WHERE id = ?1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.as_ref().map(row_to_integration))
}

fn row_to_project_integration(row: &crate::db::DbRow) -> ProjectIntegration {
    ProjectIntegration {
        id: row.get(0),
        project_id: row.get(1),
        integration_id: row.get(2),
        integration_name: row.get(3),
        integration_kind: row.get(4),
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
    }
}

const PROJECT_INTEGRATION_SELECT: &str = "SELECT pi.id, pi.project_id, pi.integration_id,
            i.name, i.kind, i.url, i.secret, i.encrypted, i.config,
            pi.notify_new_issues, pi.notify_regressions,
            pi.min_level, pi.environment_filter, pi.config, pi.enabled,
            pi.notify_threshold, pi.notify_digests
     FROM project_integrations pi
     JOIN integrations i ON i.id = pi.integration_id";

/// All integrations linked to a project -- both active and inactive.
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
    Ok(rows.iter().map(row_to_project_integration).collect())
}

/// Only the enabled integrations for a project -- this is what the notification
/// dispatcher uses to figure out where to send alerts.
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
    Ok(rows.iter().map(row_to_project_integration).collect())
}

/// Integrations not yet linked to a project -- candidates for the "add" dropdown.
pub async fn list_available_for_project(
    pool: &DbPool,
    project_id: u64,
) -> Result<Vec<Integration>> {
    let rows = sqlx::query(sql!(
        "SELECT id, name, kind, url, secret, encrypted, config, created_at
         FROM integrations
         WHERE id NOT IN (
             SELECT integration_id FROM project_integrations WHERE project_id = ?1
         )
         ORDER BY name"
    ))
    .bind(project_id as i64)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(row_to_integration).collect())
}

// --- Write operations ---

/// Create a new integration. Returns its row ID.
pub async fn create_integration(
    pool: &DbPool,
    name: &str,
    kind: &str,
    url: &str,
    secret: Option<&str>,
    config: Option<&str>,
    encrypted: bool,
) -> Result<i64> {
    #[cfg(feature = "sqlite")]
    {
        let result = sqlx::query(sql!(
            "INSERT INTO integrations (name, kind, url, secret, encrypted, config)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
        ))
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
            "INSERT INTO integrations (name, kind, url, secret, encrypted, config)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING id"
        ))
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

/// Delete an integration. Returns 0 if it wasn't found.
pub async fn delete_integration(pool: &DbPool, id: i64) -> Result<u64> {
    let result = sqlx::query(sql!("DELETE FROM integrations WHERE id = ?1"))
        .bind(id)
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
         WHERE id = ?8"
    ))
    .bind(notify_new_issues)
    .bind(notify_regressions)
    .bind(min_level)
    .bind(environment_filter)
    .bind(config)
    .bind(notify_threshold)
    .bind(notify_digests)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Remove a project integration link. Returns 0 if it wasn't found.
pub async fn deactivate_project_integration(pool: &DbPool, id: i64) -> Result<u64> {
    let result = sqlx::query(sql!("DELETE FROM project_integrations WHERE id = ?1"))
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}
