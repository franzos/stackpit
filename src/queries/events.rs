use anyhow::{Context, Result};
use sqlx::Row;

use crate::db::sql;

use super::types::{
    EventDetail, EventFilter, EventSummary, Page, PagedResult, TagFacet, TagFacetValue, TailEvent,
};

/// Append event filter conditions and their binds to an in-progress QueryBuilder.
/// Caller must have already pushed the base query (e.g. `SELECT ... FROM events`).
/// If any filters are active, a `WHERE` keyword is emitted first.
fn push_event_filter_conditions<'args>(
    qb: &mut sqlx::QueryBuilder<'args, crate::db::Db>,
    filter: &'args EventFilter,
) {
    let mut has_where = false;
    let mut push_conjunction = |qb: &mut sqlx::QueryBuilder<'args, crate::db::Db>| {
        if has_where {
            qb.push(" AND ");
        } else {
            qb.push(" WHERE ");
            has_where = true;
        }
    };

    if let Some(ref level) = filter.level {
        push_conjunction(qb);
        qb.push("level = ");
        qb.push_bind(level.as_str());
    }
    if let Some(project_id) = filter.project_id {
        push_conjunction(qb);
        qb.push("project_id = ");
        qb.push_bind(project_id as i64);
    }
    if let Some(ref query) = filter.query {
        let escaped = query
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        push_conjunction(qb);
        qb.push("title LIKE ");
        qb.push_bind(format!("%{escaped}%"));
        qb.push(" ESCAPE '\\'");
    }
    if let Some(ref item_type) = filter.item_type {
        push_conjunction(qb);
        qb.push("item_type = ");
        qb.push_bind(item_type.as_str());
    }
}

/// List events across all projects -- filters and pagination are optional.
pub async fn list_all_events(
    pool: &crate::db::DbPool,
    filter: &EventFilter,
    page: &Page,
) -> Result<PagedResult<EventSummary>> {
    use sqlx::QueryBuilder;

    let sort_col = match filter.sort.as_deref() {
        Some("project_id") => "project_id DESC, timestamp DESC",
        Some("level") => "level ASC, timestamp DESC",
        Some("platform") => "platform ASC, timestamp DESC",
        _ => "timestamp DESC",
    };

    // COUNT query
    let mut count_qb: QueryBuilder<'_, crate::db::Db> =
        QueryBuilder::new("SELECT COUNT(*) FROM events");
    push_event_filter_conditions(&mut count_qb, filter);

    let total: i64 = count_qb.build_query_scalar().fetch_one(pool).await?;

    // SELECT with ORDER BY + pagination
    let mut select_qb: QueryBuilder<'_, crate::db::Db> = QueryBuilder::new(
        "SELECT event_id, item_type, project_id, fingerprint, timestamp, level, title, platform, release, environment FROM events",
    );
    push_event_filter_conditions(&mut select_qb, filter);
    select_qb.push(" ORDER BY ");
    select_qb.push(sort_col);
    select_qb.push(" LIMIT ");
    select_qb.push_bind(page.limit as i64);
    select_qb.push(" OFFSET ");
    select_qb.push_bind(page.offset as i64);

    let rows = select_qb.build().fetch_all(pool).await?;
    let items: Vec<EventSummary> = rows
        .iter()
        .map(map_event_summary)
        .collect::<Result<Vec<_>>>()?;

    Ok(PagedResult {
        items,
        total: total as u64,
        offset: page.offset,
        limit: page.limit,
    })
}

/// List events for a single project, paginated.
pub async fn list_events(
    pool: &crate::db::DbPool,
    project_id: u64,
    page: &Page,
) -> Result<PagedResult<EventSummary>> {
    let total: i64 = sqlx::query(sql!("SELECT COUNT(*) FROM events WHERE project_id = ?1"))
        .bind(project_id as i64)
        .fetch_one(pool)
        .await?
        .get::<i64, _>(0);

    let rows = sqlx::query(sql!(
        "SELECT event_id, item_type, project_id, fingerprint, timestamp, level, title, platform, release, environment
         FROM events WHERE project_id = ?1
         ORDER BY timestamp DESC
         LIMIT ?2 OFFSET ?3"
    ))
    .bind(project_id as i64)
    .bind(page.limit as i64)
    .bind(page.offset as i64)
    .fetch_all(pool)
    .await?;

    let items: Vec<EventSummary> = rows
        .iter()
        .map(map_event_summary)
        .collect::<Result<Vec<_>>>()?;

    Ok(PagedResult {
        items,
        total: total as u64,
        offset: page.offset,
        limit: page.limit,
    })
}

