use anyhow::Result;
use sqlx::Row;

use crate::db::sql;

use crate::domain::ProjectStatus;

use super::types::{ProjectKey, ProjectNavCounts, ProjectRepo, ProjectSummary};

// --- Read queries ---

/// List projects visible in the given org, with event/issue counts.
/// Optionally narrow by name/id search and a `since` timestamp.
pub async fn list_projects(
    pool: &crate::db::DbPool,
    org_id: i64,
    sort: Option<&str>,
    query: Option<&str>,
    since: Option<i64>,
) -> Result<Vec<ProjectSummary>> {
    list_projects_inner(pool, Some(org_id), sort, query, since).await
}

/// List all projects across every org (CLI / superuser context).
pub async fn list_all_projects(
    pool: &crate::db::DbPool,
    sort: Option<&str>,
    query: Option<&str>,
    since: Option<i64>,
) -> Result<Vec<ProjectSummary>> {
    list_projects_inner(pool, None, sort, query, since).await
}

async fn list_projects_inner(
    pool: &crate::db::DbPool,
    org_id: Option<i64>,
    sort: Option<&str>,
    query: Option<&str>,
    since: Option<i64>,
) -> Result<Vec<ProjectSummary>> {
    // Safety: order_expr is always a hardcoded literal from this match, never user input.
    let order_expr = match sort {
        Some("issues") => "issue_count",
        Some("events") => "e.event_count",
        Some("first_seen") => "fs.first_seen",
        Some("project_id") => "e.project_id",
        _ => "e.last_seen",
    };

    // When org_id is given, promote to INNER JOIN with org filter as ?1.
    // The time filter shifts to ?2 so its bind slot doesn't collide with org_id.
    let (project_join, time_param) = if org_id.is_some() {
        (
            "JOIN projects p ON e.project_id = p.project_id AND p.org_id = ?1",
            "?2",
        )
    } else {
        ("LEFT JOIN projects p ON e.project_id = p.project_id", "?1")
    };

    let time_filter = if since.is_some() {
        format!("WHERE timestamp >= {time_param}")
    } else {
        String::new()
    };

    #[cfg(feature = "sqlite")]
    let platform_agg = "GROUP_CONCAT(DISTINCT platform)";
    #[cfg(not(feature = "sqlite"))]
    let platform_agg = "STRING_AGG(DISTINCT platform, ',')";

    let sql = format!(
        "SELECT
            e.project_id,
            e.event_count,
            COALESCE(i.issue_count, 0) AS issue_count,
            fs.first_seen,
            e.last_seen,
            e.platforms,
            lr.version AS latest_release,
            e.error_count,
            e.transaction_count,
            e.session_count,
            e.other_count,
            p.name
         FROM (
            SELECT
                project_id,
                COUNT(*) AS event_count,
                SUM(CASE WHEN item_type = 'event' THEN 1 ELSE 0 END) AS error_count,
                SUM(CASE WHEN item_type = 'transaction' THEN 1 ELSE 0 END) AS transaction_count,
                SUM(CASE WHEN item_type IN ('session', 'sessions') THEN 1 ELSE 0 END) AS session_count,
                SUM(CASE WHEN item_type NOT IN ('event', 'transaction', 'session', 'sessions') THEN 1 ELSE 0 END) AS other_count,
                MAX(timestamp) AS last_seen,
                {platform_agg} AS platforms
            FROM events
            {time_filter}
            GROUP BY project_id
         ) e
         LEFT JOIN (
            SELECT project_id, MIN(timestamp) AS first_seen
            FROM events
            GROUP BY project_id
         ) fs ON e.project_id = fs.project_id
         LEFT JOIN (
            SELECT project_id, COUNT(*) AS issue_count
            FROM issues
            GROUP BY project_id
         ) i ON e.project_id = i.project_id
         LEFT JOIN (
            SELECT project_id, version
            FROM releases
            WHERE id IN (
                SELECT MAX(id) FROM releases GROUP BY project_id
            )
         ) lr ON e.project_id = lr.project_id
         {project_join}
         ORDER BY {order_expr} DESC"
    );

    let sql = crate::db::translate_sql(&sql);
    let rows = match (org_id, since) {
        (Some(oid), Some(ts)) => sqlx::query(&sql).bind(oid).bind(ts).fetch_all(pool).await?,
        (Some(oid), None) => sqlx::query(&sql).bind(oid).fetch_all(pool).await?,
        (None, Some(ts)) => sqlx::query(&sql).bind(ts).fetch_all(pool).await?,
        (None, None) => sqlx::query(&sql).fetch_all(pool).await?,
    };

    let mut projects: Vec<ProjectSummary> = rows.iter().map(map_project_row).collect();

    // Filter client-side by name/id; simpler than more dynamic SQL.
    if let Some(q) = query {
        if !q.is_empty() {
            let q_lower = q.to_lowercase();
            projects.retain(|p| {
                p.project_id.to_string().contains(&q_lower)
                    || p.name
                        .as_ref()
                        .map(|n| n.to_lowercase().contains(&q_lower))
                        .unwrap_or(false)
            });
        }
    }

    Ok(projects)
}

