use anyhow::Result;
use sqlx::Row;

use crate::db::sql;
use crate::models::HLL_REGISTER_COUNT;
use simple_hll::HyperLogLog;

use super::types::{IssueFilter, IssueStatus, IssueSummary, Page, PagedResult};

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

    let sort_col = match filter.sort.as_deref() {
        Some("first_seen") => "first_seen",
        Some("event_count") => "event_count",
        _ => "last_seen",
    };

    // Build the shared WHERE clause once, then reuse for count and select.
    // We push binds inline so QueryBuilder handles the `?` placeholders itself.
    let mut count_qb: QueryBuilder<'_, crate::db::Db> =
        QueryBuilder::new("SELECT COUNT(*) FROM issues WHERE project_id = ");
    count_qb.push_bind(project_id as i64);
    push_issue_filter_conditions(&mut count_qb, filter, since);

    let total: i64 = count_qb.build_query_scalar().fetch_one(pool).await?;

    let mut select_qb: QueryBuilder<'_, crate::db::Db> = QueryBuilder::new(
        "SELECT fingerprint, project_id, title, level, first_seen, last_seen, event_count, status, item_type, user_hll
         FROM issues WHERE project_id = ",
    );
    select_qb.push_bind(project_id as i64);
    push_issue_filter_conditions(&mut select_qb, filter, since);
    select_qb.push(format!(" ORDER BY {sort_col} DESC LIMIT "));
    select_qb.push_bind(page.limit as i64);
    select_qb.push(" OFFSET ");
    select_qb.push_bind(page.offset as i64);

    let rows = select_qb.build().fetch_all(pool).await?;
    let items = rows.iter().map(map_issue_row).collect::<Result<Vec<_>>>()?;

    Ok(PagedResult {
        items,
        total: total as u64,
        offset: page.offset,
        limit: page.limit,
    })
}

/// Append filter conditions and their binds to an in-progress QueryBuilder.
/// Caller must have already pushed `WHERE project_id = ` + bind before calling this.
fn push_issue_filter_conditions<'args>(
    qb: &mut sqlx::QueryBuilder<'args, crate::db::Db>,
    filter: &'args IssueFilter,
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
        let escaped = query
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let pattern = format!("%{escaped}%");
        qb.push(" AND title LIKE ");
        qb.push_bind(pattern);
        qb.push(" ESCAPE '\\'");
    }
    if let Some(ref item_type) = filter.item_type {
        qb.push(" AND item_type = ");
        qb.push_bind(item_type.as_str());
    }
    if let Some(ref release) = filter.release {
        // The release subquery needs project_id -- we push it as a second bind here
        // so we don't have to track the placeholder index from the outer query.
        qb.push(" AND EXISTS (SELECT 1 FROM events e WHERE e.fingerprint = issues.fingerprint AND e.project_id = issues.project_id AND e.release = ");
        qb.push_bind(release.as_str());
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
        Some(_) => 0, // blob is corrupted, can't trust it
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

/// Upsert an issue row, bumping the event count by the given delta.
///
/// The thing is, during backfill the live-path title is usually better than what
/// we'd extract from old payloads -- so `prefer_existing_title` tells COALESCE
/// to keep the existing title when it's already set.
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

    let sql = crate::db::translate_sql(&sql);
    sqlx::query(&sql)
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
