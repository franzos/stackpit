//! Trace tools: `list_traces` and `get_trace`.

use serde_json::{json, Value};

use super::truncate::{clamp_limit, truncation_schema, Report, MAX_LIST_LIMIT};
use super::{i64_arg, internal, opt_u64_arg, prop, schema_object, str_arg, ToolCtx, ToolError};
use crate::mcp::principal::Target;
use crate::queries::types::Page;

pub(super) fn list_input() -> Value {
    schema_object(
        json!({
            "project_id": prop("integer", "Project to list traces for; see list_projects."),
            "limit": {
                "type": "integer",
                "minimum": 1,
                "maximum": MAX_LIST_LIMIT,
                "description": "Page size. Clamped server-side.",
            },
            "offset": prop("integer", "Rows to skip, for paging through `total`."),
        }),
        &["project_id"],
    )
}

pub(super) fn list_output() -> Value {
    schema_object(
        json!({
            "project_id": prop("integer", "Project the traces belong to."),
            "traces": {
                "type": "array",
                "description": "Most recent first. Only the project's newest spans are scanned, \
                                so old traces fall out of both the page and the total.",
                "items": schema_object(
                    json!({
                        "trace_id": prop("string", "Pass to get_trace for the waterfall."),
                        "span_count": prop("integer", "Spans stored for this trace."),
                        "first_timestamp": prop("integer", "Unix seconds of the earliest span."),
                        "last_timestamp": prop("integer", "Unix seconds of the latest span."),
                        "root_op": { "type": ["string", "null"] },
                        "root_description": { "type": ["string", "null"] },
                        "total_duration_ms": { "type": ["integer", "null"] },
                    }),
                    &["trace_id", "span_count", "first_timestamp", "last_timestamp"],
                ),
            },
            "total": prop("integer", "Traces in the scanned window."),
            "offset": prop("integer", "Offset this page starts at."),
            "limit": prop("integer", "Page size actually applied."),
            "truncation": truncation_schema(),
        }),
        &[
            "project_id",
            "traces",
            "total",
            "offset",
            "limit",
            "truncation",
        ],
    )
}

pub(super) async fn list_traces(
    ctx: &ToolCtx,
    args: &Value,
    target: Target,
) -> Result<Value, ToolError> {
    let project_id = i64_arg(args, "project_id")?;
    debug_assert_eq!(target, Target::Project(project_id));

    let limit = clamp_limit(opt_u64_arg(args, "limit")?);
    let page = Page::new(opt_u64_arg(args, "offset")?, Some(limit));

    let result = crate::queries::spans::list_traces(&ctx.pool, project_id as u64, &page)
        .await
        .map_err(|e| internal("list_traces", format!("{e:#}")))?;

    let mut report = Report::default();
    report.note_items_omitted(
        (result.total as usize).saturating_sub(result.offset as usize + result.items.len()),
    );
    let traces: Vec<Value> = result
        .items
        .iter()
        .map(|t| {
            json!({
                "trace_id": t.trace_id,
                "span_count": t.span_count,
                "first_timestamp": t.first_timestamp,
                "last_timestamp": t.last_timestamp,
                "root_op": report.opt_text(t.root_op.as_deref()),
                "root_description": report.opt_text(t.root_description.as_deref()),
                "total_duration_ms": t.total_duration_ms,
            })
        })
        .collect();

    Ok(json!({
        "project_id": project_id,
        "traces": traces,
        "total": result.total,
        "offset": result.offset,
        "limit": result.limit,
        "truncation": report.to_json(),
    }))
}

pub(super) fn get_input() -> Value {
    schema_object(
        json!({
            "project_id": prop("integer", "Project the trace belongs to."),
            "trace_id": prop("string", "Trace id, as returned by list_traces."),
        }),
        &["project_id", "trace_id"],
    )
}

