use askama::Template;
use axum::extract::{Form, Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Deserialize;

use crate::filter::admin;
use crate::html::render_template;
use crate::html::utils::Csrf;
use crate::queries;
use crate::server::AppState;

use super::HtmlError;

/// Runs a filter write, reloads the engine on success, then renders.
async fn write_then_render(
    state: &AppState,
    project_id: u64,
    csrf: &str,
    result: anyhow::Result<()>,
    success_msg: &str,
) -> axum::response::Response {
    match admin::persist_and_reload(&state.writer_pool, &state.filter_engine, result).await {
        Ok(()) => render_filters(state, project_id, Some(success_msg.into()), csrf).await,
        Err(e) => render_filters(state, project_id, Some(format!("Error: {e}")), csrf).await,
    }
}

/// Like `write_then_render`, but the query reports rows affected so a zero
/// count surfaces as a not-found message instead of a spurious success.
async fn delete_then_render(
    state: &AppState,
    project_id: u64,
    csrf: &str,
    result: anyhow::Result<u64>,
    label: &str,
    success_msg: &str,
) -> axum::response::Response {
    match result {
        Ok(0) => {
            render_filters_status(
                state,
                project_id,
                Some(format!("Error: not found: {label}")),
                csrf,
                StatusCode::NOT_FOUND,
            )
            .await
        }
        Ok(_) => {
            admin::reload(&state.writer_pool, &state.filter_engine).await;
            render_filters(state, project_id, Some(success_msg.into()), csrf).await
        }
        Err(e) => render_filters(state, project_id, Some(format!("Error: {e}")), csrf).await,
    }
}

/// Trims `value`, returning it when non-empty, otherwise an error response
/// rendering `msg` as a 422 on the filters page.
async fn require_nonempty(
    value: &str,
    msg: &str,
    state: &AppState,
    project_id: u64,
    csrf: &str,
) -> Result<String, axum::response::Response> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(validation_error(state, project_id, msg, csrf).await)
    } else {
        Ok(trimmed.to_string())
    }
}

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "project_settings_filters.html")]
struct ProjectFiltersTemplate {
    project_id: u64,
    nav: crate::queries::ProjectNavCounts,
    message: Option<String>,
    browser_extensions_enabled: bool,
    localhost_enabled: bool,
    message_filters: Vec<(i64, String)>,
    rate_limit: u32,
    environment_filters: Vec<(i64, String)>,
    release_filters: Vec<(i64, String)>,
    ua_filters: Vec<(i64, String)>,
    filter_rules: Vec<queries::RawFilterRule>,
    ip_blocks: Vec<(i64, String)>,
    discard_stats: Vec<(String, String, u64)>,
    csrf_token: String,
}

pub async fn handler(
    State(state): State<AppState>,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
) -> axum::response::Response {
    render_filters(&state, project_id, None, &csrf).await
}

async fn render_filters(
    state: &AppState,
    project_id: u64,
    message: Option<String>,
    csrf: &str,
) -> axum::response::Response {
    render_filters_status(state, project_id, message, csrf, StatusCode::OK).await
}

