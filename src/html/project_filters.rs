use askama::Template;
use axum::extract::{Form, Path, State};
use axum::http::StatusCode;
use serde::Deserialize;

use crate::html::render_template;
use crate::html::utils;
use crate::queries;
use crate::server::AppState;

use super::html_error;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "project_settings_filters.html")]
struct ProjectFiltersTemplate {
    project_id: u64,
    nav: crate::queries::ProjectNavCounts,
    message: Option<String>,
    // -- tier 1: quick toggles --
    browser_extensions_enabled: bool,
    localhost_enabled: bool,
    message_filters: Vec<(i64, String)>,
    // -- tier 2: rate limits & string filters --
    rate_limit: u32,
    environment_filters: Vec<(i64, String)>,
    release_filters: Vec<(i64, String)>,
    ua_filters: Vec<(i64, String)>,
    // -- tier 3: advanced rules --
    filter_rules: Vec<queries::RawFilterRule>,
    ip_blocks: Vec<(i64, String)>,
    discard_stats: Vec<(String, String, u64)>,
}

// ---------------------------------------------------------------------------
// GET handler
// ---------------------------------------------------------------------------

pub async fn handler(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
) -> axum::response::Response {
    render_filters(&state, project_id, None).await
}

async fn render_filters(
    state: &AppState,
    project_id: u64,
    message: Option<String>,
) -> axum::response::Response {
    // Tier 1
    let inbound = match queries::filters::get_inbound_filters(&state.pool, project_id).await {
        Ok(v) => v,
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    let browser_extensions_enabled = inbound.contains("browser_extensions");
    let localhost_enabled = inbound.contains("localhost");

    // Tier 1 (cont.)
    let message_filters =
        match queries::filters::list_message_filters(&state.pool, project_id).await {
            Ok(v) => v,
            Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        };

    let rate_limit = match queries::filters::get_rate_limit(&state.pool, project_id).await {
        Ok(v) => v,
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    let environment_filters =
        match queries::filters::list_environment_filters(&state.pool, project_id).await {
            Ok(v) => v,
            Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        };
    let release_filters =
        match queries::filters::list_release_filters(&state.pool, project_id).await {
            Ok(v) => v,
            Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        };
    let ua_filters = match queries::filters::list_user_agent_filters(&state.pool, project_id).await
    {
        Ok(v) => v,
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let filter_rules = match queries::filters::list_filter_rules(&state.pool, project_id).await {
        Ok(v) => v,
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    let ip_blocks = match queries::filters::list_ip_blocks(&state.pool, project_id).await {
        Ok(v) => v,
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    let discard_stats = match queries::filters::list_discard_stats(&state.pool, project_id).await {
        Ok(v) => v,
        Err(e) => return html_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let nav = queries::projects::get_nav_counts(&state.pool, project_id).await;

    let tmpl = ProjectFiltersTemplate {
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
    };

    render_template(&tmpl)
}

// ---------------------------------------------------------------------------
// Form structs
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// POST handlers
// ---------------------------------------------------------------------------

pub async fn set_inbound_filters(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
    Form(form): Form<InboundFilterForm>,
) -> axum::response::Response {
    let browser = form.browser_extensions.is_some();
    let localhost = form.localhost.is_some();

    for (filter_id, enabled) in [("browser_extensions", browser), ("localhost", localhost)] {
        let result = match utils::await_writer(state.writer.set_inbound_filter(
            project_id,
            filter_id.to_string(),
            enabled,
        ))
        .await
        {
            Ok(r) => r,
            Err(resp) => return resp,
        };
        if let Err(e) = result {
            return render_filters(&state, project_id, Some(format!("Error: {e}"))).await;
        }
    }

    render_filters(&state, project_id, Some("Inbound filters updated".into())).await
}

pub async fn add_message_filter(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
    Form(form): Form<PatternForm>,
) -> axum::response::Response {
    let pattern = form.pattern.trim().to_string();
    if pattern.is_empty() {
        return render_filters(&state, project_id, Some("Pattern is required".into())).await;
    }
    let s = state.clone();
    utils::writer_then_render(
        state.writer.create_message_filter(project_id, pattern),
        "Message filter added",
        |msg| render_filters(&s, project_id, msg),
    )
    .await
}

pub async fn delete_message_filter(
    State(state): State<AppState>,
    Path((project_id, id)): Path<(u64, i64)>,
) -> axum::response::Response {
    let s = state.clone();
    utils::writer_then_render(
        state.writer.delete_message_filter(id),
        "Message filter removed",
        |msg| render_filters(&s, project_id, msg),
    )
    .await
}

pub async fn set_rate_limit(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
    Form(form): Form<RateLimitForm>,
) -> axum::response::Response {
    let s = state.clone();
    utils::writer_then_render(
        state
            .writer
            .set_rate_limit(project_id, None, form.max_events_per_minute),
        "Rate limit updated",
        |msg| render_filters(&s, project_id, msg),
    )
    .await
}

pub async fn add_environment_filter(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
    Form(form): Form<EnvironmentForm>,
) -> axum::response::Response {
    let env = form.environment.trim().to_string();
    if env.is_empty() {
        return render_filters(&state, project_id, Some("Environment is required".into())).await;
    }
    let s = state.clone();
    utils::writer_then_render(
        state.writer.add_environment_filter(project_id, env),
        "Environment excluded",
        |msg| render_filters(&s, project_id, msg),
    )
    .await
}

pub async fn delete_environment_filter(
    State(state): State<AppState>,
    Path((project_id, id)): Path<(u64, i64)>,
) -> axum::response::Response {
    let s = state.clone();
    utils::writer_then_render(
        state.writer.delete_environment_filter(id),
        "Environment filter removed",
        |msg| render_filters(&s, project_id, msg),
    )
    .await
}

pub async fn add_release_filter(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
    Form(form): Form<PatternForm>,
) -> axum::response::Response {
    let pattern = form.pattern.trim().to_string();
    if pattern.is_empty() {
        return render_filters(&state, project_id, Some("Pattern is required".into())).await;
    }
    let s = state.clone();
    utils::writer_then_render(
        state.writer.add_release_filter(project_id, pattern),
        "Release filter added",
        |msg| render_filters(&s, project_id, msg),
    )
    .await
}

pub async fn delete_release_filter(
    State(state): State<AppState>,
    Path((project_id, id)): Path<(u64, i64)>,
) -> axum::response::Response {
    let s = state.clone();
    utils::writer_then_render(
        state.writer.delete_release_filter(id),
        "Release filter removed",
        |msg| render_filters(&s, project_id, msg),
    )
    .await
}

pub async fn add_ua_filter(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
    Form(form): Form<PatternForm>,
) -> axum::response::Response {
    let pattern = form.pattern.trim().to_string();
    if pattern.is_empty() {
        return render_filters(&state, project_id, Some("Pattern is required".into())).await;
    }
    let s = state.clone();
    utils::writer_then_render(
        state.writer.add_user_agent_filter(project_id, pattern),
        "User-agent filter added",
        |msg| render_filters(&s, project_id, msg),
    )
    .await
}

pub async fn delete_ua_filter(
    State(state): State<AppState>,
    Path((project_id, id)): Path<(u64, i64)>,
) -> axum::response::Response {
    let s = state.clone();
    utils::writer_then_render(
        state.writer.delete_user_agent_filter(id),
        "User-agent filter removed",
        |msg| render_filters(&s, project_id, msg),
    )
    .await
}

pub async fn add_filter_rule(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
    Form(form): Form<RuleForm>,
) -> axum::response::Response {
    use crate::filter::rules::{FilterAction, FilterField, FilterOperator};

    if !FilterField::is_valid(&form.field) {
        return render_filters(
            &state,
            project_id,
            Some(format!("Unrecognized field '{}'", form.field)),
        )
        .await;
    }
    if !FilterOperator::is_valid(&form.operator) {
        return render_filters(
            &state,
            project_id,
            Some(format!("Unrecognized operator '{}'", form.operator)),
        )
        .await;
    }
    if !FilterAction::is_valid(&form.action) {
        return render_filters(
            &state,
            project_id,
            Some(format!("Unrecognized action '{}'", form.action)),
        )
        .await;
    }

    let s = state.clone();
    utils::writer_then_render(
        state.writer.create_filter_rule(
            project_id,
            form.field,
            form.operator,
            form.value,
            form.action,
            form.sample_rate,
            form.priority,
        ),
        "Rule added",
        |msg| render_filters(&s, project_id, msg),
    )
    .await
}

pub async fn delete_filter_rule(
    State(state): State<AppState>,
    Path((project_id, id)): Path<(u64, i64)>,
) -> axum::response::Response {
    let s = state.clone();
    utils::writer_then_render(state.writer.delete_filter_rule(id), "Rule removed", |msg| {
        render_filters(&s, project_id, msg)
    })
    .await
}

pub async fn add_ip_block(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
    Form(form): Form<CidrForm>,
) -> axum::response::Response {
    let cidr = form.cidr.trim().to_string();
    if cidr.is_empty() {
        return render_filters(&state, project_id, Some("CIDR is required".into())).await;
    }
    if crate::filter::cidr::CidrBlock::parse(&cidr).is_none() {
        return render_filters(&state, project_id, Some("Invalid CIDR format".into())).await;
    }
    let s = state.clone();
    utils::writer_then_render(
        state.writer.add_ip_block(project_id, cidr),
        "IP block added",
        |msg| render_filters(&s, project_id, msg),
    )
    .await
}

pub async fn delete_ip_block(
    State(state): State<AppState>,
    Path((project_id, id)): Path<(u64, i64)>,
) -> axum::response::Response {
    let s = state.clone();
    utils::writer_then_render(
        state.writer.delete_ip_block(id),
        "IP block removed",
        |msg| render_filters(&s, project_id, msg),
    )
    .await
}
