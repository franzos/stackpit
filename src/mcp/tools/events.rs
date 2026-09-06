//! Event tools: `get_latest_event` and `get_event` (one event as a bounded typed
//! view) and `search_events` (a cross-org listing).
//!
//! The raw payload never leaves this module. `EventDetail.payload` and the
//! rendered `raw_json` are megabyte-scale and unbounded; everything here goes
//! through the same extractors the web UI uses, then gets cut to size.

use serde_json::{json, Value};

use super::truncate::{clamp_limit, truncation_schema, Report, MAX_LIST_LIMIT};
use super::{internal, opt_str_arg, opt_u64_arg, prop, schema_object, str_arg, ToolCtx, ToolError};
use crate::mcp::principal::Target;
use crate::queries::types::{EventFilter, Page};

/// Tags are one line each, but a busy SDK sends a lot of them.
const MAX_TAGS: usize = 50;

pub(super) fn latest_input() -> Value {
    schema_object(
        json!({
            "fingerprint": prop("string", "Issue fingerprint, as returned by list_issues."),
        }),
        &["fingerprint"],
    )
}

pub(super) fn get_input() -> Value {
    schema_object(
        json!({
            "event_id": prop("string", "Event id, as returned by search_events or list_issues."),
        }),
        &["event_id"],
    )
}

/// An event is addressed by id, so the project it belongs to has to be looked up
/// before the caller can be authorized against it.
pub(super) async fn event_target(ctx: &ToolCtx, args: &Value) -> Result<Target, ToolError> {
    let event_id = str_arg(args, "event_id")?;
    let project_id = crate::queries::orgs::project_of_event(&ctx.pool, event_id)
        .await
        .map_err(|e| internal("event_target", format!("{e:#}")))?
        .ok_or_else(|| ToolError::NotFound("not found".to_string()))?;
    Ok(Target::Project(project_id))
}

pub(super) fn latest_output() -> Value {
    schema_object(
        json!({
            "event_id": prop("string", "Id of the event."),
            "project_id": prop("integer", "Owning project."),
            "fingerprint": { "type": ["string", "null"] },
            "timestamp": prop("integer", "Unix seconds the event happened."),
            "received_at": prop("integer", "Unix seconds Stackpit stored it."),
            "level": { "type": ["string", "null"] },
            "title": { "type": ["string", "null"] },
            "platform": { "type": ["string", "null"] },
            "release": { "type": ["string", "null"] },
            "environment": { "type": ["string", "null"] },
            "server_name": { "type": ["string", "null"] },
            "transaction": { "type": ["string", "null"] },
            "sdk": { "type": ["string", "null"] },
            "exceptions": {
                "type": "array",
                "description": "Outermost exception first.",
                "items": schema_object(
                    json!({
                        "type": prop("string", "Exception class."),
                        "value": prop("string", "Exception message."),
                        "mechanism_type": { "type": ["string", "null"] },
                        "mechanism_handled": { "type": ["boolean", "null"] },
                        "frames_total": prop("integer", "Frames before truncation."),
                        "frames": {
                            "type": "array",
                            "description": "Top of stack first, truncated to head and tail.",
                            "items": schema_object(
                                json!({
                                    "filename": prop("string", "Source file."),
                                    "function": prop("string", "Function name."),
                                    "lineno": { "type": ["integer", "null"] },
                                    "colno": { "type": ["integer", "null"] },
                                    "in_app": prop("boolean", "Application code, not a dependency."),
                                    "context_line": { "type": ["string", "null"] },
                                }),
                                &["filename", "function", "in_app"],
                            ),
                        },
                    }),
                    &["type", "value", "frames", "frames_total"],
                ),
            },
            "breadcrumbs": {
                "type": "array",
                "description": "Most recent first-to-last, capped at the newest entries.",
                "items": schema_object(
                    json!({
                        "timestamp": prop("string", "HH:MM:SS."),
                        "level": prop("string", "Breadcrumb level."),
                        "category": prop("string", "Breadcrumb category."),
                        "message": prop("string", "Breadcrumb message."),
                        "data": prop("string", "Serialized extra data."),
                    }),
                    &["level", "category", "message"],
                ),
            },
            "tags": {
                "type": "array",
                "items": schema_object(
                    json!({
                        "key": prop("string", "Tag key."),
                        "value": prop("string", "Tag value."),
                    }),
                    &["key", "value"],
                ),
            },
            "truncation": truncation_schema(),
        }),
        &[
            "event_id",
            "project_id",
            "timestamp",
            "exceptions",
            "breadcrumbs",
            "tags",
            "truncation",
        ],
    )
}