pub(super) fn get_output() -> Value {
    schema_object(
        json!({
            "project_id": prop("integer", "Project the trace belongs to."),
            "trace_id": prop("string", "Trace that was fetched."),
            "root": {
                "type": ["object", "null"],
                "description": "The transaction that owns the trace. Null when only standalone \
                                spans landed.",
                "properties": {
                    "name": { "type": ["string", "null"] },
                    "duration_ms": { "type": ["integer", "null"] },
                },
            },
            "total_ms": prop("integer", "Trace duration: the wider of span extent and root."),
            "span_count": prop("integer", "Spans in the trace, before any row cap."),
            "spans": {
                "type": "array",
                "description": "Depth-first, siblings by start time. `depth` gives the nesting.",
                "items": schema_object(
                    json!({
                        "span_id": prop("string", "Span id."),
                        "parent_span_id": { "type": ["string", "null"] },
                        "depth": prop("integer", "Nesting depth; 0 is a root span."),
                        "op": { "type": ["string", "null"] },
                        "description": { "type": ["string", "null"] },
                        "status": { "type": ["string", "null"] },
                        "duration_ms": { "type": ["integer", "null"] },
                        "start_offset_ms": {
                            "type": ["integer", "null"],
                            "description": "Start relative to the trace start, in ms.",
                        },
                    }),
                    &["span_id", "depth"],
                ),
            },
            "errors": {
                "type": "array",
                "description": "Error events sharing this trace, newest first, at most 50.",
                "items": schema_object(
                    json!({
                        "event_id": prop("string", "Pass to get_event for the full view."),
                        "title": { "type": ["string", "null"] },
                        "level": { "type": ["string", "null"] },
                        "timestamp": prop("integer", "Unix seconds."),
                    }),
                    &["event_id", "timestamp"],
                ),
            },
            "truncation": truncation_schema(),
        }),
        &[
            "project_id",
            "trace_id",
            "total_ms",
            "span_count",
            "spans",
            "errors",
            "truncation",
        ],
    )
}