fn map_project_row(row: &crate::db::DbRow) -> ProjectSummary {
    let platforms: Option<String> = row.get("platforms");
    ProjectSummary {
        project_id: row.get::<i64, _>("project_id") as u64,
        name: row.get("name"),
        event_count: row.get::<i64, _>("event_count") as u64,
        issue_count: row.get::<i64, _>("issue_count") as u64,
        first_seen: row.get("first_seen"),
        last_seen: row.get("last_seen"),
        platforms: platforms.unwrap_or_default(),
        latest_release: row.get("latest_release"),
        error_count: row.get::<i64, _>("error_count") as u64,
        transaction_count: row.get::<i64, _>("transaction_count") as u64,
        session_count: row.get::<i64, _>("session_count") as u64,
        other_count: row.get::<i64, _>("other_count") as u64,
    }
}

/// Grab project metadata (name, status, source) in a single query.
pub async fn get_project_info(
    pool: &crate::db::DbPool,
    project_id: u64,
) -> Result<Option<super::types::ProjectInfo>> {
    let row = sqlx::query(sql!(
        "SELECT name, status, source FROM projects WHERE project_id = ?1"
    ))
    .bind(project_id as i64)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| {
        let status_str: Option<String> = row.get("status");
        super::types::ProjectInfo {
            name: row.get("name"),
            // Unknown status strings default rather than panicking the handler.
            status: status_str
                .and_then(|s| s.parse().ok())
                .unwrap_or(ProjectStatus::Active),
            source: row.get("source"),
        }
    }))
}

/// Set or clear a project's display name.
pub async fn set_project_name(pool: &crate::db::DbPool, project_id: u64, name: &str) -> Result<()> {
    let name_val: Option<&str> = if name.is_empty() { None } else { Some(name) };
    sqlx::query(sql!(
        "INSERT INTO projects (project_id, name) VALUES (?1, ?2)
         ON CONFLICT(project_id) DO UPDATE SET name = excluded.name"
    ))
    .bind(project_id as i64)
    .bind(name_val)
    .execute(pool)
    .await?;
    Ok(())
}

/// All repos linked to a project.
pub async fn get_project_repos(
    pool: &crate::db::DbPool,
    project_id: u64,
) -> Result<Vec<ProjectRepo>> {
    let rows = sqlx::query(sql!(
        "SELECT id, project_id, repo_url, forge_type, url_template
         FROM project_repos WHERE project_id = ?1 ORDER BY id"
    ))
    .bind(project_id as i64)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| ProjectRepo {
            id: row.get("id"),
            project_id: row.get::<i64, _>("project_id") as u64,
            repo_url: row.get("repo_url"),
            forge_type: row.get("forge_type"),
            url_template: row.get("url_template"),
        })
        .collect())
}

