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
        is_global: row.get::<bool, _>(8),
    })
}

/// All configured integrations (webhooks, Slack, email, etc.).
/// Pass `Some(org_id)` to scope to one org; `None` returns all (superuser only).
pub async fn list_integrations(pool: &DbPool, org_id: Option<i64>) -> Result<Vec<Integration>> {
    let rows = if let Some(oid) = org_id {
        sqlx::query(sql!(
            "SELECT id, name, kind, url, secret, encrypted, config, created_at, is_global
             FROM integrations WHERE org_id = ?1 ORDER BY name"
        ))
        .bind(oid)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(sql!(
            "SELECT id, name, kind, url, secret, encrypted, config, created_at, is_global
             FROM integrations ORDER BY name"
        ))
        .fetch_all(pool)
        .await?
    };
    rows.iter().map(row_to_integration).collect()
}

/// Fetch a single integration by ID.
/// Pass `Some(org_id)` to restrict to the caller's org (prevents cross-org reads).
pub async fn get_integration(
    pool: &DbPool,
    id: i64,
    org_id: Option<i64>,
) -> Result<Option<Integration>> {
    let row = if let Some(oid) = org_id {
        sqlx::query(sql!(
            "SELECT id, name, kind, url, secret, encrypted, config, created_at, is_global
             FROM integrations WHERE id = ?1 AND org_id = ?2"
        ))
        .bind(id)
        .bind(oid)
        .fetch_optional(pool)
        .await?
    } else {
        sqlx::query(sql!(
            "SELECT id, name, kind, url, secret, encrypted, config, created_at, is_global
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
        integration_is_global: row.get::<bool, _>(17),
    })
}

const PROJECT_INTEGRATION_SELECT: &str = "SELECT pi.id, pi.project_id, pi.integration_id,
            i.name, i.kind, i.url, i.secret, i.encrypted, i.config,
            pi.notify_new_issues, pi.notify_regressions,
            pi.min_level, pi.environment_filter, pi.config, pi.enabled,
            pi.notify_threshold, pi.notify_digests, i.is_global
     FROM project_integrations pi
     JOIN integrations i ON i.id = pi.integration_id";

/// All integrations linked to a project (active and inactive).
pub async fn list_project_integrations(
    pool: &DbPool,
    project_id: u64,
) -> Result<Vec<ProjectIntegration>> {
    let sql = format!("{PROJECT_INTEGRATION_SELECT} WHERE pi.project_id = ?1 ORDER BY i.name");
    let rows = sqlx::query(crate::db::dyn_sql(&sql))
        .bind(project_id as i64)
        .fetch_all(pool)
        .await?;
    rows.iter().map(row_to_project_integration).collect()
}

/// A project's own row for one integration, or `None` if it hasn't customised it.
pub async fn get_project_integration(
    pool: &DbPool,
    project_id: i64,
    integration_id: i64,
) -> Result<Option<ProjectIntegration>> {
    let sql =
        format!("{PROJECT_INTEGRATION_SELECT} WHERE pi.project_id = ?1 AND pi.integration_id = ?2");
    let row = sqlx::query(crate::db::dyn_sql(&sql))
        .bind(project_id)
        .bind(integration_id)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(row_to_project_integration).transpose()
}

/// Kinds a global integration can route org-wide - trackers resolve through `project_repos` instead.
const GLOBAL_CHANNEL_KINDS: &str = "'webhook', 'slack', 'email'";

/// Enabled integrations for a project, used by the notification dispatcher.
/// Global channels come back as synthetic rows with `id = 0`; nothing may write back through them.
pub async fn get_active_for_project(
    pool: &DbPool,
    project_id: u64,
) -> Result<Vec<ProjectIntegration>> {
    let sql = format!(
        "{PROJECT_INTEGRATION_SELECT}
         WHERE pi.project_id = ?1 AND pi.enabled = TRUE
           AND NOT EXISTS (
               SELECT 1 FROM integration_exclusions e
               WHERE e.integration_id = pi.integration_id AND e.project_id = ?1
           )
         UNION ALL
         SELECT CAST(0 AS BIGINT), CAST(?1 AS BIGINT), i.id,
                i.name, i.kind, i.url, i.secret, i.encrypted, i.config,
                TRUE, TRUE,
                CAST(NULL AS TEXT), CAST(NULL AS TEXT), CAST(NULL AS TEXT), TRUE,
                TRUE, TRUE, TRUE
         FROM integrations i
         JOIN projects p ON p.project_id = ?1 AND p.org_id = i.org_id
         WHERE i.is_global = TRUE
           AND i.kind IN ({GLOBAL_CHANNEL_KINDS})
           AND NOT EXISTS (
               SELECT 1 FROM project_integrations pi2
               WHERE pi2.project_id = ?1 AND pi2.integration_id = i.id
           )
           AND NOT EXISTS (
               SELECT 1 FROM integration_exclusions e
               WHERE e.integration_id = i.id AND e.project_id = ?1
           )
         ORDER BY 4"
    );
    let rows = sqlx::query(crate::db::dyn_sql(&sql))
        .bind(project_id as i64)
        .fetch_all(pool)
        .await?;
    rows.iter().map(row_to_project_integration).collect()
}

/// Enabled project integrations across every project in an org, for the alerts
/// hub's notification-types overview. Joins `projects` to scope by org.
pub async fn list_active_for_org(pool: &DbPool, org_id: i64) -> Result<Vec<ProjectIntegration>> {
    let sql = format!(
        "{PROJECT_INTEGRATION_SELECT}
         JOIN projects p ON p.project_id = pi.project_id
         WHERE p.org_id = ?1 AND pi.enabled = TRUE
         ORDER BY pi.project_id, i.name"
    );
    let rows = sqlx::query(crate::db::dyn_sql(&sql))
        .bind(org_id)
        .fetch_all(pool)
        .await?;
    rows.iter().map(row_to_project_integration).collect()
}

/// One project's routing state for a single integration.
pub struct ProjectRouting {
    pub project_id: i64,
    pub name: Option<String>,
    pub archived: bool,
    /// The project has its own `project_integrations` row.
    pub customised: bool,
    /// That row's `enabled` flag. True when there is no row.
    pub enabled: bool,
    pub excluded: bool,
}

/// Every project in the org with its state for one integration, rows or not.
pub async fn project_routing(
    pool: &DbPool,
    org_id: i64,
    integration_id: i64,
) -> Result<Vec<ProjectRouting>> {
    let rows = sqlx::query(sql!(
        "SELECT p.project_id, p.name, p.status, pi.enabled, e.id AS excluded_id
         FROM projects p
         LEFT JOIN project_integrations pi
           ON pi.project_id = p.project_id AND pi.integration_id = ?2
         LEFT JOIN integration_exclusions e
           ON e.integration_id = ?2 AND e.project_id = p.project_id
         WHERE p.org_id = ?1
         ORDER BY p.project_id"
    ))
    .bind(org_id)
    .bind(integration_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| {
            let enabled: Option<bool> = r.get("enabled");
            ProjectRouting {
                project_id: r.get("project_id"),
                name: r.get("name"),
                archived: r.get::<String, _>("status") == "archived",
                customised: enabled.is_some(),
                enabled: enabled.unwrap_or(true),
                excluded: r.get::<Option<i64>, _>("excluded_id").is_some(),
            }
        })
        .collect())
}

