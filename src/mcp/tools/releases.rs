//! Release tools: `list_releases` (cross-org) and `get_release_health`.

use serde_json::{json, Value};

use super::truncate::{clamp_limit, truncation_schema, Report, MAX_LIST_LIMIT};
use super::{i64_arg, internal, opt_str_arg, opt_u64_arg, prop, schema_object, ToolCtx, ToolError};
use crate::mcp::principal::Target;
use crate::queries::types::{Page, ReleaseFilter};

/// Sorts `list_all_releases` understands; anything else folds to version order.
const RELEASE_SORTS: [&str; 6] = [
    "version",
    "first_seen",
    "last_seen",
    "events",
    "issues",
    "adoption",
];

/// Session rollups are stored per day, so a window finer than a day cannot be
/// answered; requests are floored to a day boundary.
const SECS_PER_DAY: i64 = 86_400;
const MAX_HEALTH_DAYS: u64 = 90;

pub(super) fn list_input() -> Value {
    schema_object(
        json!({
            "query": prop("string", "Substring match on the release version."),
            "project_id": prop("integer", "Narrow to one project. Must be a project you can reach."),
            "sort": {
                "type": "string",
                "enum": RELEASE_SORTS,
                "description": "Ordering; defaults to newest version first.",
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

pub(super) fn list_output() -> Value {
    schema_object(
        json!({
            "releases": {
                "type": "array",
                "items": schema_object(
                    json!({
                        "version": prop("string", "Release version string."),
                        "project_id": prop("integer", "Owning project."),
                        "project_name": { "type": ["string", "null"] },
                        "first_seen": prop("integer", "Unix seconds."),
                        "last_seen": prop("integer", "Unix seconds."),
                        "event_count": prop("integer", "Events carrying this release."),
                        "issue_count": prop("integer", "Distinct issues in this release."),
                        "adoption": prop(
                            "number",
                            "Percent of the project's last 24 hours of events on this release.",
                        ),
                    }),
                    &["version", "project_id", "event_count", "issue_count"],
                ),
            },
            "total": prop("integer", "Releases matching the filter."),
            "offset": prop("integer", "Offset this page starts at."),
            "limit": prop("integer", "Page size actually applied."),
            "truncation": truncation_schema(),
        }),
        &["releases", "total", "offset", "limit", "truncation"],
    )
}

pub(super) async fn list_releases(
    ctx: &ToolCtx,
    args: &Value,
    _target: Target,
) -> Result<Value, ToolError> {
    let sort = opt_str_arg(args, "sort")?;
    if let Some(s) = sort {
        if !RELEASE_SORTS.contains(&s) {
            return Err(ToolError::Invalid(format!(
                "`sort` must be one of {}",
                RELEASE_SORTS.join(", ")
            )));
        }
    }
    let filter = ReleaseFilter {
        project_id: opt_u64_arg(args, "project_id")?,
        query: opt_str_arg(args, "query")?.map(str::to_string),
        sort: sort.map(str::to_string),
    };
    let limit = clamp_limit(opt_u64_arg(args, "limit")?);
    let page = Page::new(opt_u64_arg(args, "offset")?, Some(limit));

    // A `project_id` outside the caller's orgs simply matches nothing: the org
    // scope is ANDed in, never replaced by the argument.
    let result = crate::queries::releases::list_all_releases_for_orgs(
        &ctx.pool,
        &filter,
        &page,
        None,
        ctx.principal.accessible_org_ids(),
    )
    .await
    .map_err(|e| internal("list_releases", format!("{e:#}")))?;

    let mut report = Report::default();
    report.note_items_omitted(
        (result.total as usize).saturating_sub(result.offset as usize + result.items.len()),
    );
    let releases: Vec<Value> = result
        .items
        .iter()
        .map(|r| {
            json!({
                "version": report.text(&r.version),
                "project_id": r.project_id,
                "project_name": report.opt_text(r.project_name.as_deref()),
                "first_seen": r.first_seen,
                "last_seen": r.last_seen,
                "event_count": r.event_count,
                "issue_count": r.issue_count,
                "adoption": r.adoption,
            })
        })
        .collect();

    Ok(json!({
        "releases": releases,
        "total": result.total,
        "offset": result.offset,
        "limit": result.limit,
        "truncation": report.to_json(),
    }))
}

pub(super) fn health_input() -> Value {
    schema_object(
        json!({
            "project_id": prop("integer", "Project to report on; see list_projects."),
            "days": {
                "type": "integer",
                "minimum": 1,
                "maximum": MAX_HEALTH_DAYS,
                "description": "Window in days, floored to a day boundary. Omit for all time.",
            },
            "limit": {
                "type": "integer",
                "minimum": 1,
                "maximum": MAX_LIST_LIMIT,
                "description": "Maximum releases to return. Clamped server-side.",
            },
        }),
        &["project_id"],
    )
}

pub(super) fn health_output() -> Value {
    schema_object(
        json!({
            "project_id": prop("integer", "Project the figures belong to."),
            "releases": {
                "type": "array",
                "description": "Busiest release first.",
                "items": schema_object(
                    json!({
                        "release": prop("string", "Release version, or `(no release)`."),
                        "total_sessions": prop("integer", "Sessions recorded in the window."),
                        "ok_count": prop("integer", "Sessions that ended cleanly."),
                        "crashed_count": prop("integer", "Sessions that crashed."),
                        "errored_count": prop("integer", "Sessions that saw a handled error."),
                        "crash_free_rate": prop("number", "Percent of sessions that did not crash."),
                        "crash_free_users": {
                            "type": ["number", "null"],
                            "description": "Null when an identity-less aggregate contributed, so users cannot be counted.",
                        },
                        "total_users": { "type": ["integer", "null"] },
                    }),
                    &["release", "total_sessions", "crash_free_rate"],
                ),
            },
            "total": prop("integer", "Releases with sessions in the window."),
            "truncation": truncation_schema(),
        }),
        &["project_id", "releases", "total", "truncation"],
    )
}

pub(super) async fn get_release_health(
    ctx: &ToolCtx,
    args: &Value,
    target: Target,
) -> Result<Value, ToolError> {
    let project_id = i64_arg(args, "project_id")?;
    debug_assert_eq!(target, Target::Project(project_id));

    let since_ts = match opt_u64_arg(args, "days")? {
        None => None,
        Some(days) => {
            if days == 0 || days > MAX_HEALTH_DAYS {
                return Err(ToolError::Invalid(format!(
                    "`days` must be between 1 and {MAX_HEALTH_DAYS}"
                )));
            }
            let now = chrono::Utc::now().timestamp();
            Some(((now - days as i64 * SECS_PER_DAY) / SECS_PER_DAY) * SECS_PER_DAY)
        }
    };
    let limit = clamp_limit(opt_u64_arg(args, "limit")?) as usize;

    // Busiest first: with a page this small, the releases carrying traffic are
    // the ones worth reading.
    let all = crate::queries::releases::get_release_health(
        &ctx.pool,
        project_id as u64,
        since_ts,
        crate::queries::releases::ReleaseHealthSort::Sessions,
    )
    .await
    .map_err(|e| internal("get_release_health", format!("{e:#}")))?;

    let mut report = Report::default();
    let total = all.len();
    report.note_items_omitted(total.saturating_sub(limit));
    let releases: Vec<Value> = all
        .iter()
        .take(limit)
        .map(|r| {
            json!({
                "release": report.text(&r.release),
                "total_sessions": r.total_sessions,
                "ok_count": r.ok_count,
                "crashed_count": r.crashed_count,
                "errored_count": r.errored_count,
                "crash_free_rate": r.crash_free_rate,
                "crash_free_users": r.crash_free_users,
                "total_users": r.total_users,
            })
        })
        .collect();

    Ok(json!({
        "project_id": project_id,
        "releases": releases,
        "total": total,
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

    async fn seed_release(pool: &DbPool, project_id: i64, version: &str) {
        crate::queries::releases::upsert_release(
            pool,
            project_id as u64,
            &crate::queries::releases::ReleaseUpsert {
                version,
                commit_sha: None,
                date_released: Some(1_000),
                first_event: Some(1_000),
                last_event: Some(2_000),
                new_groups: 0,
            },
        )
        .await
        .unwrap();
    }

    async fn seed_sessions(
        pool: &DbPool,
        project_id: i64,
        release: &str,
        total: i64,
        crashed: i64,
    ) {
        sqlx::query(sql!(
            "INSERT INTO session_aggregates (project_id, release, environment, day_bucket, sessions_total, sessions_crashed, sessions_errored, sessions_abnormal, has_aggregate, first_seen, last_seen)
             VALUES (?1, ?2, 'prod', 0, ?3, ?4, 0, 0, 1, 0, 0)"
        ))
        .bind(project_id)
        .bind(release)
        .bind(total)
        .bind(crashed)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn list_releases_spans_every_accessible_org() {
        let pool = crate::db::open_test_pool().await;
        let a = seed_org(&pool, "rel-a").await;
        let b = seed_org(&pool, "rel-b").await;
        seed_project(&pool, 5001, a, "alpha").await;
        seed_project(&pool, 5002, b, "beta").await;
        seed_release(&pool, 5001, "1.0.0").await;
        seed_release(&pool, 5002, "2.0.0").await;

        let principal =
            McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(a, Role::Owner), (b, Role::Member)]);
        let out = call(&pool, principal, "list_releases", json!({}))
            .await
            .unwrap();

        let mut versions: Vec<&str> = out["releases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["version"].as_str().unwrap())
            .collect();
        versions.sort_unstable();
        assert_eq!(versions, vec!["1.0.0", "2.0.0"]);
        assert_eq!(out["total"], 2);
    }

    // The highest-risk path in this wave: one org id list, one query, and a
    // foreign tenant's releases must not appear in it.
    #[tokio::test]
    async fn list_releases_never_shows_a_foreign_tenants_rows() {
        let pool = crate::db::open_test_pool().await;
        let mine = seed_org(&pool, "rel-mine").await;
        let theirs = seed_org(&pool, "rel-theirs").await;
        seed_project(&pool, 5010, mine, "mine").await;
        seed_project(&pool, 5011, theirs, "theirs").await;
        seed_release(&pool, 5010, "mine-1.0").await;
        seed_release(&pool, 5011, "theirs-1.0").await;

        let principal = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(mine, Role::Owner)]);
        let out = call(&pool, principal, "list_releases", json!({}))
            .await
            .unwrap();
        assert_eq!(out["total"], 1);
        assert_eq!(out["releases"][0]["version"], "mine-1.0");

        // Naming the foreign project explicitly must not widen the scope either.
        let principal = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(mine, Role::Owner)]);
        let out = call(
            &pool,
            principal,
            "list_releases",
            json!({ "project_id": 5011 }),
        )
        .await
        .unwrap();
        assert_eq!(out["total"], 0);
        assert_eq!(out["releases"], json!([]));
    }

    // Zero memberships must produce an empty result, not an unscoped one:
    // `IN ()` is invalid SQL, so this asserts the short-circuit exists.
    #[tokio::test]
    async fn list_releases_with_no_memberships_returns_nothing() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "rel-orphan").await;
        seed_project(&pool, 5020, org, "p").await;
        seed_release(&pool, 5020, "1.0.0").await;

        let orphan = McpPrincipal::for_test(SCOPE_EVENTS_READ, Vec::new());
        let out = call(&pool, orphan, "list_releases", json!({}))
            .await
            .unwrap();
        assert_eq!(out["total"], 0);
        assert_eq!(out["releases"], json!([]));
    }

    #[tokio::test]
    async fn list_releases_without_the_read_scope_steps_up() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "rel-noscope").await;
        let principal = McpPrincipal::for_test("stackpit:projects:read", vec![(org, Role::Owner)]);
        let err = call(&pool, principal, "list_releases", json!({}))
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
    async fn release_health_reports_crash_free_rates_busiest_first() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "health-org").await;
        seed_project(&pool, 5030, org, "p").await;
        seed_sessions(&pool, 5030, "1.0.0", 10, 1).await;
        seed_sessions(&pool, 5030, "2.0.0", 100, 5).await;

        let principal = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(org, Role::Owner)]);
        let out = call(
            &pool,
            principal,
            "get_release_health",
            json!({ "project_id": 5030 }),
        )
        .await
        .unwrap();

        assert_eq!(out["total"], 2);
        assert_eq!(out["releases"][0]["release"], "2.0.0");
        assert_eq!(out["releases"][0]["total_sessions"], 100);
        assert_eq!(out["releases"][0]["crash_free_rate"], 95.0);
        assert_eq!(out["releases"][1]["release"], "1.0.0");
        // An identity-less aggregate contributed, so users cannot be counted.
        assert!(out["releases"][0]["crash_free_users"].is_null());
    }

    #[tokio::test]
    async fn release_health_in_a_foreign_project_is_not_found() {
        let pool = crate::db::open_test_pool().await;
        let theirs = seed_org(&pool, "health-theirs").await;
        seed_project(&pool, 5040, theirs, "theirs").await;
        seed_sessions(&pool, 5040, "1.0.0", 5, 0).await;
        let mine = seed_org(&pool, "health-mine").await;

        let outsider = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(mine, Role::Owner)]);
        let err = call(
            &pool,
            outsider,
            "get_release_health",
            json!({ "project_id": 5040 }),
        )
        .await
        .expect_err("a foreign project is not reachable");
        assert_eq!(err, ToolError::NotFound("not found".to_string()));
    }

    #[tokio::test]
    async fn release_health_without_the_read_scope_steps_up() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "health-noscope").await;
        seed_project(&pool, 5050, org, "p").await;

        let principal = McpPrincipal::for_test("stackpit:projects:read", vec![(org, Role::Owner)]);
        let err = call(
            &pool,
            principal,
            "get_release_health",
            json!({ "project_id": 5050 }),
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
    async fn release_health_rejects_an_out_of_range_window() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "health-days").await;
        seed_project(&pool, 5060, org, "p").await;

        let principal = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(org, Role::Owner)]);
        let err = call(
            &pool,
            principal,
            "get_release_health",
            json!({ "project_id": 5060, "days": 3650 }),
        )
        .await
        .expect_err("window is capped");
        assert!(matches!(err, ToolError::Invalid(_)), "got {err:?}");
    }
}
