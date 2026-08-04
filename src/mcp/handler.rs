//! The MCP server behind the transport: rmcp's [`ServerHandler`] driven by the
//! tool table in [`tools`](super::tools). Identity and per-tool authorization
//! stay in [`principal`](super::principal); the refusals that have to leave the
//! JSON-RPC envelope are settled in [`transport`](super::transport) before a
//! request ever reaches this handler.

use std::borrow::Cow;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::http::request::Parts;
use rmcp::handler::server::tool::{ToolCallContext, ToolRoute, ToolRouter};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CompleteRequestParams, CompleteResult, CustomRequest,
    CustomResult, ErrorCode, ErrorData, Implementation, ListPromptsResult,
    ListResourceTemplatesResult, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
    ProtocolVersion, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{RoleServer, ServerHandler};
use serde_json::Value;
use stackpit_auth::GrantedScopes;

use super::principal::{McpPrincipal, PrincipalError};
use super::tools::{self, ToolDef};
use super::transport::PreflightTool;
use super::McpState;

/// The two revisions that dropped JSON-RPC batching, which rmcp does not
/// accept: `2025-03-26` and `2024-11-05` make batching a MUST, so offering them
/// would promise a surface this server does not implement. The `2026-07-28`
/// "modern" era (stateless, per-request `_meta`, `server/discover`) needs more
/// than a version string and is absent for that reason instead.
const SUPPORTED_PROTOCOL_VERSIONS: &[ProtocolVersion] =
    &[ProtocolVersion::V_2025_11_25, ProtocolVersion::V_2025_06_18];

type ToolFuture<'a> =
    Pin<Box<dyn Future<Output = Result<CallToolResponse, ErrorData>> + Send + 'a>>;

#[derive(Clone)]
pub(super) struct McpServer {
    tools: ToolRouter<McpServer>,
}

impl McpServer {
    pub(super) fn new() -> Self {
        let mut router = ToolRouter::new();
        for tool in tools::TOOLS {
            // The listed descriptor is rebuilt per request in `list_tools`; this
            // one only backs `get_tool`, which reads the input schema.
            let attr = tools::descriptor(tool, &GrantedScopes::default());
            router.add_route(ToolRoute::new_dyn(
                attr,
                move |ctx: ToolCallContext<'_, McpServer>| -> ToolFuture<'_> {
                    Box::pin(run(tool, ctx))
                },
            ));
        }
        Self { tools: router }
    }
}

impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(ProtocolVersion::V_2025_11_25)
            .with_server_info(Implementation::new("stackpit", env!("CARGO_PKG_VERSION")))
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(SUPPORTED_PROTOCOL_VERSIONS)
    }

    /// The spec permits the advertised set to vary by authorization, and the
    /// descriptions do: an ungranted tool names the scope it needs. The set
    /// itself never varies -- see [`tools::list`].
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let scopes = context
            .extensions
            .get::<Parts>()
            .and_then(|parts| parts.extensions.get::<GrantedScopes>())
            .cloned()
            .unwrap_or_default();
        Ok(ListToolsResult::with_all_items(tools::list(&scopes)))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        // An unknown tool is a protocol-level mistake; a bad argument is not.
        if tools::find(&request.name).is_none() {
            return Err(ErrorData::invalid_params("Unknown tool", None));
        }
        self.tools
            .call(ToolCallContext::new(self, request, context))
            .await
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools.get(name).cloned()
    }

    async fn on_custom_request(
        &self,
        request: CustomRequest,
        _context: RequestContext<RoleServer>,
    ) -> Result<CustomResult, ErrorData> {
        // rmcp decodes `ClientRequest` untagged, so a `tools/call` whose
        // `arguments` is not a JSON object fails the typed shape and arrives
        // here. It is still a tool-level mistake the model can correct.
        if request.method == "tools/call" {
            let name = request
                .params
                .as_ref()
                .and_then(|params| params.get("name"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            if tools::find(name).is_none() {
                return Err(ErrorData::invalid_params("Unknown tool", None));
            }
            let mut result = tools::failure(&tools::non_object_arguments());
            // Serialized by hand, so the legacy-peer strip never runs.
            result.result_type = None;
            return serde_json::to_value(result)
                .map(CustomResult::new)
                .map_err(|_| ErrorData::internal_error("internal error", None));
        }
        Err(method_not_found())
    }

    // Stackpit exposes tools and nothing else. rmcp answers these with empty
    // results by default, which would advertise a surface that does not exist.
    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, ErrorData> {
        Err(method_not_found())
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Err(method_not_found())
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        Err(method_not_found())
    }

    async fn complete(
        &self,
        _request: CompleteRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, ErrorData> {
        Err(method_not_found())
    }
}

fn method_not_found() -> ErrorData {
    ErrorData::new(ErrorCode::METHOD_NOT_FOUND, "Method not found", None)
}

async fn run(
    tool: &'static ToolDef,
    ctx: ToolCallContext<'_, McpServer>,
) -> Result<CallToolResponse, ErrorData> {
    let ToolCallContext {
        request_context,
        arguments,
        ..
    } = ctx;
    let parts = request_context
        .extensions
        .get::<Parts>()
        .ok_or_else(not_configured)?;
    let state = parts
        .extensions
        .get::<McpState>()
        .ok_or_else(not_configured)?;
    check_preflight_tool(parts, tool)?;
    let principal = principal(parts)?;

    tracing::debug!(
        auth_source = "mcp",
        sub = %principal.sub,
        client_id = principal.client_id.as_deref().unwrap_or("-"),
        tool = tool.name,
        "mcp tool call",
    );

    let args = Value::Object(arguments.unwrap_or_default());
    Ok(match tools::invoke(tool, state, principal, &args).await {
        Ok(structured) => tools::success(structured),
        Err(err) => tools::failure(&err),
    }
    .into())
}

/// The scope was settled against the transport's own parse of the body; a
/// divergent dispatch here would run a tool nothing authorized.
fn check_preflight_tool(parts: &Parts, tool: &'static ToolDef) -> Result<(), ErrorData> {
    match parts.extensions.get::<PreflightTool>() {
        Some(PreflightTool(name)) if *name != tool.name => {
            Err(ErrorData::internal_error("tool dispatch mismatch", None))
        }
        _ => Ok(()),
    }
}

/// The transport resolves the principal ahead of rmcp, because a revoked token
/// has to become a 401 challenge and a tool handler can only produce JSON-RPC.
fn principal(parts: &Parts) -> Result<Arc<McpPrincipal>, ErrorData> {
    if let Some(principal) = parts.extensions.get::<Arc<McpPrincipal>>() {
        return Ok(principal.clone());
    }
    let err = parts
        .extensions
        .get::<PrincipalError>()
        .copied()
        .unwrap_or(PrincipalError::Unavailable);
    Err(ErrorData::internal_error(err.message(), None))
}

fn not_configured() -> ErrorData {
    ErrorData::internal_error("not configured", None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request as HttpRequest;

    fn parts_naming(tool: &'static str) -> Parts {
        let (mut parts, ()) = HttpRequest::builder().body(()).unwrap().into_parts();
        parts.extensions.insert(PreflightTool(tool));
        parts
    }

    #[test]
    fn a_dispatch_diverging_from_the_pre_check_is_refused() {
        let whoami = tools::find("whoami").expect("whoami exists");
        check_preflight_tool(&parts_naming("whoami"), whoami).expect("the agreed tool runs");
        let err = check_preflight_tool(&parts_naming("create_project"), whoami)
            .expect_err("a tool the pre-check did not authorize must not run");
        assert_eq!(err.code, ErrorCode::INTERNAL_ERROR);
    }
}