/// All events for a given issue fingerprint, paginated.
pub async fn list_events_for_issue(
    pool: &crate::db::DbPool,
    fingerprint: &str,
    page: &Page,
) -> Result<PagedResult<EventSummary>> {
    let total: i64 = sqlx::query(sql!("SELECT COUNT(*) FROM events WHERE fingerprint = ?1"))
        .bind(fingerprint)
        .fetch_one(pool)
        .await?
        .get::<i64, _>(0);

    let rows = sqlx::query(sql!(
        "SELECT event_id, item_type, project_id, fingerprint, timestamp, level, title, platform, release, environment
         FROM events WHERE fingerprint = ?1
         ORDER BY timestamp DESC
         LIMIT ?2 OFFSET ?3"
    ))
    .bind(fingerprint)
    .bind(page.limit as i64)
    .bind(page.offset as i64)
    .fetch_all(pool)
    .await?;

    let items: Vec<EventSummary> = rows
        .iter()
        .map(map_event_summary)
        .collect::<Result<Vec<_>>>()?;

    Ok(PagedResult {
        items,
        total: total as u64,
        offset: page.offset,
        limit: page.limit,
    })
}

/// Bucket event counts by day for an issue's histogram.
/// Returns (date_label, count) pairs in chronological order.
pub async fn event_histogram(
    pool: &crate::db::DbPool,
    fingerprint: &str,
    days: u32,
) -> Result<Vec<(String, f32)>> {
    let now = chrono::Utc::now();
    let start = now - chrono::Duration::days(days as i64);
    let start_ts = start.timestamp();

    let rows = sqlx::query(sql!(
        "SELECT CAST((timestamp - ?1) / 86400 AS INTEGER) AS bucket, COUNT(*)
         FROM events
         WHERE fingerprint = ?2 AND timestamp >= ?1
         GROUP BY bucket
         ORDER BY bucket"
    ))
    .bind(start_ts)
    .bind(fingerprint)
    .fetch_all(pool)
    .await?;

    let mut counts = std::collections::HashMap::new();
    for row in &rows {
        let bucket: i64 = row.get(0);
        let count: i64 = row.get(1);
        counts.insert(bucket, count as f32);
    }

    let mut buckets = Vec::with_capacity(days as usize);
    for i in 0..days as i64 {
        let day = start + chrono::Duration::days(i);
        let label = day.format("%b %d").to_string();
        let count = counts.get(&i).copied().unwrap_or(0.0);
        buckets.push((label, count));
    }

    Ok(buckets)
}

/// Bucket event counts for a project's issue list histogram.
/// Adapts bucket size to the period: hourly for <=24h, daily otherwise.
pub async fn project_event_histogram(
    pool: &crate::db::DbPool,
    project_id: u64,
    item_type: &str,
    period: &str,
) -> Result<Vec<(String, f32)>> {
    let now = chrono::Utc::now();

    let (bucket_secs, bucket_count, fmt) = match period {
        "1h" => (300i64, 12usize, "%H:%M"), // 5-min buckets
        "24h" => (3600, 24, "%H:%M"),       // hourly
        "7d" => (86400, 7, "%b %d"),
        "14d" => (86400, 14, "%b %d"),
        "30d" => (86400, 30, "%b %d"),
        "90d" => (86400, 90, "%b %d"),
        "365d" => (86400 * 7, 52, "%b %d"), // weekly buckets
        _ => return Ok(Vec::new()),         // "all time" — skip chart
    };

    let start = now - chrono::Duration::seconds(bucket_secs * bucket_count as i64);
    let start_ts = start.timestamp();

    let sql_str = format!(
        "SELECT CAST((timestamp - ?1) / {bucket_secs} AS INTEGER) AS bucket, COUNT(*)
         FROM events
         WHERE project_id = ?2 AND item_type = ?3 AND timestamp >= ?1
         GROUP BY bucket
         ORDER BY bucket"
    );
    let sql_str = crate::db::translate_sql(&sql_str);

    let rows = sqlx::query(&sql_str)
        .bind(start_ts)
        .bind(project_id as i64)
        .bind(item_type)
        .fetch_all(pool)
        .await?;

    let mut counts = std::collections::HashMap::new();
    for row in &rows {
        let bucket: i64 = row.get(0);
        let count: i64 = row.get(1);
        counts.insert(bucket, count as f32);
    }

    let mut buckets = Vec::with_capacity(bucket_count);
    for i in 0..bucket_count as i64 {
        let t = start + chrono::Duration::seconds(bucket_secs * i);
        let label = t.format(fmt).to_string();
        let count = counts.get(&i).copied().unwrap_or(0.0);
        buckets.push((label, count));
    }

    Ok(buckets)
}