async fn render_filters_status(
    state: &AppState,
    project_id: u64,
    message: Option<String>,
    csrf: &str,
    status: StatusCode,
) -> axum::response::Response {
    match build_filters_template(state, project_id, message, csrf).await {
        Ok(tmpl) => (status, render_template(&tmpl)).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn validation_error(
    state: &AppState,
    project_id: u64,
    msg: &str,
    csrf: &str,
) -> axum::response::Response {
    render_filters_status(
        state,
        project_id,
        Some(msg.into()),
        csrf,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await
}

async fn build_filters_template(
    state: &AppState,
    project_id: u64,
    message: Option<String>,
    csrf: &str,
) -> Result<ProjectFiltersTemplate, HtmlError> {
    let inbound = queries::filters::get_inbound_filters(&state.pool, project_id).await?;
    let browser_extensions_enabled = inbound.contains("browser_extensions");
    let localhost_enabled = inbound.contains("localhost");

    let message_filters = queries::filters::list_message_filters(&state.pool, project_id).await?;
    let rate_limit = queries::filters::get_rate_limit(&state.pool, project_id).await?;
    let environment_filters =
        queries::filters::list_environment_filters(&state.pool, project_id).await?;
    let release_filters = queries::filters::list_release_filters(&state.pool, project_id).await?;
    let ua_filters = queries::filters::list_user_agent_filters(&state.pool, project_id).await?;
    let filter_rules = queries::filters::list_filter_rules(&state.pool, project_id).await?;
    let ip_blocks = queries::filters::list_ip_blocks(&state.pool, project_id).await?;
    let discard_stats = queries::filters::list_discard_stats(&state.pool, project_id).await?;

    let nav = queries::projects::get_nav_counts(&state.pool, project_id).await;

    Ok(ProjectFiltersTemplate {
        project_id,
        nav,
        message,
        browser_extensions_enabled,
        localhost_enabled,
        message_filters,
        rate_limit,
        environment_filters,
        release_filters,
        ua_filters,
        filter_rules,
        ip_blocks,
        discard_stats,
        csrf_token: csrf.to_string(),
    })
}

#[derive(Deserialize)]
pub struct InboundFilterForm {
    #[serde(default)]
    pub browser_extensions: Option<String>,
    #[serde(default)]
    pub localhost: Option<String>,
}

#[derive(Deserialize)]
pub struct PatternForm {
    pub pattern: String,
}

#[derive(Deserialize)]
pub struct EnvironmentForm {
    pub environment: String,
}

#[derive(Deserialize)]
pub struct RateLimitForm {
    pub max_events_per_minute: u32,
}

#[derive(Deserialize)]
pub struct CidrForm {
    pub cidr: String,
}

#[derive(Deserialize)]
pub struct RuleForm {
    pub field: String,
    pub operator: String,
    pub value: String,
    pub action: String,
    pub sample_rate: Option<f64>,
    #[serde(default)]
    pub priority: i32,
}

pub async fn set_inbound_filters(
    State(state): State<AppState>,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
    Form(form): Form<InboundFilterForm>,
) -> axum::response::Response {
    let browser = form.browser_extensions.is_some();
    let localhost = form.localhost.is_some();

    for (filter_id, enabled) in [("browser_extensions", browser), ("localhost", localhost)] {
        if let Err(e) =
            queries::filters::set_inbound_filter(&state.writer_pool, project_id, filter_id, enabled)
                .await
        {
            return render_filters(&state, project_id, Some(format!("Error: {e}")), &csrf).await;
        }
    }
    admin::reload(&state.writer_pool, &state.filter_engine).await;

    render_filters(
        &state,
        project_id,
        Some("Inbound filters updated".into()),
        &csrf,
    )
    .await
}

pub async fn add_message_filter(
    State(state): State<AppState>,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
    Form(form): Form<PatternForm>,
) -> axum::response::Response {
    let pattern = match require_nonempty(
        &form.pattern,
        "Pattern is required",
        &state,
        project_id,
        &csrf,
    )
    .await
    {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    let result =
        queries::filters::create_message_filter(&state.writer_pool, project_id, &pattern).await;
    write_then_render(&state, project_id, &csrf, result, "Message filter added").await
}

pub async fn delete_message_filter(
    State(state): State<AppState>,
    Csrf(csrf): Csrf,
    Path((project_id, id)): Path<(u64, i64)>,
) -> axum::response::Response {
    let result = queries::filters::delete_message_filter(&state.writer_pool, id).await;
    delete_then_render(
        &state,
        project_id,
        &csrf,
        result,
        "message filter",
        "Message filter removed",
    )
    .await
}

pub async fn set_rate_limit(
    State(state): State<AppState>,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
    Form(form): Form<RateLimitForm>,
) -> axum::response::Response {
    let result = queries::filters::set_rate_limit(
        &state.writer_pool,
        project_id,
        None,
        form.max_events_per_minute,
    )
    .await;
    write_then_render(&state, project_id, &csrf, result, "Rate limit updated").await
}

pub async fn add_environment_filter(
    State(state): State<AppState>,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
    Form(form): Form<EnvironmentForm>,
) -> axum::response::Response {
    let env = match require_nonempty(
        &form.environment,
        "Environment is required",
        &state,
        project_id,
        &csrf,
    )
    .await
    {
        Ok(e) => e,
        Err(resp) => return resp,
    };
    let result =
        queries::filters::add_environment_filter(&state.writer_pool, project_id, &env).await;
    write_then_render(&state, project_id, &csrf, result, "Environment excluded").await
}

pub async fn delete_environment_filter(
    State(state): State<AppState>,
    Csrf(csrf): Csrf,
    Path((project_id, id)): Path<(u64, i64)>,
) -> axum::response::Response {
    let result = queries::filters::delete_environment_filter(&state.writer_pool, id).await;
    delete_then_render(
        &state,
        project_id,
        &csrf,
        result,
        "environment filter",
        "Environment filter removed",
    )
    .await
}

pub async fn add_release_filter(
    State(state): State<AppState>,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
    Form(form): Form<PatternForm>,
) -> axum::response::Response {
    let pattern = match require_nonempty(
        &form.pattern,
        "Pattern is required",
        &state,
        project_id,
        &csrf,
    )
    .await
    {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    let result =
        queries::filters::add_release_filter(&state.writer_pool, project_id, &pattern).await;
    write_then_render(&state, project_id, &csrf, result, "Release filter added").await
}

pub async fn delete_release_filter(
    State(state): State<AppState>,
    Csrf(csrf): Csrf,
    Path((project_id, id)): Path<(u64, i64)>,
) -> axum::response::Response {
    let result = queries::filters::delete_release_filter(&state.writer_pool, id).await;
    delete_then_render(
        &state,
        project_id,
        &csrf,
        result,
        "release filter",
        "Release filter removed",
    )
    .await
}

pub async fn add_ua_filter(
    State(state): State<AppState>,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
    Form(form): Form<PatternForm>,
) -> axum::response::Response {
    let pattern = match require_nonempty(
        &form.pattern,
        "Pattern is required",
        &state,
        project_id,
        &csrf,
    )
    .await
    {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    let result =
        queries::filters::add_user_agent_filter(&state.writer_pool, project_id, &pattern).await;
    write_then_render(&state, project_id, &csrf, result, "User-agent filter added").await
}

pub async fn delete_ua_filter(
    State(state): State<AppState>,
    Csrf(csrf): Csrf,
    Path((project_id, id)): Path<(u64, i64)>,
) -> axum::response::Response {
    let result = queries::filters::delete_user_agent_filter(&state.writer_pool, id).await;
    delete_then_render(
        &state,
        project_id,
        &csrf,
        result,
        "user-agent filter",
        "User-agent filter removed",
    )
    .await
}

pub async fn add_filter_rule(
    State(state): State<AppState>,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
    Form(form): Form<RuleForm>,
) -> axum::response::Response {
    use crate::filter::rules::{FilterAction, FilterField, FilterOperator};

    if !FilterField::is_valid(&form.field) {
        return validation_error(
            &state,
            project_id,
            &format!("Unrecognized field '{}'", form.field),
            &csrf,
        )
        .await;
    }
    if !FilterOperator::is_valid(&form.operator) {
        return validation_error(
            &state,
            project_id,
            &format!("Unrecognized operator '{}'", form.operator),
            &csrf,
        )
        .await;
    }
    if !FilterAction::is_valid(&form.action) {
        return validation_error(
            &state,
            project_id,
            &format!("Unrecognized action '{}'", form.action),
            &csrf,
        )
        .await;
    }

    let result = queries::filters::create_filter_rule(
        &state.writer_pool,
        project_id,
        &form.field,
        &form.operator,
        &form.value,
        &form.action,
        form.sample_rate,
        form.priority,
    )
    .await;
    write_then_render(&state, project_id, &csrf, result, "Rule added").await
}

pub async fn delete_filter_rule(
    State(state): State<AppState>,
    Csrf(csrf): Csrf,
    Path((project_id, id)): Path<(u64, i64)>,
) -> axum::response::Response {
    let result = queries::filters::delete_filter_rule(&state.writer_pool, id).await;
    delete_then_render(
        &state,
        project_id,
        &csrf,
        result,
        "filter rule",
        "Rule removed",
    )
    .await
}

pub async fn add_ip_block(
    State(state): State<AppState>,
    Csrf(csrf): Csrf,
    Path(project_id): Path<u64>,
    Form(form): Form<CidrForm>,
) -> axum::response::Response {
    let cidr =
        match require_nonempty(&form.cidr, "CIDR is required", &state, project_id, &csrf).await {
            Ok(c) => c,
            Err(resp) => return resp,
        };
    if crate::filter::cidr::CidrBlock::parse(&cidr).is_none() {
        return validation_error(&state, project_id, "Invalid CIDR format", &csrf).await;
    }
    let result = queries::filters::add_ip_block(&state.writer_pool, project_id, &cidr).await;
    write_then_render(&state, project_id, &csrf, result, "IP block added").await
}

pub async fn delete_ip_block(
    State(state): State<AppState>,
    Csrf(csrf): Csrf,
    Path((project_id, id)): Path<(u64, i64)>,
) -> axum::response::Response {
    let result = queries::filters::delete_ip_block(&state.writer_pool, id).await;
    delete_then_render(
        &state,
        project_id,
        &csrf,
        result,
        "IP block",
        "IP block removed",
    )
    .await
}
