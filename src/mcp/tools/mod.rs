//! The MCP tool table and its dispatcher.
//!
//! A row declares what a tool needs ([`ToolPermission`]), how to derive the
//! resource it names ([`Target`]) and how to run it. The dispatcher resolves the
//! target, runs it past [`authorize_tool`] and only then calls the handler, so
//! every tool passes the one authorization choke point without saying so itself.

mod events;
mod identity;
mod issues;
mod projects;
mod releases;
mod traces;
mod trackers;
mod truncate;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use rmcp::model::{object, CallToolResult, ContentBlock, Tool, ToolAnnotations};
use serde_json::{json, Value};
use stackpit_auth::GrantedScopes;

use super::principal::{authorize_tool, Denied, McpPrincipal, Target, ToolPermission};
use super::{McpState, SCOPE_ADMIN, SCOPE_EVENTS_READ, SCOPE_PROJECTS_READ, SCOPE_PROJECTS_WRITE};
use crate::db::DbPool;
use crate::ingest::auth::{AuthCache, NegativeAuthCache};
use crate::util::crypto::SecretEncryptor;

type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Everything a handler is allowed to touch.
pub(super) struct ToolCtx {
    pub principal: Arc<McpPrincipal>,
    pub pool: DbPool,
    pub writer_pool: DbPool,
    pub encryptor: Option<Arc<SecretEncryptor>>,
    pub license: crate::commercial::LicenseHandle,
    pub auth_cache: AuthCache,
    pub negative_auth_cache: NegativeAuthCache,
    pub web_base: String,
}

impl ToolCtx {
    pub(super) fn new(state: &McpState, principal: Arc<McpPrincipal>) -> Self {
        Self {
            principal,
            pool: state.pool.clone(),
            writer_pool: state.writer_pool.clone(),
            encryptor: state.encryptor.clone(),
            license: state.license.clone(),
            auth_cache: state.auth_cache.clone(),
            negative_auth_cache: state.negative_auth_cache.clone(),
            web_base: state.web_base.clone(),
        }
    }
}

/// Why a tool call did not produce a result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ToolError {
    /// Bad or missing arguments. Reported as a tool execution error rather than
    /// a protocol error so the model can read it and correct itself.
    Invalid(String),
    NotFound(String),
    Forbidden(String),
    /// The one refusal that leaves the JSON-RPC envelope: the client needs the
    /// `WWW-Authenticate` challenge to know which scope to ask for.
    Scope {
        required: &'static str,
    },
    /// Generic to the client; the real cause is logged.
    Internal,
}

impl ToolError {
    fn message(&self) -> String {
        match self {
            ToolError::Invalid(m) | ToolError::NotFound(m) | ToolError::Forbidden(m) => m.clone(),
            ToolError::Scope { required } => format!("this tool requires the {required} scope"),
            ToolError::Internal => "internal error".to_string(),
        }
    }

    fn outcome(&self) -> &'static str {
        match self {
            ToolError::Invalid(_) => "invalid_arguments",
            ToolError::NotFound(_) => "not_found",
            ToolError::Forbidden(_) => "forbidden",
            ToolError::Scope { .. } => "insufficient_scope",
            ToolError::Internal => "error",
        }
    }
}

impl From<Denied> for ToolError {
    fn from(denied: Denied) -> Self {
        match denied {
            Denied::Scope { required } => ToolError::Scope { required },
            // Same answer for "absent" and "in an org you are not in".
            Denied::NotFound => ToolError::NotFound("not found".to_string()),
            Denied::Forbidden => {
                ToolError::Forbidden("this action requires the owner role in that org".to_string())
            }
            Denied::Unavailable => ToolError::Internal,
        }
    }
}

/// Generic message out, real cause to the log.
pub(super) fn internal(tool: &str, e: impl std::fmt::Display) -> ToolError {
    tracing::error!(tool, "mcp tool internal error: {e}");
    ToolError::Internal
}

