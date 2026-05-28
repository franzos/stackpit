use askama::Template;
use axum::extract::{Form, Path, State};
use axum::http::StatusCode;
use serde::Deserialize;

use crate::html::render_template;
use crate::html::utils::{self, Csrf};
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
    csrf_token: String,
}

pub async fn handler(State(state): State<AppState>, Csrf(csrf): Csrf) -> axum::response::Response {
    render_list(&state, None, &csrf).await
}

#[derive(Deserialize)]
pub struct CreateForm {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub url: String,
    pub secret: Option<String>,
    pub from_address: Option<String>,
    pub from_name: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
}

pub async fn create(
    State(state): State<AppState>,
    Csrf(csrf): Csrf,
    Form(form): Form<CreateForm>,
) -> axum::response::Response {
    let name = form.name.trim().to_string();
    if name.is_empty() {
        return render_list(&state, Some("Name is required".into()), &csrf).await;
    }
    let kind = form.kind.trim().to_string();
    if !["webhook", "slack", "email"].contains(&kind.as_str()) {
        return render_list(&state, Some("Invalid integration kind".into()), &csrf).await;
    }
    // Email has no user-controlled endpoint (polymail owns it), so `url` stays
    // NULL and there's no SSRF surface. A locked mailer ignores any submitted token.
    let email_cfg = &state.config.email;
    let (url, config, ignore_secret) = if kind == "email" {
        if email_cfg.lock {
            let provider = email_cfg.provider;
            let config = serde_json::json!({ "provider": provider.as_str() }).to_string();
            (None, Some(config), true)
        } else {
            let provider_str = form.provider.as_deref().map(str::trim).unwrap_or("");
            let provider = match crate::providers::email::EmailProvider::parse(provider_str) {
                Some(p) => p,
                None => {
                    return render_list(&state, Some("Invalid email provider".into()), &csrf).await
                }
            };
            let mut cfg = serde_json::json!({ "provider": provider.as_str() });
            if let Some(from) = form
                .from_address
                .as_deref()
                .map(str::trim)
                .filter(|f| !f.is_empty())
            {
                cfg["from"] = serde_json::json!(from);
            }
            if let Some(name) = form
                .from_name
                .as_deref()
                .map(str::trim)
                .filter(|n| !n.is_empty())
            {
                cfg["from_name"] = serde_json::json!(name);
            }
            (None, Some(cfg.to_string()), false)
        }
    } else {
        let url = form.url.trim().to_string();
        if url.is_empty() {
            return render_list(&state, Some("URL is required".into()), &csrf).await;
        }
        // Block webhooks pointing at private/internal addresses. We only
        // validate here -- no HTTP request is made at creation time, so
        // there's no TOCTOU concern. The actual request happens in the
        // dispatcher which does its own pinned resolution.
        if let Err(msg) = crate::ssrf::check_ssrf(&url).await {
            return render_list(&state, Some(msg), &csrf).await;
        }
        (Some(url), None, false)
    };

    let raw_secret = if ignore_secret {
        None
    } else {
        form.secret
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
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
                    &csrf,
                ).await;
            }
        },
        None => (None, false),
    };

    let s = state.clone();
    utils::query_then_render(
        queries::integrations::create_integration(
            &state.writer_pool,
            &name,
            &kind,
            url.as_deref(),
            secret.as_deref(),
            config.as_deref(),
            encrypted,
        )
        .await,
        "Integration created",
        move |msg| async move { render_list(&s, msg, &csrf).await },
    )
    .await
}

pub async fn delete(
    State(state): State<AppState>,
    Csrf(csrf): Csrf,
    Path(id): Path<i64>,
) -> axum::response::Response {
    let msg = match queries::integrations::delete_integration(&state.writer_pool, id).await {
        Ok(0) => format!("Error: not found: integration: {id}"),
        Ok(_) => "Integration deleted".to_string(),
        Err(e) => format!("Error: {e}"),
    };
    render_list(&state, Some(msg), &csrf).await
}

pub async fn test_integration(
    State(state): State<AppState>,
    Csrf(csrf): Csrf,
    Path(id): Path<i64>,
) -> axum::response::Response {
    let integration = match queries::integrations::get_integration(&state.pool, id).await {
        Ok(Some(i)) => i,
        Ok(None) => return render_list(&state, Some("Integration not found".into()), &csrf).await,
        Err(e) => return render_list(&state, Some(format!("Error: {e}")), &csrf).await,
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

    let result = if integration.kind == "email" {
        // Endpoint isn't user-controlled -- no SSRF check or pinned client.
        crate::providers::email::send(
            &state.config.email,
            secret.as_deref(),
            integration.config.as_deref(),
            None,
            &event,
        )
        .await
    } else {
        let url = match integration.url.as_deref() {
            Some(u) if !u.is_empty() => u,
            _ => {
                return render_list(
                    &state,
                    Some("Integration has no URL configured".into()),
                    &csrf,
                )
                .await
            }
        };

        // Resolve DNS and pin it so reqwest can't re-resolve to a different (internal) IP
        let resolved = match crate::ssrf::check_ssrf(url).await {
            Ok(r) => r,
            Err(msg) => return render_list(&state, Some(msg), &csrf).await,
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

        crate::providers::dispatch(&client, &integration.kind, url, secret.as_deref(), &event).await
    };

    match result {
        Ok(()) => render_list(&state, Some("Test notification sent".into()), &csrf).await,
        Err(e) => render_list(&state, Some(format!("Test failed: {e}")), &csrf).await,
    }
}

#[derive(Template)]
#[template(path = "integration_new_webhook.html")]
struct NewWebhookTemplate {
    csrf_token: String,
}

pub async fn new_webhook(Csrf(csrf): Csrf) -> axum::response::Response {
    render_template(&NewWebhookTemplate { csrf_token: csrf })
}

#[derive(Template)]
#[template(path = "integration_new_slack.html")]
struct NewSlackTemplate {
    csrf_token: String,
}

pub async fn new_slack(Csrf(csrf): Csrf) -> axum::response::Response {
    render_template(&NewSlackTemplate { csrf_token: csrf })
}

#[derive(Template)]
#[template(path = "integration_new_email.html")]
struct NewEmailTemplate {
    csrf_token: String,
    lock: bool,
    default_provider: &'static str,
    from_placeholder: String,
    from_name_placeholder: String,
}

pub async fn new_email(
    State(state): State<AppState>,
    Csrf(csrf): Csrf,
) -> axum::response::Response {
    let email = &state.config.email;
    render_template(&NewEmailTemplate {
        csrf_token: csrf,
        lock: email.lock,
        default_provider: email.provider.as_str(),
        from_placeholder: email
            .from_address
            .clone()
            .unwrap_or_else(|| "alerts@example.com".to_string()),
        from_name_placeholder: email
            .from_name
            .clone()
            .unwrap_or_else(|| "Stackpit Alerts".to_string()),
    })
}

async fn render_list(
    state: &AppState,
    message: Option<String>,
    csrf: &str,
) -> axum::response::Response {
    let integrations = queries::integrations::list_integrations(&state.pool)
        .await
        .unwrap_or_default();

    let tmpl = IntegrationsTemplate {
        integrations,
        message,
        csrf_token: csrf.to_string(),
    };

    render_template(&tmpl)
}
