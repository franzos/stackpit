//! Issue tools: `list_issues`, `get_issue`, `update_issue_status`.

use serde_json::{json, Value};

use super::truncate::{clamp_limit, truncation_schema, Report, MAX_LIST_LIMIT};
use super::{
    i64_arg, internal, opt_str_arg, opt_u64_arg, prop, schema_object, str_arg, ToolCtx, ToolError,
};
use crate::domain::IssueStatus;
use crate::mcp::principal::Target;
use crate::queries::types::{IssueFilter, IssueSummary, Page};

const STATUSES: [&str; 3] = ["unresolved", "resolved", "ignored"];

/// 24 hourly buckets: enough to see a spike, small enough to read.
const TREND_BUCKETS: usize = 24;
const TREND_BUCKET_SECS: i64 = 3_600;

/// An issue is addressed by fingerprint, so the project it belongs to has to be
/// looked up before the caller can be authorized against it.
pub(super) async fn fingerprint_target(ctx: &ToolCtx, args: &Value) -> Result<Target, ToolError> {
    let fingerprint = str_arg(args, "fingerprint")?;
    let project_id = crate::queries::orgs::project_of_fingerprint(&ctx.pool, fingerprint)
        .await
        .map_err(|e| internal("fingerprint_target", format!("{e:#}")))?
        .ok_or_else(|| ToolError::NotFound("not found".to_string()))?;
    Ok(Target::Project(project_id))
}

fn issue_json(issue: &IssueSummary, report: &mut Report) -> Value {
    json!({
        "fingerprint": issue.fingerprint,
        "project_id": issue.project_id,
        "title": report.opt_text(issue.title.as_deref()),
        "level": issue.level,
        "status": issue.status.as_str(),
        "item_type": issue.item_type.as_str(),
        "first_seen": issue.first_seen,
        "last_seen": issue.last_seen,
        "event_count": issue.event_count,
        "user_count": issue.user_count,
    })
}

fn issue_item_schema() -> Value {
    schema_object(
        json!({
            "fingerprint": prop("string", "Stable issue id; pass it to get_issue."),
            "project_id": prop("integer", "Owning project."),
            "title": { "type": ["string", "null"] },
            "level": { "type": ["string", "null"] },
            "status": prop("string", "unresolved, resolved or ignored."),
            "item_type": prop("string", "Kind of item that grouped into this issue."),
            "first_seen": prop("integer", "Unix seconds."),
            "last_seen": prop("integer", "Unix seconds."),
            "event_count": prop("integer", "Events in this issue."),
            "user_count": prop("integer", "Approximate distinct users affected."),
        }),
        &["fingerprint", "project_id", "status", "event_count"],
    )
}

pub(super) fn list_input() -> Value {
    schema_object(
        json!({
            "project_id": prop("integer", "Project to list issues for; see list_projects."),
            "status": {
                "type": "string",
                "enum": STATUSES,
                "description": "Only issues in this state.",
            },
            "level": prop("string", "Only issues at this level, e.g. `error`."),
            "query": prop("string", "Substring match on the issue title."),
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
            "project_id": prop("integer", "Project the issues belong to."),
            "issues": { "type": "array", "items": issue_item_schema() },
            "total": prop("integer", "Issues matching the filter."),
            "offset": prop("integer", "Offset this page starts at."),
            "limit": prop("integer", "Page size actually applied."),
            "truncation": truncation_schema(),
        }),
        &[
            "project_id",
            "issues",
            "total",
            "offset",
            "limit",
            "truncation",
        ],
    )
}

