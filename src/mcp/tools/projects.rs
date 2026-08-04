//! Project tools: `list_projects` and `get_project` (org-wide reads),
//! `set_project_name`, `create_project` and `archive_project` (owner writes).

use serde_json::{json, Value};

use super::truncate::{clamp_limit, truncation_schema, Report, MAX_LIST_LIMIT};
use super::{
    bool_arg, i64_arg, internal, opt_str_arg, opt_u64_arg, prop, schema_object, str_arg, ToolCtx,
    ToolError,
};
use crate::mcp::principal::Target;

/// A project name longer than this is a mistake, not a name.
const MAX_PROJECT_NAME_CHARS: usize = 200;

pub(super) fn list_input() -> Value {
    schema_object(
        json!({
            "query": prop("string", "Optional substring match on project name or id."),
            "limit": {
                "type": "integer",
                "minimum": 1,
                "maximum": MAX_LIST_LIMIT,
                "description": "Maximum projects to return. Clamped server-side.",
            },
        }),
        &[],
    )
}

pub(super) fn list_output() -> Value {
    schema_object(
        json!({
            "projects": {
                "type": "array",
                "items": schema_object(
                    json!({
                        "project_id": prop("integer", "Project id, used by the other tools."),
                        "name": { "type": ["string", "null"] },
                        "org_id": prop("integer", "Owning organization id."),
                        "org_name": prop("string", "Owning organization label."),
                        "archived": prop("boolean", "Archived projects no longer ingest."),
                        "event_count": prop("integer", "Events of every kind."),
                        "error_count": prop("integer", "Error events only."),
                        "issue_count": prop("integer", "Distinct issues."),
                        "first_seen": { "type": ["integer", "null"] },
                        "last_seen": { "type": ["integer", "null"] },
                        "platforms": { "type": ["string", "null"] },
                        "latest_release": { "type": ["string", "null"] },
                    }),
                    &["project_id", "org_id", "org_name", "archived"],
                ),
            },
            "total": prop("integer", "Matching projects before the limit was applied."),
            "truncation": truncation_schema(),
        }),
        &["projects", "total", "truncation"],
    )
}

pub(super) async fn list_projects(
    ctx: &ToolCtx,
    args: &Value,
    _target: Target,
) -> Result<Value, ToolError> {
    let query = opt_str_arg(args, "query")?;
    let limit = clamp_limit(opt_u64_arg(args, "limit")?) as usize;

    let org_ids = ctx.principal.accessible_org_ids();
    let all =
        crate::queries::projects::list_projects_for_orgs(&ctx.pool, org_ids, None, query, None)
            .await
            .map_err(|e| internal("list_projects", format!("{e:#}")))?;

    let mut report = Report::default();
    let total = all.len();
    report.note_items_omitted(total.saturating_sub(limit));

    let projects: Vec<Value> = all
        .iter()
        .take(limit)
        .map(|p| {
            json!({
                "project_id": p.project_id,
                "name": report.opt_text(p.name.as_deref()),
                "org_id": p.org_id,
                "org_name": report.text(&p.org_name),
                "archived": p.archived,
                "event_count": p.event_count,
                "error_count": p.error_count,
                "issue_count": p.issue_count,
                "first_seen": p.first_seen,
                "last_seen": p.last_seen,
                "platforms": p.platforms,
                "latest_release": p.latest_release,
            })
        })
        .collect();

    Ok(json!({
        "projects": projects,
        "total": total,
        "truncation": report.to_json(),
    }))
}

pub(super) fn get_input() -> Value {
    schema_object(
        json!({
            "project_id": prop("integer", "Project to describe; see list_projects."),
        }),
        &["project_id"],
    )
}

pub(super) fn get_output() -> Value {
    schema_object(
        json!({
            "project_id": prop("integer", "Project id."),
            "org_id": prop("integer", "Owning organization id."),
            "name": { "type": ["string", "null"] },
            "status": prop("string", "active or archived."),
            "archived": prop("boolean", "Archived projects no longer ingest."),
            "source": {
                "type": ["string", "null"],
                "description": "How the project was created, e.g. `manual` or `auto`.",
            },
            "truncation": truncation_schema(),
        }),
        &["project_id", "org_id", "status", "archived", "truncation"],
    )
}