pub(super) async fn get_latest_event(
    ctx: &ToolCtx,
    args: &Value,
    target: Target,
) -> Result<Value, ToolError> {
    let fingerprint = str_arg(args, "fingerprint")?;
    let Target::Project(project_id) = target else {
        return Err(internal(
            "get_latest_event",
            "issue target is not a project",
        ));
    };

    let event = crate::queries::events::get_latest_event_for_issue(
        &ctx.pool,
        project_id as u64,
        fingerprint,
    )
    .await
    .map_err(|e| internal("get_latest_event", format!("{e:#}")))?
    .ok_or_else(|| ToolError::NotFound("not found".to_string()))?;

    event_json(ctx, "get_latest_event", event).await
}

pub(super) async fn get_event(
    ctx: &ToolCtx,
    args: &Value,
    _target: Target,
) -> Result<Value, ToolError> {
    let event_id = str_arg(args, "event_id")?;

    let event = crate::queries::events::get_event_detail(&ctx.pool, event_id)
        .await
        .map_err(|e| internal("get_event", format!("{e:#}")))?
        .ok_or_else(|| ToolError::NotFound("not found".to_string()))?;

    event_json(ctx, "get_event", event).await
}

/// The one place an event turns into JSON. `user`, `request` and `contexts` are
/// deliberately left out: they are the PII-carrying parts of a payload, and a
/// stack trace plus breadcrumbs is what a reader actually needs.
async fn event_json(
    ctx: &ToolCtx,
    tool: &'static str,
    event: crate::queries::types::EventDetail,
) -> Result<Value, ToolError> {
    let supplements = crate::queries::event_supplements::get_event_supplements(&ctx.pool, &event)
        .await
        .map_err(|e| internal(tool, format!("{e:#}")))?;
    // No `FrameResolver`: source-map resolution is a per-event cost the model
    // does not need to read a stack trace.
    let data = crate::queries::event_supplements::get_event_detail_data(&event, supplements, None);

    let mut report = Report::default();

    let exceptions: Vec<Value> = data
        .exceptions
        .iter()
        .map(|exc| {
            let frames_total = exc.frames.len();
            let frames: Vec<Value> = report
                .frames(&exc.frames)
                .iter()
                .map(|f| {
                    json!({
                        "filename": report.text(&f.filename),
                        "function": report.text(&f.function),
                        "lineno": f.lineno,
                        "colno": f.colno,
                        "in_app": f.in_app,
                        "context_line": f.context_line.as_deref().map(|c| report.text(c)),
                    })
                })
                .collect();
            json!({
                "type": report.text(&exc.exc_type),
                "value": report.text(&exc.exc_value),
                "mechanism_type": exc.mechanism_type,
                "mechanism_handled": exc.mechanism_handled,
                "frames_total": frames_total,
                "frames": frames,
            })
        })
        .collect();

    let breadcrumbs: Vec<Value> = report
        .breadcrumbs(data.breadcrumbs)
        .iter()
        .map(|b| {
            json!({
                "timestamp": b.timestamp,
                "level": b.level,
                "category": b.category,
                "message": report.text(&b.message),
                "data": report.text(&b.data),
            })
        })
        .collect();

    report.note_items_omitted(data.tags.len().saturating_sub(MAX_TAGS));
    let tags: Vec<Value> = data
        .tags
        .iter()
        .take(MAX_TAGS)
        .map(|t| json!({ "key": t.key, "value": report.text(&t.value) }))
        .collect();

    let sdk = event
        .sdk_name
        .as_ref()
        .map(|name| match &event.sdk_version {
            Some(version) => format!("{name} {version}"),
            None => name.clone(),
        });

    Ok(json!({
        "event_id": event.event_id,
        "project_id": event.project_id,
        "fingerprint": event.fingerprint,
        "timestamp": event.timestamp,
        "received_at": event.received_at,
        "level": event.level,
        "title": report.opt_text(event.title.as_deref()),
        "platform": event.platform,
        "release": event.release,
        "environment": event.environment,
        "server_name": event.server_name,
        "transaction": event.transaction_name,
        "sdk": sdk,
        "exceptions": exceptions,
        "breadcrumbs": breadcrumbs,
        "tags": tags,
        "truncation": report.to_json(),
    }))
}

