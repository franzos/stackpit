use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use dashmap::DashMap;
use sqlx::Row;

use crate::db::sql;

use crate::domain::ProjectStatus;

use super::types::{ProjectKey, ProjectNavCounts, ProjectRepo, ProjectSummary};

/// Per-project nav badge counts cached with a short TTL, shared via `AppState` (not a global) so each app instance and each test keeps its own map.
pub type NavCountsCache = Arc<DashMap<u64, (ProjectNavCounts, Instant)>>;

/// Nav badges are a display convenience, so 30s trades slight staleness for far fewer full-table aggregations on busy projects.
const NAV_COUNTS_TTL: Duration = Duration::from_secs(30);

/// Which orgs a project listing covers.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OrgScope {
    /// Every org, for superuser and CLI contexts.
    All,
    /// Exactly these orgs. Always canonically sorted and deduped, so two callers with
    /// the same entitlements share one cache entry and cannot see each other's data.
    Orgs(Vec<i64>),
}

impl OrgScope {
    /// Canonicalizing constructor. Sorting and deduping here is what makes the cache
    /// key sound: the key is the literal id list, never a hash, so a collision cannot
    /// serve one caller another's projects.
    pub fn orgs(ids: Vec<i64>) -> Self {
        OrgScope::Orgs(super::canonical_org_ids(ids))
    }

    fn single(org_id: i64) -> Self {
        OrgScope::Orgs(vec![org_id])
    }
}

/// The orgs a browser caller may list projects from. Superusers (admin token and
/// loopback) see everything; everyone else sees the orgs they belong to.
///
/// The system org is excluded: it collects auto-provisioned, unassigned projects and
/// `can_switch_to` already makes it unreachable as an active org, so a stray
/// `organization_members` row for it must not surface those projects here.
pub fn scope_for(active: &crate::orgs::extractor::ActiveOrg) -> OrgScope {
    if active.role.is_none() {
        return OrgScope::All;
    }
    OrgScope::orgs(
        active
            .memberships
            .iter()
            .map(|(id, _)| *id)
            .filter(|id| *id != crate::orgs::SYSTEM_ORG_ID)
            .collect(),
    )
}

/// Assembled project-list result cached with the same short TTL as [`NavCountsCache`], shared via `AppState`. Keyed by the params that shape the SQL result (org scope, sort, and a TTL-bucketed `since`); the name/id `query` filter and the org filter/sort both run over the cached clone, so they stay out of the key.
pub type ProjectListCache =
    Arc<DashMap<(OrgScope, Option<String>, Option<i64>), (Vec<ProjectSummary>, Instant)>>;

/// Same rationale as [`NAV_COUNTS_TTL`]: the project list runs full-table GROUP BYs over `events`, so a short staleness window avoids re-aggregating on every render.
const PROJECT_LIST_TTL: Duration = Duration::from_secs(30);

/// Safety valve for [`ProjectListCache`]: over this many entries an insert first drops expired ones. The live key space is tiny in practice (orgs × sort × current time bucket).
const PROJECT_LIST_CACHE_MAX_ENTRIES: usize = 512;

/// True while a cache entry of the given age is still within the TTL window.
fn nav_cache_fresh(entry_age: Duration, ttl: Duration) -> bool {
    entry_age < ttl
}

/// Cached wrapper over [`get_nav_counts`]: returns a fresh clone on hit, otherwise recomputes, stores `(counts, now)`, and returns it.
pub async fn nav_counts_cached(
    pool: &crate::db::DbPool,
    cache: &NavCountsCache,
    project_id: u64,
) -> ProjectNavCounts {
    if let Some(entry) = cache.get(&project_id) {
        if nav_cache_fresh(entry.1.elapsed(), NAV_COUNTS_TTL) {
            return entry.0.clone();
        }
    }
    let counts = get_nav_counts(pool, project_id).await;
    cache.insert(project_id, (counts.clone(), Instant::now()));
    counts
}

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
    list_projects_inner(pool, &OrgScope::single(org_id), sort, query, since).await
}

/// List projects across every org the caller belongs to.
pub async fn list_projects_for_orgs(
    pool: &crate::db::DbPool,
    org_ids: Vec<i64>,
    sort: Option<&str>,
    query: Option<&str>,
    since: Option<i64>,
) -> Result<Vec<ProjectSummary>> {
    list_projects_inner(pool, &OrgScope::orgs(org_ids), sort, query, since).await
}