/// Load nav badge counts for a project in one shot. Scans the events table
/// once with conditional aggregation, plus a count each for logs/spans/metrics.
pub async fn get_nav_counts(pool: &crate::db::DbPool, project_id: u64) -> ProjectNavCounts {
    // Transactions live in transaction_metrics; everything else comes from events.
    let transaction_count = count_transactions(pool, project_id).await.unwrap_or(0);
    let label = project_label(pool, project_id).await;

    let result = sqlx::query(sql!(
        "SELECT
            COALESCE(SUM(CASE WHEN monitor_slug IS NOT NULL THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN item_type IN ('session', 'sessions') AND session_status IS NOT NULL THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN item_type = 'user_report' THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN item_type = 'client_report' THEN 1 ELSE 0 END), 0),
            (SELECT COUNT(*) FROM logs WHERE project_id = ?1),
            (SELECT COUNT(*) FROM spans WHERE project_id = ?1),
            (SELECT COUNT(*) FROM metrics WHERE project_id = ?1),
            COALESCE(SUM(CASE WHEN item_type IN ('profile', 'profile_chunk') THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN item_type = 'replay_event' THEN 1 ELSE 0 END), 0)
         FROM events
         WHERE project_id = ?1"
    ))
    .bind(project_id as i64)
    .fetch_optional(pool)
    .await;

    match result {
        Ok(Some(row)) => ProjectNavCounts {
            transaction_count,
            monitor_count: row.get::<i64, _>(0) as u64,
            session_count: row.get::<i64, _>(1) as u64,
            user_report_count: row.get::<i64, _>(2) as u64,
            client_report_count: row.get::<i64, _>(3) as u64,
            log_count: row.get::<i64, _>(4) as u64,
            span_count: row.get::<i64, _>(5) as u64,
            metric_count: row.get::<i64, _>(6) as u64,
            profile_count: row.get::<i64, _>(7) as u64,
            replay_count: row.get::<i64, _>(8) as u64,
            label,
        },
        _ => ProjectNavCounts {
            transaction_count,
            label,
            ..Default::default()
        },
    }
}

/// Resolve the display label for a project: stored `name` if set, else
/// `Project {id}`. Never errors; falls back to the id-based label on any
/// DB failure so the heading still renders.
pub async fn project_label(pool: &crate::db::DbPool, project_id: u64) -> String {
    let stored = sqlx::query(sql!("SELECT name FROM projects WHERE project_id = ?1"))
        .bind(project_id as i64)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .and_then(|row| row.get::<Option<String>, _>(0))
        .filter(|n| !n.trim().is_empty());
    stored.unwrap_or_else(|| format!("Project {}", project_id))
}

/// Count distinct transaction names for a project's nav badge.
pub async fn count_transactions(pool: &crate::db::DbPool, project_id: u64) -> Result<u64> {
    let row = sqlx::query(sql!(
        "SELECT COUNT(DISTINCT transaction_name) FROM transaction_metrics WHERE project_id = ?1"
    ))
    .bind(project_id as i64)
    .fetch_one(pool)
    .await?;
    Ok(row.get::<i64, _>(0) as u64)
}

pub async fn count_distinct_projects(pool: &crate::db::DbPool) -> Result<usize> {
    let row = sqlx::query(sql!("SELECT COUNT(DISTINCT project_id) FROM project_keys"))
        .fetch_one(pool)
        .await?;
    Ok(row.get::<i64, _>(0) as usize)
}

/// Look up a project key by its public key string.
pub async fn get_project_key(
    pool: &crate::db::DbPool,
    public_key: &str,
) -> Result<Option<ProjectKey>> {
    let row = sqlx::query(sql!(
        "SELECT public_key, project_id, status, label, created_at
         FROM project_keys WHERE public_key = ?1"
    ))
    .bind(public_key)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| {
        let status_str: String = row.get("status");
        ProjectKey {
            public_key: row.get("public_key"),
            project_id: row.get::<i64, _>("project_id") as u64,
            status: status_str.parse().unwrap_or_default(),
            label: row.get("label"),
            created_at: row.get("created_at"),
        }
    }))
}

/// All keys for a project, ordered by creation time.
pub async fn list_project_keys(
    pool: &crate::db::DbPool,
    project_id: u64,
) -> Result<Vec<ProjectKey>> {
    let rows = sqlx::query(sql!(
        "SELECT public_key, project_id, status, label, created_at
         FROM project_keys WHERE project_id = ?1 ORDER BY created_at"
    ))
    .bind(project_id as i64)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| {
            let status_str: String = row.get("status");
            ProjectKey {
                public_key: row.get("public_key"),
                project_id: row.get::<i64, _>("project_id") as u64,
                status: status_str.parse().unwrap_or_default(),
                label: row.get("label"),
                created_at: row.get("created_at"),
            }
        })
        .collect())
}