pub(super) async fn get_project(
    ctx: &ToolCtx,
    args: &Value,
    target: Target,
) -> Result<Value, ToolError> {
    let project_id = i64_arg(args, "project_id")?;
    debug_assert_eq!(target, Target::Project(project_id));

    let info = crate::queries::projects::get_project_info(&ctx.pool, project_id as u64)
        .await
        .map_err(|e| internal("get_project", format!("{e:#}")))?
        .ok_or_else(|| ToolError::NotFound("not found".to_string()))?;
    let org_id = crate::queries::orgs::org_of_project(&ctx.pool, project_id)
        .await
        .map_err(|e| internal("get_project", format!("{e:#}")))?
        .ok_or_else(|| ToolError::NotFound("not found".to_string()))?;

    let mut report = Report::default();
    Ok(json!({
        "project_id": project_id,
        "org_id": org_id,
        "name": report.opt_text(info.name.as_deref()),
        "status": info.status.as_str(),
        "archived": info.status.is_archived(),
        "source": info.source,
        "truncation": report.to_json(),
    }))
}

pub(super) fn rename_input() -> Value {
    schema_object(
        json!({
            "project_id": prop("integer", "Project to rename; see list_projects."),
            "name": prop("string", "New display name."),
        }),
        &["project_id", "name"],
    )
}

pub(super) fn rename_output() -> Value {
    schema_object(
        json!({
            "project_id": prop("integer", "Project that was renamed."),
            "previous_name": { "type": ["string", "null"], "description": "Name before this call." },
            "name": prop("string", "Name now."),
        }),
        &["project_id", "name"],
    )
}

pub(super) async fn set_project_name(
    ctx: &ToolCtx,
    args: &Value,
    target: Target,
) -> Result<Value, ToolError> {
    let project_id = i64_arg(args, "project_id")?;
    let name = str_arg(args, "name")?.trim();
    if name.is_empty() {
        return Err(ToolError::Invalid("`name` must not be empty".to_string()));
    }
    if name.chars().count() > MAX_PROJECT_NAME_CHARS {
        return Err(ToolError::Invalid(format!(
            "`name` must be at most {MAX_PROJECT_NAME_CHARS} characters"
        )));
    }
    debug_assert_eq!(target, Target::Project(project_id));

    let before = crate::queries::projects::get_project_info(&ctx.pool, project_id as u64)
        .await
        .map_err(|e| internal("set_project_name", format!("{e:#}")))?
        .ok_or_else(|| ToolError::NotFound("not found".to_string()))?;

    crate::queries::projects::set_project_name(&ctx.writer_pool, project_id as u64, name)
        .await
        .map_err(|e| internal("set_project_name", format!("{e:#}")))?;

    let mut report = Report::default();
    Ok(json!({
        "project_id": project_id,
        "previous_name": report.opt_text(before.name.as_deref()),
        "name": report.text(name),
    }))
}

pub(super) fn archive_input() -> Value {
    schema_object(
        json!({
            "project_id": prop("integer", "Project to archive or restore; see list_projects."),
            "archived": prop(
                "boolean",
                "True archives the project so it stops accepting events; false restores it.",
            ),
        }),
        &["project_id", "archived"],
    )
}

pub(super) fn archive_output() -> Value {
    schema_object(
        json!({
            "project_id": prop("integer", "Project that was updated."),
            "archived": prop("boolean", "State now."),
            "previously_archived": prop("boolean", "State before this call."),
            "changed": prop("boolean", "False when it was already in the requested state."),
        }),
        &["project_id", "archived", "previously_archived", "changed"],
    )
}