/// Cached [`list_projects`]: a fresh hit returns the stored list (query-filtered),
/// otherwise recompute the full list, cache it, and return the filtered view. The
/// all-time `first_seen` semantics are unchanged; only the assembled Vec is cached.
pub async fn list_projects_cached(
    pool: &crate::db::DbPool,
    cache: &ProjectListCache,
    scope: OrgScope,
    sort: Option<&str>,
    query: Option<&str>,
    since: Option<i64>,
) -> Result<Vec<ProjectSummary>> {
    // Bucket `since` to the TTL width: callers pass a live `now - period` timestamp
    // that changes every second, so keying on it raw would never hit and would grow
    // the cache without bound. Flooring to TTL-wide windows collapses a window onto
    // one key; the small resulting staleness is within the TTL the cache already accepts.
    let since_bucket = since.map(|s| s / PROJECT_LIST_TTL.as_secs() as i64);
    // Key on the normalized sort, not the raw parameter: the SQL folds every unknown
    // value to the default ORDER BY, so keying on the raw string would let `?sort=aaa`,
    // `?sort=aab`, ... miss the cache forever and re-run the full aggregation per request.
    let key = (
        scope.clone(),
        normalized_sort(sort).map(str::to_string),
        since_bucket,
    );
    if let Some(entry) = cache.get(&key) {
        if nav_cache_fresh(entry.1.elapsed(), PROJECT_LIST_TTL) {
            let mut projects = entry.0.clone();
            filter_projects_by_query(&mut projects, query);
            return Ok(projects);
        }
    }
    let full = list_projects_inner(pool, &scope, sort, None, since).await?;
    // Keying on the membership set instead of a single org makes the live key space
    // per-entitlement rather than per-org, so expiry alone no longer bounds it: drop
    // expired entries first, then evict oldest-first if that was not enough.
    if cache.len() > PROJECT_LIST_CACHE_MAX_ENTRIES {
        cache.retain(|_, (_, inserted)| nav_cache_fresh(inserted.elapsed(), PROJECT_LIST_TTL));
        evict_to_cap(cache, PROJECT_LIST_CACHE_MAX_ENTRIES);
    }
    cache.insert(key, (full.clone(), Instant::now()));
    let mut projects = full;
    filter_projects_by_query(&mut projects, query);
    Ok(projects)
}

/// The sort values that actually change the SQL ordering. Anything else (including
/// `org`, which is sorted in the handler) collapses to the default order.
fn normalized_sort(sort: Option<&str>) -> Option<&str> {
    sort.filter(|s| matches!(*s, "issues" | "events" | "first_seen" | "project_id"))
}

/// Drop oldest entries until the cache is back under `cap`.
fn evict_to_cap(cache: &ProjectListCache, cap: usize) {
    if cache.len() <= cap {
        return;
    }
    let mut entries: Vec<_> = cache
        .iter()
        .map(|e| (e.key().clone(), e.value().1))
        .collect();
    entries.sort_by_key(|(_, inserted)| *inserted);
    for (key, _) in entries.into_iter().take(cache.len().saturating_sub(cap)) {
        cache.remove(&key);
    }
}

/// List all projects across every org (CLI / superuser context).
pub async fn list_all_projects(
    pool: &crate::db::DbPool,
    sort: Option<&str>,
    query: Option<&str>,
    since: Option<i64>,
) -> Result<Vec<ProjectSummary>> {
    list_projects_inner(pool, &OrgScope::All, sort, query, since).await
}