/// Check whether a project is active or archived.
pub async fn get_project_status(
    pool: &crate::db::DbPool,
    project_id: u64,
) -> Result<Option<ProjectStatus>> {
    let row = sqlx::query(sql!("SELECT status FROM projects WHERE project_id = ?1"))
        .bind(project_id as i64)
        .fetch_optional(pool)
        .await?;

    Ok(row.map(|row| {
        let s: String = row.get("status");
        // Match `list_projects` / `get_project_key`: an unrecognised value
        // (manual DB edit, mid-rollout migration) defaults rather than panics.
        s.parse().unwrap_or_default()
    }))
}

// --- Write operations ---

/// Create a new project with its first key. Returns (project_id, public_key).
pub async fn create_project(
    pool: &crate::db::DbPool,
    org_id: i64,
    name: &str,
    platform: Option<&str>,
) -> Result<(u64, String)> {
    let mut tx = pool.begin().await?;

    let row = sqlx::query(sql!(
        "SELECT MAX(id) FROM (
            SELECT MAX(project_id) AS id FROM projects
            UNION ALL
            SELECT MAX(project_id) AS id FROM events
         ) AS t"
    ))
    .fetch_one(&mut *tx)
    .await?;
    let max: Option<i64> = row.get(0);
    let project_id = max.unwrap_or(0) as u64 + 1;

    let public_key = crate::util::crypto::random_hex::<16>();
    let name_val: Option<&str> = if name.is_empty() { None } else { Some(name) };
    sqlx::query(sql!(
        "INSERT INTO projects (project_id, name, status, source, org_id) VALUES (?1, ?2, 'active', 'manual', ?3)"
    ))
    .bind(project_id as i64)
    .bind(name_val)
    .bind(org_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(sql!(
        "INSERT INTO project_keys (public_key, project_id, status, label) VALUES (?1, ?2, 'active', ?3)"
    ))
    .bind(&public_key)
    .bind(project_id as i64)
    .bind(platform)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok((project_id, public_key))
}

