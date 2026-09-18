use anyhow::{Context, Result};
use sqlx::Row;

use crate::db::sql;
use crate::db::DbRowExt;

use super::types::{
    EventDetail, EventFilter, EventSummary, IssueFilter, Page, PagedResult, TagFacet,
    TagFacetValue, TailEvent,
};

/// Closed set of allowed ORDER BY clauses, so the value reaching
/// `QueryBuilder::push` is provably a `&'static str` chosen here.
enum EventSort {
    ProjectId,
    Level,
    Platform,
    Timestamp,
}

impl EventSort {
    fn parse(sort: Option<&str>) -> Self {
        match sort {
            Some("project_id") => Self::ProjectId,
            Some("level") => Self::Level,
            Some("platform") => Self::Platform,
            _ => Self::Timestamp,
        }
    }

    fn as_sql_ident(&self) -> &'static str {
        match self {
            Self::ProjectId => "events.project_id DESC, events.timestamp DESC",
            Self::Level => "events.level ASC, events.timestamp DESC",
            Self::Platform => "events.platform ASC, events.timestamp DESC",
            Self::Timestamp => "events.timestamp DESC",
        }
    }
}

/// Append event filter conditions and their binds to an in-progress QueryBuilder.
/// Caller must have already pushed the base query (e.g. `SELECT ... FROM events`).
/// If any filters are active, a `WHERE` keyword is emitted first.
fn push_event_filter_conditions(
    qb: &mut sqlx::QueryBuilder<crate::db::Db>,
    filter: &EventFilter,
    org_ids: Option<&[i64]>,
) {
    let mut has_where = false;
    let mut push_conjunction = |qb: &mut sqlx::QueryBuilder<crate::db::Db>| {
        if has_where {
            qb.push(" AND ");
        } else {
            qb.push(" WHERE ");
            has_where = true;
        }
    };

    if let Some(ref level) = filter.level {
        push_conjunction(qb);
        qb.push("events.level = ");
        qb.push_bind(level.as_str());
    }
    if let Some(project_id) = filter.project_id {
        push_conjunction(qb);
        qb.push("events.project_id = ");
        qb.push_bind(project_id as i64);
    }
    if let Some(ref query) = filter.query {
        push_conjunction(qb);
        // A pasted trace id is still a search term, so it widens the predicate
        // rather than replacing it; anything else never looks like one.
        let as_trace = super::trace_id_candidate(query);
        if as_trace.is_some() {
            qb.push("(");
        }
        qb.push("events.title LIKE ");
        qb.push_bind(super::like_contains(query));
        qb.push(" ESCAPE '\\'");
        if let Some(ref trace_id) = as_trace {
            qb.push(" OR ");
            super::push_trace_id_predicate(qb, "events.trace_id", trace_id);
            qb.push(")");
        }
    }
    if let Some(ref trace_id) = filter.trace_id {
        push_conjunction(qb);
        super::push_trace_id_predicate(qb, "events.trace_id", trace_id);
    }
    if let Some(ref item_type) = filter.item_type {
        push_conjunction(qb);
        qb.push("events.item_type = ");
        qb.push_bind(item_type.as_str());
    }
    if let Some(since_ts) = filter.since_ts {
        push_conjunction(qb);
        qb.push("events.timestamp >= ");
        qb.push_bind(since_ts);
    }
    if let Some(ids) = org_ids {
        push_conjunction(qb);
        super::push_org_scope_predicate(qb, "events.project_id", ids);
    }
}

/// List events across all projects -- filters and pagination are optional.
/// Pass `org_id = Some(id)` to scope to that org; `None` returns all (superuser).
pub async fn list_all_events(
    pool: &crate::db::DbPool,
    filter: &EventFilter,
    page: &Page,
    org_id: Option<i64>,
) -> Result<PagedResult<EventSummary>> {
    let one: Option<Vec<i64>> = org_id.map(|id| vec![id]);
    list_all_events_inner(pool, filter, page, one.as_deref()).await
}

/// List events across every org the caller belongs to. An empty list entitles
/// the caller to nothing, which is not the same as the superuser's "all orgs".
pub async fn list_all_events_for_orgs(
    pool: &crate::db::DbPool,
    filter: &EventFilter,
    page: &Page,
    org_ids: Vec<i64>,
) -> Result<PagedResult<EventSummary>> {
    let ids = super::canonical_org_ids(org_ids);
    list_all_events_inner(pool, filter, page, Some(&ids)).await
}