async fn list_projects_inner(
    pool: &crate::db::DbPool,
    scope: &OrgScope,
    sort: Option<&str>,
    query: Option<&str>,
    since: Option<i64>,
) -> Result<Vec<ProjectSummary>> {
    // Safety: order_expr is always a hardcoded literal from this match, never user input.
    // COALESCE over the outer-joined aggregates keeps projects with no events in the
    // period ordering the same way on SQLite and PostgreSQL, which disagree on where
    // NULLs land in a DESC sort.
    let order_expr = match sort {
        Some("issues") => "COALESCE(i.issue_count, 0)",
        Some("events") => "COALESCE(e.event_count, 0)",
        Some("first_seen") => "COALESCE(fs.first_seen, 0)",
        Some("project_id") => "p.project_id",
        _ => "COALESCE(e.last_seen, 0)",
    };

    // An empty scope entitles the caller to nothing. `IN ()` is not valid SQL on
    // either backend, so this must short-circuit rather than fall through.
    let org_ids: &[i64] = match scope {
        OrgScope::All => &[],
        OrgScope::Orgs(ids) if ids.is_empty() => return Ok(Vec::new()),
        OrgScope::Orgs(ids) => ids,
    };

    // Org ids occupy ?1..?n, so the time filter binds at ?n+1. Getting this wrong is
    // invisible on SQLite and fatal on PostgreSQL, where `sql!`/`dyn_sql` rewrite
    // positional params to $n.
    let org_filter = if org_ids.is_empty() {
        String::new()
    } else {
        let params: Vec<String> = (1..=org_ids.len()).map(|i| format!("?{i}")).collect();
        format!("WHERE p.org_id IN ({})", params.join(", "))
    };

    let time_filter = if since.is_some() {
        format!("WHERE timestamp >= ?{}", org_ids.len() + 1)
    } else {
        String::new()
    };

    #[cfg(feature = "sqlite")]
    let platform_agg = "GROUP_CONCAT(DISTINCT platform)";
    #[cfg(not(feature = "sqlite"))]
    let platform_agg = "STRING_AGG(DISTINCT platform, ',')";

    // Driven from `projects`, not from the event aggregate: a project that received
    // nothing in the period (newly created, dormant) still gets a row, with NULL
    // counts the mapper folds to zero.
    let sql = format!(
        "SELECT
            p.project_id,
            p.name,
            p.status,
            p.org_id,
            o.name AS org_name,
            o.slug AS org_slug,
            COALESCE(e.event_count, 0) AS event_count,
            COALESCE(i.issue_count, 0) AS issue_count,
            fs.first_seen,
            e.last_seen,
            e.platforms,
            lr.version AS latest_release,
            COALESCE(e.error_count, 0) AS error_count,
            COALESCE(e.transaction_count, 0) AS transaction_count,
            COALESCE(e.session_count, 0) AS session_count,
            COALESCE(e.other_count, 0) AS other_count
         FROM projects p
         JOIN organizations o ON o.org_id = p.org_id
         LEFT JOIN (
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
         ) e ON e.project_id = p.project_id
         LEFT JOIN (
            SELECT project_id, MIN(timestamp) AS first_seen
            FROM events
            GROUP BY project_id
         ) fs ON fs.project_id = p.project_id
         LEFT JOIN (
            SELECT project_id, COUNT(*) AS issue_count
            FROM issues
            GROUP BY project_id
         ) i ON i.project_id = p.project_id
         LEFT JOIN (
            SELECT project_id, version FROM (
                SELECT project_id, version,
                       ROW_NUMBER() OVER (
                           PARTITION BY project_id
                           ORDER BY COALESCE(last_event, date_released, created_at) DESC, id DESC
                       ) AS rn
                FROM releases
            ) ranked WHERE rn = 1
         ) lr ON lr.project_id = p.project_id
         {org_filter}
         ORDER BY CASE WHEN fs.first_seen IS NULL THEN 1 ELSE 0 END, {order_expr} DESC, p.project_id DESC"
    );

    // Bind order must mirror the placeholder numbering above: orgs first, then `since`.
    let mut q = sqlx::query(crate::db::dyn_sql(&sql));
    for id in org_ids {
        q = q.bind(*id);
    }
    if let Some(ts) = since {
        q = q.bind(ts);
    }
    let rows = q.fetch_all(pool).await?;

    let mut projects: Vec<ProjectSummary> = rows.iter().map(map_project_row).collect();
    filter_projects_by_query(&mut projects, query);
    Ok(projects)
}

/// Narrow the list to projects whose id or name contains `query` (case-insensitive).
/// Client-side so it can run over a cached list without re-querying.
pub fn filter_projects_by_query(projects: &mut Vec<ProjectSummary>, query: Option<&str>) {
    let Some(q) = query.filter(|q| !q.is_empty()) else {
        return;
    };
    let q_lower = q.to_lowercase();
    projects.retain(|p| {
        p.project_id.to_string().contains(&q_lower)
            || p.name
                .as_ref()
                .map(|n| n.to_lowercase().contains(&q_lower))
                .unwrap_or(false)
    });
}