/// Archive a project. Returns 0 if it doesn't exist.
pub async fn archive_project(pool: &crate::db::DbPool, project_id: u64) -> Result<u64> {
    let result = sqlx::query(sql!(
        "UPDATE projects SET status = 'archived' WHERE project_id = ?1"
    ))
    .bind(project_id as i64)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Bring a project back from archived. Returns 0 if it doesn't exist.
pub async fn unarchive_project(pool: &crate::db::DbPool, project_id: u64) -> Result<u64> {
    let result = sqlx::query(sql!(
        "UPDATE projects SET status = 'active' WHERE project_id = ?1"
    ))
    .bind(project_id as i64)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Make sure a project and its key exist -- auto-provisions on first event.
pub async fn ensure_project_key(
    pool: &crate::db::DbPool,
    project_id: u64,
    public_key: &str,
) -> Result<()> {
    #[cfg(feature = "sqlite")]
    sqlx::query(sql!(
        "INSERT OR IGNORE INTO projects (project_id, status, source) VALUES (?1, 'active', 'auto')"
    ))
    .bind(project_id as i64)
    .execute(pool)
    .await?;
    #[cfg(not(feature = "sqlite"))]
    sqlx::query(sql!(
        "INSERT INTO projects (project_id, status, source) VALUES (?1, 'active', 'auto') ON CONFLICT (project_id) DO NOTHING"
    ))
    .bind(project_id as i64)
    .execute(pool)
    .await?;

    #[cfg(feature = "sqlite")]
    sqlx::query(sql!(
        "INSERT OR IGNORE INTO project_keys (public_key, project_id, status) VALUES (?1, ?2, 'active')"
    ))
    .bind(public_key)
    .bind(project_id as i64)
    .execute(pool)
    .await?;
    #[cfg(not(feature = "sqlite"))]
    sqlx::query(sql!(
        "INSERT INTO project_keys (public_key, project_id, status) VALUES (?1, ?2, 'active') ON CONFLICT (public_key) DO NOTHING"
    ))
    .bind(public_key)
    .bind(project_id as i64)
    .execute(pool)
    .await?;
    Ok(())
}

/// Generate a new key for a project. Returns the public_key hex string.
pub async fn create_project_key(
    pool: &crate::db::DbPool,
    project_id: u64,
    label: Option<&str>,
) -> Result<String> {
    let public_key = crate::util::crypto::random_hex::<16>();
    sqlx::query(sql!(
        "INSERT INTO project_keys (public_key, project_id, status, label) VALUES (?1, ?2, 'active', ?3)"
    ))
    .bind(&public_key)
    .bind(project_id as i64)
    .bind(label)
    .execute(pool)
    .await?;
    Ok(public_key)
}

/// Delete a project key scoped to the given project. Returns 0 if not found.
pub async fn delete_project_key(
    pool: &crate::db::DbPool,
    project_id: u64,
    public_key: &str,
) -> Result<u64> {
    let result = sqlx::query(sql!(
        "DELETE FROM project_keys WHERE public_key = ?1 AND project_id = ?2"
    ))
    .bind(public_key)
    .bind(project_id as i64)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Link a repo to a project (or update its settings if already linked).
pub async fn upsert_project_repo(
    pool: &crate::db::DbPool,
    project_id: u64,
    repo_url: &str,
    forge_type: &str,
    url_template: Option<&str>,
) -> Result<()> {
    sqlx::query(sql!(
        "INSERT INTO project_repos (project_id, repo_url, forge_type, url_template)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(project_id, repo_url) DO UPDATE SET
             forge_type = excluded.forge_type,
             url_template = excluded.url_template"
    ))
    .bind(project_id as i64)
    .bind(repo_url)
    .bind(forge_type)
    .bind(url_template)
    .execute(pool)
    .await?;
    Ok(())
}

/// Unlink a repo from a project. Returns 0 if it wasn't found.
pub async fn delete_project_repo(
    pool: &crate::db::DbPool,
    project_id: u64,
    repo_id: i64,
) -> Result<u64> {
    let result = sqlx::query(sql!(
        "DELETE FROM project_repos WHERE id = ?1 AND project_id = ?2"
    ))
    .bind(repo_id)
    .bind(project_id as i64)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub struct UnassignedProject {
    pub project_id: i64,
    pub name: Option<String>,
    pub source: Option<String>,
}

/// Projects still in org_id=1 (system/unassigned); shown in the superuser triage view.
pub async fn list_unassigned_projects(pool: &crate::db::DbPool) -> Result<Vec<UnassignedProject>> {
    let rows = sqlx::query(sql!(
        "SELECT project_id, name, source FROM projects WHERE org_id = ?1 ORDER BY project_id"
    ))
    .bind(crate::orgs::SYSTEM_ORG_ID)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| UnassignedProject {
            project_id: r.get("project_id"),
            name: r.get("name"),
            source: r.get("source"),
        })
        .collect())
}

/// Move a project from its current org into `org_id`. Returns rows affected.
pub async fn reassign_project(pool: &crate::db::DbPool, project_id: i64, org_id: i64) -> Result<u64> {
    let result = sqlx::query(sql!(
        "UPDATE projects SET org_id = ?2 WHERE project_id = ?1"
    ))
    .bind(project_id)
    .bind(org_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Upsert an org by slug. Returns its org_id.
pub async fn upsert_organization(
    pool: &crate::db::DbPool,
    slug: &str,
    name: Option<&str>,
) -> Result<u64> {
    sqlx::query(sql!(
        "INSERT INTO organizations (slug, name) VALUES (?1, ?2)
         ON CONFLICT(slug) DO UPDATE SET name = COALESCE(excluded.name, organizations.name)"
    ))
    .bind(slug)
    .bind(name)
    .execute(pool)
    .await?;
    let row = sqlx::query(sql!("SELECT org_id FROM organizations WHERE slug = ?1"))
        .bind(slug)
        .fetch_one(pool)
        .await?;
    Ok(row.get::<i64, _>("org_id") as u64)
}

/// Upsert a project that came in via Sentry API sync.
pub async fn upsert_synced_project(
    pool: &crate::db::DbPool,
    project_id: u64,
    name: &str,
    org_id: u64,
) -> Result<()> {
    sqlx::query(sql!(
        "INSERT INTO projects (project_id, name, status, source, org_id) VALUES (?1, ?2, 'active', 'synced', ?3)
         ON CONFLICT(project_id) DO UPDATE SET name = excluded.name, source = 'synced', org_id = excluded.org_id"
    ))
    .bind(project_id as i64)
    .bind(name)
    .bind(org_id as i64)
    .execute(pool)
    .await?;
    Ok(())
}

/// Upsert a project key imported from a Sentry sync.
/// Inserts the key if it doesn't exist yet, otherwise leaves it alone.
pub async fn upsert_synced_key(
    pool: &crate::db::DbPool,
    project_id: u64,
    public_key: &str,
    label: Option<&str>,
    active: bool,
) -> Result<()> {
    let status = if active { "active" } else { "inactive" };

    #[cfg(feature = "sqlite")]
    sqlx::query(sql!(
        "INSERT OR IGNORE INTO project_keys (public_key, project_id, status, label) VALUES (?1, ?2, ?3, ?4)"
    ))
    .bind(public_key)
    .bind(project_id as i64)
    .bind(status)
    .bind(label)
    .execute(pool)
    .await?;

    #[cfg(not(feature = "sqlite"))]
    sqlx::query(sql!(
        "INSERT INTO project_keys (public_key, project_id, status, label) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (public_key) DO NOTHING"
    ))
    .bind(public_key)
    .bind(project_id as i64)
    .bind(status)
    .bind(label)
    .execute(pool)
    .await?;

    Ok(())
}

/// Project-scoped tables deleted by a plain `WHERE project_id = ?1`. Excludes
/// `projects` itself and the child tables reached via subquery (attachments,
/// issue_tag_values, alert_state). The guard test below fails if a new
/// `project_id`-bearing table is added without being listed here.
const PROJECT_SCOPED_TABLES: &[&str] = &[
    "events",
    "logs",
    "spans",
    "metrics",
    "issues",
    "project_keys",
    "project_repos",
    "releases",
    "discarded_fingerprints",
    "inbound_filters",
    "message_filters",
    "rate_limits",
    "environment_filters",
    "release_filters",
    "user_agent_filters",
    "filter_rules",
    "ip_blocklist",
    "discard_stats",
    "project_integrations",
    "alert_rules",
    "digest_schedules",
    "api_keys",
    "sourcemaps",
    "upload_chunks",
    "session_aggregates",
    "transaction_metrics",
];

/// Delete a project and all it owns, reusing the caller's transaction.
pub async fn delete_project_in_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    project_id: i64,
) -> Result<()> {
    let pid = project_id;

    sqlx::query(sql!(
        "DELETE FROM attachments WHERE event_id IN (
            SELECT event_id FROM events WHERE project_id = ?1
        )"
    ))
    .bind(pid)
    .execute(&mut **tx)
    .await?;

    sqlx::query(sql!(
        "DELETE FROM issue_tag_values WHERE fingerprint IN (
            SELECT fingerprint FROM issues WHERE project_id = ?1
        )"
    ))
    .bind(pid)
    .execute(&mut **tx)
    .await?;

    sqlx::query(sql!(
        "DELETE FROM alert_state WHERE alert_rule_id IN (
            SELECT id FROM alert_rules WHERE project_id = ?1
        )"
    ))
    .bind(pid)
    .execute(&mut **tx)
    .await?;

    for table in PROJECT_SCOPED_TABLES {
        let raw = format!("DELETE FROM {table} WHERE project_id = ?1");
        let stmt = crate::db::translate_sql(&raw);
        sqlx::query(&stmt).bind(pid).execute(&mut **tx).await?;
    }

    sqlx::query(sql!("DELETE FROM projects WHERE project_id = ?1"))
        .bind(pid)
        .execute(&mut **tx)
        .await?;

    Ok(())
}

