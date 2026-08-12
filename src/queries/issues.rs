use anyhow::Result;
use sqlx::Row;

use crate::db::sql;
use crate::ingest::models::HLL_REGISTER_COUNT;
use simple_hll::HyperLogLog;

use crate::domain::IssueStatus;

use super::types::{IssueFilter, IssueSummary, Page, PagedResult};

/// Closed set of allowed ORDER BY columns; the rendered ident is always
/// `&'static str`, so user input can't reach the SQL string.
enum IssueSort {
    FirstSeen,
    EventCount,
    LastSeen,
}

impl IssueSort {
    fn parse(sort: Option<&str>) -> Self {
        match sort {
            Some("first_seen") => Self::FirstSeen,
            Some("event_count") => Self::EventCount,
            _ => Self::LastSeen,
        }
    }

    fn as_sql_ident(&self) -> &'static str {
        match self {
            Self::FirstSeen => "first_seen",
            Self::EventCount => "event_count",
            Self::LastSeen => "last_seen",
        }
    }
}

// --- Read queries ---

/// List issues for a project with optional filters and pagination.
/// Pass `since` to narrow it down to issues active after a given timestamp.
pub async fn list_issues(
    pool: &crate::db::DbPool,
    project_id: u64,
    filter: &IssueFilter,
    page: &Page,
    since: Option<i64>,
) -> Result<PagedResult<IssueSummary>> {
    use sqlx::QueryBuilder;

    let sort = IssueSort::parse(filter.sort.as_deref());

    // Shared WHERE clause is built for both the count and select queries.
    let mut count_qb: QueryBuilder<crate::db::Db> =
        QueryBuilder::new("SELECT COUNT(*) FROM issues WHERE project_id = ");
    count_qb.push_bind(project_id as i64);
    push_issue_filter_conditions(&mut count_qb, filter, since);

    let total: i64 = count_qb.build_query_scalar().fetch_one(pool).await?;

    let mut select_qb: QueryBuilder<crate::db::Db> = QueryBuilder::new(
        "SELECT fingerprint, project_id, title, level, first_seen, last_seen, event_count, status, item_type, user_hll
         FROM issues WHERE project_id = ",
    );
    select_qb.push_bind(project_id as i64);
    push_issue_filter_conditions(&mut select_qb, filter, since);
    select_qb.push(" ORDER BY ");
    select_qb.push(sort.as_sql_ident());
    select_qb.push(" DESC LIMIT ");
    select_qb.push_bind(page.limit as i64);
    select_qb.push(" OFFSET ");
    select_qb.push_bind(page.offset as i64);

    let rows = select_qb.build().fetch_all(pool).await?;
    let items = rows.iter().map(map_issue_row).collect::<Result<Vec<_>>>()?;

    Ok(PagedResult::from_page(items, total, page))
}

/// Append filter conditions and their binds to an in-progress QueryBuilder.
/// Caller must have already pushed `WHERE project_id = ` + bind before calling this.
fn push_issue_filter_conditions(
    qb: &mut sqlx::QueryBuilder<crate::db::Db>,
    filter: &IssueFilter,
    since: Option<i64>,
) {
    if let Some(ref level) = filter.level {
        qb.push(" AND level = ");
        qb.push_bind(level.as_str());
    }
    if let Some(ref status) = filter.status {
        qb.push(" AND status = ");
        qb.push_bind(status.as_str());
    }
    if let Some(ref query) = filter.query {
        qb.push(" AND title LIKE ");
        qb.push_bind(super::like_contains(query));
        qb.push(" ESCAPE '\\'");
    }
    if let Some(ref item_type) = filter.item_type {
        qb.push(" AND item_type = ");
        qb.push_bind(item_type.as_str());
    }
    if let Some(ref release) = filter.release {
        qb.push(" AND EXISTS (SELECT 1 FROM events e WHERE e.fingerprint = issues.fingerprint AND e.project_id = issues.project_id AND e.release = ");
        qb.push_bind(release.as_str());
        qb.push(")");
    }
    if let Some(ref environment) = filter.environment {
        qb.push(" AND EXISTS (SELECT 1 FROM events e WHERE e.fingerprint = issues.fingerprint AND e.project_id = issues.project_id AND e.environment = ");
        qb.push_bind(environment.as_str());
        qb.push(")");
    }
    if let Some((ref key, ref value)) = filter.tag {
        qb.push(" AND EXISTS (SELECT 1 FROM issue_tag_values itv WHERE itv.fingerprint = issues.fingerprint AND itv.tag_key = ");
        qb.push_bind(key.as_str());
        qb.push(" AND itv.tag_value = ");
        qb.push_bind(value.as_str());
        qb.push(")");
    }
    if let Some(ts) = since {
        qb.push(" AND last_seen >= ");
        qb.push_bind(ts);
    }
}