/// Grab the most recent event for an issue.
pub async fn get_latest_event_for_issue(
    pool: &crate::db::DbPool,
    fingerprint: &str,
) -> Result<Option<EventDetail>> {
    let row = sqlx::query(sql!(
        "SELECT event_id, item_type, project_id, fingerprint, timestamp, level, title, platform, release, environment, server_name, transaction_name, sdk_name, sdk_version, received_at, payload
         FROM events WHERE fingerprint = ?1
         ORDER BY timestamp DESC
         LIMIT 1"
    ))
    .bind(fingerprint)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(row) => {
            let (detail, blob) = map_event_detail_row(&row)?;
            let payload = decompress_payload(&blob)?;
            Ok(Some(EventDetail { payload, ..detail }))
        }
        None => Ok(None),
    }
}

/// Full event detail by ID -- decompresses the zstd payload and parses it as JSON.
pub async fn get_event_detail(
    pool: &crate::db::DbPool,
    event_id: &str,
) -> Result<Option<EventDetail>> {
    let row = sqlx::query(sql!(
        "SELECT event_id, item_type, project_id, fingerprint, timestamp, level, title, platform, release, environment, server_name, transaction_name, sdk_name, sdk_version, received_at, payload
         FROM events WHERE event_id = ?1"
    ))
    .bind(event_id)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(row) => {
            let (detail, blob) = map_event_detail_row(&row)?;
            let payload = decompress_payload(&blob)?;
            Ok(Some(EventDetail { payload, ..detail }))
        }
        None => Ok(None),
    }
}

/// Tail events newer than a given `received_at` timestamp, chronological order.
pub async fn tail_events(
    pool: &crate::db::DbPool,
    after_received_at: i64,
) -> Result<Vec<TailEvent>> {
    let rows = sqlx::query(sql!(
        "SELECT item_type, project_id, timestamp, level, title, received_at
         FROM events WHERE received_at > ?1 ORDER BY received_at ASC LIMIT 1000"
    ))
    .bind(after_received_at)
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(|row| {
            Ok(TailEvent {
                item_type: row.get::<String, _>("item_type"),
                project_id: row.get::<i64, _>("project_id") as u64,
                timestamp: row.get::<i64, _>("timestamp"),
                level: row.get::<Option<String>, _>("level"),
                title: row.get::<Option<String>, _>("title"),
                received_at: row.get::<i64, _>("received_at"),
            })
        })
        .collect()
}

/// Tag facets for an issue -- grouped by key, top 5 values each.
pub async fn get_tag_facets(pool: &crate::db::DbPool, fingerprint: &str) -> Result<Vec<TagFacet>> {
    // Pull all rows sorted by key, then count desc -- we'll group them in a single pass
    let rows = sqlx::query(sql!(
        "SELECT tag_key, tag_value, count
         FROM issue_tag_values
         WHERE fingerprint = ?1
         ORDER BY tag_key, count DESC
         LIMIT 1000"
    ))
    .bind(fingerprint)
    .fetch_all(pool)
    .await?;

    let mut facets: Vec<TagFacet> = Vec::new();
    let mut current_key: Option<String> = None;

    for row in &rows {
        let key: String = row.get("tag_key");
        let value: String = row.get("tag_value");
        let count: i64 = row.get("count");
        let count = count as u64;

        if current_key.as_deref() != Some(&key) {
            facets.push(TagFacet {
                key: key.clone(),
                top_values: Vec::new(),
                total_count: 0,
            });
            current_key = Some(key);
        }

        let facet = facets.last_mut().unwrap();
        facet.total_count += count;
        if facet.top_values.len() < 5 {
            facet.top_values.push(TagFacetValue { value, count });
        }
    }

    Ok(facets)
}