pub(super) async fn get_trace(
    ctx: &ToolCtx,
    args: &Value,
    target: Target,
) -> Result<Value, ToolError> {
    let project_id = i64_arg(args, "project_id")?;
    let trace_id = str_arg(args, "trace_id")?;
    debug_assert_eq!(target, Target::Project(project_id));

    // Project-scoped spans, unlike the web UI's trace page: a distributed trace
    // spans projects, and this caller is only entitled to one of them.
    let (spans, errors, root) = tokio::join!(
        crate::queries::spans::get_trace_spans_for_project(&ctx.pool, project_id as u64, trace_id),
        crate::queries::spans::get_trace_errors(&ctx.pool, project_id as u64, trace_id),
        crate::queries::spans::get_trace_root(&ctx.pool, project_id as u64, trace_id),
    );
    let spans = spans.map_err(|e| internal("get_trace", format!("{e:#}")))?;
    let errors = errors.map_err(|e| internal("get_trace", format!("{e:#}")))?;
    let root = root.map_err(|e| internal("get_trace", format!("{e:#}")))?;

    if spans.is_empty() && errors.is_empty() && root.is_none() {
        return Err(ToolError::NotFound("not found".to_string()));
    }

    let span_rows: Vec<crate::queries::spans::SpanRow> = spans.iter().map(Into::into).collect();
    let root_duration_ms = root.as_ref().and_then(|r| r.duration_ms).unwrap_or(0);
    let waterfall = crate::queries::spans::build_waterfall(&span_rows, root_duration_ms);

    let mut report = Report::default();
    // `build_waterfall` already caps rows at MAX_WATERFALL_ROWS; report the
    // difference rather than cutting again.
    report.note_items_omitted(waterfall.span_count.saturating_sub(waterfall.rows.len()));

    let out_spans: Vec<Value> = waterfall
        .rows
        .iter()
        .map(|r| {
            json!({
                "span_id": r.span_id,
                "parent_span_id": r.parent_span_id,
                "depth": r.depth,
                "op": report.opt_text(r.op.as_deref()),
                "description": report.opt_text(r.description.as_deref()),
                "status": r.status,
                "duration_ms": r.duration_ms,
                "start_offset_ms": r.start_offset_ms,
            })
        })
        .collect();

    let out_errors: Vec<Value> = errors
        .iter()
        .map(|e| {
            json!({
                "event_id": e.event_id,
                "title": report.opt_text(e.title.as_deref()),
                "level": e.level,
                "timestamp": e.timestamp,
            })
        })
        .collect();

    Ok(json!({
        "project_id": project_id,
        "trace_id": trace_id,
        "root": root.map(|r| json!({ "name": r.name, "duration_ms": r.duration_ms })),
        "total_ms": waterfall.total_ms,
        "span_count": waterfall.span_count,
        "spans": out_spans,
        "errors": out_errors,
        "truncation": report.to_json(),
    }))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{call, seed_org, seed_project};
    use super::ToolError;
    use crate::db::{sql, DbPool};
    use crate::mcp::principal::McpPrincipal;
    use crate::mcp::SCOPE_EVENTS_READ;
    use crate::orgs::Role;
    use serde_json::json;

    #[allow(clippy::too_many_arguments)]
    async fn insert_span(
        pool: &DbPool,
        span_id: &str,
        trace_id: &str,
        parent: Option<&str>,
        project_id: i64,
        timestamp: i64,
        start_ms: i64,
        duration_ms: i64,
        description: Option<&str>,
    ) {
        let payload = zstd::encode_all([0u8; 0].as_slice(), 3).unwrap();
        sqlx::query(sql!(
            "INSERT INTO spans (span_id, payload, project_id, public_key, timestamp, trace_id, parent_span_id, op, description, status, duration_ms, start_ms)
             VALUES (?1, ?2, ?3, 'testkey', ?4, ?5, ?6, 'db.query', ?7, 'ok', ?8, ?9)"
        ))
        .bind(span_id)
        .bind(&payload)
        .bind(project_id)
        .bind(timestamp)
        .bind(trace_id)
        .bind(parent)
        .bind(description)
        .bind(duration_ms)
        .bind(start_ms)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn insert_txn(pool: &DbPool, event_id: &str, project_id: i64, trace_id: &str) {
        let payload = serde_json::json!({ "contexts": { "trace": { "op": "http.server" } } });
        let compressed =
            zstd::encode_all(serde_json::to_vec(&payload).unwrap().as_slice(), 3).unwrap();
        sqlx::query(sql!(
            "INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp, trace_id, transaction_name, duration_ms, level)
             VALUES (?1, 'transaction', ?2, ?3, 'testkey', 100, ?4, 'GET /checkout', 1000, 'info')"
        ))
        .bind(event_id)
        .bind(&compressed)
        .bind(project_id)
        .bind(trace_id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn insert_error(pool: &DbPool, event_id: &str, project_id: i64, trace_id: &str) {
        let compressed = zstd::encode_all([0u8; 0].as_slice(), 3).unwrap();
        sqlx::query(sql!(
            "INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp, trace_id, title, level)
             VALUES (?1, 'event', ?2, ?3, 'testkey', 110, ?4, 'boom', 'error')"
        ))
        .bind(event_id)
        .bind(&compressed)
        .bind(project_id)
        .bind(trace_id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed(pool: &DbPool, slug: &str, project_id: i64) -> i64 {
        let org = seed_org(pool, slug).await;
        seed_project(pool, project_id, org, slug).await;
        org
    }

    #[tokio::test]
    async fn list_traces_returns_the_projects_traces() {
        let pool = crate::db::open_test_pool().await;
        let org = seed(&pool, "traces-list", 4001).await;
        insert_span(&pool, "s1", "t1", None, 4001, 100, 0, 50, Some("SELECT 1")).await;
        insert_span(&pool, "s2", "t1", Some("s1"), 4001, 101, 10, 20, None).await;

        let principal = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(org, Role::Owner)]);
        let out = call(
            &pool,
            principal,
            "list_traces",
            json!({ "project_id": 4001 }),
        )
        .await
        .unwrap();

        assert_eq!(out["total"], 1);
        assert_eq!(out["limit"], 25);
        assert_eq!(out["traces"][0]["trace_id"], "t1");
        assert_eq!(out["traces"][0]["span_count"], 2);
        assert_eq!(out["truncation"]["truncated"], false);
    }

    #[tokio::test]
    async fn list_traces_in_a_foreign_project_is_not_found() {
        let pool = crate::db::open_test_pool().await;
        seed(&pool, "traces-theirs", 4010).await;
        insert_span(&pool, "s1", "t1", None, 4010, 100, 0, 50, None).await;
        let mine = seed_org(&pool, "traces-mine").await;

        let outsider = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(mine, Role::Owner)]);
        let err = call(
            &pool,
            outsider,
            "list_traces",
            json!({ "project_id": 4010 }),
        )
        .await
        .expect_err("a foreign project is not reachable");
        assert_eq!(err, ToolError::NotFound("not found".to_string()));
    }

    #[tokio::test]
    async fn list_traces_without_the_read_scope_steps_up() {
        let pool = crate::db::open_test_pool().await;
        let org = seed(&pool, "traces-noscope", 4020).await;
        let principal = McpPrincipal::for_test("stackpit:projects:read", vec![(org, Role::Owner)]);
        let err = call(
            &pool,
            principal,
            "list_traces",
            json!({ "project_id": 4020 }),
        )
        .await
        .expect_err("events:read is required");
        assert_eq!(
            err,
            ToolError::Scope {
                required: SCOPE_EVENTS_READ
            }
        );
    }

    #[tokio::test]
    async fn get_trace_composes_spans_root_and_errors() {
        let pool = crate::db::open_test_pool().await;
        let org = seed(&pool, "trace-get", 4030).await;
        insert_span(&pool, "s1", "t1", None, 4030, 100, 0, 50, Some("SELECT 1")).await;
        insert_span(&pool, "s2", "t1", Some("s1"), 4030, 101, 10, 20, None).await;
        insert_txn(&pool, "tx1", 4030, "t1").await;
        insert_error(&pool, "e1", 4030, "t1").await;

        let principal = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(org, Role::Owner)]);
        let out = call(
            &pool,
            principal,
            "get_trace",
            json!({ "project_id": 4030, "trace_id": "t1" }),
        )
        .await
        .unwrap();

        assert_eq!(out["trace_id"], "t1");
        assert_eq!(out["span_count"], 2);
        // The root transaction's own duration widens the axis past the spans.
        assert_eq!(out["root"]["name"], "GET /checkout");
        assert_eq!(out["total_ms"], 1000);
        let spans = out["spans"].as_array().unwrap();
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0]["span_id"], "s1");
        assert_eq!(spans[0]["depth"], 0);
        assert_eq!(spans[1]["span_id"], "s2");
        assert_eq!(spans[1]["depth"], 1);
        assert_eq!(out["errors"][0]["event_id"], "e1");
        assert_eq!(out["truncation"]["truncated"], false);
        // Display geometry is for the web UI, not for a model.
        assert!(spans[0].get("offset_pct").is_none());
    }

    // The spans lookup keys on trace id alone, so the project argument is the
    // only thing scoping it; a trace in someone else's org must be unreachable.
    #[tokio::test]
    async fn get_trace_in_a_foreign_project_is_not_found() {
        let pool = crate::db::open_test_pool().await;
        seed(&pool, "trace-foreign", 4040).await;
        insert_span(
            &pool,
            "s1",
            "t-secret",
            None,
            4040,
            100,
            0,
            50,
            Some("SELECT secrets"),
        )
        .await;
        insert_txn(&pool, "tx1", 4040, "t-secret").await;
        let mine = seed_org(&pool, "trace-outsider").await;

        let outsider = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(mine, Role::Owner)]);
        let foreign = call(
            &pool,
            outsider,
            "get_trace",
            json!({ "project_id": 4040, "trace_id": "t-secret" }),
        )
        .await
        .expect_err("foreign trace");

        let absent = call(
            &pool,
            McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(mine, Role::Owner)]),
            "get_trace",
            json!({ "project_id": 4040, "trace_id": "t-does-not-exist" }),
        )
        .await
        .expect_err("absent trace");
        assert_eq!(foreign, absent, "existence must not be observable");
    }

    // A trace id is shared across projects in a distributed trace, and the spans
    // table is indexed by trace id alone. Presenting a shared id from a project
    // the caller does own must not pull in the other project's spans.
    #[tokio::test]
    async fn get_trace_shows_only_the_authorized_projects_spans() {
        let pool = crate::db::open_test_pool().await;
        let mine = seed(&pool, "trace-shared-mine", 4070).await;
        let theirs = seed_org(&pool, "trace-shared-theirs").await;
        seed_project(&pool, 4071, theirs, "theirs").await;
        insert_span(
            &pool,
            "s-mine",
            "t-shared",
            None,
            4070,
            100,
            0,
            50,
            Some("mine"),
        )
        .await;
        insert_span(
            &pool,
            "s-theirs",
            "t-shared",
            None,
            4071,
            100,
            0,
            50,
            Some("secret"),
        )
        .await;

        let principal = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(mine, Role::Owner)]);
        let out = call(
            &pool,
            principal,
            "get_trace",
            json!({ "project_id": 4070, "trace_id": "t-shared" }),
        )
        .await
        .unwrap();

        let spans = out["spans"].as_array().unwrap();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0]["span_id"], "s-mine");
        assert!(!out.to_string().contains("secret"), "foreign span leaked");
    }

    #[tokio::test]
    async fn get_trace_without_the_read_scope_steps_up() {
        let pool = crate::db::open_test_pool().await;
        let org = seed(&pool, "trace-noscope", 4050).await;
        insert_span(&pool, "s1", "t1", None, 4050, 100, 0, 50, None).await;

        let principal = McpPrincipal::for_test("stackpit:projects:read", vec![(org, Role::Owner)]);
        let err = call(
            &pool,
            principal,
            "get_trace",
            json!({ "project_id": 4050, "trace_id": "t1" }),
        )
        .await
        .expect_err("events:read is required");
        assert_eq!(
            err,
            ToolError::Scope {
                required: SCOPE_EVENTS_READ
            }
        );
    }

    // A long description is cut like every other string, and the cut is counted.
    #[tokio::test]
    async fn get_trace_truncates_long_span_descriptions() {
        let pool = crate::db::open_test_pool().await;
        let org = seed(&pool, "trace-long", 4060).await;
        let long = "x".repeat(900);
        insert_span(&pool, "s1", "t1", None, 4060, 100, 0, 50, Some(&long)).await;

        let principal = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(org, Role::Owner)]);
        let out = call(
            &pool,
            principal,
            "get_trace",
            json!({ "project_id": 4060, "trace_id": "t1" }),
        )
        .await
        .unwrap();

        let description = out["spans"][0]["description"].as_str().unwrap();
        assert!(
            description.ends_with("[+400 chars truncated]"),
            "got {description}"
        );
        assert_eq!(out["truncation"]["strings_truncated"], 1);
        assert_eq!(out["truncation"]["truncated"], true);
    }
}