type TargetFn = for<'a> fn(&'a ToolCtx, &'a Value) -> BoxFut<'a, Result<Target, ToolError>>;
type RunFn = for<'a> fn(&'a ToolCtx, &'a Value, Target) -> BoxFut<'a, Result<Value, ToolError>>;

pub(super) struct ToolDef {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub permission: ToolPermission,
    pub read_only: bool,
    pub destructive: bool,
    pub input_schema: fn() -> Value,
    pub output_schema: fn() -> Value,
    /// Resolves the resource the arguments name, before authorization.
    pub target: TargetFn,
    pub run: RunFn,
}

const fn permission(scope: &'static str, owner_only: bool) -> ToolPermission {
    ToolPermission { scope, owner_only }
}

/// Tools with no org- or project-scoped resource.
fn no_target<'a>(_ctx: &'a ToolCtx, _args: &'a Value) -> BoxFut<'a, Result<Target, ToolError>> {
    Box::pin(async { Ok(Target::None) })
}

/// The shared sentence every description carries: authorizing an MCP client
/// grants it the caller's whole org, because Stackpit has no per-project access
/// control. The person clicking Allow deserves to read that here.
const ORG_REACH: &str =
    " Covers every project in every organization you belong to; Stackpit has no per-project \
     access control, so this sees exactly what you see in the web UI.";

