use anyhow::Result;
use sqlx::Row;

use crate::db::sql;
use crate::db::DbPool;

use super::types::{Page, PagedResult, Release, ReleaseFilter, ReleaseHealth, ReleaseSummary};

/// Which projects have a specific release version deployed.
pub async fn find_projects_by_version(pool: &DbPool, version: &str) -> Result<Vec<u64>> {
    let rows = sqlx::query(sql!("SELECT project_id FROM releases WHERE version = ?1"))
        .bind(version)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| row.get::<i64, _>(0) as u64)
        .collect())
}

/// Upsert a release -- creates it if new, or updates fields with fresh data.
pub async fn upsert_release(
    pool: &DbPool,
    project_id: u64,
    info: &ReleaseUpsert<'_>,
) -> Result<()> {
    sqlx::query(sql!(
        "INSERT INTO releases (project_id, version, commit_sha, date_released, first_event, last_event, new_groups)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(project_id, version) DO UPDATE SET
             commit_sha = COALESCE(excluded.commit_sha, releases.commit_sha),
             date_released = COALESCE(excluded.date_released, releases.date_released),
             first_event = COALESCE(excluded.first_event, releases.first_event),
             last_event = COALESCE(excluded.last_event, releases.last_event),
             new_groups = CASE WHEN excluded.new_groups > 0 THEN excluded.new_groups ELSE releases.new_groups END"
    ))
    .bind(project_id as i64)
    .bind(info.version)
    .bind(info.commit_sha)
    .bind(info.date_released)
    .bind(info.first_event)
    .bind(info.last_event)
    .bind(info.new_groups as i64)
    .execute(pool)
    .await?;
    Ok(())
}

/// Fields for upserting a release.
pub struct ReleaseUpsert<'a> {
    pub version: &'a str,
    pub commit_sha: Option<&'a str>,
    pub date_released: Option<i64>,
    pub first_event: Option<i64>,
    pub last_event: Option<i64>,
    pub new_groups: u64,
}

/// Look up a release by project + version.
#[allow(dead_code)]
pub async fn get_release(pool: &DbPool, project_id: u64, version: &str) -> Result<Option<Release>> {
    let row = sqlx::query(sql!(
        "SELECT id, project_id, version, commit_sha, date_released, first_event, last_event, new_groups, created_at
         FROM releases WHERE project_id = ?1 AND version = ?2"
    ))
    .bind(project_id as i64)
    .bind(version)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| Release {
        id: row.get(0),
        project_id: row.get::<i64, _>(1) as u64,
        version: row.get(2),
        commit_sha: row.get(3),
        date_released: row.get(4),
        first_event: row.get(5),
        last_event: row.get(6),
        new_groups: row.get::<i64, _>(7) as u64,
        created_at: row.get(8),
    }))
}

/// Distinct releases for a project, most recent first. Capped at 50.
pub async fn list_releases_for_project(pool: &DbPool, project_id: u64) -> Result<Vec<String>> {
    // Try the releases table first (populated by sync), fall back to events.release
    let rows = sqlx::query(sql!(
        "SELECT version FROM releases
         WHERE project_id = ?1
         ORDER BY created_at DESC
         LIMIT 50"
    ))
    .bind(project_id as i64)
    .fetch_all(pool)
    .await?;

    if !rows.is_empty() {
        return Ok(rows
            .into_iter()
            .map(|row| row.get::<String, _>(0))
            .collect());
    }

    // Fallback: releases extracted from event payloads directly
    let rows = sqlx::query(sql!(
        "SELECT release, MAX(timestamp) AS latest FROM events
         WHERE project_id = ?1 AND release IS NOT NULL
         GROUP BY release
         ORDER BY latest DESC
         LIMIT 50"
    ))
    .bind(project_id as i64)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>(0))
        .collect())
}

/// Release health stats -- crash-free rate per release, computed from session data.
pub async fn get_release_health(pool: &DbPool, project_id: u64) -> Result<Vec<ReleaseHealth>> {
    let rows = sqlx::query(sql!(
        "SELECT
            COALESCE(release, '(no release)') AS rel,
            COUNT(*) AS total,
            SUM(CASE WHEN session_status = 'ok' OR session_status = 'exited' THEN 1 ELSE 0 END) AS ok_count,
            SUM(CASE WHEN session_status = 'crashed' THEN 1 ELSE 0 END) AS crashed,
            SUM(CASE WHEN session_status = 'errored' OR session_status = 'abnormal' THEN 1 ELSE 0 END) AS errored
         FROM events
         WHERE project_id = ?1
           AND item_type IN ('session', 'sessions')
           AND session_status IS NOT NULL
           AND session_status != 'init'
         GROUP BY rel
         ORDER BY total DESC
         LIMIT 200"
    ))
    .bind(project_id as i64)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let total: i64 = row.get(1);
            let crashed: i64 = row.get(3);
            let errored: i64 = row.get(4);
            let crash_free = if total > 0 {
                ((total - crashed - errored) as f64 / total as f64) * 100.0
            } else {
                100.0
            };
            ReleaseHealth {
                release: row.get(0),
                total_sessions: total as u64,
                ok_count: row.get::<i64, _>(2) as u64,
                crashed_count: crashed as u64,
                errored_count: errored as u64,
                crash_free_rate: (crash_free * 100.0).round() / 100.0,
            }
        })
        .collect())
}