pub(super) async fn archive_project(
    ctx: &ToolCtx,
    args: &Value,
    target: Target,
) -> Result<Value, ToolError> {
    let project_id = i64_arg(args, "project_id")?;
    let archived = bool_arg(args, "archived")?;
    debug_assert_eq!(target, Target::Project(project_id));

    let before = crate::queries::projects::get_project_info(&ctx.pool, project_id as u64)
        .await
        .map_err(|e| internal("archive_project", format!("{e:#}")))?
        .ok_or_else(|| ToolError::NotFound("not found".to_string()))?;
    let previously_archived = before.status.is_archived();

    let affected = if archived {
        crate::queries::projects::archive_project(&ctx.writer_pool, project_id as u64).await
    } else {
        crate::queries::projects::unarchive_project(&ctx.writer_pool, project_id as u64).await
    }
    .map_err(|e| internal("archive_project", format!("{e:#}")))?;
    if affected == 0 {
        return Err(ToolError::NotFound("not found".to_string()));
    }

    // Without this, ingest keeps admitting (or keeps denying) the project's keys
    // until the cached entries expire.
    crate::ingest::auth::invalidate_project(
        &ctx.auth_cache,
        &ctx.negative_auth_cache,
        project_id as u64,
    );

    Ok(json!({
        "project_id": project_id,
        "archived": archived,
        "previously_archived": previously_archived,
        "changed": previously_archived != archived,
    }))
}

pub(super) fn create_input() -> Value {
    schema_object(
        json!({
            "org_id": prop("integer", "Organization to create the project in. You must own it."),
            "name": prop("string", "Display name for the project."),
            "platform": prop("string", "Optional platform label, e.g. `python` or `rust`."),
        }),
        &["org_id", "name"],
    )
}

pub(super) fn create_output() -> Value {
    schema_object(
        json!({
            "project_id": prop("integer", "Id of the created project."),
            "org_id": prop("integer", "Organization it was created in."),
            "name": prop("string", "Name it was created with."),
            "ingest_key_created": prop(
                "boolean",
                "An ingest key was created. Its value is deliberately not returned; read the DSN \
                 from the project's settings page.",
            ),
        }),
        &["project_id", "org_id", "name", "ingest_key_created"],
    )
}