fn map_project_row(row: &crate::db::DbRow) -> ProjectSummary {
    let platforms: Option<String> = row.get("platforms");
    let status: Option<String> = row.get("status");
    // organizations.name is nullable; the slug is NOT NULL, so it is the fallback label.
    let org_name: Option<String> = row.get("org_name");
    ProjectSummary {
        project_id: row.get::<i64, _>("project_id") as u64,
        name: row.get("name"),
        org_id: row.get("org_id"),
        org_name: org_name.unwrap_or_else(|| row.get("org_slug")),
        archived: status
            .and_then(|s| s.parse::<ProjectStatus>().ok())
            .is_some_and(|s| s.is_archived()),
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
pub async fn reassign_project(
    pool: &crate::db::DbPool,
    project_id: i64,
    org_id: i64,
) -> Result<u64> {
    let result = sqlx::query(sql!(
        "UPDATE projects SET org_id = ?2 WHERE project_id = ?1"
    ))
    .bind(project_id)
    .bind(org_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Move a project into `to_org_id`, but only if it is currently in `from_org_id`.
/// Carries the org-denormalized alert/digest rows along and unlinks notification
/// integrations. Returns `Ok(false)` when the project wasn't in `from_org_id`
/// (a concurrent move/delete raced us), leaving all rows untouched.
pub async fn move_project_to_org(
    writer_pool: &crate::db::DbPool,
    project_id: i64,
    from_org_id: i64,
    to_org_id: i64,
) -> Result<bool> {
    let mut tx = writer_pool.begin().await?;

    let moved = sqlx::query(sql!(
        "UPDATE projects SET org_id = ?1 WHERE project_id = ?2 AND org_id = ?3"
    ))
    .bind(to_org_id)
    .bind(project_id)
    .bind(from_org_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if moved == 0 {
        tx.rollback().await?;
        return Ok(false);
    }

    sqlx::query(sql!(
        "UPDATE alert_rules SET org_id = ?1 WHERE project_id = ?2"
    ))
    .bind(to_org_id)
    .bind(project_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(sql!(
        "UPDATE digest_schedules SET org_id = ?1 WHERE project_id = ?2"
    ))
    .bind(to_org_id)
    .bind(project_id)
    .execute(&mut *tx)
    .await?;

    // Notification integrations belong to the source org; dropping the links
    // stops deliveries to it after the move (the new org re-adds its own).
    sqlx::query(sql!(
        "DELETE FROM project_integrations WHERE project_id = ?1"
    ))
    .bind(project_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(true)
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
    "issue_external_links",
    "project_tracker_targets",
    "replay_metadata",
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
        sqlx::query(crate::db::dyn_sql(&raw))
            .bind(pid)
            .execute(&mut **tx)
            .await?;
    }

    sqlx::query(sql!("DELETE FROM projects WHERE project_id = ?1"))
        .bind(pid)
        .execute(&mut **tx)
        .await?;

    Ok(())
}

/// Per-chunk delete cap; keeps each write-lock hold short so ingest can interleave.
const DELETE_CHUNK_LIMIT: i64 = 5000;

/// Chunk-delete rows matching `where_clause` (binding ?1 = pid), pausing
/// between chunks so a waiting writer can grab the DB write lock.
async fn chunked_delete(
    pool: &crate::db::DbPool,
    table: &'static str,
    where_clause: &'static str,
    pid: i64,
) -> Result<()> {
    #[cfg(feature = "sqlite")]
    let row_ref = "rowid";
    #[cfg(not(feature = "sqlite"))]
    let row_ref = "ctid";

    loop {
        let raw = format!(
            "DELETE FROM {table} WHERE {row_ref} IN (
                SELECT {row_ref} FROM {table} WHERE {where_clause} LIMIT ?2
            )"
        );
        let deleted = sqlx::query(crate::db::dyn_sql(&raw))
            .bind(pid)
            .bind(DELETE_CHUNK_LIMIT)
            .execute(pool)
            .await?
            .rows_affected();

        if deleted < DELETE_CHUNK_LIMIT as u64 {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Chunk-delete a project's high-volume rows (attachments, events, logs,
/// spans, metrics) outside any transaction, so a following transactional
/// cascade only holds the write lock for the small metadata tables.
pub(crate) async fn prechunk_project_data(pool: &crate::db::DbPool, pid: i64) -> Result<()> {
    chunked_delete(
        pool,
        "attachments",
        "event_id IN (SELECT event_id FROM events WHERE project_id = ?1)",
        pid,
    )
    .await?;
    for table in ["events", "logs", "spans", "metrics"] {
        chunked_delete(pool, table, "project_id = ?1", pid).await?;
    }
    Ok(())
}

/// Delete a project and everything it owns (events, issues, keys, repos, releases).
pub async fn delete_project(pool: &crate::db::DbPool, project_id: u64) -> Result<()> {
    let pid = project_id as i64;

    prechunk_project_data(pool, pid).await?;

    let mut tx = pool.begin().await?;
    delete_project_in_tx(&mut tx, pid).await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::test_helpers::*;

    const ORG_A: i64 = 1;
    const ORG_B: i64 = 2;

    // The membership-set key makes the live key space per-entitlement rather than
    // per-org, so expiry alone no longer bounds it; the cap has to actually cap.
    #[test]
    fn evict_to_cap_drops_oldest_first() {
        let cache: ProjectListCache = Default::default();
        for i in 0..10i64 {
            cache.insert(
                (OrgScope::orgs(vec![i]), None, None),
                (Vec::new(), Instant::now()),
            );
        }
        assert_eq!(cache.len(), 10);

        evict_to_cap(&cache, 4);
        assert_eq!(cache.len(), 4, "must evict down to the cap");
        // Insertion order is ascending, so the survivors are the newest keys.
        for i in 6..10i64 {
            assert!(
                cache.contains_key(&(OrgScope::orgs(vec![i]), None, None)),
                "newest entries must survive"
            );
        }
    }

    #[test]
    fn evict_to_cap_is_a_noop_under_the_cap() {
        let cache: ProjectListCache = Default::default();
        cache.insert(
            (OrgScope::orgs(vec![1]), None, None),
            (Vec::new(), Instant::now()),
        );
        evict_to_cap(&cache, 4);
        assert_eq!(cache.len(), 1);
    }

    // Unknown sort values all fold to the same SQL ordering, so they must fold to the
    // same cache key too, or an attacker-controlled `?sort=` re-runs the aggregation.
    #[tokio::test]
    async fn unknown_sorts_share_one_cache_entry() {
        let pool = open_test_db().await;
        let cache: ProjectListCache = Default::default();
        set_project_org(&pool, 1, ORG_B).await;
        let scope = OrgScope::orgs(vec![ORG_B]);

        for s in ["aaa", "aab", "org", ""] {
            list_projects_cached(&pool, &cache, scope.clone(), Some(s), None, None)
                .await
                .unwrap();
        }
        assert_eq!(cache.len(), 1, "unknown sorts must not each get a key");

        list_projects_cached(&pool, &cache, scope, Some("issues"), None, None)
            .await
            .unwrap();
        assert_eq!(cache.len(), 2, "a real sort still gets its own key");
    }

    #[test]
    fn scope_for_superuser_is_all_orgs() {
        use crate::orgs::extractor::ActiveOrg;
        assert_eq!(scope_for(&ActiveOrg::bare(1, None)), OrgScope::All);
    }

    // A membership row for the system org must not surface every auto-provisioned,
    // unassigned project; org 1 is superuser-only by construction.
    #[test]
    fn scope_for_drops_the_system_org() {
        use crate::orgs::extractor::ActiveOrg;
        use crate::orgs::{Role, SYSTEM_ORG_ID};

        let active = ActiveOrg::with_memberships(
            9,
            Some(Role::Member),
            vec![(SYSTEM_ORG_ID, Role::Owner), (9, Role::Member)],
        );
        assert_eq!(scope_for(&active), OrgScope::Orgs(vec![9]));
    }

    #[test]
    fn scope_for_canonicalizes_and_fails_closed() {
        use crate::orgs::extractor::ActiveOrg;
        use crate::orgs::Role;

        let active = ActiveOrg::with_memberships(
            5,
            Some(Role::Member),
            vec![(9, Role::Member), (5, Role::Owner), (9, Role::Member)],
        );
        assert_eq!(scope_for(&active), OrgScope::Orgs(vec![5, 9]));

        let orphan = ActiveOrg::with_memberships(5, Some(Role::Member), Vec::new());
        assert_eq!(scope_for(&orphan), OrgScope::Orgs(Vec::new()));
    }

    // Ensure the org row exists, then upsert the project with the given org_id.
    async fn set_project_org(pool: &crate::db::DbPool, project_id: i64, org_id: i64) {
        sqlx::query(sql!(
            "INSERT INTO organizations (org_id, slug, name) VALUES (?1, ?2, ?2)
             ON CONFLICT(org_id) DO NOTHING"
        ))
        .bind(org_id)
        .bind(format!("org-{org_id}"))
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(sql!(
            "INSERT INTO projects (project_id, status, source, org_id) VALUES (?1, 'active', 'auto', ?2)
             ON CONFLICT(project_id) DO UPDATE SET org_id = excluded.org_id"
        ))
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
        assert_eq!(projects[0].first_seen, Some(100));
        assert_eq!(projects[0].last_seen, Some(200));

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

    // The list is driven by `projects`, not by the event aggregate: a project
    // that has never ingested anything still gets a row, with zeroed counts and
    // no first/last seen.
    #[tokio::test]
    async fn list_projects_includes_project_without_events() {
        let pool = open_test_db().await;
        set_project_org(&pool, 1, ORG_A).await;

        let projects = list_projects(&pool, ORG_A, None, None, None).await.unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].project_id, 1);
        assert_eq!(projects[0].event_count, 0);
        assert_eq!(projects[0].issue_count, 0);
        assert_eq!(projects[0].first_seen, None);
        assert_eq!(projects[0].last_seen, None);
        assert!(projects[0].platforms.is_empty());
    }

    // A project that has never received an event has nothing meaningful to sort
    // on, so it stays at the bottom whatever column is picked -- including the
    // id sort, where its high id would otherwise put it on top.
    #[tokio::test]
    async fn list_projects_sorts_never_seen_projects_last() {
        let pool = open_test_db().await;
        set_project_org(&pool, 1, ORG_A).await;
        set_project_org(&pool, 2, ORG_A).await;
        insert_test_event(&pool, "e1", 1, 100, None, None, None).await;

        for sort in [None, Some("project_id"), Some("first_seen"), Some("events")] {
            let projects = list_projects(&pool, ORG_A, sort, None, None).await.unwrap();
            assert_eq!(
                projects.last().unwrap().project_id,
                2,
                "empty project should sort last for {sort:?}"
            );
        }
    }

    // A project whose only events fall outside the period stays listed, but its
    // period-scoped counts are zero and `last_seen` is empty; `first_seen` is
    // all-time so it survives.
    #[tokio::test]
    async fn list_projects_includes_dormant_project_for_period() {
        let pool = open_test_db().await;
        set_project_org(&pool, 1, ORG_A).await;
        set_project_org(&pool, 2, ORG_A).await;
        let now = chrono::Utc::now().timestamp();
        insert_test_event(&pool, "old", 1, now - 86_400 * 30, None, None, None).await;
        insert_test_event(&pool, "new", 2, now - 60, None, None, None).await;

        let projects = list_projects(&pool, ORG_A, None, None, Some(now - 3600))
            .await
            .unwrap();
        assert_eq!(projects.len(), 2);
        // Active project sorts above the dormant one (NULL last_seen orders last).
        assert_eq!(projects[0].project_id, 2);
        assert_eq!(projects[0].event_count, 1);

        assert_eq!(projects[1].project_id, 1);
        assert_eq!(projects[1].event_count, 0);
        assert_eq!(projects[1].last_seen, None);
        assert_eq!(projects[1].first_seen, Some(now - 86_400 * 30));
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
        assert_eq!(only_a[0].org_id, ORG_A);
    }

    // The cross-org list is the union of the caller's orgs and nothing else.
    #[tokio::test]
    async fn list_projects_for_orgs_returns_the_union() {
        let pool = open_test_db().await;
        set_project_org(&pool, 1, ORG_A).await;
        set_project_org(&pool, 2, ORG_B).await;
        set_project_org(&pool, 3, 7).await;

        let both = list_projects_for_orgs(&pool, vec![ORG_A, ORG_B], None, None, None)
            .await
            .unwrap();
        let mut ids: Vec<u64> = both.iter().map(|p| p.project_id).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![1, 2]);

        // Duplicates and ordering in the input must not change the result.
        let dup = list_projects_for_orgs(&pool, vec![ORG_B, ORG_A, ORG_A], None, None, None)
            .await
            .unwrap();
        assert_eq!(dup.len(), 2);
    }

    // Fail closed: no memberships must mean no projects, never every project.
    #[tokio::test]
    async fn list_projects_for_orgs_denies_on_empty_scope() {
        let pool = open_test_db().await;
        set_project_org(&pool, 1, ORG_A).await;
        let none = list_projects_for_orgs(&pool, Vec::new(), None, None, None)
            .await
            .unwrap();
        assert!(none.is_empty(), "an empty org set must list nothing");
    }

    // Exercises the generated `?1..?n` placeholders with the time filter binding at
    // ?n+1. Wrong bind positions are invisible on SQLite and fatal on PostgreSQL.
    #[tokio::test]
    async fn list_projects_for_orgs_binds_since_after_the_org_ids() {
        let pool = open_test_db().await;
        let now = chrono::Utc::now().timestamp();
        set_project_org(&pool, 1, ORG_A).await;
        set_project_org(&pool, 2, ORG_B).await;
        set_project_org(&pool, 3, 7).await;
        insert_test_event(&pool, "recent", 1, now, Some("fp1"), Some("error"), None).await;
        insert_test_event(
            &pool,
            "old",
            2,
            now - 86_400 * 30,
            Some("fp2"),
            Some("error"),
            None,
        )
        .await;

        let since = now - 86_400;
        let listed = list_projects_for_orgs(&pool, vec![ORG_A, ORG_B, 7], None, None, Some(since))
            .await
            .unwrap();

        // All three projects still appear (the list is driven from `projects`), but
        // only the one with an in-window event carries a count.
        assert_eq!(listed.len(), 3);
        let p1 = listed.iter().find(|p| p.project_id == 1).unwrap();
        let p2 = listed.iter().find(|p| p.project_id == 2).unwrap();
        assert_eq!(p1.event_count, 1, "in-window event must be counted");
        assert_eq!(p2.event_count, 0, "out-of-window event must not be counted");
    }

    // ORG_A is the system org, which migrations pre-seed with its own name, so this
    // uses a helper-created org to prove the label comes from `organizations`.
    #[tokio::test]
    async fn list_projects_carries_the_org_label() {
        let pool = open_test_db().await;
        set_project_org(&pool, 1, ORG_B).await;
        let listed = list_projects_for_orgs(&pool, vec![ORG_B], None, None, None)
            .await
            .unwrap();
        assert_eq!(listed[0].org_id, ORG_B);
        assert_eq!(listed[0].org_name, format!("org-{ORG_B}"));
    }

    // Two callers whose entitlements differ must never share a cache entry, and two
    // callers with the same entitlements (in any order) must share exactly one.
    #[tokio::test]
    async fn cache_key_isolates_distinct_membership_sets() {
        let pool = open_test_db().await;
        let cache: ProjectListCache = Default::default();
        set_project_org(&pool, 1, ORG_A).await;
        set_project_org(&pool, 2, ORG_B).await;

        let a_only =
            list_projects_cached(&pool, &cache, OrgScope::orgs(vec![ORG_A]), None, None, None)
                .await
                .unwrap();
        let both = list_projects_cached(
            &pool,
            &cache,
            OrgScope::orgs(vec![ORG_A, ORG_B]),
            None,
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(a_only.len(), 1, "the narrower caller must not see org B");
        assert_eq!(both.len(), 2);
        assert_eq!(cache.len(), 2, "distinct scopes must not collide");

        // Same set, different input order: canonicalization must hit the same entry.
        let reordered = list_projects_cached(
            &pool,
            &cache,
            OrgScope::orgs(vec![ORG_B, ORG_A]),
            None,
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(reordered.len(), 2);
        assert_eq!(cache.len(), 2, "reordered ids must reuse the cached entry");
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn create_project_stores_org_id() {
        let pool = open_test_db().await;
        sqlx::query(
            "INSERT INTO organizations (org_id, slug, name) VALUES (5, 'test-org', 'Test Org')",
        )
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
    async fn move_project_carries_alerts_and_unlinks_integrations() {
        use sqlx::Row;
        let pool = open_test_db().await;
        set_project_org(&pool, 700, ORG_A).await;
        sqlx::query("INSERT INTO organizations (org_id, slug, name) VALUES (?1, ?1, ?1)")
            .bind(ORG_B)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO alert_rules (org_id, project_id, trigger_kind) VALUES (?1, 700, 'threshold')",
        )
        .bind(ORG_A)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO digest_schedules (org_id, project_id, interval_secs) VALUES (?1, 700, 3600)",
        )
        .bind(ORG_A)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO integrations (id, name, kind, url) VALUES (1, 'i1', 'webhook', 'http://x')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO project_integrations (project_id, integration_id) VALUES (700, 1)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let moved = move_project_to_org(&pool, 700, ORG_A, ORG_B).await.unwrap();
        assert!(moved);

        let porg: i64 = sqlx::query("SELECT org_id FROM projects WHERE project_id = 700")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
        assert_eq!(porg, ORG_B, "project org must follow the move");
        let arorg: i64 = sqlx::query("SELECT org_id FROM alert_rules WHERE project_id = 700")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
        assert_eq!(arorg, ORG_B, "alert_rules org must follow the move");
        let dsorg: i64 = sqlx::query("SELECT org_id FROM digest_schedules WHERE project_id = 700")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
        assert_eq!(dsorg, ORG_B, "digest_schedules org must follow the move");
        let pi: i64 =
            sqlx::query("SELECT COUNT(*) FROM project_integrations WHERE project_id = 700")
                .fetch_one(&pool)
                .await
                .unwrap()
                .get(0);
        assert_eq!(pi, 0, "notification integrations must be unlinked on move");
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn move_project_conflict_when_from_org_mismatch() {
        use sqlx::Row;
        let pool = open_test_db().await;
        set_project_org(&pool, 701, ORG_A).await;
        sqlx::query("INSERT INTO organizations (org_id, slug, name) VALUES (?1, ?1, ?1)")
            .bind(ORG_B)
            .execute(&pool)
            .await
            .unwrap();

        // Claim the project is in ORG_B when it is actually in ORG_A: no-op.
        let moved = move_project_to_org(&pool, 701, ORG_B, 999).await.unwrap();
        assert!(!moved, "mismatched from_org must not move");

        let porg: i64 = sqlx::query("SELECT org_id FROM projects WHERE project_id = 701")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
        assert_eq!(porg, ORG_A, "project must stay put on conflict");
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
        assert_eq!(
            count, 1,
            "key_b must still exist after rejected cross-project delete"
        );

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

    /// The standalone path (chunked pre-deletes + final transactional cascade)
    /// must leave no rows behind in any table the project touches.
    #[tokio::test]
    async fn delete_project_removes_events_and_attachments() {
        let pool = open_test_db().await;
        set_project_org(&pool, 77, ORG_A).await;

        insert_test_event(&pool, "ev1", 77, 1000, Some("fp77"), Some("error"), None).await;
        insert_test_event(&pool, "ev2", 77, 1000, Some("fp77"), Some("error"), None).await;
        insert_test_issue(
            &pool,
            "fp77",
            77,
            Some("t"),
            Some("error"),
            0,
            0,
            2,
            "unresolved",
        )
        .await;

        sqlx::query(sql!(
            "INSERT INTO attachments (event_id, filename, data) VALUES ('ev1', 'a.txt', ?1)"
        ))
        .bind(&b"blob"[..])
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(sql!(
            "INSERT INTO logs (payload, project_id, public_key, timestamp) VALUES (?1, 77, 'k', 0)"
        ))
        .bind(&b"{}"[..])
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(sql!(
            "INSERT INTO spans (span_id, payload, project_id, public_key, timestamp) VALUES ('s1', ?1, 77, 'k', 0)"
        ))
        .bind(&b"{}"[..])
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(sql!(
            "INSERT INTO metrics (project_id, timestamp, mri, metric_type) VALUES (77, 0, 'm', 'c')"
        ))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(sql!(
            "INSERT INTO project_keys (public_key, project_id) VALUES ('key77', 77)"
        ))
        .execute(&pool)
        .await
        .unwrap();

        delete_project(&pool, 77).await.unwrap();

        for table in [
            "events",
            "attachments",
            "logs",
            "spans",
            "metrics",
            "issues",
            "project_keys",
            "projects",
        ] {
            let n: i64 =
                sqlx::query_scalar(crate::db::dyn_sql(&format!("SELECT COUNT(*) FROM {table}")))
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(n, 0, "table `{table}` should be empty after delete_project");
        }
    }

    #[test]
    fn nav_cache_fresh_within_ttl_is_hit() {
        assert!(nav_cache_fresh(
            Duration::from_secs(5),
            Duration::from_secs(30)
        ));
    }

    #[test]
    fn nav_cache_stale_past_ttl_is_miss() {
        assert!(!nav_cache_fresh(
            Duration::from_secs(31),
            Duration::from_secs(30)
        ));
    }

    #[test]
    fn nav_cache_at_ttl_boundary_is_miss() {
        assert!(!nav_cache_fresh(
            Duration::from_secs(30),
            Duration::from_secs(30)
        ));
    }

    #[test]
    fn nav_cache_absent_entry_is_miss() {
        let cache: NavCountsCache = Arc::new(DashMap::new());
        let fresh_hit = cache
            .get(&42)
            .is_some_and(|e| nav_cache_fresh(e.1.elapsed(), Duration::from_secs(30)));
        assert!(!fresh_hit, "absent project_id must not be a cache hit");
    }
}
