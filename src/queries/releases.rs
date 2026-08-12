use anyhow::Result;
use sqlx::Row;

use crate::db::sql;
use crate::db::DbPool;
use crate::util::version::version_sort_key;

use super::types::{
    DailySessions, Page, PagedResult, Release, ReleaseFilter, ReleaseHealth, ReleaseSummary,
};

/// Closed set of allowed ORDER BY clauses; the rendered ident is always
/// `&'static str`, keeping user input out of the SQL string.
enum ReleaseSort {
    Version,
    FirstSeen,
    Events,
    Issues,
    Adoption,
    ProjectId,
    LastSeen,
}

impl ReleaseSort {
    fn parse(sort: Option<&str>) -> Self {
        match sort {
            Some("first_seen") => Self::FirstSeen,
            Some("events") => Self::Events,
            Some("issues") => Self::Issues,
            Some("adoption") => Self::Adoption,
            Some("project_id") => Self::ProjectId,
            Some("last_seen") => Self::LastSeen,
            _ => Self::Version,
        }
    }

    fn as_sql_ident(&self) -> &'static str {
        match self {
            // COALESCE so a row whose key predates the backfill still lands in
            // roughly the right place instead of sorting as NULL.
            Self::Version => "COALESCE(r.version_sort, r.version) DESC",
            Self::FirstSeen => "first_seen ASC",
            Self::Events => "event_count DESC, last_seen DESC",
            Self::Issues => "issue_count DESC, last_seen DESC",
            Self::Adoption => "adoption DESC, last_seen DESC",
            Self::ProjectId => "r.project_id ASC, last_seen DESC",
            Self::LastSeen => "last_seen DESC",
        }
    }
}

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