pub(super) async fn create_project(
    ctx: &ToolCtx,
    args: &Value,
    target: Target,
) -> Result<Value, ToolError> {
    let org_id = i64_arg(args, "org_id")?;
    let name = str_arg(args, "name")?;
    if name.chars().count() > MAX_PROJECT_NAME_CHARS {
        return Err(ToolError::Invalid(format!(
            "`name` must be at most {MAX_PROJECT_NAME_CHARS} characters"
        )));
    }
    let platform = opt_str_arg(args, "platform")?;
    debug_assert_eq!(target, Target::Org(org_id));

    // The DSN this also mints is an ingest credential and is not returned.
    let (project_id, _public_key) =
        crate::queries::projects::create_project(&ctx.writer_pool, org_id, name, platform)
            .await
            .map_err(|e| internal("create_project", format!("{e:#}")))?;

    Ok(json!({
        "project_id": project_id,
        "org_id": org_id,
        "name": name,
        "ingest_key_created": true,
    }))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{call, seed_org, seed_project};
    use crate::mcp::principal::McpPrincipal;
    use crate::mcp::{SCOPE_ADMIN, SCOPE_PROJECTS_READ, SCOPE_PROJECTS_WRITE};
    use crate::orgs::Role;
    use serde_json::json;

    async fn stored(pool: &crate::db::DbPool, project_id: i64) -> crate::queries::ProjectInfo {
        crate::queries::projects::get_project_info(pool, project_id as u64)
            .await
            .unwrap()
            .expect("project row exists")
    }

    #[tokio::test]
    async fn list_projects_spans_every_accessible_org() {
        let pool = crate::db::open_test_pool().await;
        let a = seed_org(&pool, "proj-a").await;
        let b = seed_org(&pool, "proj-b").await;
        seed_project(&pool, 8001, a, "alpha").await;
        seed_project(&pool, 8002, b, "beta").await;

        let principal = McpPrincipal::for_test(
            SCOPE_PROJECTS_READ,
            vec![(a, Role::Owner), (b, Role::Member)],
        );
        let out = call(&pool, principal, "list_projects", json!({}))
            .await
            .unwrap();

        let mut ids: Vec<i64> = out["projects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["project_id"].as_i64().unwrap())
            .collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![8001, 8002]);
        assert_eq!(out["total"], 2);
        assert_eq!(out["truncation"]["truncated"], false);
        assert!(out["projects"][0]["org_name"].is_string());
    }

    #[tokio::test]
    async fn list_projects_never_shows_a_foreign_org() {
        let pool = crate::db::open_test_pool().await;
        let mine = seed_org(&pool, "proj-mine").await;
        let theirs = seed_org(&pool, "proj-theirs").await;
        seed_project(&pool, 8010, mine, "mine").await;
        seed_project(&pool, 8011, theirs, "theirs").await;

        let principal = McpPrincipal::for_test(SCOPE_PROJECTS_READ, vec![(mine, Role::Owner)]);
        let out = call(&pool, principal, "list_projects", json!({}))
            .await
            .unwrap();
        let ids: Vec<i64> = out["projects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["project_id"].as_i64().unwrap())
            .collect();
        assert_eq!(ids, vec![8010]);
    }

    #[tokio::test]
    async fn list_projects_clamps_the_requested_limit() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "proj-many").await;
        for i in 0..60 {
            seed_project(&pool, 8100 + i, org, &format!("p{i}")).await;
        }

        let principal = McpPrincipal::for_test(SCOPE_PROJECTS_READ, vec![(org, Role::Owner)]);
        let out = call(
            &pool,
            principal,
            "list_projects",
            json!({ "limit": 10_000 }),
        )
        .await
        .unwrap();

        assert_eq!(out["projects"].as_array().unwrap().len(), 50);
        assert_eq!(out["total"], 60);
        assert_eq!(out["truncation"]["truncated"], true);
        assert_eq!(out["truncation"]["list_items_omitted"], 10);
    }

    #[tokio::test]
    async fn get_project_returns_its_metadata_and_org() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "get-proj").await;
        seed_project(&pool, 8200, org, "alpha").await;

        let principal = McpPrincipal::for_test(SCOPE_PROJECTS_READ, vec![(org, Role::Owner)]);
        let out = call(
            &pool,
            principal,
            "get_project",
            json!({ "project_id": 8200 }),
        )
        .await
        .unwrap();

        assert_eq!(out["project_id"], 8200);
        assert_eq!(out["org_id"], org);
        assert_eq!(out["name"], "alpha");
        assert_eq!(out["status"], "active");
        assert_eq!(out["archived"], false);
        assert_eq!(out["source"], "manual");
    }

    #[tokio::test]
    async fn get_project_in_a_foreign_org_is_not_found() {
        let pool = crate::db::open_test_pool().await;
        let theirs = seed_org(&pool, "get-proj-theirs").await;
        seed_project(&pool, 8210, theirs, "secret").await;
        let mine = seed_org(&pool, "get-proj-mine").await;

        let outsider = McpPrincipal::for_test(SCOPE_PROJECTS_READ, vec![(mine, Role::Owner)]);
        let foreign = call(
            &pool,
            outsider,
            "get_project",
            json!({ "project_id": 8210 }),
        )
        .await
        .expect_err("a foreign project is not reachable");

        let absent = call(
            &pool,
            McpPrincipal::for_test(SCOPE_PROJECTS_READ, vec![(mine, Role::Owner)]),
            "get_project",
            json!({ "project_id": 999_999 }),
        )
        .await
        .expect_err("absent project");
        assert_eq!(foreign, absent, "existence must not be observable");
    }

    #[tokio::test]
    async fn get_project_without_the_read_scope_steps_up() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "get-proj-noscope").await;
        seed_project(&pool, 8220, org, "p").await;

        let principal = McpPrincipal::for_test("stackpit:events:read", vec![(org, Role::Owner)]);
        let err = call(
            &pool,
            principal,
            "get_project",
            json!({ "project_id": 8220 }),
        )
        .await
        .expect_err("projects:read is required");
        assert_eq!(
            err,
            super::ToolError::Scope {
                required: SCOPE_PROJECTS_READ
            }
        );
    }

    #[tokio::test]
    async fn set_project_name_writes_and_reports_the_previous_name() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "rename-ok").await;
        seed_project(&pool, 8300, org, "before").await;

        let owner = McpPrincipal::for_test(SCOPE_PROJECTS_WRITE, vec![(org, Role::Owner)]);
        let out = call(
            &pool,
            owner,
            "set_project_name",
            json!({ "project_id": 8300, "name": "  after  " }),
        )
        .await
        .unwrap();

        assert_eq!(out["previous_name"], "before");
        assert_eq!(out["name"], "after");
        assert_eq!(stored(&pool, 8300).await.name.as_deref(), Some("after"));
    }

    // The web UI requires the owner role to rename a project.
    #[tokio::test]
    async fn set_project_name_refuses_a_member_and_leaves_the_row_alone() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "rename-member").await;
        seed_project(&pool, 8310, org, "untouched").await;

        let member = McpPrincipal::for_test(SCOPE_PROJECTS_WRITE, vec![(org, Role::Member)]);
        let err = call(
            &pool,
            member,
            "set_project_name",
            json!({ "project_id": 8310, "name": "renamed" }),
        )
        .await
        .expect_err("members cannot rename projects");
        assert!(matches!(err, super::ToolError::Forbidden(_)), "{err:?}");
        assert_eq!(stored(&pool, 8310).await.name.as_deref(), Some("untouched"));
    }

    #[tokio::test]
    async fn set_project_name_in_a_foreign_org_is_not_found() {
        let pool = crate::db::open_test_pool().await;
        let theirs = seed_org(&pool, "rename-theirs").await;
        seed_project(&pool, 8320, theirs, "theirs").await;
        let mine = seed_org(&pool, "rename-mine").await;

        let outsider = McpPrincipal::for_test(SCOPE_PROJECTS_WRITE, vec![(mine, Role::Owner)]);
        let err = call(
            &pool,
            outsider,
            "set_project_name",
            json!({ "project_id": 8320, "name": "mine now" }),
        )
        .await
        .expect_err("a foreign project is not reachable");
        assert_eq!(err, super::ToolError::NotFound("not found".to_string()));
        assert_eq!(stored(&pool, 8320).await.name.as_deref(), Some("theirs"));
    }

    #[tokio::test]
    async fn set_project_name_without_the_write_scope_steps_up() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "rename-noscope").await;
        seed_project(&pool, 8330, org, "before").await;

        let reader = McpPrincipal::for_test(SCOPE_PROJECTS_READ, vec![(org, Role::Owner)]);
        let err = call(
            &pool,
            reader,
            "set_project_name",
            json!({ "project_id": 8330, "name": "after" }),
        )
        .await
        .expect_err("projects:write is required");
        assert_eq!(
            err,
            super::ToolError::Scope {
                required: SCOPE_PROJECTS_WRITE
            }
        );
        assert_eq!(stored(&pool, 8330).await.name.as_deref(), Some("before"));
    }

    #[tokio::test]
    async fn set_project_name_rejects_an_empty_or_oversized_name() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "rename-args").await;
        seed_project(&pool, 8340, org, "before").await;

        for args in [
            json!({ "project_id": 8340, "name": "   " }),
            json!({ "project_id": 8340, "name": "x".repeat(super::MAX_PROJECT_NAME_CHARS + 1) }),
            json!({ "project_id": 8340 }),
            json!({ "name": "after" }),
        ] {
            let err = call(
                &pool,
                McpPrincipal::for_test(SCOPE_PROJECTS_WRITE, vec![(org, Role::Owner)]),
                "set_project_name",
                args.clone(),
            )
            .await
            .expect_err("bad arguments");
            assert!(
                matches!(err, super::ToolError::Invalid(_)),
                "{args} produced {err:?}"
            );
        }
        assert_eq!(stored(&pool, 8340).await.name.as_deref(), Some("before"));
    }

    #[tokio::test]
    async fn archive_project_flips_the_status_both_ways() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "archive-ok").await;
        seed_project(&pool, 8400, org, "p").await;

        let owner = || McpPrincipal::for_test(SCOPE_ADMIN, vec![(org, Role::Owner)]);
        let out = call(
            &pool,
            owner(),
            "archive_project",
            json!({ "project_id": 8400, "archived": true }),
        )
        .await
        .unwrap();
        assert_eq!(out["archived"], true);
        assert_eq!(out["previously_archived"], false);
        assert_eq!(out["changed"], true);
        assert!(stored(&pool, 8400).await.status.is_archived());

        // Same tool, other direction; a no-op reports itself as unchanged.
        let again = call(
            &pool,
            owner(),
            "archive_project",
            json!({ "project_id": 8400, "archived": true }),
        )
        .await
        .unwrap();
        assert_eq!(again["changed"], false);

        let restored = call(
            &pool,
            owner(),
            "archive_project",
            json!({ "project_id": 8400, "archived": false }),
        )
        .await
        .unwrap();
        assert_eq!(restored["archived"], false);
        assert_eq!(restored["changed"], true);
        assert!(!stored(&pool, 8400).await.status.is_archived());
    }

    // Archiving stops ingest; the ingest auth cache has to be flushed with it or
    // the project's keys keep working until the entry expires.
    #[tokio::test]
    async fn archive_project_flushes_the_ingest_auth_caches() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "archive-cache").await;
        seed_project(&pool, 8410, org, "p").await;

        let state = crate::mcp::test_support::state_with_pool(pool.clone()).await;
        state.negative_auth_cache.insert(
            ("some-key".to_string(), 8410),
            crate::ingest::auth::NegativeEntry {
                denial: crate::ingest::auth::Denial::Denied("test"),
                inserted_at: std::time::Instant::now(),
            },
        );

        let tool = super::super::find("archive_project").expect("tool exists");
        super::super::invoke(
            tool,
            &state,
            std::sync::Arc::new(McpPrincipal::for_test(
                SCOPE_ADMIN,
                vec![(org, Role::Owner)],
            )),
            &json!({ "project_id": 8410, "archived": true }),
        )
        .await
        .unwrap();

        assert!(
            state.negative_auth_cache.is_empty(),
            "the project's cached denials must be dropped"
        );
    }

    #[tokio::test]
    async fn archive_project_refuses_a_member_and_leaves_the_row_alone() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "archive-member").await;
        seed_project(&pool, 8420, org, "p").await;

        let member = McpPrincipal::for_test(SCOPE_ADMIN, vec![(org, Role::Member)]);
        let err = call(
            &pool,
            member,
            "archive_project",
            json!({ "project_id": 8420, "archived": true }),
        )
        .await
        .expect_err("members cannot archive projects");
        assert!(matches!(err, super::ToolError::Forbidden(_)), "{err:?}");
        assert!(!stored(&pool, 8420).await.status.is_archived());
    }

    #[tokio::test]
    async fn archive_project_in_a_foreign_org_is_not_found() {
        let pool = crate::db::open_test_pool().await;
        let theirs = seed_org(&pool, "archive-theirs").await;
        seed_project(&pool, 8430, theirs, "p").await;
        let mine = seed_org(&pool, "archive-mine").await;

        let outsider = McpPrincipal::for_test(SCOPE_ADMIN, vec![(mine, Role::Owner)]);
        let err = call(
            &pool,
            outsider,
            "archive_project",
            json!({ "project_id": 8430, "archived": true }),
        )
        .await
        .expect_err("a foreign project is not reachable");
        assert_eq!(err, super::ToolError::NotFound("not found".to_string()));
        assert!(!stored(&pool, 8430).await.status.is_archived());
    }

    #[tokio::test]
    async fn archive_project_without_the_admin_scope_steps_up() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "archive-noscope").await;
        seed_project(&pool, 8440, org, "p").await;

        let writer = McpPrincipal::for_test(SCOPE_PROJECTS_WRITE, vec![(org, Role::Owner)]);
        let err = call(
            &pool,
            writer,
            "archive_project",
            json!({ "project_id": 8440, "archived": true }),
        )
        .await
        .expect_err("admin scope is required");
        assert_eq!(
            err,
            super::ToolError::Scope {
                required: SCOPE_ADMIN
            }
        );
        assert!(!stored(&pool, 8440).await.status.is_archived());
    }

    #[tokio::test]
    async fn archive_project_requires_an_explicit_boolean() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "archive-args").await;
        seed_project(&pool, 8450, org, "p").await;

        for args in [
            json!({ "project_id": 8450 }),
            json!({ "project_id": 8450, "archived": "yes" }),
            json!({ "project_id": 8450, "archived": 1 }),
            json!({ "archived": true }),
        ] {
            let err = call(
                &pool,
                McpPrincipal::for_test(SCOPE_ADMIN, vec![(org, Role::Owner)]),
                "archive_project",
                args.clone(),
            )
            .await
            .expect_err("bad arguments");
            assert!(
                matches!(err, super::ToolError::Invalid(_)),
                "{args} produced {err:?}"
            );
        }
        assert!(!stored(&pool, 8450).await.status.is_archived());
    }

    #[tokio::test]
    async fn create_project_is_owner_only_and_org_scoped() {
        let pool = crate::db::open_test_pool().await;
        let mine = seed_org(&pool, "create-mine").await;
        let theirs = seed_org(&pool, "create-theirs").await;
        let owner = McpPrincipal::for_test(SCOPE_ADMIN, vec![(mine, Role::Owner)]);

        let out = call(
            &pool,
            owner,
            "create_project",
            json!({ "org_id": mine, "name": "svc", "platform": "rust" }),
        )
        .await
        .unwrap();
        assert_eq!(out["org_id"], mine);
        assert_eq!(out["name"], "svc");
        assert_eq!(out["ingest_key_created"], true);

        // An org the caller is not a member of is indistinguishable from absent.
        let outsider = McpPrincipal::for_test(SCOPE_ADMIN, vec![(mine, Role::Owner)]);
        let err = call(
            &pool,
            outsider,
            "create_project",
            json!({ "org_id": theirs, "name": "svc" }),
        )
        .await
        .expect_err("a foreign org is not reachable");
        assert_eq!(err, super::ToolError::NotFound("not found".to_string()));
    }

    // A DSN is an ingest write credential; it must never reach an LLM context.
    #[tokio::test]
    async fn create_project_does_not_return_the_ingest_key() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "create-nokey").await;
        let owner = McpPrincipal::for_test(SCOPE_ADMIN, vec![(org, Role::Owner)]);
        let out = call(
            &pool,
            owner,
            "create_project",
            json!({ "org_id": org, "name": "svc" }),
        )
        .await
        .unwrap();

        let project_id = out["project_id"].as_i64().unwrap();
        let key: String = sqlx::query_scalar(crate::db::sql!(
            "SELECT public_key FROM project_keys WHERE project_id = ?1"
        ))
        .bind(project_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(!key.is_empty());
        assert!(!out.to_string().contains(&key), "the DSN key leaked");
    }

    #[tokio::test]
    async fn create_project_without_the_admin_scope_steps_up() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "create-noscope").await;
        let owner = McpPrincipal::for_test("stackpit:projects:write", vec![(org, Role::Owner)]);
        let err = call(
            &pool,
            owner,
            "create_project",
            json!({ "org_id": org, "name": "svc" }),
        )
        .await
        .expect_err("admin scope is required");
        assert_eq!(
            err,
            super::ToolError::Scope {
                required: SCOPE_ADMIN
            }
        );
    }

    #[tokio::test]
    async fn create_project_rejects_bad_arguments_as_tool_errors() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "create-args").await;

        for args in [
            json!({ "name": "svc" }),
            json!({ "org_id": "not-a-number", "name": "svc" }),
            json!({ "org_id": org }),
            json!({ "org_id": org, "name": "" }),
        ] {
            let err = call(
                &pool,
                McpPrincipal::for_test(SCOPE_ADMIN, vec![(org, Role::Owner)]),
                "create_project",
                args.clone(),
            )
            .await
            .expect_err("bad arguments");
            assert!(
                matches!(err, super::ToolError::Invalid(_)),
                "{args} produced {err:?}"
            );
        }
    }
}