/// Deterministic order: this is the order `tools/list` reports.
pub(super) const TOOLS: &[ToolDef] = &[
    ToolDef {
        name: "whoami",
        title: "Who am I",
        description: "Report the identity behind this connection: subject, Stackpit user id, \
                      OAuth client, granted scopes and the organizations reachable from it. Call \
                      this first when another tool refuses, to see what you actually hold.",
        permission: permission("", false),
        read_only: true,
        destructive: false,
        input_schema: identity::whoami_input,
        output_schema: identity::whoami_output,
        target: no_target,
        run: |ctx, args, target| Box::pin(identity::whoami(ctx, args, target)),
    },
    ToolDef {
        name: "list_projects",
        title: "List projects",
        description: "List Stackpit projects with their event and issue counts.",
        permission: permission(SCOPE_PROJECTS_READ, false),
        read_only: true,
        destructive: false,
        input_schema: projects::list_input,
        output_schema: projects::list_output,
        target: no_target,
        run: |ctx, args, target| Box::pin(projects::list_projects(ctx, args, target)),
    },
    ToolDef {
        name: "get_project",
        title: "Get project",
        description: "Fetch one project's metadata: name, organization, archived state and how it \
                      was created. Use list_projects for event and issue counts.",
        permission: permission(SCOPE_PROJECTS_READ, false),
        read_only: true,
        destructive: false,
        input_schema: projects::get_input,
        output_schema: projects::get_output,
        target: |_ctx, args| Box::pin(async move { project_target(args) }),
        run: |ctx, args, target| Box::pin(projects::get_project(ctx, args, target)),
    },
    ToolDef {
        name: "list_issues",
        title: "List issues",
        description: "List a project's issues, newest activity first, with optional status, \
                      level and title filters.",
        permission: permission(SCOPE_EVENTS_READ, false),
        read_only: true,
        destructive: false,
        input_schema: issues::list_input,
        output_schema: issues::list_output,
        target: |_ctx, args| Box::pin(async move { project_target(args) }),
        run: |ctx, args, target| Box::pin(issues::list_issues(ctx, args, target)),
    },
    ToolDef {
        name: "get_issue",
        title: "Get issue",
        description: "Fetch one issue by fingerprint: counts, first and last seen, a 24-hour \
                      event trend and any linked tracker issues.",
        permission: permission(SCOPE_EVENTS_READ, false),
        read_only: true,
        destructive: false,
        input_schema: issues::get_input,
        output_schema: issues::get_output,
        target: |ctx, args| Box::pin(issues::fingerprint_target(ctx, args)),
        run: |ctx, args, target| Box::pin(issues::get_issue(ctx, args, target)),
    },
    ToolDef {
        name: "get_latest_event",
        title: "Get latest event for an issue",
        description: "Fetch the most recent event of an issue: exceptions with stack frames, \
                      breadcrumbs and tags. Large fields are truncated and the response reports \
                      what was dropped.",
        permission: permission(SCOPE_EVENTS_READ, false),
        read_only: true,
        destructive: false,
        input_schema: events::latest_input,
        output_schema: events::latest_output,
        target: |ctx, args| Box::pin(issues::fingerprint_target(ctx, args)),
        run: |ctx, args, target| Box::pin(events::get_latest_event(ctx, args, target)),
    },
    ToolDef {
        name: "get_event",
        title: "Get event",
        description: "Fetch one event by id: exceptions with stack frames, breadcrumbs and tags. \
                      The raw payload, request data and user identity are never returned; large \
                      fields are truncated and the response reports what was dropped.",
        permission: permission(SCOPE_EVENTS_READ, false),
        read_only: true,
        destructive: false,
        input_schema: events::get_input,
        output_schema: events::latest_output,
        target: |ctx, args| Box::pin(events::event_target(ctx, args)),
        run: |ctx, args, target| Box::pin(events::get_event(ctx, args, target)),
    },
    ToolDef {
        name: "search_events",
        title: "Search events",
        description: "Search raw events across every project you can reach, by title, level, kind \
                      or project, newest first.",
        permission: permission(SCOPE_EVENTS_READ, false),
        read_only: true,
        destructive: false,
        input_schema: events::search_input,
        output_schema: events::search_output,
        target: no_target,
        run: |ctx, args, target| Box::pin(events::search_events(ctx, args, target)),
    },
    ToolDef {
        name: "list_releases",
        title: "List releases",
        description: "List releases across every project you can reach, with event and issue \
                      counts and the share of recent traffic on each.",
        permission: permission(SCOPE_EVENTS_READ, false),
        read_only: true,
        destructive: false,
        input_schema: releases::list_input,
        output_schema: releases::list_output,
        target: no_target,
        run: |ctx, args, target| Box::pin(releases::list_releases(ctx, args, target)),
    },
    ToolDef {
        name: "get_release_health",
        title: "Get release health",
        description: "Session health per release for one project: totals, crashes and crash-free \
                      rates, busiest release first.",
        permission: permission(SCOPE_EVENTS_READ, false),
        read_only: true,
        destructive: false,
        input_schema: releases::health_input,
        output_schema: releases::health_output,
        target: |_ctx, args| Box::pin(async move { project_target(args) }),
        run: |ctx, args, target| Box::pin(releases::get_release_health(ctx, args, target)),
    },
    ToolDef {
        name: "list_traces",
        title: "List traces",
        description: "List a project's recent traces with span counts and durations, newest \
                      first.",
        permission: permission(SCOPE_EVENTS_READ, false),
        read_only: true,
        destructive: false,
        input_schema: traces::list_input,
        output_schema: traces::list_output,
        target: |_ctx, args| Box::pin(async move { project_target(args) }),
        run: |ctx, args, target| Box::pin(traces::list_traces(ctx, args, target)),
    },
    ToolDef {
        name: "get_trace",
        title: "Get trace",
        description: "Fetch one trace: its owning transaction, the span tree with durations and \
                      nesting, and the error events that share the trace.",
        permission: permission(SCOPE_EVENTS_READ, false),
        read_only: true,
        destructive: false,
        input_schema: traces::get_input,
        output_schema: traces::get_output,
        target: |_ctx, args| Box::pin(async move { project_target(args) }),
        run: |ctx, args, target| Box::pin(traces::get_trace(ctx, args, target)),
    },
    ToolDef {
        name: "update_issue_status",
        title: "Update issue status",
        description: "Set an issue to unresolved, resolved or ignored. Requires the owner role \
                      in the organization that owns the project, the same rule as the web UI.",
        permission: permission(SCOPE_PROJECTS_WRITE, true),
        read_only: false,
        destructive: false,
        input_schema: issues::update_input,
        output_schema: issues::update_output,
        target: |ctx, args| Box::pin(issues::fingerprint_target(ctx, args)),
        run: |ctx, args, target| Box::pin(issues::update_issue_status(ctx, args, target)),
    },
    ToolDef {
        name: "set_project_name",
        title: "Rename a project",
        description: "Change a project's display name. Requires the owner role in the \
                      organization that owns it, the same rule as the web UI.",
        permission: permission(SCOPE_PROJECTS_WRITE, true),
        read_only: false,
        destructive: false,
        input_schema: projects::rename_input,
        output_schema: projects::rename_output,
        target: |_ctx, args| Box::pin(async move { project_target(args) }),
        run: |ctx, args, target| Box::pin(projects::set_project_name(ctx, args, target)),
    },
    ToolDef {
        name: "create_tracker_issue",
        title: "Create a tracker issue",
        description: "Open an issue for a Stackpit issue in a configured GitHub, Forgejo or \
                      GitLab integration and link the two. The tracker is called with the \
                      integration's own stored credential, never with yours. Calling it twice \
                      returns the existing link instead of opening a second tracker issue. \
                      Requires the owner role in the organization that owns the project.",
        permission: permission(SCOPE_PROJECTS_WRITE, true),
        read_only: false,
        destructive: false,
        input_schema: trackers::create_input,
        output_schema: trackers::create_output,
        target: |ctx, args| Box::pin(issues::fingerprint_target(ctx, args)),
        run: |ctx, args, target| Box::pin(trackers::create_tracker_issue(ctx, args, target)),
    },
    ToolDef {
        name: "create_project",
        title: "Create project",
        description: "Create a project in one of your organizations. Requires the owner role \
                      there. The ingest key created with it is not returned; read it from the \
                      project's settings page in the web UI.",
        permission: permission(SCOPE_ADMIN, true),
        read_only: false,
        destructive: false,
        input_schema: projects::create_input,
        output_schema: projects::create_output,
        target: |_ctx, args| Box::pin(async move { org_target(args) }),
        run: |ctx, args, target| Box::pin(projects::create_project(ctx, args, target)),
    },
    ToolDef {
        name: "archive_project",
        title: "Archive or restore a project",
        description: "Archive a project so it stops accepting events, or restore an archived one. \
                      Pass `archived` to choose which. Stored events are kept either way. \
                      Requires the owner role in the organization that owns it.",
        permission: permission(SCOPE_ADMIN, true),
        read_only: false,
        destructive: true,
        input_schema: projects::archive_input,
        output_schema: projects::archive_output,
        target: |_ctx, args| Box::pin(async move { project_target(args) }),
        run: |ctx, args, target| Box::pin(projects::archive_project(ctx, args, target)),
    },
];