/// Sorts `list_all_events` understands; anything else folds to newest-first.
const EVENT_SORTS: [&str; 4] = ["timestamp", "project_id", "level", "platform"];

pub(super) fn search_input() -> Value {
    schema_object(
        json!({
            "query": prop("string", "Substring match on the event title."),
            "level": prop("string", "Only events at this level, e.g. `error` or `warning`."),
            "item_type": prop(
                "string",
                "Only items of this kind, e.g. `event`, `transaction`, `log` or `span`.",
            ),
            "project_id": prop("integer", "Narrow to one project. Must be a project you can reach."),
            "sort": {
                "type": "string",
                "enum": EVENT_SORTS,
                "description": "Ordering; defaults to newest first.",
            },
            "limit": {
                "type": "integer",
                "minimum": 1,
                "maximum": MAX_LIST_LIMIT,
                "description": "Page size. Clamped server-side.",
            },
            "offset": prop("integer", "Rows to skip, for paging through `total`."),
        }),
        &[],
    )
}

pub(super) fn search_output() -> Value {
    schema_object(
        json!({
            "events": {
                "type": "array",
                "items": schema_object(
                    json!({
                        "event_id": prop("string", "Pass to get_event for the full view."),
                        "item_type": prop("string", "Kind of item, e.g. `event` or `transaction`."),
                        "project_id": prop("integer", "Owning project."),
                        "project_name": { "type": ["string", "null"] },
                        "fingerprint": { "type": ["string", "null"] },
                        "timestamp": prop("integer", "Unix seconds."),
                        "level": { "type": ["string", "null"] },
                        "title": { "type": ["string", "null"] },
                        "platform": { "type": ["string", "null"] },
                        "release": { "type": ["string", "null"] },
                        "environment": { "type": ["string", "null"] },
                    }),
                    &["event_id", "item_type", "project_id", "timestamp"],
                ),
            },
            "total": prop("integer", "Events matching the filter."),
            "offset": prop("integer", "Offset this page starts at."),
            "limit": prop("integer", "Page size actually applied."),
            "truncation": truncation_schema(),
        }),
        &["events", "total", "offset", "limit", "truncation"],
    )
}