/// Integrations not yet linked to a project (candidates for the "add" dropdown).
/// Scoped to `org_id` so only same-org integrations are offered.
pub async fn list_available_for_project(
    pool: &DbPool,
    project_id: u64,
    org_id: i64,
) -> Result<Vec<Integration>> {
    let rows = sqlx::query(sql!(
        "SELECT id, name, kind, url, secret, encrypted, config, created_at, is_global
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
#[allow(clippy::too_many_arguments)]
pub async fn create_integration(
    pool: &DbPool,
    org_id: i64,
    name: &str,
    kind: &str,
    url: Option<&str>,
    secret: Option<&str>,
    config: Option<&str>,
    encrypted: bool,
    is_global: bool,
) -> Result<i64> {
    #[cfg(feature = "sqlite")]
    {
        let result = sqlx::query(sql!(
            "INSERT INTO integrations (org_id, name, kind, url, secret, encrypted, config, is_global)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
        ))
        .bind(org_id)
        .bind(name)
        .bind(kind)
        .bind(url)
        .bind(secret)
        .bind(encrypted)
        .bind(config)
        .bind(is_global)
        .execute(pool)
        .await?;
        Ok(result.last_insert_rowid())
    }
    #[cfg(not(feature = "sqlite"))]
    {
        let row = sqlx::query(sql!(
            "INSERT INTO integrations (org_id, name, kind, url, secret, encrypted, config, is_global)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) RETURNING id"
        ))
        .bind(org_id)
        .bind(name)
        .bind(kind)
        .bind(url)
        .bind(secret)
        .bind(encrypted)
        .bind(config)
        .bind(is_global)
        .fetch_one(pool)
        .await?;
        Ok(row.get::<i64, _>("id"))
    }
}

/// Flip an integration's global flag. Returns 0 if not found or wrong org.
pub async fn set_global(pool: &DbPool, id: i64, org_id: i64, is_global: bool) -> Result<u64> {
    let result = sqlx::query(sql!(
        "UPDATE integrations SET is_global = ?3 WHERE id = ?1 AND org_id = ?2"
    ))
    .bind(id)
    .bind(org_id)
    .bind(is_global)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Delete an integration in the given org. Returns 0 if not found or wrong org.
pub async fn delete_integration(pool: &DbPool, id: i64, org_id: i64) -> Result<u64> {
    // Filed links outlive the integration, so there's no cascade: clear the reference here.
    sqlx::query(sql!(
        "UPDATE issue_external_links SET integration_id = NULL \
         WHERE integration_id IN (SELECT id FROM integrations WHERE id = ?1 AND org_id = ?2)"
    ))
    .bind(id)
    .bind(org_id)
    .execute(pool)
    .await?;

    let result = sqlx::query(sql!(
        "DELETE FROM integrations WHERE id = ?1 AND org_id = ?2"
    ))
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

/// Update only the new-issue / regression toggles on a project integration,
/// leaving level/environment/recipient and the other notify flags untouched.
/// Backs the alerts-hub notification-types section, which surfaces just these
/// two columns.
pub async fn update_project_integration_notify_types(
    pool: &DbPool,
    project_id: i64,
    id: i64,
    notify_new_issues: bool,
    notify_regressions: bool,
) -> Result<u64> {
    let result = sqlx::query(sql!(
        "UPDATE project_integrations SET
             notify_new_issues = ?1, notify_regressions = ?2
         WHERE id = ?3 AND project_id = ?4"
    ))
    .bind(notify_new_issues)
    .bind(notify_regressions)
    .bind(id)
    .bind(project_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Remove a project integration link. Returns 0 if it wasn't found.
pub async fn deactivate_project_integration(
    pool: &DbPool,
    project_id: i64,
    id: i64,
) -> Result<u64> {
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

    // Org 1 is seeded by migrations; any other org must exist to satisfy the FK.
    async fn ensure_org(pool: &DbPool, org_id: i64) {
        sqlx::query(sql!(
            "INSERT INTO organizations (org_id, slug, name) VALUES (?1, ?2, ?2)
             ON CONFLICT(org_id) DO NOTHING"
        ))
        .bind(org_id)
        .bind(format!("org-{org_id}"))
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_project_integration(pool: &DbPool, project_id: u64) -> i64 {
        create_integration(
            pool,
            1,
            "test-intg",
            "webhook",
            Some("https://example.com"),
            None,
            None,
            false,
            false,
        )
        .await
        .unwrap();
        let integration_id: i64 =
            sqlx::query(sql!("SELECT id FROM integrations WHERE name = 'test-intg'"))
                .fetch_one(pool)
                .await
                .unwrap()
                .get(0);
        activate_project_integration(
            pool,
            project_id,
            integration_id,
            false,
            false,
            None,
            None,
            None,
            false,
            false,
        )
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

    async fn ensure_project(pool: &DbPool, project_id: i64, org_id: i64) {
        ensure_org(pool, org_id).await;
        sqlx::query(sql!(
            "INSERT INTO projects (project_id, name, org_id) VALUES (?1, ?2, ?3)
             ON CONFLICT(project_id) DO UPDATE SET org_id = excluded.org_id"
        ))
        .bind(project_id)
        .bind(format!("project-{project_id}"))
        .bind(org_id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn global_integration(pool: &DbPool, org_id: i64, name: &str, kind: &str) -> i64 {
        ensure_org(pool, org_id).await;
        create_integration(
            pool,
            org_id,
            name,
            kind,
            Some("https://hooks.example/x"),
            None,
            None,
            false,
            true,
        )
        .await
        .unwrap()
    }

    /// A kind missing from the literal would silently never route.
    #[test]
    fn global_channel_kinds_literal_matches_the_enum() {
        for kind in IntegrationKind::ALL {
            let listed = GLOBAL_CHANNEL_KINDS.contains(&format!("'{}'", kind.as_str()));
            assert_eq!(
                listed,
                !kind.is_tracker(),
                "{} is {}listed in GLOBAL_CHANNEL_KINDS",
                kind.as_str(),
                if listed { "" } else { "not " }
            );
        }
    }

    #[tokio::test]
    async fn global_channel_delivers_under_defaults_without_an_explicit_row() {
        let pool = open_test_db().await;
        ensure_project(&pool, 101, 5).await;
        let global_id = global_integration(&pool, 5, "org-wide-slack", "slack").await;
        // Same org, not global: stays invisible until someone activates it.
        create_integration(
            &pool,
            5,
            "opt-in-slack",
            "slack",
            Some("https://hooks.example/y"),
            None,
            None,
            false,
            false,
        )
        .await
        .unwrap();

        let active = get_active_for_project(&pool, 101).await.unwrap();
        assert_eq!(active.len(), 1, "only the global integration routes");
        let pi = &active[0];
        assert_eq!(pi.integration_id, global_id);
        assert_eq!(pi.integration_name, "org-wide-slack");
        assert_eq!(pi.project_id, 101);
        assert_eq!(pi.id, 0, "a synthetic row must not carry a writable id");
        assert!(pi.enabled);
        assert!(pi.notify_new_issues);
        assert!(pi.notify_regressions);
        assert!(pi.notify_threshold);
        assert!(pi.notify_digests);
        assert!(pi.min_level.is_none());
        assert!(pi.environment_filter.is_none());
        assert!(pi.config.is_none());
    }

    #[tokio::test]
    async fn explicit_row_wins_over_global_and_is_not_double_delivered() {
        let pool = open_test_db().await;
        ensure_project(&pool, 102, 5).await;
        let global_id = global_integration(&pool, 5, "customised-slack", "slack").await;
        activate_project_integration(
            &pool,
            102,
            global_id,
            false,
            true,
            Some("error"),
            None,
            None,
            false,
            false,
        )
        .await
        .unwrap();

        let active = get_active_for_project(&pool, 102).await.unwrap();
        assert_eq!(active.len(), 1, "the global fallback must not duplicate it");
        let pi = &active[0];
        assert_ne!(pi.id, 0, "the explicit row's own id is what surfaces");
        assert!(!pi.notify_new_issues, "the project's customisation wins");
        assert_eq!(pi.min_level.as_deref(), Some("error"));
    }

    #[tokio::test]
    async fn disabled_explicit_row_suppresses_the_global_fallback() {
        let pool = open_test_db().await;
        ensure_project(&pool, 103, 5).await;
        let global_id = global_integration(&pool, 5, "silenced-slack", "slack").await;
        activate_project_integration(
            &pool, 103, global_id, true, true, None, None, None, true, true,
        )
        .await
        .unwrap();
        sqlx::query(sql!(
            "UPDATE project_integrations SET enabled = FALSE
             WHERE project_id = ?1 AND integration_id = ?2"
        ))
        .bind(103i64)
        .bind(global_id)
        .execute(&pool)
        .await
        .unwrap();

        let active = get_active_for_project(&pool, 103).await.unwrap();
        assert!(
            active.is_empty(),
            "switching a customised row off must silence it, not resurrect global defaults"
        );
    }

    #[tokio::test]
    async fn exclusion_suppresses_a_global_integration() {
        let pool = open_test_db().await;
        ensure_project(&pool, 104, 5).await;
        let global_id = global_integration(&pool, 5, "excluded-slack", "slack").await;
        assert_eq!(get_active_for_project(&pool, 104).await.unwrap().len(), 1);

        crate::queries::integration_exclusions::exclude(&pool, 5, global_id, 104)
            .await
            .unwrap();
        assert!(get_active_for_project(&pool, 104).await.unwrap().is_empty());

        crate::queries::integration_exclusions::un_exclude(&pool, 5, global_id, 104)
            .await
            .unwrap();
        assert_eq!(
            get_active_for_project(&pool, 104).await.unwrap().len(),
            1,
            "un-excluding resumes delivery"
        );
    }

    #[tokio::test]
    async fn exclusion_suppresses_a_project_that_customised_the_integration() {
        let pool = open_test_db().await;
        ensure_project(&pool, 108, 5).await;
        let global_id = global_integration(&pool, 5, "excluded-custom", "slack").await;
        activate_project_integration(
            &pool, 108, global_id, true, true, None, None, None, false, false,
        )
        .await
        .unwrap();
        assert_eq!(
            get_active_for_project(&pool, 108).await.unwrap().len(),
            1,
            "the customised row delivers before the exclusion"
        );

        crate::queries::integration_exclusions::exclude(&pool, 5, global_id, 108)
            .await
            .unwrap();
        assert!(
            get_active_for_project(&pool, 108).await.unwrap().is_empty(),
            "an exclusion has to beat the project's own row, not just the fallback"
        );

        crate::queries::integration_exclusions::un_exclude(&pool, 5, global_id, 108)
            .await
            .unwrap();
        assert_eq!(
            get_active_for_project(&pool, 108).await.unwrap().len(),
            1,
            "and lifting it brings the customised row back"
        );
    }

    #[tokio::test]
    async fn global_integration_does_not_cross_org_boundaries() {
        let pool = open_test_db().await;
        ensure_project(&pool, 105, 5).await;
        ensure_project(&pool, 106, 6).await;
        global_integration(&pool, 5, "org5-only", "webhook").await;

        assert_eq!(get_active_for_project(&pool, 105).await.unwrap().len(), 1);
        assert!(
            get_active_for_project(&pool, 106).await.unwrap().is_empty(),
            "org 6's project must never see org 5's global integration"
        );
    }

    #[tokio::test]
    async fn tracker_kinds_ignore_the_global_flag() {
        let pool = open_test_db().await;
        ensure_project(&pool, 107, 5).await;
        global_integration(&pool, 5, "org-wide-github", "github").await;
        assert!(
            get_active_for_project(&pool, 107).await.unwrap().is_empty(),
            "trackers resolve through project_repos, not through routing"
        );
    }

    /// Covers a project with no `projects` row, which the global branch's org join drops.
    #[tokio::test]
    async fn without_any_global_integration_results_match_the_explicit_rows() {
        let pool = open_test_db().await;
        let pi_id = seed_project_integration(&pool, 1).await;

        let active = get_active_for_project(&pool, 1).await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, pi_id);
        assert_eq!(active[0].integration_name, "test-intg");
        assert!(
            get_active_for_project(&pool, 2).await.unwrap().is_empty(),
            "an unrelated project still resolves to nothing"
        );
    }

    #[tokio::test]
    async fn update_project_integration_cross_project_affects_zero_rows() {
        let pool = open_test_db().await;
        let pi_id = seed_project_integration(&pool, 1).await;
        let rows = update_project_integration(
            &pool, 2, pi_id, false, false, None, None, None, false, false,
        )
        .await
        .unwrap();
        assert_eq!(rows, 0, "cross-project update must affect 0 rows");
        let rows = update_project_integration(
            &pool, 1, pi_id, true, false, None, None, None, false, false,
        )
        .await
        .unwrap();
        assert_eq!(rows, 1);
    }

    #[tokio::test]
    async fn update_notify_types_flips_flags_and_preserves_others() {
        let pool = open_test_db().await;
        let pi_id = seed_project_integration(&pool, 1).await;
        // Seed a min_level so we can confirm the narrow update leaves it alone.
        update_project_integration(
            &pool,
            1,
            pi_id,
            false,
            false,
            Some("error"),
            None,
            None,
            true,
            true,
        )
        .await
        .unwrap();

        let rows = update_project_integration_notify_types(&pool, 1, pi_id, true, false)
            .await
            .unwrap();
        assert_eq!(rows, 1);

        let pi = list_project_integrations(&pool, 1)
            .await
            .unwrap()
            .into_iter()
            .find(|p| p.id == pi_id)
            .unwrap();
        assert!(pi.notify_new_issues, "new-issue toggle persisted");
        assert!(!pi.notify_regressions, "regression toggle persisted");
        // Untouched columns keep their prior values.
        assert_eq!(pi.min_level.as_deref(), Some("error"));
        assert!(pi.notify_threshold);
        assert!(pi.notify_digests);

        // Cross-project update must not match.
        let rows = update_project_integration_notify_types(&pool, 2, pi_id, false, false)
            .await
            .unwrap();
        assert_eq!(rows, 0, "cross-project update must affect 0 rows");
    }

    #[tokio::test]
    async fn deactivate_project_integration_cross_project_affects_zero_rows() {
        let pool = open_test_db().await;
        let pi_id = seed_project_integration(&pool, 1).await;
        let rows = deactivate_project_integration(&pool, 2, pi_id)
            .await
            .unwrap();
        assert_eq!(rows, 0, "cross-project delete must affect 0 rows");
        let rows = deactivate_project_integration(&pool, 1, pi_id)
            .await
            .unwrap();
        assert_eq!(rows, 1);
    }

    #[tokio::test]
    async fn list_integrations_scoped_excludes_other_org() {
        let pool = open_test_db().await;
        ensure_org(&pool, 2).await;
        create_integration(
            &pool,
            1,
            "intg-org1",
            "webhook",
            Some("https://a.example"),
            None,
            None,
            false,
            false,
        )
        .await
        .unwrap();
        create_integration(
            &pool,
            2,
            "intg-org2",
            "webhook",
            Some("https://b.example"),
            None,
            None,
            false,
            false,
        )
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
        let id = create_integration(
            &pool,
            1,
            "cross-get",
            "webhook",
            Some("https://x.example"),
            None,
            None,
            false,
            false,
        )
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
        let id = create_integration(
            &pool,
            1,
            "cross-del",
            "webhook",
            Some("https://y.example"),
            None,
            None,
            false,
            false,
        )
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
        ensure_org(&pool, 2).await;
        create_integration(
            &pool,
            1,
            "org1-intg",
            "webhook",
            Some("https://a.example"),
            None,
            None,
            false,
            false,
        )
        .await
        .unwrap();
        create_integration(
            &pool,
            2,
            "org2-intg",
            "webhook",
            Some("https://b.example"),
            None,
            None,
            false,
            false,
        )
        .await
        .unwrap();
        // project 99 belongs to org 1 (no links yet)
        let available = list_available_for_project(&pool, 99, 1).await.unwrap();
        assert!(
            available.iter().any(|i| i.name == "org1-intg"),
            "org1 integration must be offered"
        );
        assert!(
            !available.iter().any(|i| i.name == "org2-intg"),
            "org2 integration must be excluded"
        );
    }

    #[tokio::test]
    async fn activate_cross_org_integration_guard_rejects() {
        let pool = open_test_db().await;
        ensure_org(&pool, 2).await;
        // Integration belongs to org 2; project owner is in org 1.
        let foreign_id = create_integration(
            &pool,
            2,
            "foreign-intg",
            "webhook",
            Some("https://c.example"),
            None,
            None,
            false,
            false,
        )
        .await
        .unwrap();
        // The activate handler guards by calling get_integration with the project's org_id.
        // Confirm it returns None for the wrong org so the handler correctly rejects.
        assert!(
            get_integration(&pool, foreign_id, Some(1))
                .await
                .unwrap()
                .is_none(),
            "cross-org integration must not be visible to org 1, so activation is rejected"
        );
    }
}