/// Max decompressed payload size (16 MB) -- prevents decompression bombs
const MAX_DECOMPRESSED_SIZE: u64 = 16 * 1024 * 1024;

/// Decompress a zstd blob and parse it as JSON.
///
/// Falls back to parsing the raw bytes as JSON if zstd decompression fails,
/// since payloads may be stored uncompressed when compression fails on the
/// write path.
pub(crate) fn decompress_payload(blob: &[u8]) -> Result<serde_json::Value> {
    if let Ok(mut decoder) = zstd::Decoder::new(blob) {
        let mut decompressed = Vec::new();
        if std::io::Read::read_to_end(
            &mut std::io::Read::take(&mut decoder, MAX_DECOMPRESSED_SIZE + 1),
            &mut decompressed,
        )
        .is_ok()
        {
            if decompressed.len() as u64 > MAX_DECOMPRESSED_SIZE {
                anyhow::bail!("decompressed payload exceeds {MAX_DECOMPRESSED_SIZE} byte limit");
            }
            let value: serde_json::Value = serde_json::from_slice(&decompressed)
                .context("Failed to parse decompressed payload as JSON")?;
            return Ok(value);
        }
    }

    // Payload wasn't zstd-compressed -- try parsing the raw bytes as JSON
    let value: serde_json::Value =
        serde_json::from_slice(blob).context("Payload is neither valid zstd nor valid JSON")?;
    Ok(value)
}

fn map_event_summary(row: &crate::db::DbRow) -> Result<EventSummary> {
    let item_type_str: String = row.get("item_type");
    Ok(EventSummary {
        event_id: row.get("event_id"),
        item_type: item_type_str.parse().unwrap_or_default(),
        project_id: row.get::<i64, _>("project_id") as u64,
        fingerprint: row.get("fingerprint"),
        timestamp: row.get("timestamp"),
        level: row.get("level"),
        title: row.get("title"),
        platform: row.get("platform"),
        release: row.get("release"),
        environment: row.get("environment"),
    })
}