/// Issues whose events carry a given `transaction_name`, most recently seen
/// first. Powers the related-issues panel on the transaction summary.
///
/// Only `ItemType::Event` fingerprints (`can_fingerprint`), so the transaction's
/// own rows can never come back here — the match is on error events that were
/// recorded while serving this transaction.
pub async fn list_issues_for_transaction(
    pool: &crate::db::DbPool,
    project_id: u64,
    transaction_name: &str,
    since_ts: i64,
    limit: u32,
) -> Result<Vec<IssueSummary>> {
    let rows = sqlx::query(sql!(
        "SELECT fingerprint, project_id, title, level, first_seen, last_seen, event_count, status, item_type, user_hll \
         FROM issues \
         WHERE project_id = ?1 \
           AND EXISTS (SELECT 1 FROM events e \
                       WHERE e.fingerprint = issues.fingerprint AND e.project_id = issues.project_id \
                         AND e.transaction_name = ?2 AND e.timestamp >= ?3) \
         ORDER BY last_seen DESC \
         LIMIT ?4"
    ))
    .bind(project_id as i64)
    .bind(transaction_name)
    .bind(since_ts)
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;

    rows.iter().map(map_issue_row).collect()
}

/// Fetch a single issue by its fingerprint.
pub async fn get_issue(
    pool: &crate::db::DbPool,
    fingerprint: &str,
) -> Result<Option<IssueSummary>> {
    let row = sqlx::query(sql!(
        "SELECT fingerprint, project_id, title, level, first_seen, last_seen, event_count, status, item_type, user_hll
         FROM issues WHERE fingerprint = ?1"
    ))
    .bind(fingerprint)
    .fetch_optional(pool)
    .await?;

    row.map(|r| map_issue_row(&r)).transpose()
}

/// Fetch the release string from the earliest and latest events for an issue.
pub async fn get_issue_release_range(
    pool: &crate::db::DbPool,
    fingerprint: &str,
) -> Result<(Option<String>, Option<String>)> {
    let first: Option<String> = sqlx::query(sql!(
        "SELECT release FROM events WHERE fingerprint = ?1 AND release IS NOT NULL
         ORDER BY timestamp ASC LIMIT 1"
    ))
    .bind(fingerprint)
    .fetch_optional(pool)
    .await?
    .and_then(|r| r.get("release"));

    let last: Option<String> = sqlx::query(sql!(
        "SELECT release FROM events WHERE fingerprint = ?1 AND release IS NOT NULL
         ORDER BY timestamp DESC LIMIT 1"
    ))
    .bind(fingerprint)
    .fetch_optional(pool)
    .await?
    .and_then(|r| r.get("release"));

    Ok((first, last))
}