async fn list_all_events_inner(
    pool: &crate::db::DbPool,
    filter: &EventFilter,
    page: &Page,
    org_ids: Option<&[i64]>,
) -> Result<PagedResult<EventSummary>> {
    use sqlx::QueryBuilder;

    // `IN ()` is not valid SQL on either backend, so an empty scope has to
    // short-circuit rather than fall through to an unscoped query.
    if org_ids.is_some_and(<[i64]>::is_empty) {
        return Ok(PagedResult::from_page(Vec::new(), 0, page));
    }

    let sort = EventSort::parse(filter.sort.as_deref());

    let mut count_qb: QueryBuilder<crate::db::Db> =
        QueryBuilder::new("SELECT COUNT(*) FROM events");
    push_event_filter_conditions(&mut count_qb, filter, org_ids);

    let total: i64 = count_qb.build_query_scalar().fetch_one(pool).await?;

    // Join projects so the firehose shows names instead of bare numeric ids.
    let mut select_qb: QueryBuilder<crate::db::Db> = QueryBuilder::new(
        "SELECT events.event_id, events.item_type, events.project_id, p.name AS project_name, \
         events.fingerprint, events.timestamp, events.level, events.title, events.platform, \
         events.release, events.environment \
         FROM events LEFT JOIN projects p ON p.project_id = events.project_id",
    );
    push_event_filter_conditions(&mut select_qb, filter, org_ids);
    select_qb.push(" ORDER BY ");
    select_qb.push(sort.as_sql_ident());
    select_qb.push(" LIMIT ");
    select_qb.push_bind(page.limit as i64);
    select_qb.push(" OFFSET ");
    select_qb.push_bind(page.offset as i64);

    let rows = select_qb.build().fetch_all(pool).await?;
    let items: Vec<EventSummary> = rows
        .iter()
        .map(map_event_summary)
        .collect::<Result<Vec<_>>>()?;

    Ok(PagedResult::from_page(items, total, page))
}

/// Resolve a full or partial trace id to the project and trace it belongs to.
/// `None` when nothing matches, and also when a prefix spans more than one
/// trace: a lookup that could land on either is not a lookup.
///
/// `project_id` narrows to one project; `org_ids` is the same scope the event
/// list uses, so a lookup can never point at a project the caller cannot open.
pub async fn resolve_trace_id(
    pool: &crate::db::DbPool,
    trace_id: &str,
    project_id: Option<u64>,
    org_ids: Option<&[i64]>,
) -> Result<Option<(u64, String)>> {
    use sqlx::QueryBuilder;

    if org_ids.is_some_and(<[i64]>::is_empty) {
        return Ok(None);
    }

    // Grouped by trace, not by (trace, project): a distributed trace lives in
    // several projects at once, and landing on the lowest-numbered one is a
    // lookup. Only a prefix matching two *traces* is ambiguous.
    let mut qb: QueryBuilder<crate::db::Db> = QueryBuilder::new(
        "SELECT events.trace_id, MIN(events.project_id) AS project_id FROM events WHERE ",
    );
    super::push_trace_id_predicate(&mut qb, "events.trace_id", trace_id);
    if let Some(pid) = project_id {
        qb.push(" AND events.project_id = ");
        qb.push_bind(pid as i64);
    }
    if let Some(ids) = org_ids {
        qb.push(" AND ");
        super::push_org_scope_predicate(&mut qb, "events.project_id", ids);
    }
    qb.push(" GROUP BY events.trace_id LIMIT 2");

    let rows = qb.build().fetch_all(pool).await?;
    let [row] = rows.as_slice() else {
        return Ok(None);
    };
    let Some(resolved) = row.get_opt_string("trace_id") else {
        return Ok(None);
    };
    Ok(Some((row.get_u64("project_id"), resolved)))
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

    Ok(PagedResult::from_page(items, total, page))
}

