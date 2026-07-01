use anyhow::Result;
use sqlx::Row;

use crate::db::sql;
use crate::db::DbPool;

use super::types::{
    DailySessions, Page, PagedResult, Release, ReleaseFilter, ReleaseHealth, ReleaseSummary,
};

/// Closed set of allowed ORDER BY clauses; the rendered ident is always
/// `&'static str`, keeping user input out of the SQL string.
enum ReleaseSort {
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
            _ => Self::LastSeen,
        }
    }

    fn as_sql_ident(&self) -> &'static str {
        match self {
            Self::FirstSeen => "first_seen ASC",
            Self::Events => "event_count DESC, last_seen DESC",
            Self::Issues => "issue_count DESC, last_seen DESC",
            Self::Adoption => "adoption DESC, last_seen DESC",
            Self::ProjectId => "e.project_id ASC, last_seen DESC",
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
    // Prefer the releases table (populated by sync), fall back to events.release.
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

    // Fallback: releases from event payloads.
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

/// Crash-free rate per release from the `session_aggregates` rollup, summing
/// environments. User-level crash-free merges HLL sketches and is None when an
/// identity-less aggregate contributed to the release.
pub async fn get_release_health(pool: &DbPool, project_id: u64) -> Result<Vec<ReleaseHealth>> {
    use crate::ingest::models::HLL_REGISTER_COUNT;
    use simple_hll::HyperLogLog;

    let rows = sqlx::query(sql!(
        "SELECT release, sessions_total, sessions_crashed, sessions_errored, sessions_abnormal,
                users_hll, users_crashed_hll, has_aggregate
         FROM session_aggregates
         WHERE project_id = ?1"
    ))
    .bind(project_id as i64)
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

    out.sort_by_key(|r| std::cmp::Reverse(r.total_sessions));
    out.truncate(200);
    Ok(out)
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
    let adoption_since_ts =
        adoption_since.unwrap_or_else(|| chrono::Utc::now().timestamp() - 86400);

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
        count_qb.push(" AND e.release LIKE ");
        count_qb.push_bind(super::like_contains(query));
        count_qb.push(" ESCAPE '\\'");
    }
    if let Some(oid) = org_id {
        count_qb.push(" AND e.project_id IN (SELECT project_id FROM projects WHERE org_id = ");
        count_qb.push_bind(oid);
        count_qb.push(")");
    }

    #[cfg(feature = "sqlite")]
    count_qb.push(" GROUP BY e.project_id, e.release)");
    #[cfg(not(feature = "sqlite"))]
    count_qb.push(" GROUP BY e.project_id, e.release) AS sub");

    let total: i64 = count_qb.build().fetch_one(pool).await?.get(0);

    let sort = ReleaseSort::parse(filter.sort.as_deref());

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
        qb.push(" AND e.release LIKE ");
        qb.push_bind(super::like_contains(query));
        qb.push(" ESCAPE '\\'");
    }
    if let Some(oid) = org_id {
        qb.push(" AND e.project_id IN (SELECT project_id FROM projects WHERE org_id = ");
        qb.push_bind(oid);
        qb.push(")");
    }

    qb.push(" GROUP BY e.project_id, e.release ORDER BY ");
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

        let health = get_release_health(&pool, 1).await.unwrap();
        assert_eq!(health.len(), 1);
        assert_eq!(health[0].crash_free_rate, 100.0);
    }

    #[tokio::test]
    async fn crash_free_sessions_with_crashes() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        insert_agg(&pool, 1, "app@1.0", "prod", 100, 5, 0, 0, None, None).await;

        let health = get_release_health(&pool, 1).await.unwrap();
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
        let health = get_release_health(&pool, 1).await.unwrap();
        assert_eq!(health[0].crash_free_rate, 0.0);
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

        let health = get_release_health(&pool, 1).await.unwrap();
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

        let health = get_release_health(&pool, 1).await.unwrap();
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

        let health = get_release_health(&pool, 1).await.unwrap();
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
        sqlx::query(sql!("INSERT INTO organizations (slug, name) VALUES (?1, ?1)"))
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
        sqlx::query(sql!(
            "UPDATE events SET release = ?1 WHERE event_id = ?2"
        ))
        .bind(release)
        .bind(event_id)
        .execute(pool)
        .await
        .unwrap();
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
}