/// Per-fingerprint event counts bucketed across `[start_ts, now]` into
/// `bucket_count` equal slots, keyed by fingerprint. Powers the issue-row trend
/// sparklines: one query for the whole page rather than one per row. Missing
/// fingerprints simply won't appear in the map (their sparkline renders blank).
pub async fn issue_sparklines(
    pool: &crate::db::DbPool,
    project_id: u64,
    fingerprints: &[String],
    start_ts: i64,
    bucket_secs: i64,
    bucket_count: usize,
) -> Result<std::collections::HashMap<String, Vec<f32>>> {
    use sqlx::QueryBuilder;

    let mut out: std::collections::HashMap<String, Vec<f32>> = std::collections::HashMap::new();
    if fingerprints.is_empty() || bucket_secs <= 0 || bucket_count == 0 {
        return Ok(out);
    }

    // bucket_secs is a server-computed integer, safe to inline; everything
    // caller-influenced is bound.
    let mut qb: QueryBuilder<crate::db::Db> =
        QueryBuilder::new("SELECT fingerprint, CAST((timestamp - ");
    qb.push_bind(start_ts);
    qb.push(format!(
        ") / {bucket_secs} AS BIGINT) AS bucket, COUNT(*) FROM events WHERE project_id = "
    ));
    qb.push_bind(project_id as i64);
    qb.push(" AND timestamp >= ");
    qb.push_bind(start_ts);
    qb.push(" AND fingerprint IN (");
    {
        let mut sep = qb.separated(", ");
        for fp in fingerprints {
            sep.push_bind(fp.as_str());
        }
    }
    qb.push(") GROUP BY fingerprint, bucket");

    let rows = qb.build().fetch_all(pool).await?;
    for row in &rows {
        let fp: String = row.get(0);
        let bucket: i64 = row.get(1);
        let count: i64 = row.get(2);
        let idx = bucket.clamp(0, bucket_count as i64 - 1) as usize;
        let series = out.entry(fp).or_insert_with(|| vec![0.0; bucket_count]);
        series[idx] += count as f32;
    }
    Ok(out)
}

// --- Write operations ---