pub(super) async fn search_events(
    ctx: &ToolCtx,
    args: &Value,
    _target: Target,
) -> Result<Value, ToolError> {
    let sort = opt_str_arg(args, "sort")?;
    if let Some(s) = sort {
        if !EVENT_SORTS.contains(&s) {
            return Err(ToolError::Invalid(format!(
                "`sort` must be one of {}",
                EVENT_SORTS.join(", ")
            )));
        }
    }
    let filter = EventFilter {
        level: opt_str_arg(args, "level")?.map(str::to_string),
        project_id: opt_u64_arg(args, "project_id")?,
        query: opt_str_arg(args, "query")?.map(str::to_string),
        sort: sort.map(str::to_string),
        item_type: opt_str_arg(args, "item_type")?.map(str::to_string),
    };
    let limit = clamp_limit(opt_u64_arg(args, "limit")?);
    let page = Page::new(opt_u64_arg(args, "offset")?, Some(limit));

    // A `project_id` outside the caller's orgs simply matches nothing: the org
    // scope is ANDed in, never replaced by the argument.
    let result = crate::queries::events::list_all_events_for_orgs(
        &ctx.pool,
        &filter,
        &page,
        ctx.principal.accessible_org_ids(),
    )
    .await
    .map_err(|e| internal("search_events", format!("{e:#}")))?;

    let mut report = Report::default();
    report.note_items_omitted(
        (result.total as usize).saturating_sub(result.offset as usize + result.items.len()),
    );
    let events: Vec<Value> = result
        .items
        .iter()
        .map(|e| {
            json!({
                "event_id": e.event_id,
                "item_type": e.item_type.as_str(),
                "project_id": e.project_id,
                "project_name": report.opt_text(e.project_name.as_deref()),
                "fingerprint": e.fingerprint,
                "timestamp": e.timestamp,
                "level": e.level,
                "title": report.opt_text(e.title.as_deref()),
                "platform": e.platform,
                "release": e.release,
                "environment": e.environment,
            })
        })
        .collect();

    Ok(json!({
        "events": events,
        "total": result.total,
        "offset": result.offset,
        "limit": result.limit,
        "truncation": report.to_json(),
    }))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{call, seed_org, seed_project};
    use super::ToolError;
    use crate::db::sql;
    use crate::mcp::principal::McpPrincipal;
    use crate::mcp::SCOPE_EVENTS_READ;
    use crate::orgs::Role;
    use crate::queries::test_helpers::{insert_test_event, insert_test_issue};
    use serde_json::{json, Value};

    const SECRET: &str = "SUPER-SECRET-PAYLOAD-MARKER";

    fn rich_payload(frame_count: usize, breadcrumb_count: usize) -> Value {
        let frames: Vec<Value> = (0..frame_count)
            .map(|i| {
                json!({
                    "filename": format!("src/f{i}.rs"),
                    "function": format!("fn_{i}"),
                    "lineno": i + 1,
                    "in_app": true,
                })
            })
            .collect();
        let crumbs: Vec<Value> = (0..breadcrumb_count)
            .map(|i| json!({ "category": "http", "level": "info", "message": format!("step {i}") }))
            .collect();
        json!({
            "event_id": "e-rich",
            "internal_note": SECRET,
            "exception": { "values": [{
                "type": "ValueError",
                "value": "x".repeat(900),
                "mechanism": { "type": "generic", "handled": false },
                "stacktrace": { "frames": frames },
            }] },
            "breadcrumbs": crumbs,
            "tags": { "server": "web-1" },
        })
    }

    async fn seed_event(
        pool: &crate::db::DbPool,
        project_id: i64,
        fingerprint: &str,
        payload: Value,
    ) {
        let compressed =
            zstd::encode_all(serde_json::to_vec(&payload).unwrap().as_slice(), 3).unwrap();
        sqlx::query(sql!(
            "INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp, level, title, platform, release, environment, server_name, transaction_name, sdk_name, sdk_version, received_at, fingerprint)
             VALUES ('e-rich', 'event', ?1, ?2, 'testkey', 1000, 'error', 'boom', 'python', 'v1', 'prod', 'web-1', '/api', 'sentry.python', '1.0', 1000, ?3)"
        ))
        .bind(&compressed)
        .bind(project_id)
        .bind(fingerprint)
        .execute(pool)
        .await
        .unwrap();
        insert_test_issue(
            pool,
            fingerprint,
            project_id,
            Some("boom"),
            Some("error"),
            1000,
            1000,
            1,
            "unresolved",
        )
        .await;
    }

    async fn setup(pool: &crate::db::DbPool, slug: &str, project_id: i64, payload: Value) -> i64 {
        let org = seed_org(pool, slug).await;
        seed_project(pool, project_id, org, slug).await;
        seed_event(pool, project_id, "fp-event", payload).await;
        org
    }

    #[tokio::test]
    async fn the_latest_event_comes_back_as_a_typed_view() {
        let pool = crate::db::open_test_pool().await;
        let org = setup(&pool, "events-typed", 6001, rich_payload(3, 2)).await;

        let principal = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(org, Role::Owner)]);
        let out = call(
            &pool,
            principal,
            "get_latest_event",
            json!({ "fingerprint": "fp-event" }),
        )
        .await
        .unwrap();

        assert_eq!(out["event_id"], "e-rich");
        assert_eq!(out["project_id"], 6001);
        assert_eq!(out["sdk"], "sentry.python 1.0");
        assert_eq!(out["exceptions"][0]["type"], "ValueError");
        assert_eq!(out["exceptions"][0]["mechanism_handled"], false);
        assert_eq!(out["exceptions"][0]["frames"].as_array().unwrap().len(), 3);
        assert_eq!(out["breadcrumbs"].as_array().unwrap().len(), 2);
        assert_eq!(out["tags"][0]["key"], "server");
    }

    // The raw payload is unbounded and carries whatever the SDK attached.
    #[tokio::test]
    async fn the_raw_payload_never_leaves_the_server() {
        let pool = crate::db::open_test_pool().await;
        let org = setup(&pool, "events-raw", 6010, rich_payload(2, 1)).await;

        let principal = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(org, Role::Owner)]);
        let out = call(
            &pool,
            principal,
            "get_latest_event",
            json!({ "fingerprint": "fp-event" }),
        )
        .await
        .unwrap();

        let rendered = out.to_string();
        assert!(!rendered.contains(SECRET), "payload leaked");
        assert!(out.get("raw_json").is_none());
        assert!(out.get("payload").is_none());
    }

    #[tokio::test]
    async fn a_deep_stack_and_long_message_are_cut_and_counted() {
        let pool = crate::db::open_test_pool().await;
        let org = setup(&pool, "events-cut", 6020, rich_payload(40, 30)).await;

        let principal = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(org, Role::Owner)]);
        let out = call(
            &pool,
            principal,
            "get_latest_event",
            json!({ "fingerprint": "fp-event" }),
        )
        .await
        .unwrap();

        assert_eq!(out["exceptions"][0]["frames_total"], 40);
        assert_eq!(out["exceptions"][0]["frames"].as_array().unwrap().len(), 10);
        assert_eq!(
            out["breadcrumbs"].as_array().unwrap().len(),
            crate::mcp::tools::truncate::MAX_BREADCRUMBS
        );
        assert_eq!(out["truncation"]["truncated"], true);
        assert_eq!(out["truncation"]["stack_frames_omitted"], 30);
        assert_eq!(out["truncation"]["breadcrumbs_omitted"], 10);
        assert!(out["truncation"]["strings_truncated"].as_u64().unwrap() >= 1);
        let value = out["exceptions"][0]["value"].as_str().unwrap();
        assert!(value.ends_with("[+400 chars truncated]"), "got {value}");
    }

    #[tokio::test]
    async fn an_event_in_a_foreign_org_is_not_found() {
        let pool = crate::db::open_test_pool().await;
        setup(&pool, "events-foreign", 6030, rich_payload(1, 1)).await;
        let mine = seed_org(&pool, "events-outsider").await;

        let outsider = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(mine, Role::Owner)]);
        let err = call(
            &pool,
            outsider,
            "get_latest_event",
            json!({ "fingerprint": "fp-event" }),
        )
        .await
        .expect_err("foreign event");
        assert_eq!(err, ToolError::NotFound("not found".to_string()));
    }

    #[tokio::test]
    async fn without_the_read_scope_the_caller_is_stepped_up() {
        let pool = crate::db::open_test_pool().await;
        let org = setup(&pool, "events-noscope", 6040, rich_payload(1, 1)).await;

        let principal = McpPrincipal::for_test("stackpit:projects:read", vec![(org, Role::Owner)]);
        let err = call(
            &pool,
            principal,
            "get_latest_event",
            json!({ "fingerprint": "fp-event" }),
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
    async fn get_event_reaches_the_same_view_by_event_id() {
        let pool = crate::db::open_test_pool().await;
        let org = setup(&pool, "get-event", 6100, rich_payload(3, 2)).await;

        let principal = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(org, Role::Owner)]);
        let out = call(
            &pool,
            principal,
            "get_event",
            json!({ "event_id": "e-rich" }),
        )
        .await
        .unwrap();

        assert_eq!(out["event_id"], "e-rich");
        assert_eq!(out["project_id"], 6100);
        assert_eq!(out["exceptions"][0]["type"], "ValueError");
        assert!(!out.to_string().contains(SECRET), "payload leaked");
        // Wave A's omissions hold here too: no PII-carrying sections.
        for absent in ["user", "request", "contexts", "raw_json", "payload"] {
            assert!(out.get(absent).is_none(), "{absent} must not be returned");
        }
    }

    // The project is resolved from the event id, so an event in someone else's
    // org must answer exactly like one that does not exist.
    #[tokio::test]
    async fn get_event_hides_a_foreign_event_behind_not_found() {
        let pool = crate::db::open_test_pool().await;
        setup(&pool, "get-event-foreign", 6110, rich_payload(1, 1)).await;
        let mine = seed_org(&pool, "get-event-outsider").await;

        let outsider = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(mine, Role::Owner)]);
        let foreign = call(
            &pool,
            outsider,
            "get_event",
            json!({ "event_id": "e-rich" }),
        )
        .await
        .expect_err("foreign event");

        let absent = call(
            &pool,
            McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(mine, Role::Owner)]),
            "get_event",
            json!({ "event_id": "e-nope" }),
        )
        .await
        .expect_err("absent event");
        assert_eq!(foreign, absent, "existence must not be observable");
    }

    #[tokio::test]
    async fn get_event_without_the_read_scope_steps_up() {
        let pool = crate::db::open_test_pool().await;
        let org = setup(&pool, "get-event-noscope", 6120, rich_payload(1, 1)).await;

        let principal = McpPrincipal::for_test("stackpit:projects:read", vec![(org, Role::Owner)]);
        let err = call(
            &pool,
            principal,
            "get_event",
            json!({ "event_id": "e-rich" }),
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
    async fn search_events_spans_every_accessible_org() {
        let pool = crate::db::open_test_pool().await;
        let a = seed_org(&pool, "search-a").await;
        let b = seed_org(&pool, "search-b").await;
        seed_project(&pool, 6200, a, "alpha").await;
        seed_project(&pool, 6201, b, "beta").await;
        insert_test_event(
            &pool,
            "ev-a",
            6200,
            100,
            None,
            Some("error"),
            Some("boom a"),
        )
        .await;
        insert_test_event(
            &pool,
            "ev-b",
            6201,
            200,
            None,
            Some("warning"),
            Some("boom b"),
        )
        .await;

        let principal =
            McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(a, Role::Owner), (b, Role::Member)]);
        let out = call(&pool, principal, "search_events", json!({}))
            .await
            .unwrap();

        let mut ids: Vec<&str> = out["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["event_id"].as_str().unwrap())
            .collect();
        ids.sort_unstable();
        assert_eq!(ids, vec!["ev-a", "ev-b"]);
        assert_eq!(out["total"], 2);
        assert_eq!(out["events"][0]["item_type"], "event");
        assert_eq!(out["events"][0]["project_name"], "beta");
    }

    // The highest-risk path in this wave: one org id list, one query, and a
    // foreign tenant's events must not appear in it.
    #[tokio::test]
    async fn search_events_never_shows_a_foreign_tenants_rows() {
        let pool = crate::db::open_test_pool().await;
        let mine = seed_org(&pool, "search-mine").await;
        let theirs = seed_org(&pool, "search-theirs").await;
        seed_project(&pool, 6210, mine, "mine").await;
        seed_project(&pool, 6211, theirs, "theirs").await;
        insert_test_event(
            &pool,
            "ev-mine",
            6210,
            100,
            None,
            Some("error"),
            Some("mine"),
        )
        .await;
        insert_test_event(
            &pool,
            "ev-theirs",
            6211,
            200,
            None,
            Some("error"),
            Some("secret"),
        )
        .await;

        let principal = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(mine, Role::Owner)]);
        let out = call(&pool, principal, "search_events", json!({}))
            .await
            .unwrap();
        assert_eq!(out["total"], 1);
        assert_eq!(out["events"][0]["event_id"], "ev-mine");

        // Naming the foreign project explicitly must not widen the scope either.
        let principal = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(mine, Role::Owner)]);
        let out = call(
            &pool,
            principal,
            "search_events",
            json!({ "project_id": 6211 }),
        )
        .await
        .unwrap();
        assert_eq!(out["total"], 0);
        assert_eq!(out["events"], json!([]));
    }

    // Zero memberships must produce an empty result, not an unscoped one:
    // `IN ()` is invalid SQL, so this asserts the short-circuit exists.
    #[tokio::test]
    async fn search_events_with_no_memberships_returns_nothing() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "search-orphan").await;
        seed_project(&pool, 6220, org, "p").await;
        insert_test_event(&pool, "ev-1", 6220, 100, None, Some("error"), Some("t")).await;

        let orphan = McpPrincipal::for_test(SCOPE_EVENTS_READ, Vec::new());
        let out = call(&pool, orphan, "search_events", json!({}))
            .await
            .unwrap();
        assert_eq!(out["total"], 0);
        assert_eq!(out["events"], json!([]));
    }

    #[tokio::test]
    async fn search_events_clamps_the_page_size_and_reports_the_rest() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "search-page").await;
        seed_project(&pool, 6230, org, "p").await;
        for i in 0..70 {
            insert_test_event(
                &pool,
                &format!("ev-{i}"),
                6230,
                100 + i,
                None,
                Some("error"),
                Some("t"),
            )
            .await;
        }

        let principal = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(org, Role::Owner)]);
        let out = call(
            &pool,
            principal,
            "search_events",
            json!({ "limit": 500, "query": "t" }),
        )
        .await
        .unwrap();

        assert_eq!(out["limit"], 50);
        assert_eq!(out["events"].as_array().unwrap().len(), 50);
        assert_eq!(out["total"], 70);
        assert_eq!(out["truncation"]["list_items_omitted"], 20);
    }

    #[tokio::test]
    async fn search_events_without_the_read_scope_steps_up() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "search-noscope").await;
        let principal = McpPrincipal::for_test("stackpit:projects:read", vec![(org, Role::Owner)]);
        let err = call(&pool, principal, "search_events", json!({}))
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
    async fn search_events_rejects_an_unknown_sort() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "search-sort").await;
        let principal = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(org, Role::Owner)]);
        let err = call(
            &pool,
            principal,
            "search_events",
            json!({ "sort": "drop table" }),
        )
        .await
        .expect_err("unknown sort");
        assert!(matches!(err, ToolError::Invalid(_)), "got {err:?}");
    }

    // The same fingerprint in a second project must not pull that project's
    // events into the caller's view: the event comes from the target project.
    #[tokio::test]
    async fn get_latest_event_reads_only_the_target_project() {
        let pool = crate::db::open_test_pool().await;
        let org = setup(&pool, "events-shared-a", 6060, rich_payload(1, 1)).await;
        let other = seed_org(&pool, "events-shared-b").await;
        seed_project(&pool, 6061, other, "events-shared-b").await;
        insert_test_issue(
            &pool,
            "fp-event",
            6061,
            Some("other"),
            Some("error"),
            1,
            1,
            1,
            "unresolved",
        )
        .await;
        sqlx::query(sql!(
            "INSERT INTO events (event_id, item_type, payload, project_id, public_key, timestamp, received_at, fingerprint)
             VALUES ('e-other', 'event', ?1, 6061, 'k', 9000, 9000, 'fp-event')"
        ))
        .bind(zstd::encode_all(b"{}".as_slice(), 3).unwrap())
        .execute(&pool)
        .await
        .unwrap();

        let out = call(
            &pool,
            McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(org, Role::Owner)]),
            "get_latest_event",
            json!({ "fingerprint": "fp-event" }),
        )
        .await
        .unwrap();
        assert_eq!(out["event_id"], "e-rich");
        assert_eq!(out["project_id"], 6060);
    }
}