/// Delete a project and everything it owns (events, issues, keys, repos, releases).
pub async fn delete_project(pool: &crate::db::DbPool, project_id: u64) -> Result<()> {
    let mut tx = pool.begin().await?;
    delete_project_in_tx(&mut tx, project_id as i64).await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::test_helpers::*;

    const ORG_A: i64 = 1;
    const ORG_B: i64 = 2;

    // Ensure the org row exists, then upsert the project with the given org_id.
    async fn set_project_org(pool: &crate::db::DbPool, project_id: i64, org_id: i64) {
        sqlx::query(
            "INSERT INTO organizations (org_id, slug, name) VALUES (?1, ?1, ?1)
             ON CONFLICT(org_id) DO NOTHING",
        )
        .bind(org_id)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO projects (project_id, status, source, org_id) VALUES (?1, 'active', 'auto', ?2)
             ON CONFLICT(project_id) DO UPDATE SET org_id = excluded.org_id",
        )
        .bind(project_id)
        .bind(org_id)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn list_projects_empty() {
        let pool = open_test_db().await;
        let projects = list_projects(&pool, ORG_A, None, None, None).await.unwrap();
        assert!(projects.is_empty());
    }

    #[tokio::test]
    async fn list_projects_multiple() {
        let pool = open_test_db().await;
        set_project_org(&pool, 1, ORG_A).await;
        set_project_org(&pool, 2, ORG_A).await;
        insert_test_event(
            &pool,
            "e1",
            1,
            100,
            Some("fp1"),
            Some("error"),
            Some("Error A"),
        )
        .await;
        insert_test_event(
            &pool,
            "e2",
            1,
            200,
            Some("fp1"),
            Some("error"),
            Some("Error A"),
        )
        .await;
        insert_test_event(
            &pool,
            "e3",
            2,
            150,
            Some("fp2"),
            Some("warning"),
            Some("Warn B"),
        )
        .await;

        insert_test_issue(
            &pool,
            "fp1",
            1,
            Some("Error A"),
            Some("error"),
            100,
            200,
            2,
            "unresolved",
        )
        .await;
        insert_test_issue(
            &pool,
            "fp2",
            2,
            Some("Warn B"),
            Some("warning"),
            150,
            150,
            1,
            "unresolved",
        )
        .await;

        let projects = list_projects(&pool, ORG_A, None, None, None).await.unwrap();
        assert_eq!(projects.len(), 2);

        // Newest activity first, so project 1 (last_seen=200) comes first
        assert_eq!(projects[0].project_id, 1);
        assert_eq!(projects[0].event_count, 2);
        assert_eq!(projects[0].issue_count, 1);
        assert_eq!(projects[0].first_seen, 100);
        assert_eq!(projects[0].last_seen, 200);

        assert_eq!(projects[1].project_id, 2);
        assert_eq!(projects[1].event_count, 1);
        assert_eq!(projects[1].issue_count, 1);
    }

    #[tokio::test]
    async fn list_projects_no_issues() {
        let pool = open_test_db().await;
        set_project_org(&pool, 1, ORG_A).await;
        insert_test_event(&pool, "e1", 1, 100, None, Some("error"), Some("Error")).await;

        let projects = list_projects(&pool, ORG_A, None, None, None).await.unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].issue_count, 0);
        assert_eq!(projects[0].event_count, 1);
    }

    #[tokio::test]
    async fn list_projects_is_scoped_to_org() {
        let pool = open_test_db().await;
        set_project_org(&pool, 1, ORG_A).await;
        set_project_org(&pool, 2, ORG_B).await;
        insert_test_event(&pool, "e1", 1, 100, Some("fp1"), Some("error"), Some("A")).await;
        insert_test_event(&pool, "e2", 2, 100, Some("fp2"), Some("error"), Some("B")).await;
        let only_a = list_projects(&pool, ORG_A, None, None, None).await.unwrap();
        assert_eq!(only_a.len(), 1);
        assert_eq!(only_a[0].project_id, 1);
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn create_project_stores_org_id() {
        let pool = open_test_db().await;
        sqlx::query("INSERT INTO organizations (org_id, slug, name) VALUES (5, 'test-org', 'Test Org')")
            .execute(&pool)
            .await
            .unwrap();
        let (project_id, _key) = create_project(&pool, 5, "My Project", Some("rust"))
            .await
            .unwrap();
        let row = sqlx::query("SELECT org_id FROM projects WHERE project_id = ?1")
            .bind(project_id as i64)
            .fetch_one(&pool)
            .await
            .unwrap();
        let stored_org: i64 = row.get(0);
        assert_eq!(stored_org, 5);
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn reassign_project_changes_org_id() {
        use sqlx::Row;
        let pool = open_test_db().await;
        set_project_org(&pool, 500, ORG_A).await;
        sqlx::query("INSERT INTO organizations (org_id, slug, name) VALUES (?1, ?1, ?1)")
            .bind(ORG_B)
            .execute(&pool)
            .await
            .unwrap();

        let affected = reassign_project(&pool, 500, ORG_B).await.unwrap();
        assert_eq!(affected, 1);

        let row = sqlx::query("SELECT org_id FROM projects WHERE project_id = 500")
            .fetch_one(&pool)
            .await
            .unwrap();
        let stored: i64 = row.get(0);
        assert_eq!(stored, ORG_B);
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn list_unassigned_projects_returns_system_org_only() {
        use crate::orgs::SYSTEM_ORG_ID;
        let pool = open_test_db().await;
        // project 600 stays in SYSTEM_ORG_ID=1 (the default)
        sqlx::query("INSERT INTO projects (project_id, status, source, org_id) VALUES (600, 'active', 'auto', ?1)")
            .bind(SYSTEM_ORG_ID)
            .execute(&pool)
            .await
            .unwrap();
        // project 601 belongs to a real org (org_id=99) -- must be excluded
        sqlx::query("INSERT OR IGNORE INTO organizations (org_id, slug, name) VALUES (99, 'real-org-99', 'Real Org 99')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO projects (project_id, status, source, org_id) VALUES (601, 'active', 'auto', 99)")
            .execute(&pool)
            .await
            .unwrap();

        let unassigned = list_unassigned_projects(&pool).await.unwrap();
        let ids: Vec<i64> = unassigned.iter().map(|p| p.project_id).collect();
        assert!(ids.contains(&600), "project 600 must appear");
        assert!(!ids.contains(&601), "project 601 must be excluded");
    }

    /// Ensures delete_project_key won't cross project boundaries.
    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn delete_project_key_respects_project_scope() {
        use sqlx::Row;
        let pool = open_test_db().await;
        set_project_org(&pool, 10, ORG_A).await;
        set_project_org(&pool, 20, ORG_B).await;
        let key_a = create_project_key(&pool, 10, None).await.unwrap();
        let key_b = create_project_key(&pool, 20, None).await.unwrap();

        // Cross-org attempt: project 10 tries to delete key_b (owned by project 20).
        let affected = delete_project_key(&pool, 10, &key_b).await.unwrap();
        assert_eq!(affected, 0, "cross-project delete must affect 0 rows");

        // key_b must still exist.
        let count: i64 = sqlx::query("SELECT COUNT(*) FROM project_keys WHERE public_key = ?1")
            .bind(&key_b)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
        assert_eq!(count, 1, "key_b must still exist after rejected cross-project delete");

        // Legitimate delete still works.
        let affected = delete_project_key(&pool, 10, &key_a).await.unwrap();
        assert_eq!(affected, 1);
    }

    /// Fails if a new `project_id`-bearing table is added without being wired
    /// into `delete_project` (via `PROJECT_SCOPED_TABLES`), which would orphan
    /// its rows on project deletion.
    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn delete_project_covers_all_project_scoped_tables() {
        use sqlx::Row;
        let pool = open_test_db().await;
        let rows = sqlx::query(
            "SELECT DISTINCT m.name FROM sqlite_master m, pragma_table_info(m.name) p \
             WHERE m.type='table' AND p.name='project_id'",
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        for row in &rows {
            let table: String = row.get(0);
            assert!(
                table == "projects" || PROJECT_SCOPED_TABLES.contains(&table.as_str()),
                "table `{table}` has a project_id column but is not in PROJECT_SCOPED_TABLES; \
                 add it (and a delete_project case) or it will orphan rows"
            );
        }
    }
}