/// Flip an issue's status. Returns 0 if the fingerprint doesn't exist.
pub async fn update_issue_status(
    pool: &crate::db::DbPool,
    fingerprint: &str,
    status: IssueStatus,
) -> Result<u64> {
    let result = sqlx::query(sql!("UPDATE issues SET status = ?1 WHERE fingerprint = ?2"))
        .bind(status.as_str())
        .bind(fingerprint)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Link an issue to its upstream Sentry group ID -- only if not already set.
pub async fn set_sentry_group_id(
    pool: &crate::db::DbPool,
    fingerprint: &str,
    group_id: &str,
) -> Result<()> {
    sqlx::query(sql!(
        "UPDATE issues SET sentry_group_id = ?1
         WHERE fingerprint = ?2 AND sentry_group_id IS NULL"
    ))
    .bind(group_id)
    .bind(fingerprint)
    .execute(pool)
    .await?;
    Ok(())
}

/// Update issue status by Sentry group ID. Returns rows affected.
pub async fn update_status_by_group_id(
    pool: &crate::db::DbPool,
    project_id: u64,
    group_id: &str,
    status: &str,
) -> Result<u64> {
    let result = sqlx::query(sql!(
        "UPDATE issues SET status = ?1
         WHERE project_id = ?2 AND sentry_group_id = ?3 AND status != ?1"
    ))
    .bind(status)
    .bind(project_id as i64)
    .bind(group_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

fn map_issue_row(row: &crate::db::DbRow) -> Result<IssueSummary> {
    let hll_blob: Option<Vec<u8>> = row.get("user_hll");
    let user_count = match hll_blob {
        Some(buf) if buf.len() == HLL_REGISTER_COUNT => {
            HyperLogLog::<12>::with_registers(buf).count() as u64
        }
        Some(_) => 0, // unexpected register length: treat as corrupt
        None => 0,
    };

    let status_str: String = row.get("status");
    let item_type_str: String = row.get("item_type");

    Ok(IssueSummary {
        fingerprint: row.get("fingerprint"),
        project_id: row.get::<i64, _>("project_id") as u64,
        title: row.get("title"),
        level: row.get("level"),
        first_seen: row.get("first_seen"),
        last_seen: row.get("last_seen"),
        event_count: row.get::<i64, _>("event_count") as u64,
        status: status_str.parse().unwrap_or_default(),
        item_type: item_type_str.parse().unwrap_or_default(),
        user_count,
    })
}

/// Upsert issue; `prefer_existing_title` preserves live-path titles during backfill.
#[allow(clippy::too_many_arguments)]
pub async fn upsert_issue(
    pool: &crate::db::DbPool,
    fingerprint: &str,
    project_id: u64,
    title: Option<&str>,
    level: Option<&str>,
    first_seen: i64,
    last_seen: i64,
    event_count: u64,
    item_type: &str,
    prefer_existing_title: bool,
) -> Result<()> {
    let (title_coalesce, level_coalesce) = if prefer_existing_title {
        (
            "COALESCE(issues.title, excluded.title)",
            "COALESCE(issues.level, excluded.level)",
        )
    } else {
        (
            "COALESCE(excluded.title, issues.title)",
            "COALESCE(excluded.level, issues.level)",
        )
    };

    #[cfg(feature = "sqlite")]
    let sql = format!(
        "INSERT INTO issues (fingerprint, project_id, title, level, first_seen, last_seen, event_count, status, item_type)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'unresolved', ?8)
         ON CONFLICT(fingerprint) DO UPDATE SET
             first_seen = MIN(issues.first_seen, excluded.first_seen),
             last_seen = MAX(issues.last_seen, excluded.last_seen),
             event_count = issues.event_count + excluded.event_count,
             title = {title_coalesce},
             level = {level_coalesce},
             status = CASE WHEN issues.status = 'resolved' THEN 'unresolved' ELSE issues.status END"
    );

    #[cfg(not(feature = "sqlite"))]
    let sql = format!(
        "INSERT INTO issues (fingerprint, project_id, title, level, first_seen, last_seen, event_count, status, item_type)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'unresolved', ?8)
         ON CONFLICT(fingerprint) DO UPDATE SET
             first_seen = LEAST(issues.first_seen, excluded.first_seen),
             last_seen = GREATEST(issues.last_seen, excluded.last_seen),
             event_count = issues.event_count + excluded.event_count,
             title = {title_coalesce},
             level = {level_coalesce},
             status = CASE WHEN issues.status = 'resolved' THEN 'unresolved' ELSE issues.status END"
    );

    sqlx::query(crate::db::dyn_sql(&sql))
        .bind(fingerprint)
        .bind(project_id as i64)
        .bind(title)
        .bind(level)
        .bind(first_seen)
        .bind(last_seen)
        .bind(event_count as i64)
        .bind(item_type)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::test_helpers::*;

    #[tokio::test]
    async fn list_issues_empty() {
        let pool = open_test_db().await;
        let filter = IssueFilter::default();
        let page = Page::new(None, None);
        let result = list_issues(&pool, 1, &filter, &page, None).await.unwrap();
        assert!(result.items.is_empty());
        assert_eq!(result.total, 0);
    }

    #[tokio::test]
    async fn list_issues_basic() {
        let pool = open_test_db().await;
        insert_test_issue(
            &pool,
            "fp1",
            1,
            Some("Error A"),
            Some("error"),
            100,
            200,
            5,
            "unresolved",
        )
        .await;
        insert_test_issue(
            &pool,
            "fp2",
            1,
            Some("Warn B"),
            Some("warning"),
            150,
            300,
            2,
            "resolved",
        )
        .await;
        insert_test_issue(
            &pool,
            "fp3",
            2,
            Some("Other"),
            Some("error"),
            100,
            100,
            1,
            "unresolved",
        )
        .await;

        let filter = IssueFilter::default();
        let page = Page::new(None, None);
        let result = list_issues(&pool, 1, &filter, &page, None).await.unwrap();
        assert_eq!(result.total, 2);
        assert_eq!(result.items.len(), 2);
        // Should come back newest-first
        assert_eq!(result.items[0].fingerprint, "fp2");
        assert_eq!(result.items[1].fingerprint, "fp1");
    }

    #[tokio::test]
    async fn list_issues_filter_level() {
        let pool = open_test_db().await;
        insert_test_issue(
            &pool,
            "fp1",
            1,
            Some("Error A"),
            Some("error"),
            100,
            200,
            5,
            "unresolved",
        )
        .await;
        insert_test_issue(
            &pool,
            "fp2",
            1,
            Some("Warn B"),
            Some("warning"),
            150,
            300,
            2,
            "unresolved",
        )
        .await;

        let filter = IssueFilter {
            level: Some("error".to_string()),
            ..Default::default()
        };
        let page = Page::new(None, None);
        let result = list_issues(&pool, 1, &filter, &page, None).await.unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(result.items[0].fingerprint, "fp1");
    }

    #[tokio::test]
    async fn list_issues_filter_status() {
        let pool = open_test_db().await;
        insert_test_issue(
            &pool,
            "fp1",
            1,
            Some("Error A"),
            Some("error"),
            100,
            200,
            5,
            "unresolved",
        )
        .await;
        insert_test_issue(
            &pool,
            "fp2",
            1,
            Some("Error B"),
            Some("error"),
            150,
            300,
            2,
            "resolved",
        )
        .await;

        let filter = IssueFilter {
            status: Some("resolved".to_string()),
            ..Default::default()
        };
        let page = Page::new(None, None);
        let result = list_issues(&pool, 1, &filter, &page, None).await.unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(result.items[0].fingerprint, "fp2");
    }

    #[tokio::test]
    async fn list_issues_filter_query() {
        let pool = open_test_db().await;
        insert_test_issue(
            &pool,
            "fp1",
            1,
            Some("NullPointerException in handler"),
            Some("error"),
            100,
            200,
            5,
            "unresolved",
        )
        .await;
        insert_test_issue(
            &pool,
            "fp2",
            1,
            Some("Connection timeout"),
            Some("error"),
            150,
            300,
            2,
            "unresolved",
        )
        .await;

        let filter = IssueFilter {
            query: Some("NullPointer".to_string()),
            ..Default::default()
        };
        let page = Page::new(None, None);
        let result = list_issues(&pool, 1, &filter, &page, None).await.unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(result.items[0].fingerprint, "fp1");
    }

    // The environment filter mirrors `release`: an EXISTS over the issue's events,
    // not a column on `issues`. An issue qualifies if *any* of its events landed in
    // that environment.
    #[tokio::test]
    async fn list_issues_filter_environment() {
        let pool = open_test_db().await;
        for (fp, title) in [("fp1", "Error A"), ("fp2", "Error B")] {
            insert_test_issue(
                &pool,
                fp,
                1,
                Some(title),
                Some("error"),
                100,
                200,
                1,
                "unresolved",
            )
            .await;
        }
        // insert_test_event hard-codes environment 'production'; override per row.
        crate::queries::test_helpers::insert_test_event(
            &pool,
            "e1",
            1,
            100,
            Some("fp1"),
            Some("error"),
            Some("Error A"),
        )
        .await;
        crate::queries::test_helpers::insert_test_event(
            &pool,
            "e2",
            1,
            150,
            Some("fp2"),
            Some("error"),
            Some("Error B"),
        )
        .await;
        sqlx::query(sql!(
            "UPDATE events SET environment = 'staging' WHERE event_id = 'e2'"
        ))
        .execute(&pool)
        .await
        .unwrap();

        let page = Page::new(None, None);

        let prod = list_issues(
            &pool,
            1,
            &IssueFilter {
                environment: Some("production".to_string()),
                ..Default::default()
            },
            &page,
            None,
        )
        .await
        .unwrap();
        assert_eq!(prod.total, 1);
        assert_eq!(prod.items[0].fingerprint, "fp1");

        let staging = list_issues(
            &pool,
            1,
            &IssueFilter {
                environment: Some("staging".to_string()),
                ..Default::default()
            },
            &page,
            None,
        )
        .await
        .unwrap();
        assert_eq!(staging.total, 1);
        assert_eq!(staging.items[0].fingerprint, "fp2");

        // An environment nothing was sent to matches nothing, rather than everything.
        let none = list_issues(
            &pool,
            1,
            &IssueFilter {
                environment: Some("nope".to_string()),
                ..Default::default()
            },
            &page,
            None,
        )
        .await
        .unwrap();
        assert_eq!(none.total, 0);

        // No filter: both issues.
        let all = list_issues(&pool, 1, &IssueFilter::default(), &page, None)
            .await
            .unwrap();
        assert_eq!(all.total, 2);
    }

    // Powers the transaction summary's related-issues panel: an issue qualifies
    // when any of its events was recorded while serving that transaction.
    #[tokio::test]
    async fn issues_for_transaction_scope_by_transaction_name_and_period() {
        let pool = open_test_db().await;
        for (fp, title, last_seen) in [("fp1", "Error A", 200), ("fp2", "Error B", 300)] {
            insert_test_issue(
                &pool,
                fp,
                1,
                Some(title),
                Some("error"),
                100,
                last_seen,
                1,
                "unresolved",
            )
            .await;
        }
        // insert_test_event hard-codes transaction_name '/api/test'; repoint one.
        crate::queries::test_helpers::insert_test_event(
            &pool,
            "e1",
            1,
            100,
            Some("fp1"),
            Some("error"),
            Some("Error A"),
        )
        .await;
        crate::queries::test_helpers::insert_test_event(
            &pool,
            "e2",
            1,
            150,
            Some("fp2"),
            Some("error"),
            Some("Error B"),
        )
        .await;
        sqlx::query(sql!(
            "UPDATE events SET transaction_name = '/checkout' WHERE event_id = 'e2'"
        ))
        .execute(&pool)
        .await
        .unwrap();

        let checkout = list_issues_for_transaction(&pool, 1, "/checkout", 0, 10)
            .await
            .unwrap();
        assert_eq!(checkout.len(), 1);
        assert_eq!(checkout[0].fingerprint, "fp2");

        let api = list_issues_for_transaction(&pool, 1, "/api/test", 0, 10)
            .await
            .unwrap();
        assert_eq!(api.len(), 1);
        assert_eq!(api[0].fingerprint, "fp1");

        // A transaction nothing was recorded against matches nothing.
        assert!(list_issues_for_transaction(&pool, 1, "/nope", 0, 10)
            .await
            .unwrap()
            .is_empty());

        // Another project's issues never surface.
        assert!(list_issues_for_transaction(&pool, 2, "/checkout", 0, 10)
            .await
            .unwrap()
            .is_empty());

        // The period bounds the *events*, not the issue's last_seen.
        assert!(list_issues_for_transaction(&pool, 1, "/checkout", 200, 10)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn issues_for_transaction_orders_by_last_seen_and_honours_the_limit() {
        let pool = open_test_db().await;
        for (fp, last_seen) in [("fp1", 200), ("fp2", 400), ("fp3", 300)] {
            insert_test_issue(
                &pool,
                fp,
                1,
                Some(fp),
                Some("error"),
                100,
                last_seen,
                1,
                "unresolved",
            )
            .await;
            crate::queries::test_helpers::insert_test_event(
                &pool,
                &format!("e-{fp}"),
                1,
                100,
                Some(fp),
                Some("error"),
                Some(fp),
            )
            .await;
        }

        let all = list_issues_for_transaction(&pool, 1, "/api/test", 0, 10)
            .await
            .unwrap();
        let order: Vec<&str> = all.iter().map(|i| i.fingerprint.as_str()).collect();
        assert_eq!(order, ["fp2", "fp3", "fp1"]);

        let capped = list_issues_for_transaction(&pool, 1, "/api/test", 0, 2)
            .await
            .unwrap();
        assert_eq!(capped.len(), 2);
        assert_eq!(capped[0].fingerprint, "fp2");
    }

    #[tokio::test]
    async fn list_environments_for_project_is_distinct_and_skips_blanks() {
        let pool = open_test_db().await;
        for (id, ts) in [("v1", 100), ("v2", 150), ("v3", 200), ("v4", 250)] {
            crate::queries::test_helpers::insert_test_event(&pool, id, 1, ts, None, None, None)
                .await;
        }
        // v1 keeps the default 'production'; give the rest distinct/blank values.
        for (id, env) in [("v2", "staging"), ("v3", "production"), ("v4", "")] {
            sqlx::query(sql!(
                "UPDATE events SET environment = ?1 WHERE event_id = ?2"
            ))
            .bind(env)
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        }

        let envs = crate::queries::events::list_environments_for_project(&pool, 1)
            .await
            .unwrap();
        assert_eq!(envs, vec!["production".to_string(), "staging".to_string()]);
    }

    #[tokio::test]
    async fn list_issues_pagination() {
        let pool = open_test_db().await;
        for i in 0..10i64 {
            insert_test_issue(
                &pool,
                &format!("fp{i}"),
                1,
                Some(&format!("Issue {i}")),
                Some("error"),
                100 + i,
                200 + i,
                1,
                "unresolved",
            )
            .await;
        }

        let filter = IssueFilter::default();

        // First page
        let page = Page::new(Some(0), Some(3));
        let result = list_issues(&pool, 1, &filter, &page, None).await.unwrap();
        assert_eq!(result.total, 10);
        assert_eq!(result.items.len(), 3);
        assert!(result.has_next());
        assert!(!result.has_prev());

        // Second page
        let page = Page::new(Some(3), Some(3));
        let result = list_issues(&pool, 1, &filter, &page, None).await.unwrap();
        assert_eq!(result.items.len(), 3);
        assert!(result.has_next());
        assert!(result.has_prev());

        // Last page
        let page = Page::new(Some(9), Some(3));
        let result = list_issues(&pool, 1, &filter, &page, None).await.unwrap();
        assert_eq!(result.items.len(), 1);
        assert!(!result.has_next());
        assert!(result.has_prev());
    }

    #[tokio::test]
    async fn issue_sparklines_buckets_per_fingerprint() {
        let pool = open_test_db().await;
        // 10 buckets of 100s each, window [1000, 2000).
        let start = 1000i64;
        let bucket_secs = 100i64;
        let buckets = 10usize;
        // fp1: two events in bucket 0, one in bucket 3.
        insert_test_event(&pool, "e1", 1, 1010, Some("fp1"), None, None).await;
        insert_test_event(&pool, "e2", 1, 1050, Some("fp1"), None, None).await;
        insert_test_event(&pool, "e3", 1, 1320, Some("fp1"), None, None).await;
        // fp2: one event in bucket 5.
        insert_test_event(&pool, "e4", 1, 1550, Some("fp2"), None, None).await;
        // fp3: an event before the window and one for another project — both excluded.
        insert_test_event(&pool, "e5", 1, 500, Some("fp3"), None, None).await;
        insert_test_event(&pool, "e6", 2, 1500, Some("fp1"), None, None).await;

        let fps = vec!["fp1".to_string(), "fp2".to_string(), "fp3".to_string()];
        let out = issue_sparklines(&pool, 1, &fps, start, bucket_secs, buckets)
            .await
            .unwrap();

        let fp1 = out.get("fp1").expect("fp1 present");
        assert_eq!(fp1.len(), buckets);
        assert_eq!(fp1[0], 2.0);
        assert_eq!(fp1[3], 1.0);
        assert_eq!(fp1[5], 0.0); // project 2 event must not leak in
        assert_eq!(out.get("fp2").unwrap()[5], 1.0);
        // fp3's only event predates the window, so it has no bucketed row.
        assert!(!out.contains_key("fp3"));
    }

    #[tokio::test]
    async fn issue_sparklines_empty_inputs() {
        let pool = open_test_db().await;
        assert!(issue_sparklines(&pool, 1, &[], 0, 100, 10)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn get_issue_found() {
        let pool = open_test_db().await;
        insert_test_issue(
            &pool,
            "fp1",
            1,
            Some("Error A"),
            Some("error"),
            100,
            200,
            5,
            "unresolved",
        )
        .await;

        let issue = get_issue(&pool, "fp1").await.unwrap().unwrap();
        assert_eq!(issue.fingerprint, "fp1");
        assert_eq!(issue.project_id, 1);
        assert_eq!(issue.title.as_deref(), Some("Error A"));
        assert_eq!(issue.event_count, 5);
        assert_eq!(issue.status, IssueStatus::Unresolved);
    }

    #[tokio::test]
    async fn get_issue_not_found() {
        let pool = open_test_db().await;
        assert!(get_issue(&pool, "nonexistent").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn update_issue_status_valid() {
        let pool = open_test_db().await;
        insert_test_issue(
            &pool,
            "fp1",
            1,
            Some("Error A"),
            Some("error"),
            100,
            200,
            5,
            "unresolved",
        )
        .await;

        update_issue_status(&pool, "fp1", IssueStatus::Resolved)
            .await
            .unwrap();
        let issue = get_issue(&pool, "fp1").await.unwrap().unwrap();
        assert_eq!(issue.status, IssueStatus::Resolved);

        update_issue_status(&pool, "fp1", IssueStatus::Ignored)
            .await
            .unwrap();
        let issue = get_issue(&pool, "fp1").await.unwrap().unwrap();
        assert_eq!(issue.status, IssueStatus::Ignored);

        update_issue_status(&pool, "fp1", IssueStatus::Unresolved)
            .await
            .unwrap();
        let issue = get_issue(&pool, "fp1").await.unwrap().unwrap();
        assert_eq!(issue.status, IssueStatus::Unresolved);
    }

    #[tokio::test]
    async fn update_issue_status_not_found() {
        let pool = open_test_db().await;
        let rows = update_issue_status(&pool, "nonexistent", IssueStatus::Resolved)
            .await
            .unwrap();
        assert_eq!(rows, 0);
    }
}