/// Tool names are `[A-Za-z0-9_.-]`; asserted rather than validated at runtime.
#[cfg(test)]
fn name_is_wellformed(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
}

pub(super) fn find(name: &str) -> Option<&'static ToolDef> {
    TOOLS.iter().find(|t| t.name == name)
}

/// Every tool, whatever the token currently carries. Filtering to the granted
/// scopes would be legal, but it deadlocks incremental consent: first consent
/// grants only the read scopes `scopes_supported` advertises, so a filtered list
/// hides the write tools, the client never calls one, and the 403 that would
/// prompt the step-up never happens. Listing them and refusing at call time is
/// what makes the scope reachable.
pub(super) fn list(scopes: &GrantedScopes) -> Vec<Tool> {
    TOOLS.iter().map(|t| descriptor(t, scopes)).collect()
}

pub(super) fn descriptor(tool: &ToolDef, scopes: &GrantedScopes) -> Tool {
    let granted = tool.permission.scope.is_empty() || scopes.has(tool.permission.scope);
    let scope_note = if granted {
        String::new()
    } else {
        format!(
            " Requires the `{}` scope, which this token does not carry: calling it returns 403 and asks your client to request that scope.",
            tool.permission.scope
        )
    };
    Tool::new(
        tool.name,
        format!("{}{}{}", tool.description, ORG_REACH, scope_note),
        object((tool.input_schema)()),
    )
    .with_title(tool.title)
    .with_raw_output_schema(Arc::new(object((tool.output_schema)())))
    .with_annotations(
        ToolAnnotations::with_title(tool.title)
            .read_only(tool.read_only)
            .destructive(tool.destructive),
    )
}