/// All events for a given issue, paginated. The total is the issue row's
/// `event_count`, which the writer maintains and every delete path recomputes;
/// it can trail the live table by one aggregation flush.
pub async fn list_events_for_issue(
    pool: &crate::db::DbPool,
    project_id: u64,
    fingerprint: &str,
    page: &Page,
) -> Result<PagedResult<EventSummary>> {
    let total: i64 = sqlx::query(sql!(
        "SELECT event_count FROM issues WHERE project_id = ?1 AND fingerprint = ?2"
    ))
    .bind(project_id as i64)
    .bind(fingerprint)
    .fetch_optional(pool)
    .await?
    .map(|r| r.get::<i64, _>(0))
    .unwrap_or(0);

    let rows = sqlx::query(sql!(
        "SELECT event_id, item_type, project_id, fingerprint, timestamp, level, title, platform, release, environment
         FROM events WHERE project_id = ?1 AND fingerprint = ?2
         ORDER BY timestamp DESC
         LIMIT ?3 OFFSET ?4"
    ))
    .bind(project_id as i64)
    .bind(fingerprint)
    .bind(page.limit as i64)
    .bind(page.offset as i64)
    .fetch_all(pool)
    .await?;

    let items: Vec<EventSummary> = rows
        .iter()
        .map(map_event_summary)
        .collect::<Result<Vec<_>>>()?;

    Ok(PagedResult::from_page(items, total, page))
}

