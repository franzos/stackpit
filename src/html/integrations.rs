use askama::Template;
use axum::extract::{Form, Path, State};
use axum::http::StatusCode;
use serde::Deserialize;

use crate::html::render_template;
use crate::html::utils;
use crate::queries;
use crate::queries::types::Integration;
use crate::server::AppState;

use super::html_error;

#[allow(unused_imports)]
use crate::html::filters;

#[derive(Template)]
#[template(path = "integrations.html")]
struct IntegrationsTemplate {
    integrations: Vec<Integration>,
    message: Option<String>,
}

pub async fn handler(State(state): State<AppState>) -> axum::response::Response {
    render_list(&state, None).await
}

#[derive(Deserialize)]
pub struct CreateForm {
    pub name: String,
    pub kind: String,
    pub url: String,
    pub secret: Option<String>,
    pub from_address: Option<String>,
}

pub async fn create(
    State(state): State<AppState>,
    Form(form): Form<CreateForm>,
) -> axum::response::Response {
    let name = form.name.trim().to_string();
    if name.is_empty() {
        return render_list(&state, Some("Name is required".into())).await;
    }
    let kind = form.kind.trim().to_string();
    if !["webhook", "slack", "email"].contains(&kind.as_str()) {
        return render_list(&state, Some("Invalid integration kind".into())).await;
    }
    let url = form.url.trim().to_string();
    if url.is_empty() {
        return render_list(&state, Some("URL is required".into())).await;
    }

    // Block webhooks pointing at private/internal addresses
    if let Err(msg) = crate::ssrf::check_ssrf(&url).await {
        return render_list(&state, Some(msg)).await;
    }
    // NOTE: We only validate here -- no HTTP request is made at creation time,
    // so there's no TOCTOU concern. The actual request happens in the dispatcher
    // which does its own pinned resolution.

    let raw_secret = form
        .secret
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string());

    // Email integrations need a "from" address in the config JSON
    let config = if kind == "email" {
        form.from_address
            .filter(|s| !s.trim().is_empty())
            .map(|s| serde_json::json!({ "from": s.trim() }).to_string())
    } else {
        None
    };

    // Encrypt the secret if provided -- refuse to store plaintext
    let (secret, encrypted) = match raw_secret {
        Some(ref s) => match crate::crypto::encrypt_secret(s, state.encryptor.as_deref()) {
            Ok(val) => (Some(val), true),
            Err(e) => {
                tracing::warn!("refusing to store plaintext secret: {e}");
                return render_list(
                    &state,
                    Some("Cannot store secret: encryption is not configured. Set STACKPIT_MASTER_KEY to enable secret storage.".into()),
                ).await;
            }
        },
        None => (None, false),
    };

    let result = match utils::await_writer(
        state
            .writer
            .create_integration(name, kind, url, secret, config, encrypted),
    )
    .await
    {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    match result {
        Ok(_id) => render_list(&state, Some("Integration created".into())).await,
        Err(e) => render_list(&state, Some(format!("Error: {e}"))).await,
    }
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> axum::response::Response {
    let result = match utils::await_writer(state.writer.delete_integration(id)).await {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    match result {
        Ok(()) => render_list(&state, Some("Integration deleted".into())).await,
        Err(e) => render_list(&state, Some(format!("Error: {e}"))).await,
    }
}

pub async fn test_integration(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> axum::response::Response {
    let integration = match queries::integrations::get_integration(&state.pool, id).await {
        Ok(Some(i)) => i,
        Ok(None) => return render_list(&state, Some("Integration not found".into())).await,
        Err(e) => return render_list(&state, Some(format!("Error: {e}"))).await,
    };

    // Resolve DNS and pin it so reqwest can't re-resolve to a different (internal) IP
    let resolved = match crate::ssrf::check_ssrf(&integration.url).await {
        Ok(r) => r,
        Err(msg) => return render_list(&state, Some(msg)).await,
    };

    // Decrypt the secret if it was stored encrypted
    let secret = match (&integration.secret, integration.encrypted, &state.encryptor) {
        (Some(s), true, Some(enc)) => enc.decrypt(s),
        (Some(s), false, _) => Some(s.clone()),
        _ => None,
    };

    let event = crate::notify::NotificationEvent {
        trigger: crate::notify::NotifyTrigger::NewIssue,
        project_id: 0,
        fingerprint: "test-fingerprint".to_string(),
        title: Some("Test notification from Stackpit".to_string()),
        level: Some("info".to_string()),
        environment: Some("test".to_string()),
        event_id: "test-event-id".to_string(),
        digest: None,
    };

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .resolve(&resolved.hostname, resolved.addr)
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("failed to build pinned reqwest client: {e}");
            return html_error(StatusCode::INTERNAL_SERVER_ERROR, "internal error");
        }
    };

    let result = crate::providers::dispatch(
        &client,
        &integration.kind,
        &integration.url,
        secret.as_deref(),
        integration.config.as_deref(),
        None,
        &event,
    )
    .await;

    match result {
        Ok(()) => render_list(&state, Some("Test notification sent".into())).await,
        Err(e) => render_list(&state, Some(format!("Test failed: {e}"))).await,
    }
}

#[derive(Template)]
#[template(path = "integration_new_webhook.html")]
struct NewWebhookTemplate;

pub async fn new_webhook() -> axum::response::Response {
    render_template(&NewWebhookTemplate)
}

#[derive(Template)]
#[template(path = "integration_new_slack.html")]
struct NewSlackTemplate;

pub async fn new_slack() -> axum::response::Response {
    render_template(&NewSlackTemplate)
}

#[derive(Template)]
#[template(path = "integration_new_email.html")]
struct NewEmailTemplate;

pub async fn new_email() -> axum::response::Response {
    render_template(&NewEmailTemplate)
}

async fn render_list(state: &AppState, message: Option<String>) -> axum::response::Response {
    let integrations = queries::integrations::list_integrations(&state.pool)
        .await
        .unwrap_or_default();

    let tmpl = IntegrationsTemplate {
        integrations,
        message,
    };

    render_template(&tmpl)
}