pub(super) async fn list_issues(
    ctx: &ToolCtx,
    args: &Value,
    target: Target,
) -> Result<Value, ToolError> {
    let project_id = i64_arg(args, "project_id")? as u64;
    debug_assert_eq!(target, Target::Project(project_id as i64));

    let status = opt_str_arg(args, "status")?;
    if let Some(s) = status {
        if !STATUSES.contains(&s) {
            return Err(ToolError::Invalid(format!(
                "`status` must be one of {}",
                STATUSES.join(", ")
            )));
        }
    }

    let filter = IssueFilter {
        level: opt_str_arg(args, "level")?.map(str::to_string),
        status: status.map(str::to_string),
        query: opt_str_arg(args, "query")?.map(str::to_string),
        sort: None,
        item_type: None,
        release: None,
        tag: None,
    };
    let limit = clamp_limit(opt_u64_arg(args, "limit")?);
    let page = Page::new(opt_u64_arg(args, "offset")?, Some(limit));

    let result = crate::queries::issues::list_issues(&ctx.pool, project_id, &filter, &page, None)
        .await
        .map_err(|e| internal("list_issues", format!("{e:#}")))?;

    let mut report = Report::default();
    report.note_items_omitted(
        (result.total as usize).saturating_sub(result.offset as usize + result.items.len()),
    );
    let issues: Vec<Value> = result
        .items
        .iter()
        .map(|i| issue_json(i, &mut report))
        .collect();

    Ok(json!({
        "project_id": project_id,
        "issues": issues,
        "total": result.total,
        "offset": result.offset,
        "limit": result.limit,
        "truncation": report.to_json(),
    }))
}

pub(super) fn get_input() -> Value {
    schema_object(
        json!({
            "fingerprint": prop("string", "Issue fingerprint, as returned by list_issues."),
        }),
        &["fingerprint"],
    )
}

pub(super) fn get_output() -> Value {
    let mut schema = issue_item_schema();
    let props = schema["properties"].as_object_mut().expect("object schema");
    props.insert(
        "trend_24h".to_string(),
        json!({
            "type": "array",
            "items": { "type": "number" },
            "description": "Event counts in 24 hourly buckets, oldest first.",
        }),
    );
    props.insert(
        "external_links".to_string(),
        json!({
            "type": "array",
            "description": "Tracker issues linked to this issue.",
            "items": schema_object(
                json!({
                    "integration_kind": prop("string", "Tracker kind, e.g. `github`."),
                    "integration_name": prop("string", "Configured integration name."),
                    "external_id": prop("string", "Id in the tracker."),
                    "external_url": prop("string", "Link to the tracker issue."),
                    "external_state": { "type": ["string", "null"] },
                }),
                &["integration_kind", "external_id", "external_url"],
            ),
        }),
    );
    props.insert("truncation".to_string(), truncation_schema());
    schema
}

pub(super) async fn get_issue(
    ctx: &ToolCtx,
    args: &Value,
    target: Target,
) -> Result<Value, ToolError> {
    let fingerprint = str_arg(args, "fingerprint")?;
    let Target::Project(project_id) = target else {
        return Err(internal("get_issue", "issue target is not a project"));
    };

    let issue = crate::queries::issues::get_issue(&ctx.pool, fingerprint)
        .await
        .map_err(|e| internal("get_issue", format!("{e:#}")))?
        .ok_or_else(|| ToolError::NotFound("not found".to_string()))?;

    let start_ts = chrono::Utc::now().timestamp() - TREND_BUCKET_SECS * TREND_BUCKETS as i64;
    let trend = crate::queries::issues::issue_sparklines(
        &ctx.pool,
        project_id as u64,
        std::slice::from_ref(&issue.fingerprint),
        start_ts,
        TREND_BUCKET_SECS,
        TREND_BUCKETS,
    )
    .await
    .map_err(|e| internal("get_issue", format!("{e:#}")))?
    .remove(&issue.fingerprint)
    .unwrap_or_else(|| vec![0.0; TREND_BUCKETS]);

    let links = crate::queries::issue_links::links_for_issue(&ctx.pool, fingerprint)
        .await
        .map_err(|e| internal("get_issue", format!("{e:#}")))?;

    let mut report = Report::default();
    let mut out = issue_json(&issue, &mut report);
    let external_links: Vec<Value> = links
        .iter()
        .map(|l| {
            json!({
                "integration_kind": l.integration_kind,
                "integration_name": l.integration_name,
                "external_id": l.external_id,
                "external_url": l.external_url,
                "external_state": l.external_state,
            })
        })
        .collect();
    let obj = out.as_object_mut().expect("issue_json builds an object");
    obj.insert("trend_24h".to_string(), json!(trend));
    obj.insert("external_links".to_string(), json!(external_links));
    obj.insert("truncation".to_string(), report.to_json());
    Ok(out)
}