/// All releases across projects with event counts, issue counts, and adoption %.
/// `adoption_since` sets the time window for computing the adoption ratio.
pub async fn list_all_releases(
    pool: &DbPool,
    filter: &ReleaseFilter,
    page: &Page,
    adoption_since: Option<i64>,
) -> Result<PagedResult<ReleaseSummary>> {
    let adoption_since_ts =
        adoption_since.unwrap_or_else(|| chrono::Utc::now().timestamp() - 86400);

    // Build the count query to get the total before pagination.
    // We collect filter conditions separately so we can reuse them in both queries.
    let mut count_qb = sqlx::QueryBuilder::<crate::db::Db>::new(
        "SELECT COUNT(*) FROM (
            SELECT e.project_id, e.release
            FROM events e
            WHERE e.release IS NOT NULL",
    );

    if let Some(project_id) = filter.project_id {
        count_qb.push(" AND e.project_id = ");
        count_qb.push_bind(project_id as i64);
    }
    if let Some(ref query) = filter.query {
        let escaped = query
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        count_qb.push(" AND e.release LIKE ");
        count_qb.push_bind(format!("%{escaped}%"));
        count_qb.push(" ESCAPE '\\'");
    }

    #[cfg(feature = "sqlite")]
    count_qb.push(" GROUP BY e.project_id, e.release)");
    #[cfg(not(feature = "sqlite"))]
    count_qb.push(" GROUP BY e.project_id, e.release) AS sub");

    let total: i64 = count_qb.build().fetch_one(pool).await?.get(0);

    let sort_col = match filter.sort.as_deref() {
        Some("first_seen") => "first_seen ASC",
        Some("events") => "event_count DESC, last_seen DESC",
        Some("issues") => "issue_count DESC, last_seen DESC",
        Some("adoption") => "adoption DESC, last_seen DESC",
        Some("project_id") => "e.project_id ASC, last_seen DESC",
        _ => "last_seen DESC",
    };

    // Main CTE query with pagination.
    let mut qb = sqlx::QueryBuilder::<crate::db::Db>::new(
        "WITH project_totals AS (
            SELECT project_id, COUNT(*) AS total
            FROM events
            WHERE timestamp >= ",
    );
    qb.push_bind(adoption_since_ts);
    qb.push(
        "
            GROUP BY project_id
        )
        SELECT
            e.release,
            e.project_id,
            p.name,
            MIN(e.timestamp) AS first_seen,
            MAX(e.timestamp) AS last_seen,
            COUNT(*) AS event_count,
            COUNT(DISTINCT e.fingerprint) AS issue_count,
            COALESCE(
                CAST(SUM(CASE WHEN e.timestamp >= ",
    );
    qb.push_bind(adoption_since_ts);
    qb.push(
        " THEN 1 ELSE 0 END) AS REAL) /
                NULLIF(pt.total, 0) * 100.0,
                0.0
            ) AS adoption
         FROM events e
         LEFT JOIN projects p ON p.project_id = e.project_id
         LEFT JOIN project_totals pt ON pt.project_id = e.project_id
         WHERE e.release IS NOT NULL",
    );

    if let Some(project_id) = filter.project_id {
        qb.push(" AND e.project_id = ");
        qb.push_bind(project_id as i64);
    }
    if let Some(ref query) = filter.query {
        let escaped = query
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        qb.push(" AND e.release LIKE ");
        qb.push_bind(format!("%{escaped}%"));
        qb.push(" ESCAPE '\\'");
    }

    qb.push(" GROUP BY e.project_id, e.release ORDER BY ");
    qb.push(sort_col);
    qb.push(" LIMIT ");
    qb.push_bind(page.limit as i64);
    qb.push(" OFFSET ");
    qb.push_bind(page.offset as i64);

    let rows = qb.build().fetch_all(pool).await?;

    let items: Vec<ReleaseSummary> = rows
        .into_iter()
        .map(|row| {
            let adoption_raw: f64 = row.get(7);
            ReleaseSummary {
                version: row.get(0),
                project_id: row.get::<i64, _>(1) as u64,
                project_name: row.get(2),
                first_seen: row.get(3),
                last_seen: row.get(4),
                event_count: row.get::<i64, _>(5) as u64,
                issue_count: row.get::<i64, _>(6) as u64,
                adoption: (adoption_raw * 10.0).round() / 10.0,
            }
        })
        .collect();

    Ok(PagedResult {
        items,
        total: total as u64,
        offset: page.offset,
        limit: page.limit,
    })
}