/// Bucket event counts by day for an issue's histogram.
/// Returns (date_label, count) pairs in chronological order.
pub async fn event_histogram(
    pool: &crate::db::DbPool,
    project_id: u64,
    fingerprint: &str,
    days: u32,
) -> Result<Vec<(String, f32)>> {
    let now = chrono::Utc::now();
    let start = now - chrono::Duration::days(days as i64);
    let start_ts = start.timestamp();

    let rows = sqlx::query(sql!(
        "SELECT CAST((timestamp - ?1) / 86400 AS BIGINT) AS bucket, COUNT(*)
         FROM events
         WHERE project_id = ?2 AND fingerprint = ?3 AND timestamp >= ?1
         GROUP BY bucket
         ORDER BY bucket"
    ))
    .bind(start_ts)
    .bind(project_id as i64)
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

/// Start instant of the first histogram bucket.
///
/// Daily and weekly buckets floor to UTC midnight (to the start of the ISO week for
/// weekly), the same anchoring release health uses. Anchored to raw `now` instead,
/// the final bucket spans e.g. `[Aug 09 23:23, Aug 10 23:23)` — it holds today's
/// events but carries yesterday's label. Sub-day periods are genuinely rolling
/// windows and stay anchored to `now`.
///
/// `now` is an argument rather than read inline so the boundary is testable at a
/// fixed clock instead of only when CI happens to run near midnight.
fn histogram_start(
    now: chrono::DateTime<chrono::Utc>,
    bucket_secs: i64,
    bucket_count: usize,
) -> chrono::DateTime<chrono::Utc> {
    use chrono::{Datelike, Duration, NaiveTime};

    let span = Duration::seconds(bucket_secs * bucket_count as i64);
    if bucket_secs < 86400 {
        return now - span;
    }

    let day = now.date_naive();
    let anchor = if bucket_secs >= 86400 * 7 {
        day - Duration::days(day.weekday().num_days_from_monday() as i64)
    } else {
        day
    };
    // The current day/week is the last full bucket, so the window ends where it ends.
    let end = anchor.and_time(NaiveTime::MIN).and_utc() + Duration::seconds(bucket_secs);
    end - span
}

/// Bucket event counts for a project's issue list histogram, narrowed by the same
/// filter as the list underneath it so chart and table always agree.
/// Adapts bucket size to the period: hourly for <=24h, daily otherwise.
pub async fn project_event_histogram(
    pool: &crate::db::DbPool,
    project_id: u64,
    filter: &IssueFilter,
    period: &str,
) -> Result<Vec<(String, f32)>> {
    use sqlx::QueryBuilder;

    let now = chrono::Utc::now();

    let (bucket_secs, bucket_count, fmt) = match period {
        "1h" => (300i64, 12usize, "%H:%M"), // 5-min buckets
        "24h" => (3600, 24, "%H:%M"),       // hourly
        "7d" => (86400, 7, "%b %d"),
        "14d" => (86400, 14, "%b %d"),
        "30d" => (86400, 30, "%b %d"),
        "90d" => (86400, 90, "%b %d"),
        "365d" => (86400 * 7, 52, "%b %d"), // weekly buckets
        _ => return Ok(Vec::new()),         // "all time": skip chart
    };

    let start = histogram_start(now, bucket_secs, bucket_count);
    let start_ts = start.timestamp();

    // bucket_secs is a server-computed integer, safe to inline; everything
    // caller-influenced is bound.
    let mut qb: QueryBuilder<crate::db::Db> = QueryBuilder::new("SELECT CAST((timestamp - ");
    qb.push_bind(start_ts);
    qb.push(format!(
        ") / {bucket_secs} AS BIGINT) AS bucket, COUNT(*) FROM events WHERE project_id = "
    ));
    qb.push_bind(project_id as i64);
    qb.push(" AND timestamp >= ");
    qb.push_bind(start_ts);
    push_issue_filter_on_events(&mut qb, filter);
    qb.push(" GROUP BY bucket ORDER BY bucket");

    let rows = qb.build().fetch_all(pool).await?;

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

/// Translate an [`IssueFilter`] onto the `events` table for the histogram.
/// `level`, `title` and `release` live on the event row itself; `status` and the
/// tag facet only exist per issue, so they go through the fingerprint.
fn push_issue_filter_on_events(qb: &mut sqlx::QueryBuilder<crate::db::Db>, filter: &IssueFilter) {
    if let Some(ref item_type) = filter.item_type {
        qb.push(" AND item_type = ");
        qb.push_bind(item_type.as_str());
    }
    if let Some(ref level) = filter.level {
        qb.push(" AND level = ");
        qb.push_bind(level.as_str());
    }
    if let Some(ref query) = filter.query {
        qb.push(" AND title LIKE ");
        qb.push_bind(super::like_contains(query));
        qb.push(" ESCAPE '\\'");
    }
    if let Some(ref release) = filter.release {
        qb.push(" AND release = ");
        qb.push_bind(release.as_str());
    }
    if let Some(ref environment) = filter.environment {
        qb.push(" AND environment = ");
        qb.push_bind(environment.as_str());
    }
    if let Some(ref status) = filter.status {
        qb.push(" AND EXISTS (SELECT 1 FROM issues i WHERE i.fingerprint = events.fingerprint AND i.project_id = events.project_id AND i.status = ");
        qb.push_bind(status.as_str());
        qb.push(")");
    }
    if let Some((ref key, ref value)) = filter.tag {
        qb.push(" AND EXISTS (SELECT 1 FROM issue_tag_values itv WHERE itv.project_id = events.project_id AND itv.fingerprint = events.fingerprint AND itv.tag_key =");
        qb.push_bind(key.as_str());
        qb.push(" AND itv.tag_value = ");
        qb.push_bind(value.as_str());
        qb.push(")");
    }
}

/// Distinct environments seen in a project, for the issue-stream filter dropdown.
/// Mirrors `list_releases_for_project`: newest-looking first, capped, blanks dropped.
pub async fn list_environments_for_project(
    pool: &crate::db::DbPool,
    project_id: u64,
) -> Result<Vec<String>> {
    let rows = sqlx::query(sql!(
        "SELECT DISTINCT environment FROM events
         WHERE project_id = ?1 AND environment IS NOT NULL AND environment <> ''
         ORDER BY environment
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

/// Grab the most recent event for an issue.
pub async fn get_latest_event_for_issue(
    pool: &crate::db::DbPool,
    project_id: u64,
    fingerprint: &str,
) -> Result<Option<EventDetail>> {
    let row = sqlx::query(sql!(
        "SELECT event_id, item_type, project_id, fingerprint, timestamp, level, title, platform, release, environment, server_name, transaction_name, sdk_name, sdk_version, received_at, payload
         FROM events WHERE project_id = ?1 AND fingerprint = ?2
         ORDER BY timestamp DESC
         LIMIT 1"
    ))
    .bind(project_id as i64)
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
                project_id: row.get_u64("project_id"),
                timestamp: row.get::<i64, _>("timestamp"),
                level: row.get_opt_string("level"),
                title: row.get_opt_string("title"),
                received_at: row.get::<i64, _>("received_at"),
            })
        })
        .collect()
}

/// Tag facets for an issue -- grouped by key, top 5 values each.
pub async fn get_tag_facets(
    pool: &crate::db::DbPool,
    project_id: u64,
    fingerprint: &str,
) -> Result<Vec<TagFacet>> {
    let rows = sqlx::query(sql!(
        "SELECT tag_key, tag_value, count
         FROM issue_tag_values
         WHERE project_id = ?1 AND fingerprint = ?2
         ORDER BY tag_key, count DESC
         LIMIT 1000"
    ))
    .bind(project_id as i64)
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
        project_id: row.get_u64("project_id"),
        project_name: row.try_get("project_name").ok().flatten(),
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
            project_id: row.get_u64("project_id"),
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
    use crate::db::sql;
    use crate::queries::test_helpers::*;
    use sqlx::Row;

    // The issue-list chart must move with the list filters, not just the period:
    // release, level and status each narrow the buckets.
    #[tokio::test]
    async fn project_event_histogram_applies_issue_filters() {
        let pool = open_test_db().await;
        let now = chrono::Utc::now().timestamp();

        for (event_id, fingerprint, level, release) in [
            ("e1", "fp-open", "error", "app@1.0"),
            ("e2", "fp-open", "warning", "app@1.0"),
            ("e3", "fp-done", "error", "app@2.0"),
        ] {
            sqlx::query(sql!(
                "INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp, level, title, release, received_at, fingerprint)
                 VALUES (?1, 'event', ?2, 1, 'testkey', ?3, ?4, 'Boom', ?5, ?3, ?6)"
            ))
            .bind(event_id)
            .bind(Vec::<u8>::new())
            .bind(now - 60)
            .bind(level)
            .bind(release)
            .bind(fingerprint)
            .execute(&pool)
            .await
            .unwrap();
        }
        insert_test_issue(&pool, "fp-open", 1, None, None, now, now, 2, "unresolved").await;
        insert_test_issue(&pool, "fp-done", 1, None, None, now, now, 1, "resolved").await;

        let total = |buckets: Vec<(String, f32)>| buckets.iter().map(|(_, c)| c).sum::<f32>();
        let filter = |f: fn(&mut IssueFilter)| {
            let mut filter = IssueFilter {
                item_type: Some("event".to_string()),
                ..Default::default()
            };
            f(&mut filter);
            filter
        };

        let unfiltered = filter(|_| {});
        assert_eq!(
            total(
                project_event_histogram(&pool, 1, &unfiltered, "24h")
                    .await
                    .unwrap()
            ),
            3.0
        );

        let by_release = filter(|f| f.release = Some("app@1.0".to_string()));
        assert_eq!(
            total(
                project_event_histogram(&pool, 1, &by_release, "24h")
                    .await
                    .unwrap()
            ),
            2.0
        );

        let by_level = filter(|f| f.level = Some("error".to_string()));
        assert_eq!(
            total(
                project_event_histogram(&pool, 1, &by_level, "24h")
                    .await
                    .unwrap()
            ),
            2.0
        );

        let by_status = filter(|f| f.status = Some("resolved".to_string()));
        assert_eq!(
            total(
                project_event_histogram(&pool, 1, &by_status, "24h")
                    .await
                    .unwrap()
            ),
            1.0
        );
    }

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

        insert_test_issue(
            &pool,
            "fp1",
            1,
            Some("Error A"),
            None,
            100,
            200,
            2,
            "unresolved",
        )
        .await;

        let page = Page::new(None, None);
        let result = list_events_for_issue(&pool, 1, "fp1", &page).await.unwrap();
        assert_eq!(result.total, 2);
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.items[0].event_id, "e2");
        assert_eq!(result.items[1].event_id, "e1");
    }

    // The total is the writer-maintained `issues.event_count`, not a live COUNT(*).
    #[tokio::test]
    async fn list_events_for_issue_total_comes_from_the_issue_row() {
        let pool = open_test_db().await;
        insert_test_event(&pool, "e1", 1, 100, Some("fp1"), Some("error"), None).await;
        insert_test_issue(&pool, "fp1", 1, None, None, 100, 100, 7, "unresolved").await;
        // Same fingerprint in another project must not leak into the total.
        insert_test_issue(&pool, "fp1", 2, None, None, 100, 100, 50, "unresolved").await;

        let result = list_events_for_issue(&pool, 1, "fp1", &Page::new(None, None))
            .await
            .unwrap();
        assert_eq!(result.total, 7);
        assert_eq!(result.items.len(), 1);
    }

    #[tokio::test]
    async fn list_events_for_issue_empty() {
        let pool = open_test_db().await;
        let page = Page::new(None, None);
        let result = list_events_for_issue(&pool, 1, "nonexistent", &page)
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

        let latest = get_latest_event_for_issue(&pool, 1, "fp1")
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
        assert!(get_latest_event_for_issue(&pool, 1, "nonexistent")
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

    async fn insert_org(pool: &crate::db::DbPool, slug: &str) -> i64 {
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

    async fn insert_project(pool: &crate::db::DbPool, project_id: i64, org_id: i64) {
        sqlx::query(sql!(
            "INSERT INTO projects (project_id, org_id) VALUES (?1, ?2)"
        ))
        .bind(project_id)
        .bind(org_id)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn list_all_events_org_scoped_returns_only_that_org() {
        let pool = open_test_db().await;
        let org_a = insert_org(&pool, "ev-org-a").await;
        let org_b = insert_org(&pool, "ev-org-b").await;
        insert_project(&pool, 101, org_a).await;
        insert_project(&pool, 102, org_b).await;
        insert_test_event(
            &pool,
            "ea1",
            101,
            100,
            Some("fpa"),
            Some("error"),
            Some("A"),
        )
        .await;
        insert_test_event(
            &pool,
            "eb1",
            102,
            200,
            Some("fpb"),
            Some("error"),
            Some("B"),
        )
        .await;

        let filter = EventFilter::default();
        let page = Page::new(None, None);

        let scoped = list_all_events(&pool, &filter, &page, Some(org_a))
            .await
            .unwrap();
        assert_eq!(scoped.total, 1);
        assert_eq!(scoped.items[0].event_id, "ea1");

        let all = list_all_events(&pool, &filter, &page, None).await.unwrap();
        assert_eq!(all.total, 2);
    }

    #[tokio::test]
    async fn list_all_events_org_b_scoped_excludes_org_a() {
        let pool = open_test_db().await;
        let org_a = insert_org(&pool, "ev2-org-a").await;
        let org_b = insert_org(&pool, "ev2-org-b").await;
        insert_project(&pool, 201, org_a).await;
        insert_project(&pool, 202, org_b).await;
        insert_test_event(
            &pool,
            "ec1",
            201,
            100,
            Some("fpc"),
            Some("error"),
            Some("C"),
        )
        .await;
        insert_test_event(
            &pool,
            "ec2",
            201,
            150,
            Some("fpc2"),
            Some("error"),
            Some("C2"),
        )
        .await;
        insert_test_event(
            &pool,
            "ed1",
            202,
            200,
            Some("fpd"),
            Some("error"),
            Some("D"),
        )
        .await;

        let filter = EventFilter::default();
        let page = Page::new(None, None);

        let scoped_b = list_all_events(&pool, &filter, &page, Some(org_b))
            .await
            .unwrap();
        assert_eq!(scoped_b.total, 1);
        assert_eq!(scoped_b.items[0].event_id, "ed1");
    }

    fn at(s: &str) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339(s).unwrap().to_utc()
    }

    // The defect: anchored to raw `now`, the last daily bucket spanned
    // [Aug 09 23:23, Aug 10 23:23) — it held today's events under yesterday's
    // label. Asserted at fixed instants because a real-clock assertion only
    // catches it if CI happens to run near UTC midnight.
    #[test]
    fn daily_histogram_buckets_floor_to_utc_midnight() {
        for now in ["2026-08-10T23:23:00Z", "2026-08-10T00:01:00Z"] {
            let start = histogram_start(at(now), 86400, 7);
            assert_eq!(
                start,
                at("2026-08-04T00:00:00Z"),
                "7d window from {now} must start at a UTC midnight"
            );
            // Last bucket is today, and carries today's label.
            let last = start + chrono::Duration::seconds(86400 * 6);
            assert_eq!(last, at("2026-08-10T00:00:00Z"));
            assert_eq!(last.format("%b %d").to_string(), "Aug 10");
        }

        // Same shape for the longer daily periods.
        assert_eq!(
            histogram_start(at("2026-08-10T23:23:00Z"), 86400, 30),
            at("2026-07-12T00:00:00Z")
        );
    }

    // 365d buckets weekly, so it floors to the start of the ISO week, not the day.
    #[test]
    fn weekly_histogram_buckets_floor_to_week_start() {
        // 2026-08-10 is a Monday; 2026-08-13 a Thursday. Both sit in the same week
        // and must produce the same window.
        let from_monday = histogram_start(at("2026-08-10T09:00:00Z"), 86400 * 7, 52);
        let from_thursday = histogram_start(at("2026-08-13T09:00:00Z"), 86400 * 7, 52);
        assert_eq!(from_monday, from_thursday);
        assert_eq!(from_monday, at("2025-08-18T00:00:00Z"));

        let last = from_monday + chrono::Duration::seconds(86400 * 7 * 51);
        assert_eq!(last, at("2026-08-10T00:00:00Z"), "last bucket is this week");
    }

    // 1h and 24h are genuinely rolling windows and must keep tracking `now`.
    #[test]
    fn intraday_histogram_buckets_stay_rolling() {
        let now = at("2026-08-10T23:23:00Z");
        assert_eq!(histogram_start(now, 3600, 24), at("2026-08-09T23:23:00Z"));
        assert_eq!(histogram_start(now, 300, 12), at("2026-08-10T22:23:00Z"));
    }

    // Three orgs, because a two-org fixture (or the single-org seed) passes even
    // with the cross-project page scoped to one org. Both directions asserted:
    // the caller's two orgs appear, the third does not.
    #[tokio::test]
    async fn list_all_events_for_orgs_spans_memberships_and_excludes_others() {
        let pool = open_test_db().await;
        let org_a = insert_org(&pool, "evm-org-a").await;
        let org_b = insert_org(&pool, "evm-org-b").await;
        let org_c = insert_org(&pool, "evm-org-c").await;
        insert_project(&pool, 301, org_a).await;
        insert_project(&pool, 302, org_b).await;
        insert_project(&pool, 303, org_c).await;
        for (id, project, ts) in [("ma1", 301, 100), ("mb1", 302, 200), ("mc1", 303, 300)] {
            insert_test_event(&pool, id, project, ts, Some(id), Some("error"), Some(id)).await;
        }

        let filter = EventFilter::default();
        let page = Page::new(None, None);

        let mine = list_all_events_for_orgs(&pool, &filter, &page, vec![org_a, org_b])
            .await
            .unwrap();
        let ids: Vec<&str> = mine.items.iter().map(|e| e.event_id.as_str()).collect();
        assert_eq!(mine.total, 2);
        assert!(ids.contains(&"ma1") && ids.contains(&"mb1"));
        assert!(
            !ids.contains(&"mc1"),
            "another org's events must not appear"
        );

        // An empty entitlement is *nothing*, not the superuser's "everything".
        let none = list_all_events_for_orgs(&pool, &filter, &page, vec![])
            .await
            .unwrap();
        assert_eq!(none.total, 0);
        assert!(none.items.is_empty());
    }

    // The per-item-type list pages (user reports, client reports) filter through
    // `EventFilter`, so the window has to bind in both the count and the page.
    #[tokio::test]
    async fn event_filter_since_ts_windows_count_and_page() {
        let pool = open_test_db().await;
        insert_test_event(&pool, "old", 1, 100, None, Some("error"), Some("A")).await;
        insert_test_event(&pool, "new", 1, 900, None, Some("error"), Some("B")).await;

        let page = Page::new(Some(0), Some(25));

        let all = list_all_events(&pool, &EventFilter::default(), &page, None)
            .await
            .unwrap();
        assert_eq!(all.total, 2);

        let filter = EventFilter {
            since_ts: Some(500),
            ..Default::default()
        };
        let windowed = list_all_events(&pool, &filter, &page, None).await.unwrap();
        assert_eq!(windowed.total, 1);
        assert_eq!(windowed.items[0].event_id, "new");
    }

    const TRACE_A: &str = "aaaaaaaabbbbbbbbccccccccdddddddd";
    const TRACE_B: &str = "aaaaaaaabbbbbbbbcccccccc11111111";

    async fn insert_traced_event(
        pool: &crate::db::DbPool,
        event_id: &str,
        project_id: i64,
        trace_id: &str,
        title: &str,
    ) {
        insert_test_event(
            pool,
            event_id,
            project_id,
            100,
            None,
            Some("error"),
            Some(title),
        )
        .await;
        sqlx::query(sql!("UPDATE events SET trace_id = ?1 WHERE event_id = ?2"))
            .bind(trace_id)
            .bind(event_id)
            .execute(pool)
            .await
            .unwrap();
    }

    // The event list narrows to one trace on the full id and on a prefix, and
    // the count has to narrow with it or the pager offers pages that cannot fill.
    #[tokio::test]
    async fn trace_filter_matches_full_id_and_prefix() {
        let pool = open_test_db().await;
        insert_traced_event(&pool, "ta1", 1, TRACE_A, "boom").await;
        insert_traced_event(&pool, "ta2", 1, TRACE_A, "bang").await;
        insert_traced_event(&pool, "tb1", 1, TRACE_B, "thud").await;
        insert_test_event(&pool, "tn1", 1, 100, None, Some("error"), Some("no trace")).await;

        let page = Page::new(None, None);

        let exact = EventFilter {
            trace_id: Some(TRACE_A.to_string()),
            ..Default::default()
        };
        let got = list_all_events(&pool, &exact, &page, None).await.unwrap();
        assert_eq!(got.total, 2);
        assert_eq!(got.items.len(), 2);

        // 24 shared characters: both traces, neither of the untraced rows.
        let prefix = EventFilter {
            trace_id: Some(TRACE_A[..24].to_string()),
            ..Default::default()
        };
        let got = list_all_events(&pool, &prefix, &page, None).await.unwrap();
        assert_eq!(got.total, 3);

        let miss = EventFilter {
            trace_id: Some("ffffffff".to_string()),
            ..Default::default()
        };
        assert_eq!(
            list_all_events(&pool, &miss, &page, None)
                .await
                .unwrap()
                .total,
            0
        );
    }

    // Pasting a trace id into the search box must find the trace without
    // costing the title search its own matches.
    #[tokio::test]
    async fn search_matches_trace_ids_as_well_as_titles() {
        let pool = open_test_db().await;
        insert_traced_event(&pool, "sa1", 1, TRACE_A, "boom").await;
        insert_test_event(&pool, "sn1", 1, 100, None, Some("error"), Some("boom")).await;

        let page = Page::new(None, None);

        let by_trace = EventFilter {
            query: Some(TRACE_A.to_uppercase()),
            ..Default::default()
        };
        let got = list_all_events(&pool, &by_trace, &page, None)
            .await
            .unwrap();
        assert_eq!(got.total, 1);
        assert_eq!(got.items[0].event_id, "sa1");

        let by_title = EventFilter {
            query: Some("boom".to_string()),
            ..Default::default()
        };
        let got = list_all_events(&pool, &by_title, &page, None)
            .await
            .unwrap();
        assert_eq!(got.total, 2, "a word search still only reads the title");
    }

    // The lookup only redirects when it is unambiguous: a prefix shared by two
    // traces has no single waterfall to land on.
    #[tokio::test]
    async fn resolve_trace_id_needs_a_single_match() {
        let pool = open_test_db().await;
        let org = insert_org(&pool, "trace-org").await;
        let other = insert_org(&pool, "trace-org-other").await;
        insert_project(&pool, 401, org).await;
        insert_traced_event(&pool, "ra1", 401, TRACE_A, "boom").await;
        insert_traced_event(&pool, "rb1", 401, TRACE_B, "bang").await;

        let hit = resolve_trace_id(&pool, TRACE_A, None, None).await.unwrap();
        assert_eq!(hit, Some((401, TRACE_A.to_string())));

        // Unique prefix resolves to the full id.
        let hit = resolve_trace_id(&pool, &TRACE_A[..25], None, None)
            .await
            .unwrap();
        assert_eq!(hit, Some((401, TRACE_A.to_string())));

        // Shared prefix is ambiguous.
        assert!(resolve_trace_id(&pool, &TRACE_A[..24], None, None)
            .await
            .unwrap()
            .is_none());

        // Another org's trace is not reachable, and an empty entitlement reaches nothing.
        assert!(resolve_trace_id(&pool, TRACE_A, None, Some(&[other]))
            .await
            .unwrap()
            .is_none());
        assert!(resolve_trace_id(&pool, TRACE_A, None, Some(&[]))
            .await
            .unwrap()
            .is_none());
        assert!(resolve_trace_id(&pool, TRACE_A, Some(999), None)
            .await
            .unwrap()
            .is_none());
    }

    /// A distributed trace is in several projects at once. Grouping by
    /// (trace, project) made that read as ambiguous, so pasting the id of the
    /// only kind of trace worth looking up fell through to the event list.
    #[tokio::test]
    async fn resolve_trace_id_lands_on_a_project_when_the_trace_spans_several() {
        let pool = open_test_db().await;
        let org = insert_org(&pool, "cross-org").await;
        insert_project(&pool, 410, org).await;
        insert_project(&pool, 411, org).await;
        insert_traced_event(&pool, "c1", 411, TRACE_A, "boom").await;
        insert_traced_event(&pool, "c2", 410, TRACE_A, "bang").await;

        assert_eq!(
            resolve_trace_id(&pool, TRACE_A, None, None).await.unwrap(),
            Some((410, TRACE_A.to_string())),
            "lowest project id, deterministically; the page banners the rest"
        );

        // Narrowing still wins, and the scope still holds.
        assert_eq!(
            resolve_trace_id(&pool, TRACE_A, Some(411), None)
                .await
                .unwrap(),
            Some((411, TRACE_A.to_string()))
        );
        assert!(resolve_trace_id(&pool, TRACE_A, None, Some(&[]))
            .await
            .unwrap()
            .is_none());
    }
}