/// Resolve the target, authorize, run. The only caller of [`authorize_tool`].
pub(super) async fn invoke(
    tool: &ToolDef,
    state: &McpState,
    principal: Arc<McpPrincipal>,
    args: &Value,
) -> Result<Value, ToolError> {
    let ctx = ToolCtx::new(state, principal);

    let target = (tool.target)(&ctx, args).await?;
    authorize_tool(&ctx.principal, &ctx.pool, tool.permission, target).await?;

    let result = (tool.run)(&ctx, args, target).await;
    if !tool.read_only {
        audit(&ctx, tool.name, target, &result);
    }
    result
}

/// Write-tool audit trail. Structured `tracing`, not a DB table: Stackpit has no
/// audit-log subsystem and building one is a separate decision.
fn audit(ctx: &ToolCtx, tool: &str, target: Target, result: &Result<Value, ToolError>) {
    let (target_kind, target_id) = match target {
        Target::None => ("none", 0),
        Target::Project(id) => ("project", id),
        Target::Org(id) => ("org", id),
    };
    tracing::info!(
        auth_source = "mcp",
        iss = %ctx.principal.iss,
        sub = %ctx.principal.sub,
        user_id = ctx.principal.user_id,
        client_id = ctx.principal.client_id.as_deref().unwrap_or("-"),
        tool,
        target_kind,
        target_id,
        outcome = result.as_ref().map_or_else(ToolError::outcome, |_| "ok"),
        "mcp write tool",
    );
}

// Result envelopes

/// Both a text block and `structuredContent`: the text copy is what a client
/// that predates structured output still renders.
pub(super) fn success(structured: Value) -> CallToolResult {
    let text = serde_json::to_string_pretty(&structured).unwrap_or_else(|_| "{}".to_string());
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    result.structured_content = Some(structured);
    result
}

pub(super) fn failure(err: &ToolError) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(err.message())])
}

/// The one argument mistake no input schema can describe.
pub(super) fn non_object_arguments() -> ToolError {
    ToolError::Invalid("arguments must be a JSON object".to_string())
}

// Shared argument handling

fn project_target(args: &Value) -> Result<Target, ToolError> {
    Ok(Target::Project(i64_arg(args, "project_id")?))
}

fn org_target(args: &Value) -> Result<Target, ToolError> {
    Ok(Target::Org(i64_arg(args, "org_id")?))
}

pub(super) fn str_arg<'a>(args: &'a Value, name: &str) -> Result<&'a str, ToolError> {
    match args.get(name) {
        Some(Value::String(s)) if !s.trim().is_empty() => Ok(s.as_str()),
        Some(Value::String(_)) => Err(ToolError::Invalid(format!("`{name}` must not be empty"))),
        Some(_) => Err(ToolError::Invalid(format!("`{name}` must be a string"))),
        None => Err(ToolError::Invalid(format!("`{name}` is required"))),
    }
}

pub(super) fn opt_str_arg<'a>(args: &'a Value, name: &str) -> Result<Option<&'a str>, ToolError> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.trim().is_empty() => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.as_str())),
        Some(_) => Err(ToolError::Invalid(format!("`{name}` must be a string"))),
    }
}