pub(super) fn update_input() -> Value {
    schema_object(
        json!({
            "fingerprint": prop("string", "Issue fingerprint, as returned by list_issues."),
            "status": {
                "type": "string",
                "enum": STATUSES,
                "description": "State to move the issue to.",
            },
        }),
        &["fingerprint", "status"],
    )
}

pub(super) fn update_output() -> Value {
    schema_object(
        json!({
            "fingerprint": prop("string", "Issue that was updated."),
            "project_id": prop("integer", "Owning project."),
            "previous_status": prop("string", "State before this call."),
            "status": prop("string", "State now."),
        }),
        &["fingerprint", "project_id", "previous_status", "status"],
    )
}

pub(super) async fn update_issue_status(
    ctx: &ToolCtx,
    args: &Value,
    target: Target,
) -> Result<Value, ToolError> {
    let fingerprint = str_arg(args, "fingerprint")?;
    let requested = str_arg(args, "status")?;
    if !STATUSES.contains(&requested) {
        return Err(ToolError::Invalid(format!(
            "`status` must be one of {}",
            STATUSES.join(", ")
        )));
    }
    let status: IssueStatus = requested.parse().unwrap_or_default();
    let Target::Project(project_id) = target else {
        return Err(internal("update_issue_status", "target is not a project"));
    };

    let before = crate::queries::issues::get_issue(&ctx.pool, fingerprint)
        .await
        .map_err(|e| internal("update_issue_status", format!("{e:#}")))?
        .ok_or_else(|| ToolError::NotFound("not found".to_string()))?;

    let affected =
        crate::queries::issues::update_issue_status(&ctx.writer_pool, fingerprint, status)
            .await
            .map_err(|e| internal("update_issue_status", format!("{e:#}")))?;
    if affected == 0 {
        return Err(ToolError::NotFound("not found".to_string()));
    }

    Ok(json!({
        "fingerprint": fingerprint,
        "project_id": project_id,
        "previous_status": before.status.as_str(),
        "status": status.as_str(),
    }))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{call, seed_org, seed_project};
    use super::ToolError;
    use crate::mcp::principal::McpPrincipal;
    use crate::mcp::{SCOPE_EVENTS_READ, SCOPE_PROJECTS_WRITE};
    use crate::orgs::Role;
    use crate::queries::test_helpers::insert_test_issue;
    use serde_json::json;

    const READ_AND_WRITE: &str = "stackpit:events:read stackpit:projects:write";

    async fn seed(pool: &crate::db::DbPool, slug: &str, project_id: i64) -> i64 {
        let org = seed_org(pool, slug).await;
        seed_project(pool, project_id, org, slug).await;
        org
    }

    async fn issue(pool: &crate::db::DbPool, fingerprint: &str, project_id: i64, title: &str) {
        insert_test_issue(
            pool,
            fingerprint,
            project_id,
            Some(title),
            Some("error"),
            1_000,
            2_000,
            7,
            "unresolved",
        )
        .await;
    }

    #[tokio::test]
    async fn list_issues_returns_the_projects_issues() {
        let pool = crate::db::open_test_pool().await;
        let org = seed(&pool, "issues-list", 7001).await;
        issue(&pool, "fp-1", 7001, "boom").await;

        let principal = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(org, Role::Owner)]);
        let out = call(
            &pool,
            principal,
            "list_issues",
            json!({ "project_id": 7001 }),
        )
        .await
        .unwrap();

        assert_eq!(out["total"], 1);
        assert_eq!(out["limit"], 25);
        assert_eq!(out["issues"][0]["fingerprint"], "fp-1");
        assert_eq!(out["issues"][0]["title"], "boom");
        assert_eq!(out["issues"][0]["status"], "unresolved");
        assert_eq!(out["truncation"]["truncated"], false);
    }

    #[tokio::test]
    async fn list_issues_in_a_foreign_project_is_not_found() {
        let pool = crate::db::open_test_pool().await;
        seed(&pool, "issues-theirs", 7010).await;
        let mine = seed_org(&pool, "issues-mine").await;

        let outsider = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(mine, Role::Owner)]);
        let err = call(
            &pool,
            outsider,
            "list_issues",
            json!({ "project_id": 7010 }),
        )
        .await
        .expect_err("a foreign project is not reachable");
        assert_eq!(err, ToolError::NotFound("not found".to_string()));
    }

    #[tokio::test]
    async fn list_issues_without_the_read_scope_steps_up() {
        let pool = crate::db::open_test_pool().await;
        let org = seed(&pool, "issues-noscope", 7020).await;
        let principal = McpPrincipal::for_test("stackpit:projects:read", vec![(org, Role::Owner)]);
        let err = call(
            &pool,
            principal,
            "list_issues",
            json!({ "project_id": 7020 }),
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
    async fn list_issues_clamps_the_page_size_and_reports_the_rest() {
        let pool = crate::db::open_test_pool().await;
        let org = seed(&pool, "issues-page", 7030).await;
        for i in 0..70 {
            issue(&pool, &format!("fp-page-{i}"), 7030, "t").await;
        }

        let principal = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(org, Role::Owner)]);
        let out = call(
            &pool,
            principal,
            "list_issues",
            json!({ "project_id": 7030, "limit": 500 }),
        )
        .await
        .unwrap();

        assert_eq!(out["limit"], 50);
        assert_eq!(out["issues"].as_array().unwrap().len(), 50);
        assert_eq!(out["total"], 70);
        assert_eq!(out["truncation"]["list_items_omitted"], 20);
    }

    #[tokio::test]
    async fn a_long_title_is_truncated_and_reported() {
        let pool = crate::db::open_test_pool().await;
        let org = seed(&pool, "issues-long", 7040).await;
        issue(&pool, "fp-long", 7040, &"a".repeat(900)).await;

        let principal = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(org, Role::Owner)]);
        let out = call(
            &pool,
            principal,
            "list_issues",
            json!({ "project_id": 7040 }),
        )
        .await
        .unwrap();

        let title = out["issues"][0]["title"].as_str().unwrap();
        assert!(title.ends_with("[+400 chars truncated]"), "got {title}");
        assert_eq!(out["truncation"]["strings_truncated"], 1);
        assert_eq!(out["truncation"]["truncated"], true);
    }

    #[tokio::test]
    async fn get_issue_resolves_the_project_from_the_fingerprint() {
        let pool = crate::db::open_test_pool().await;
        let org = seed(&pool, "issues-get", 7050).await;
        issue(&pool, "fp-get", 7050, "kaboom").await;

        let principal = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(org, Role::Owner)]);
        let out = call(
            &pool,
            principal,
            "get_issue",
            json!({ "fingerprint": "fp-get" }),
        )
        .await
        .unwrap();

        assert_eq!(out["project_id"], 7050);
        assert_eq!(out["title"], "kaboom");
        assert_eq!(out["trend_24h"].as_array().unwrap().len(), 24);
        assert_eq!(out["external_links"], json!([]));
    }

    // The fingerprint's own project decides access, so an issue in someone
    // else's org must answer exactly like one that does not exist.
    #[tokio::test]
    async fn get_issue_hides_a_foreign_issue_behind_not_found() {
        let pool = crate::db::open_test_pool().await;
        seed(&pool, "issues-foreign", 7060).await;
        issue(&pool, "fp-foreign", 7060, "secret").await;
        let mine = seed_org(&pool, "issues-outsider").await;

        let outsider = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(mine, Role::Owner)]);
        let foreign = call(
            &pool,
            outsider,
            "get_issue",
            json!({ "fingerprint": "fp-foreign" }),
        )
        .await
        .expect_err("foreign issue");

        let absent = call(
            &pool,
            McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(mine, Role::Owner)]),
            "get_issue",
            json!({ "fingerprint": "fp-does-not-exist" }),
        )
        .await
        .expect_err("absent issue");
        assert_eq!(foreign, absent, "existence must not be observable");
    }

    #[tokio::test]
    async fn update_issue_status_writes_and_reports_the_previous_state() {
        let pool = crate::db::open_test_pool().await;
        let org = seed(&pool, "issues-update", 7070).await;
        issue(&pool, "fp-update", 7070, "flip me").await;

        let owner = McpPrincipal::for_test(READ_AND_WRITE, vec![(org, Role::Owner)]);
        let out = call(
            &pool,
            owner,
            "update_issue_status",
            json!({ "fingerprint": "fp-update", "status": "resolved" }),
        )
        .await
        .unwrap();

        assert_eq!(out["previous_status"], "unresolved");
        assert_eq!(out["status"], "resolved");
        assert_eq!(out["project_id"], 7070);

        let stored = crate::queries::issues::get_issue(&pool, "fp-update")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.status.as_str(), "resolved");
    }

    // The web UI requires the owner role for this; MCP must never exceed what
    // the same person can do in the browser.
    #[tokio::test]
    async fn update_issue_status_refuses_a_member() {
        let pool = crate::db::open_test_pool().await;
        let org = seed(&pool, "issues-member", 7080).await;
        issue(&pool, "fp-member", 7080, "nope").await;

        let member = McpPrincipal::for_test(READ_AND_WRITE, vec![(org, Role::Member)]);
        let err = call(
            &pool,
            member,
            "update_issue_status",
            json!({ "fingerprint": "fp-member", "status": "resolved" }),
        )
        .await
        .expect_err("members cannot change status");
        assert!(matches!(err, ToolError::Forbidden(_)));

        let stored = crate::queries::issues::get_issue(&pool, "fp-member")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.status.as_str(), "unresolved", "no write happened");
    }

    #[tokio::test]
    async fn update_issue_status_without_the_write_scope_steps_up() {
        let pool = crate::db::open_test_pool().await;
        let org = seed(&pool, "issues-wscope", 7090).await;
        issue(&pool, "fp-wscope", 7090, "nope").await;

        let reader = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(org, Role::Owner)]);
        let err = call(
            &pool,
            reader,
            "update_issue_status",
            json!({ "fingerprint": "fp-wscope", "status": "resolved" }),
        )
        .await
        .expect_err("projects:write is required");
        assert_eq!(
            err,
            ToolError::Scope {
                required: SCOPE_PROJECTS_WRITE
            }
        );
    }

    #[tokio::test]
    async fn update_issue_status_rejects_an_unknown_status() {
        let pool = crate::db::open_test_pool().await;
        let org = seed(&pool, "issues-badstatus", 7100).await;
        issue(&pool, "fp-badstatus", 7100, "nope").await;

        let owner = McpPrincipal::for_test(READ_AND_WRITE, vec![(org, Role::Owner)]);
        let err = call(
            &pool,
            owner,
            "update_issue_status",
            json!({ "fingerprint": "fp-badstatus", "status": "wontfix" }),
        )
        .await
        .expect_err("unknown status");
        assert!(matches!(err, ToolError::Invalid(_)), "got {err:?}");

        let stored = crate::queries::issues::get_issue(&pool, "fp-badstatus")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.status.as_str(), "unresolved");
    }
}