/// Upsert a release, creating it or refreshing its fields.
pub async fn upsert_release(
    pool: &DbPool,
    project_id: u64,
    info: &ReleaseUpsert<'_>,
) -> Result<()> {
    sqlx::query(sql!(
        "INSERT INTO releases (project_id, version, commit_sha, date_released, first_event, last_event, new_groups, version_sort)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(project_id, version) DO UPDATE SET
             commit_sha = COALESCE(excluded.commit_sha, releases.commit_sha),
             date_released = COALESCE(excluded.date_released, releases.date_released),
             first_event = COALESCE(excluded.first_event, releases.first_event),
             last_event = COALESCE(excluded.last_event, releases.last_event),
             new_groups = CASE WHEN excluded.new_groups > 0 THEN excluded.new_groups ELSE releases.new_groups END,
             version_sort = excluded.version_sort"
    ))
    .bind(project_id as i64)
    .bind(info.version)
    .bind(info.commit_sha)
    .bind(info.date_released)
    .bind(info.first_event)
    .bind(info.last_event)
    .bind(info.new_groups as i64)
    .bind(version_sort_key(info.version))
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
    let rows = sqlx::query(sql!(
        "SELECT version FROM releases
         WHERE project_id = ?1
         ORDER BY COALESCE(version_sort, version) DESC
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

/// How many release rows to key per backfill round-trip.
const VERSION_SORT_BACKFILL_BATCH: i64 = 500;

/// One-shot startup fixup: compute `version_sort` for rows that pre-date
/// migration 021. Every write path sets it, so this only ever runs once per
/// upgrade; it works in batches so a large release table can't stall boot on a
/// single statement.
pub async fn backfill_version_sort(pool: &DbPool) -> Result<u64> {
    let mut updated = 0u64;
    loop {
        let rows = sqlx::query(sql!(
            "SELECT id, version FROM releases WHERE version_sort IS NULL LIMIT ?1"
        ))
        .bind(VERSION_SORT_BACKFILL_BATCH)
        .fetch_all(pool)
        .await?;

        if rows.is_empty() {
            return Ok(updated);
        }

        for row in &rows {
            let id: i64 = row.get(0);
            let version: String = row.get(1);
            let res = sqlx::query(sql!(
                "UPDATE releases SET version_sort = ?1 WHERE id = ?2 AND version_sort IS NULL"
            ))
            .bind(version_sort_key(&version))
            .bind(id)
            .execute(pool)
            .await?;
            updated += res.rows_affected();
        }
    }
}

/// Crash-free rate per release from the `session_aggregates` rollup, summing
/// environments. User-level crash-free merges HLL sketches and is None when an
/// identity-less aggregate contributed to the release.
///
/// `since_ts` bounds the window on `day_bucket`; the caller floors it to a day
/// boundary because the rollup has no finer granularity. `None` means all time.
pub async fn get_release_health(
    pool: &DbPool,
    project_id: u64,
    since_ts: Option<i64>,
    sort: ReleaseHealthSort,
) -> Result<Vec<ReleaseHealth>> {
    use crate::ingest::models::HLL_REGISTER_COUNT;
    use simple_hll::HyperLogLog;

    let rows = sqlx::query(sql!(
        "SELECT release, sessions_total, sessions_crashed, sessions_errored, sessions_abnormal,
                users_hll, users_crashed_hll, has_aggregate
         FROM session_aggregates
         WHERE project_id = ?1 AND day_bucket >= ?2"
    ))
    .bind(project_id as i64)
    .bind(since_ts.unwrap_or(0))
    .fetch_all(pool)
    .await?;

    struct Acc {
        total: u64,
        crashed: u64,
        errored: u64,
        abnormal: u64,
        has_aggregate: bool,
        users: HyperLogLog<12>,
        users_crashed: HyperLogLog<12>,
        has_user_data: bool,
    }

    let mut by_release: std::collections::HashMap<String, Acc> = std::collections::HashMap::new();
    for row in &rows {
        let release: String = row.get("release");
        let acc = by_release.entry(release).or_insert_with(|| Acc {
            total: 0,
            crashed: 0,
            errored: 0,
            abnormal: 0,
            has_aggregate: false,
            users: HyperLogLog::new(),
            users_crashed: HyperLogLog::new(),
            has_user_data: false,
        });
        acc.total += row.get::<i64, _>("sessions_total") as u64;
        acc.crashed += row.get::<i64, _>("sessions_crashed") as u64;
        acc.errored += row.get::<i64, _>("sessions_errored") as u64;
        acc.abnormal += row.get::<i64, _>("sessions_abnormal") as u64;
        if row.get::<i64, _>("has_aggregate") != 0 {
            acc.has_aggregate = true;
        }

        if let Some(buf) = row.get::<Option<Vec<u8>>, _>("users_hll") {
            if buf.len() == HLL_REGISTER_COUNT {
                acc.users.merge(&HyperLogLog::with_registers(buf));
                acc.has_user_data = true;
            }
        }
        if let Some(buf) = row.get::<Option<Vec<u8>>, _>("users_crashed_hll") {
            if buf.len() == HLL_REGISTER_COUNT {
                acc.users_crashed.merge(&HyperLogLog::with_registers(buf));
            }
        }
    }

    let mut out: Vec<ReleaseHealth> = by_release
        .into_iter()
        .map(|(release, acc)| {
            let total = acc.total;
            let crash_free_sessions = if total > 0 {
                (total.saturating_sub(acc.crashed) as f64 / total as f64) * 100.0
            } else {
                100.0
            };
            let label = if release.is_empty() {
                "(no release)".to_string()
            } else {
                release
            };

            let (crash_free_users, total_users) = if acc.has_aggregate || !acc.has_user_data {
                (None, None)
            } else {
                let users = acc.users.count() as u64;
                let crashed_users = acc.users_crashed.count() as u64;
                let cfu = if users > 0 {
                    ((users.saturating_sub(crashed_users)) as f64 / users as f64) * 100.0
                } else {
                    100.0
                };
                (Some((cfu * 100.0).round() / 100.0), Some(users))
            };

            ReleaseHealth {
                release: label,
                total_sessions: total,
                ok_count: total.saturating_sub(acc.crashed + acc.errored + acc.abnormal),
                crashed_count: acc.crashed,
                errored_count: acc.errored,
                crash_free_rate: (crash_free_sessions * 100.0).round() / 100.0,
                crash_free_users,
                total_users,
            }
        })
        .collect();

    // Sorted before the cap so the 200 rows kept are the ones the active sort
    // actually asks for, not the top 200 by sessions re-ordered afterwards.
    match sort {
        ReleaseHealthSort::Sessions => out.sort_by_key(|r| std::cmp::Reverse(r.total_sessions)),
        ReleaseHealthSort::Release => {
            out.sort_by_cached_key(|r| std::cmp::Reverse(version_sort_key(&r.release)))
        }
    }
    out.truncate(200);
    Ok(out)
}

/// Column the release-health table is ordered by. Both are descending: newest
/// release first, or busiest release first.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ReleaseHealthSort {
    Release,
    Sessions,
}

impl ReleaseHealthSort {
    pub fn parse(sort: Option<&str>) -> Self {
        match sort {
            Some("sessions") => Self::Sessions,
            _ => Self::Release,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Release => "release",
            Self::Sessions => "sessions",
        }
    }
}

/// Per-day session totals for a project, from `day_bucket` >= `since_ts`,
/// summed across releases and environments. Ordered oldest-first for charting.
pub async fn get_release_health_daily(
    pool: &DbPool,
    project_id: u64,
    since_ts: i64,
) -> Result<Vec<DailySessions>> {
    let rows = sqlx::query(sql!(
        "SELECT day_bucket, \
                CAST(SUM(sessions_total) AS BIGINT) AS total, \
                CAST(SUM(sessions_crashed) AS BIGINT) AS crashed, \
                CAST(SUM(sessions_errored) AS BIGINT) AS errored \
         FROM session_aggregates \
         WHERE project_id = ?1 AND day_bucket >= ?2 \
         GROUP BY day_bucket \
         ORDER BY day_bucket"
    ))
    .bind(project_id as i64)
    .bind(since_ts)
    .fetch_all(pool)
    .await?;

    let present: Vec<DailySessions> = rows
        .into_iter()
        .map(|row| DailySessions {
            day: row.get::<i64, _>("day_bucket"),
            total: row.get::<i64, _>("total") as u64,
            crashed: row.get::<i64, _>("crashed") as u64,
            errored: row.get::<i64, _>("errored") as u64,
        })
        .collect();

    Ok(fill_session_gaps(present, since_ts))
}

const SECS_PER_DAY: i64 = 86400;

/// Insert zero-valued entries for missing days so the chart x-axis is
/// time-proportional. Fills from the requested `since_day` (day-aligned) to the
/// last present day, capped at 90 days to avoid pathological output.
fn fill_session_gaps(present: Vec<DailySessions>, since_ts: i64) -> Vec<DailySessions> {
    let Some(last_day) = present.last().map(|d| d.day) else {
        return present;
    };
    let since_day = (since_ts / SECS_PER_DAY) * SECS_PER_DAY;
    let first_present = present.first().map(|d| d.day).unwrap_or(last_day);
    let mut start = since_day.min(first_present);
    if (last_day - start) / SECS_PER_DAY >= 90 {
        start = last_day - 89 * SECS_PER_DAY;
    }

    let by_day: std::collections::HashMap<i64, &DailySessions> =
        present.iter().map(|d| (d.day, d)).collect();

    let mut out = Vec::new();
    let mut day = start;
    while day <= last_day {
        match by_day.get(&day) {
            Some(d) => out.push((*d).clone()),
            None => out.push(DailySessions {
                day,
                total: 0,
                crashed: 0,
                errored: 0,
            }),
        }
        day += SECS_PER_DAY;
    }
    out
}

/// All releases across projects with event counts, issue counts, and adoption %.
/// `adoption_since` sets the time window for computing the adoption ratio.
/// Pass `org_id = Some(id)` to scope to that org; `None` returns all (superuser).
pub async fn list_all_releases(
    pool: &DbPool,
    filter: &ReleaseFilter,
    page: &Page,
    adoption_since: Option<i64>,
    org_id: Option<i64>,
) -> Result<PagedResult<ReleaseSummary>> {
    let one: Option<Vec<i64>> = org_id.map(|id| vec![id]);
    list_all_releases_inner(pool, filter, page, adoption_since, one.as_deref()).await
}

/// List releases across every org the caller belongs to. An empty list entitles
/// the caller to nothing, which is not the same as the superuser's "all orgs".
pub async fn list_all_releases_for_orgs(
    pool: &DbPool,
    filter: &ReleaseFilter,
    page: &Page,
    adoption_since: Option<i64>,
    org_ids: Vec<i64>,
) -> Result<PagedResult<ReleaseSummary>> {
    let ids = super::canonical_org_ids(org_ids);
    list_all_releases_inner(pool, filter, page, adoption_since, Some(&ids)).await
}

async fn list_all_releases_inner(
    pool: &DbPool,
    filter: &ReleaseFilter,
    page: &Page,
    adoption_since: Option<i64>,
    org_ids: Option<&[i64]>,
) -> Result<PagedResult<ReleaseSummary>> {
    // `IN ()` is not valid SQL on either backend, so an empty scope has to
    // short-circuit rather than fall through to an unscoped query.
    if org_ids.is_some_and(<[i64]>::is_empty) {
        return Ok(PagedResult::from_page(Vec::new(), 0, page));
    }

    let adoption_since_ts =
        adoption_since.unwrap_or_else(|| chrono::Utc::now().timestamp() - 86400);

    let mut count_qb =
        sqlx::QueryBuilder::<crate::db::Db>::new("SELECT COUNT(*) FROM releases r WHERE 1 = 1");

    if let Some(project_id) = filter.project_id {
        count_qb.push(" AND r.project_id = ");
        count_qb.push_bind(project_id as i64);
    }
    if let Some(ref query) = filter.query {
        count_qb.push(" AND r.version LIKE ");
        count_qb.push_bind(super::like_contains(query));
        count_qb.push(" ESCAPE '\\'");
    }
    if let Some(ids) = org_ids {
        count_qb.push(" AND ");
        super::push_org_scope_predicate(&mut count_qb, "r.project_id", ids);
    }

    let total: i64 = count_qb.build().fetch_one(pool).await?.get(0);

    let sort = ReleaseSort::parse(filter.sort.as_deref());

    // Driven off `releases`, not `events`: a release registered up front but not
    // yet seen in traffic is a real release with zero events, not a missing row.
    let mut qb = sqlx::QueryBuilder::<crate::db::Db>::new(
        "WITH project_totals AS (
            SELECT project_id, COUNT(*) AS total
            FROM events
            WHERE timestamp >= ",
    );
    qb.push_bind(adoption_since_ts);
    if let Some(project_id) = filter.project_id {
        qb.push(" AND project_id = ");
        qb.push_bind(project_id as i64);
    }
    if let Some(ids) = org_ids {
        qb.push(" AND ");
        super::push_org_scope_predicate(&mut qb, "project_id", ids);
    }
    qb.push(
        "
            GROUP BY project_id
        ),
        release_events AS (
            SELECT
                project_id,
                release,
                MIN(timestamp) AS first_seen,
                MAX(timestamp) AS last_seen,
                COUNT(*) AS event_count,
                COUNT(DISTINCT fingerprint) AS issue_count,
                SUM(CASE WHEN timestamp >= ",
    );
    qb.push_bind(adoption_since_ts);
    qb.push(
        " THEN 1 ELSE 0 END) AS recent_count
            FROM events
            WHERE release IS NOT NULL",
    );
    // The same filters as the outer query, repeated inside the CTE. The outer
    // ones sit on the preserved side of a LEFT JOIN, and sqlite won't infer
    // them through it -- without this it materializes every release in the
    // events table to answer a single-project page.
    if let Some(project_id) = filter.project_id {
        qb.push(" AND project_id = ");
        qb.push_bind(project_id as i64);
    }
    if let Some(ref query) = filter.query {
        qb.push(" AND release LIKE ");
        qb.push_bind(super::like_contains(query));
        qb.push(" ESCAPE '\\'");
    }
    if let Some(ids) = org_ids {
        qb.push(" AND ");
        super::push_org_scope_predicate(&mut qb, "project_id", ids);
    }
    qb.push(
        "
            GROUP BY project_id, release
        )
        SELECT
            r.version,
            r.project_id,
            p.name,
            COALESCE(re.first_seen, r.first_event, r.created_at) AS first_seen,
            COALESCE(re.last_seen, r.last_event, r.created_at) AS last_seen,
            COALESCE(re.event_count, 0) AS event_count,
            COALESCE(re.issue_count, 0) AS issue_count,
            COALESCE(
                CAST(COALESCE(re.recent_count, 0) AS REAL) /
                NULLIF(pt.total, 0) * 100.0,
                0.0
            ) AS adoption
         FROM releases r
         LEFT JOIN projects p ON p.project_id = r.project_id
         LEFT JOIN project_totals pt ON pt.project_id = r.project_id
         LEFT JOIN release_events re
                ON re.project_id = r.project_id AND re.release = r.version
         WHERE 1 = 1",
    );

    if let Some(project_id) = filter.project_id {
        qb.push(" AND r.project_id = ");
        qb.push_bind(project_id as i64);
    }
    if let Some(ref query) = filter.query {
        qb.push(" AND r.version LIKE ");
        qb.push_bind(super::like_contains(query));
        qb.push(" ESCAPE '\\'");
    }
    if let Some(ids) = org_ids {
        qb.push(" AND ");
        super::push_org_scope_predicate(&mut qb, "r.project_id", ids);
    }

    qb.push(" ORDER BY ");
    qb.push(sort.as_sql_ident());
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

    Ok(PagedResult::from_page(items, total, page))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::sql;
    use simple_hll::HyperLogLog;
    use sqlx::Row;

    #[allow(clippy::too_many_arguments)]
    async fn insert_agg(
        pool: &DbPool,
        project_id: i64,
        release: &str,
        environment: &str,
        total: i64,
        crashed: i64,
        errored: i64,
        has_aggregate: i64,
        users_hll: Option<Vec<u8>>,
        users_crashed_hll: Option<Vec<u8>>,
    ) {
        insert_agg_day(
            pool,
            project_id,
            release,
            environment,
            0,
            total,
            crashed,
            errored,
            has_aggregate,
            users_hll,
            users_crashed_hll,
        )
        .await;
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_agg_day(
        pool: &DbPool,
        project_id: i64,
        release: &str,
        environment: &str,
        day_bucket: i64,
        total: i64,
        crashed: i64,
        errored: i64,
        has_aggregate: i64,
        users_hll: Option<Vec<u8>>,
        users_crashed_hll: Option<Vec<u8>>,
    ) {
        sqlx::query(sql!(
            "INSERT INTO session_aggregates (project_id, release, environment, day_bucket, sessions_total, sessions_crashed, sessions_errored, sessions_abnormal, has_aggregate, users_hll, users_crashed_hll, first_seen, last_seen)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?9, ?10, 1000, 2000)"
        ))
        .bind(project_id)
        .bind(release)
        .bind(environment)
        .bind(day_bucket)
        .bind(total)
        .bind(crashed)
        .bind(errored)
        .bind(has_aggregate)
        .bind(users_hll)
        .bind(users_crashed_hll)
        .execute(pool)
        .await
        .unwrap();
    }

    /// All-time health for a project, newest release first.
    async fn health_all(pool: &DbPool, project_id: u64) -> Vec<ReleaseHealth> {
        get_release_health(pool, project_id, None, ReleaseHealthSort::Release)
            .await
            .unwrap()
    }

    fn hll_of(ids: &[&str]) -> Vec<u8> {
        let mut h: HyperLogLog<12> = HyperLogLog::new();
        for id in ids {
            h.add_object(id);
        }
        h.get_registers().to_vec()
    }

    #[tokio::test]
    async fn crash_free_sessions_errored_only_is_full() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        // 100 sessions, 0 crashed, 10 errored -> crash-free should be 100%.
        insert_agg(&pool, 1, "app@1.0", "prod", 100, 0, 10, 0, None, None).await;

        let health = health_all(&pool, 1).await;
        assert_eq!(health.len(), 1);
        assert_eq!(health[0].crash_free_rate, 100.0);
    }

    #[tokio::test]
    async fn crash_free_sessions_with_crashes() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        insert_agg(&pool, 1, "app@1.0", "prod", 100, 5, 0, 0, None, None).await;

        let health = health_all(&pool, 1).await;
        assert_eq!(health[0].crash_free_rate, 95.0);
    }

    #[tokio::test]
    async fn crash_free_sessions_does_not_underflow_when_crashed_exceeds_total() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        // Crash updates land with init=false (total=0) while the init row's total
        // is in a separate aggregate row; a release can sum to crashed > total.
        insert_agg(&pool, 1, "app@1.0", "prod", 0, 1, 0, 0, None, None).await;
        insert_agg(&pool, 1, "app@1.0", "staging", 1, 1, 0, 0, None, None).await;

        // total=1, crashed=2 -> must clamp to 0%, never panic or wrap.
        let health = health_all(&pool, 1).await;
        assert_eq!(health[0].crash_free_rate, 0.0);
    }

    #[tokio::test]
    async fn health_default_sort_is_newest_release_first() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        // Busiest release is the oldest one, so a session sort would invert this.
        insert_agg(&pool, 1, "app@1.0.9", "prod", 900, 0, 0, 0, None, None).await;
        insert_agg(&pool, 1, "app@1.0.12", "prod", 5, 0, 0, 0, None, None).await;
        insert_agg(&pool, 1, "app@1.0.10", "prod", 50, 0, 0, 0, None, None).await;

        let by_release: Vec<String> = health_all(&pool, 1)
            .await
            .into_iter()
            .map(|r| r.release)
            .collect();
        assert_eq!(by_release, ["app@1.0.12", "app@1.0.10", "app@1.0.9"]);

        let by_sessions: Vec<String> =
            get_release_health(&pool, 1, None, ReleaseHealthSort::Sessions)
                .await
                .unwrap()
                .into_iter()
                .map(|r| r.release)
                .collect();
        assert_eq!(by_sessions, ["app@1.0.9", "app@1.0.10", "app@1.0.12"]);
    }

    #[tokio::test]
    async fn health_window_excludes_days_before_since() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        let day1 = 1_609_459_200;
        let day2 = day1 + 86400;
        insert_agg_day(&pool, 1, "app@1.0", "prod", day1, 10, 0, 0, 0, None, None).await;
        insert_agg_day(&pool, 1, "app@2.0", "prod", day2, 20, 0, 0, 0, None, None).await;

        let all = health_all(&pool, 1).await;
        assert_eq!(all.len(), 2, "unbounded window keeps both days");

        let recent = get_release_health(&pool, 1, Some(day2), ReleaseHealthSort::Release)
            .await
            .unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].release, "app@2.0");
        assert_eq!(recent[0].total_sessions, 20);
    }

    #[tokio::test]
    async fn crash_free_users_from_hll() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        // 4 distinct users, 1 crashed -> 75% crash-free users.
        let users = hll_of(&["u1", "u2", "u3", "u4"]);
        let crashed = hll_of(&["u1"]);
        insert_agg(
            &pool,
            1,
            "app@1.0",
            "prod",
            4,
            1,
            0,
            0,
            Some(users),
            Some(crashed),
        )
        .await;

        let health = health_all(&pool, 1).await;
        assert_eq!(health[0].total_users, Some(4));
        assert_eq!(health[0].crash_free_users, Some(75.0));
    }

    #[tokio::test]
    async fn crash_free_users_none_when_aggregate_contributed() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        // Same release: one singular row with users, one aggregate row.
        insert_agg(
            &pool,
            1,
            "app@1.0",
            "prod",
            4,
            1,
            0,
            0,
            Some(hll_of(&["u1", "u2", "u3", "u4"])),
            Some(hll_of(&["u1"])),
        )
        .await;
        insert_agg(&pool, 1, "app@1.0", "staging", 100, 3, 0, 1, None, None).await;

        let health = health_all(&pool, 1).await;
        assert_eq!(health.len(), 1, "environments summed under one release");
        assert!(health[0].crash_free_users.is_none());
        assert!(health[0].total_users.is_none());
        // Sessions still summed: 104 total, 4 crashed.
        assert_eq!(health[0].total_sessions, 104);
        assert_eq!(health[0].crashed_count, 4);
    }

    #[tokio::test]
    async fn snapshot_sums_across_days_into_one_release_row() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        let day1 = 1_609_459_200;
        let day2 = day1 + 86400;
        // Same release spread over two days: 60 + 40 = 100 total, 3 + 2 = 5 crashed.
        insert_agg_day(&pool, 1, "app@1.0", "prod", day1, 60, 3, 0, 0, None, None).await;
        insert_agg_day(&pool, 1, "app@1.0", "prod", day2, 40, 2, 0, 0, None, None).await;

        let health = health_all(&pool, 1).await;
        assert_eq!(health.len(), 1, "one row per release across days");
        assert_eq!(health[0].total_sessions, 100);
        assert_eq!(health[0].crashed_count, 5);
        assert_eq!(health[0].crash_free_rate, 95.0);
    }

    #[tokio::test]
    async fn daily_groups_by_day_ordered_respecting_since() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        let day1 = 1_609_459_200;
        let day2 = day1 + 86400;
        let day3 = day2 + 86400;

        // day1 has two env rows that should sum together.
        insert_agg_day(&pool, 1, "app@1.0", "prod", day1, 10, 1, 2, 0, None, None).await;
        insert_agg_day(&pool, 1, "app@1.0", "staging", day1, 5, 0, 1, 0, None, None).await;
        insert_agg_day(&pool, 1, "app@1.0", "prod", day2, 7, 2, 0, 0, None, None).await;
        insert_agg_day(&pool, 1, "app@1.0", "prod", day3, 9, 3, 1, 0, None, None).await;

        // since = day2 -> day1 excluded.
        let daily = get_release_health_daily(&pool, 1, day2).await.unwrap();
        assert_eq!(daily.len(), 2);
        assert_eq!(daily[0].day, day2);
        assert_eq!(daily[0].total, 7);
        assert_eq!(daily[0].crashed, 2);
        assert_eq!(daily[1].day, day3);
        assert_eq!(daily[1].total, 9);

        // since = day1 -> all three, day1 env rows summed.
        let daily = get_release_health_daily(&pool, 1, day1).await.unwrap();
        assert_eq!(daily.len(), 3);
        assert_eq!(daily[0].day, day1);
        assert_eq!(daily[0].total, 15);
        assert_eq!(daily[0].crashed, 1);
        assert_eq!(daily[0].errored, 3);
    }

    async fn insert_org_rel(pool: &DbPool, slug: &str) -> i64 {
        sqlx::query(sql!(
            "INSERT INTO organizations (slug, name) VALUES (?1, ?1)"
        ))
        .bind(slug)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(sql!("SELECT org_id FROM organizations WHERE slug = ?1"))
            .bind(slug)
            .fetch_one(pool)
            .await
            .unwrap()
            .get("org_id")
    }

    async fn insert_project_rel(pool: &DbPool, project_id: i64, org_id: i64) {
        sqlx::query(sql!(
            "INSERT INTO projects (project_id, org_id) VALUES (?1, ?2)"
        ))
        .bind(project_id)
        .bind(org_id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn insert_event_with_release(
        pool: &DbPool,
        event_id: &str,
        project_id: i64,
        release: &str,
    ) {
        crate::queries::test_helpers::insert_test_event(
            pool,
            event_id,
            project_id,
            1000,
            None,
            Some("error"),
            Some("test"),
        )
        .await;
        // Overwrite the placeholder release set by insert_test_event with the real value
        sqlx::query(sql!("UPDATE events SET release = ?1 WHERE event_id = ?2"))
            .bind(release)
            .bind(event_id)
            .execute(pool)
            .await
            .unwrap();
        // Ingest materializes the release row; mirror that here.
        let info = ReleaseUpsert {
            version: release,
            commit_sha: None,
            date_released: None,
            first_event: Some(1000),
            last_event: Some(1000),
            new_groups: 0,
        };
        upsert_release(pool, project_id as u64, &info)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn list_releases_for_project_keeps_ingested_and_uploaded_together() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        insert_event_with_release(&pool, "rm1", 501, "v1.0").await;
        insert_event_with_release(&pool, "rm2", 501, "v1.1").await;

        assert_eq!(
            list_releases_for_project(&pool, 501).await.unwrap().len(),
            2
        );

        // A sourcemap upload registers a version with no events. It must join the
        // ingested ones rather than displace them.
        let info = ReleaseUpsert {
            version: "v2.0",
            commit_sha: None,
            date_released: None,
            first_event: None,
            last_event: None,
            new_groups: 0,
        };
        upsert_release(&pool, 501, &info).await.unwrap();

        let all = list_releases_for_project(&pool, 501).await.unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0], "v2.0", "highest version first, events or not");
        assert!(all.contains(&"v1.0".to_string()));
        assert!(all.contains(&"v1.1".to_string()));
    }

    // The filter dropdown used to order by last activity, which interleaves old
    // versions that are still sending events with genuinely newer ones.
    #[tokio::test]
    async fn list_releases_for_project_orders_by_version_not_activity() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        for (id, version) in [
            ("rv1", "com.softmax.did@1.0.4+111"),
            ("rv2", "com.softmax.did@1.0.12+119"),
            ("rv3", "com.softmax.did@1.0.9+116"),
        ] {
            insert_event_with_release(&pool, id, 502, version).await;
        }
        // 1.0.4 is the most recently active, but it is not the newest release.
        sqlx::query(sql!(
            "UPDATE releases SET last_event = 9999 WHERE version = 'com.softmax.did@1.0.4+111'"
        ))
        .execute(&pool)
        .await
        .unwrap();

        assert_eq!(
            list_releases_for_project(&pool, 502).await.unwrap(),
            [
                "com.softmax.did@1.0.12+119",
                "com.softmax.did@1.0.9+116",
                "com.softmax.did@1.0.4+111",
            ]
        );
    }

    #[tokio::test]
    async fn backfill_version_sort_keys_pre_migration_rows() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        insert_event_with_release(&pool, "rb1", 503, "app@1.0.9").await;
        insert_event_with_release(&pool, "rb2", 503, "app@1.0.12").await;
        // Simulate rows written before migration 021 added the column.
        sqlx::query(sql!("UPDATE releases SET version_sort = NULL"))
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(backfill_version_sort(&pool).await.unwrap(), 2);
        assert_eq!(
            list_releases_for_project(&pool, 503).await.unwrap(),
            ["app@1.0.12", "app@1.0.9"]
        );
        // Idempotent: a second boot has nothing left to do.
        assert_eq!(backfill_version_sort(&pool).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn list_all_releases_includes_uploaded_release_with_zero_events() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        let org = insert_org_rel(&pool, "rel-zero-org").await;
        insert_project_rel(&pool, 601, org).await;
        insert_event_with_release(&pool, "rz1", 601, "v1.0").await;

        let info = ReleaseUpsert {
            version: "v2.0",
            commit_sha: None,
            date_released: None,
            first_event: None,
            last_event: None,
            new_groups: 0,
        };
        upsert_release(&pool, 601, &info).await.unwrap();

        let page = list_all_releases(
            &pool,
            &ReleaseFilter::default(),
            &Page::new(None, None),
            None,
            Some(org),
        )
        .await
        .unwrap();

        assert_eq!(page.total, 2);
        let uploaded = page
            .items
            .iter()
            .find(|r| r.version == "v2.0")
            .expect("uploaded release listed");
        assert_eq!(uploaded.event_count, 0);
        assert_eq!(uploaded.issue_count, 0);
        assert_eq!(uploaded.adoption, 0.0);
        assert!(uploaded.first_seen > 0, "falls back to created_at");

        let ingested = page
            .items
            .iter()
            .find(|r| r.version == "v1.0")
            .expect("ingested release listed");
        assert_eq!(ingested.event_count, 1);
    }

    // The list is paginated in SQL, so ordering has to happen there: sorting the
    // page in the handler would only order 25 rows at a time.
    #[tokio::test]
    async fn list_all_releases_defaults_to_version_order() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        let org = insert_org_rel(&pool, "rel-ver-org").await;
        insert_project_rel(&pool, 701, org).await;
        for (id, version) in [
            ("rw1", "app@1.0.9"),
            ("rw2", "app@1.0.12"),
            ("rw3", "app@1.0.10"),
            ("rw4", "app@1.0.2"),
        ] {
            insert_event_with_release(&pool, id, 701, version).await;
        }

        let versions: Vec<String> = list_all_releases(
            &pool,
            &ReleaseFilter::default(),
            &Page::new(None, None),
            None,
            Some(org),
        )
        .await
        .unwrap()
        .items
        .into_iter()
        .map(|r| r.version)
        .collect();

        assert_eq!(
            versions,
            ["app@1.0.12", "app@1.0.10", "app@1.0.9", "app@1.0.2"],
            "plain string order would put 1.0.9 above 1.0.12"
        );
    }

    #[tokio::test]
    async fn list_all_releases_org_scoped_returns_only_that_org() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        let org_a = insert_org_rel(&pool, "rel-org-a").await;
        let org_b = insert_org_rel(&pool, "rel-org-b").await;
        insert_project_rel(&pool, 301, org_a).await;
        insert_project_rel(&pool, 302, org_b).await;
        insert_event_with_release(&pool, "re1", 301, "v1.0").await;
        insert_event_with_release(&pool, "re2", 302, "v2.0").await;

        let filter = ReleaseFilter::default();
        let page = Page::new(None, None);

        let scoped = list_all_releases(&pool, &filter, &page, None, Some(org_a))
            .await
            .unwrap();
        assert_eq!(scoped.total, 1);
        assert_eq!(scoped.items[0].version, "v1.0");

        let all = list_all_releases(&pool, &filter, &page, None, None)
            .await
            .unwrap();
        assert_eq!(all.total, 2);
    }

    #[tokio::test]
    async fn list_all_releases_org_b_scoped_excludes_org_a() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        let org_a = insert_org_rel(&pool, "rel2-org-a").await;
        let org_b = insert_org_rel(&pool, "rel2-org-b").await;
        insert_project_rel(&pool, 401, org_a).await;
        insert_project_rel(&pool, 402, org_b).await;
        insert_event_with_release(&pool, "rf1", 401, "vA").await;
        insert_event_with_release(&pool, "rf2", 401, "vA2").await;
        insert_event_with_release(&pool, "rf3", 402, "vB").await;

        let filter = ReleaseFilter::default();
        let page = Page::new(None, None);

        let scoped_b = list_all_releases(&pool, &filter, &page, None, Some(org_b))
            .await
            .unwrap();
        assert_eq!(scoped_b.total, 1);
        assert_eq!(scoped_b.items[0].version, "vB");
    }

    // Three orgs, because a two-org fixture (or the single-org seed) passes even
    // with the cross-project page scoped to one org. Both directions asserted:
    // the caller's two orgs appear, the third does not.
    #[tokio::test]
    async fn list_all_releases_for_orgs_spans_memberships_and_excludes_others() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        let org_a = insert_org_rel(&pool, "relm-org-a").await;
        let org_b = insert_org_rel(&pool, "relm-org-b").await;
        let org_c = insert_org_rel(&pool, "relm-org-c").await;
        insert_project_rel(&pool, 801, org_a).await;
        insert_project_rel(&pool, 802, org_b).await;
        insert_project_rel(&pool, 803, org_c).await;
        insert_event_with_release(&pool, "rm1", 801, "vMineA").await;
        insert_event_with_release(&pool, "rm2", 802, "vMineB").await;
        insert_event_with_release(&pool, "rm3", 803, "vTheirs").await;

        let filter = ReleaseFilter::default();
        let page = Page::new(None, None);

        let mine = list_all_releases_for_orgs(&pool, &filter, &page, None, vec![org_a, org_b])
            .await
            .unwrap();
        let versions: Vec<&str> = mine.items.iter().map(|r| r.version.as_str()).collect();
        assert_eq!(mine.total, 2);
        assert!(versions.contains(&"vMineA") && versions.contains(&"vMineB"));
        assert!(
            !versions.contains(&"vTheirs"),
            "another org's releases must not appear"
        );

        // An empty entitlement is *nothing*, not the superuser's "everything".
        let none = list_all_releases_for_orgs(&pool, &filter, &page, None, vec![])
            .await
            .unwrap();
        assert_eq!(none.total, 0);
        assert!(none.items.is_empty());
    }
}