pub(super) fn bool_arg(args: &Value, name: &str) -> Result<bool, ToolError> {
    match args.get(name) {
        Some(Value::Bool(b)) => Ok(*b),
        Some(v) => Err(ToolError::Invalid(format!(
            "`{name}` must be true or false, got {v}"
        ))),
        None => Err(ToolError::Invalid(format!("`{name}` is required"))),
    }
}

pub(super) fn i64_arg(args: &Value, name: &str) -> Result<i64, ToolError> {
    match args.get(name) {
        Some(v) => v
            .as_i64()
            .ok_or_else(|| ToolError::Invalid(format!("`{name}` must be an integer, got {v}"))),
        None => Err(ToolError::Invalid(format!("`{name}` is required"))),
    }
}

pub(super) fn opt_u64_arg(args: &Value, name: &str) -> Result<Option<u64>, ToolError> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .ok_or_else(|| {
                ToolError::Invalid(format!("`{name}` must be a non-negative integer, got {v}"))
            })
            .map(Some),
    }
}

/// JSON Schema fragments, kept terse: the model reads these on every list.
pub(super) fn schema_object(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
    })
}

pub(super) fn prop(ty: &str, description: &str) -> Value {
    json!({ "type": ty, "description": description })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::sql;
    use crate::orgs::Role;
    use sqlx::Row;

    pub(super) async fn seed_org(pool: &DbPool, slug: &str) -> i64 {
        sqlx::query(sql!(
            "INSERT INTO organizations (slug, name) VALUES (?1, ?2)"
        ))
        .bind(slug)
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

    pub(super) async fn seed_project(pool: &DbPool, project_id: i64, org_id: i64, name: &str) {
        sqlx::query(sql!(
            "INSERT INTO projects (project_id, org_id, name, status, source) VALUES (?1, ?2, ?3, 'active', 'manual')"
        ))
        .bind(project_id)
        .bind(org_id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    }

    /// Runs a tool end-to-end through [`invoke`], the same path the dispatcher
    /// takes, so every test exercises the authorization choke point.
    pub(super) async fn call(
        pool: &DbPool,
        principal: McpPrincipal,
        name: &str,
        args: Value,
    ) -> Result<Value, ToolError> {
        let state = crate::mcp::test_support::state_with_pool(pool.clone()).await;
        let tool = find(name).expect("tool exists");
        invoke(tool, &state, Arc::new(principal), &args).await
    }

    // An unknown name has to fall through to the dispatcher's `-32602`, which
    // only happens if the lookup misses.
    #[test]
    fn an_unknown_tool_has_no_table_row() {
        assert!(find("drop_database").is_none());
        assert!(find("").is_none());
        assert!(find("whoami").is_some());
    }

    #[test]
    fn tool_names_are_wellformed_and_unique() {
        let mut seen = Vec::new();
        for tool in TOOLS {
            assert!(name_is_wellformed(tool.name), "{}", tool.name);
            assert!(!seen.contains(&tool.name), "duplicate {}", tool.name);
            seen.push(tool.name);
        }
    }

    // The consent screen is coarse: whoever authorized this client needs to read
    // the reach from the tool list, not from a design doc.
    #[test]
    fn every_description_states_the_org_wide_reach() {
        for tool in TOOLS {
            let text = descriptor(tool, &GrantedScopes::default())
                .description
                .unwrap_or_default();
            assert!(
                text.contains("no per-project access control"),
                "{} hides its reach",
                tool.name
            );
        }
    }

    fn listed_names(scopes: &GrantedScopes) -> Vec<String> {
        list(scopes).iter().map(|t| t.name.to_string()).collect()
    }

    fn listed_description(scopes: &GrantedScopes, name: &str) -> String {
        list(scopes)
            .into_iter()
            .find(|t| t.name == name)
            .expect("tool is listed")
            .description
            .unwrap_or_default()
            .into_owned()
    }

    /// Hiding the write tools from a read token would strand them: the client
    /// would never call one, so it would never see the 403 that asks for the
    /// scope.
    #[test]
    fn every_tool_is_listed_whatever_the_token_carries() {
        let all = vec![
            "whoami",
            "list_projects",
            "get_project",
            "list_issues",
            "get_issue",
            "get_latest_event",
            "get_event",
            "search_events",
            "list_releases",
            "get_release_health",
            "list_traces",
            "get_trace",
            "update_issue_status",
            "set_project_name",
            "create_tracker_issue",
            "create_project",
            "archive_project",
        ];
        assert_eq!(listed_names(&GrantedScopes::default()), all);
        assert_eq!(
            listed_names(&GrantedScopes::parse(Some(
                "stackpit:events:read stackpit:projects:read"
            ))),
            all,
        );
    }

    #[test]
    fn an_ungranted_tool_names_the_scope_it_needs() {
        let scopes = GrantedScopes::parse(Some("stackpit:events:read stackpit:projects:read"));
        let ungranted = listed_description(&scopes, "create_project");
        assert!(ungranted.contains("stackpit:admin"), "{ungranted}");
        assert!(ungranted.contains("403"), "{ungranted}");
        assert!(
            !listed_description(&scopes, "list_issues").contains("403"),
            "a granted tool must not carry the step-up note",
        );
    }

    #[test]
    fn each_descriptor_carries_its_schema_and_hints() {
        let listed: Vec<Value> = list(&GrantedScopes::default())
            .into_iter()
            .map(|t| serde_json::to_value(t).unwrap())
            .collect();
        let whoami = &listed[0];
        assert_eq!(whoami["name"], "whoami");
        assert_eq!(whoami["title"], "Who am I");
        assert_eq!(whoami["annotations"]["readOnlyHint"], true);
        assert_eq!(whoami["annotations"]["destructiveHint"], false);
        assert!(whoami["inputSchema"].is_object());
        assert!(whoami["outputSchema"].is_object());
        let write = listed
            .iter()
            .find(|t| t["name"] == "update_issue_status")
            .unwrap();
        assert_eq!(write["annotations"]["readOnlyHint"], false);
    }

    #[tokio::test]
    async fn an_owner_only_tool_refuses_a_member() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "tools-member").await;
        let member = McpPrincipal::for_test(SCOPE_ADMIN, vec![(org, Role::Member)]);
        let err = call(
            &pool,
            member,
            "create_project",
            json!({ "org_id": org, "name": "nope" }),
        )
        .await
        .expect_err("a member must not create projects");
        assert_eq!(err.outcome(), "forbidden");
    }

    #[tokio::test]
    async fn a_missing_scope_becomes_a_step_up() {
        let pool = crate::db::open_test_pool().await;
        let org = seed_org(&pool, "tools-noscope").await;
        let reader = McpPrincipal::for_test(SCOPE_EVENTS_READ, vec![(org, Role::Owner)]);
        let err = call(&pool, reader, "list_projects", json!({}))
            .await
            .expect_err("projects:read is required");
        assert_eq!(
            err,
            ToolError::Scope {
                required: SCOPE_PROJECTS_READ
            }
        );
    }

    #[test]
    fn a_result_carries_both_a_text_block_and_structured_content() {
        let result = serde_json::to_value(success(json!({ "ok": true }))).unwrap();
        assert_eq!(result["structuredContent"]["ok"], true);
        assert_eq!(result["content"][0]["type"], "text");
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"ok\""), "got {text}");
        assert_eq!(result["isError"], false);

        let failed = serde_json::to_value(failure(&ToolError::Invalid("bad".to_string()))).unwrap();
        assert_eq!(failed["isError"], true);
        assert_eq!(failed["content"][0]["text"], "bad");
        assert!(failed.get("structuredContent").is_none());
    }
}