/// Maps a row to EventDetail but keeps the raw blob separate -- the caller
/// handles decompression. Payload field is a placeholder null until then.
fn map_event_detail_row(row: &crate::db::DbRow) -> Result<(EventDetail, Vec<u8>)> {
    let blob: Vec<u8> = row.get("payload");
    let item_type_str: String = row.get("item_type");
    Ok((
        EventDetail {
            event_id: row.get("event_id"),
            item_type: item_type_str.parse().unwrap_or_default(),
            project_id: row.get::<i64, _>("project_id") as u64,
            fingerprint: row.get("fingerprint"),
            timestamp: row.get("timestamp"),
            level: row.get("level"),
            title: row.get("title"),
            platform: row.get("platform"),
            release: row.get("release"),
            environment: row.get("environment"),
            server_name: row.get("server_name"),
            transaction_name: row.get("transaction_name"),
            sdk_name: row.get("sdk_name"),
            sdk_version: row.get("sdk_version"),
            received_at: row.get("received_at"),
            payload: serde_json::Value::Null, // caller fills this in after decompression
        },
        blob,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::test_helpers::*;

    #[tokio::test]
    async fn list_events_empty() {
        let pool = open_test_db().await;
        let page = Page::new(None, None);
        let result = list_events(&pool, 1, &page).await.unwrap();
        assert!(result.items.is_empty());
        assert_eq!(result.total, 0);
    }

    #[tokio::test]
    async fn list_events_basic() {
        let pool = open_test_db().await;
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
            Some("Error B"),
        )
        .await;
        insert_test_event(
            &pool,
            "e3",
            2,
            150,
            Some("fp2"),
            Some("warning"),
            Some("Warn C"),
        )
        .await;

        let page = Page::new(None, None);
        let result = list_events(&pool, 1, &page).await.unwrap();
        assert_eq!(result.total, 2);
        assert_eq!(result.items.len(), 2);
        // Newest first
        assert_eq!(result.items[0].event_id, "e2");
        assert_eq!(result.items[1].event_id, "e1");
    }

    #[tokio::test]
    async fn list_events_pagination() {
        let pool = open_test_db().await;
        for i in 0..10 {
            insert_test_event(
                &pool,
                &format!("e{i}"),
                1,
                100 + i,
                Some("fp1"),
                Some("error"),
                Some(&format!("Event {i}")),
            )
            .await;
        }

        // First page
        let page = Page::new(Some(0), Some(3));
        let result = list_events(&pool, 1, &page).await.unwrap();
        assert_eq!(result.total, 10);
        assert_eq!(result.items.len(), 3);
        assert!(result.has_next());

        // Middle page
        let page = Page::new(Some(3), Some(3));
        let result = list_events(&pool, 1, &page).await.unwrap();
        assert_eq!(result.items.len(), 3);
        assert!(result.has_next());
        assert!(result.has_prev());

        // Last partial page
        let page = Page::new(Some(9), Some(3));
        let result = list_events(&pool, 1, &page).await.unwrap();
        assert_eq!(result.items.len(), 1);
        assert!(!result.has_next());
    }

    #[tokio::test]
    async fn list_events_for_issue_basic() {
        let pool = open_test_db().await;
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
            Some("Error A again"),
        )
        .await;
        insert_test_event(
            &pool,
            "e3",
            1,
            150,
            Some("fp2"),
            Some("error"),
            Some("Different issue"),
        )
        .await;

        let page = Page::new(None, None);
        let result = list_events_for_issue(&pool, "fp1", &page).await.unwrap();
        assert_eq!(result.total, 2);
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.items[0].event_id, "e2");
        assert_eq!(result.items[1].event_id, "e1");
    }

    #[tokio::test]
    async fn list_events_for_issue_empty() {
        let pool = open_test_db().await;
        let page = Page::new(None, None);
        let result = list_events_for_issue(&pool, "nonexistent", &page)
            .await
            .unwrap();
        assert!(result.items.is_empty());
        assert_eq!(result.total, 0);
    }

    #[tokio::test]
    async fn get_event_detail_found() {
        let pool = open_test_db().await;
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

        let detail = get_event_detail(&pool, "e1").await.unwrap().unwrap();
        assert_eq!(detail.event_id, "e1");
        assert_eq!(detail.project_id, 1);
        assert_eq!(detail.level.as_deref(), Some("error"));
        assert_eq!(detail.title.as_deref(), Some("Error A"));
        assert_eq!(detail.platform.as_deref(), Some("rust"));
        assert_eq!(detail.release.as_deref(), Some("v1.0"));
        assert_eq!(detail.environment.as_deref(), Some("production"));
        assert_eq!(detail.server_name.as_deref(), Some("server1"));
        assert_eq!(detail.sdk_name.as_deref(), Some("sentry.rust"));
        assert_eq!(detail.sdk_version.as_deref(), Some("0.1.0"));
        assert_eq!(detail.fingerprint.as_deref(), Some("fp1"));
        // Payload should be valid JSON
        assert!(detail.payload.is_object());
        assert_eq!(detail.payload["event_id"], "e1");
    }

    #[tokio::test]
    async fn get_event_detail_not_found() {
        let pool = open_test_db().await;
        assert!(get_event_detail(&pool, "nonexistent")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn get_latest_event_for_issue_found() {
        let pool = open_test_db().await;
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
            Some("Error A later"),
        )
        .await;
        insert_test_event(
            &pool,
            "e3",
            1,
            300,
            Some("fp2"),
            Some("error"),
            Some("Different"),
        )
        .await;

        let latest = get_latest_event_for_issue(&pool, "fp1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(latest.event_id, "e2");
        assert_eq!(latest.timestamp, 200);
        assert!(latest.payload.is_object());
    }

    #[tokio::test]
    async fn get_latest_event_for_issue_not_found() {
        let pool = open_test_db().await;
        assert!(get_latest_event_for_issue(&pool, "nonexistent")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn get_event_detail_bad_payload() {
        let pool = open_test_db().await;
        // Shove in a garbage payload to make sure decompression errors surface
        sqlx::query(sql!(
            "INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp, received_at)
             VALUES ('bad', 'event', ?1, 1, 'testkey', 100, 100)"
        ))
        .bind([0xDEu8, 0xAD, 0xBE, 0xEF].as_slice())
        .execute(&pool)
        .await
        .unwrap();

        let result = get_event_detail(&pool, "bad").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("neither valid zstd nor valid JSON"));
    }
}
