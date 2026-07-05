use askama::Template;
use axum::extract::{Form, Path, State};
use axum::http::StatusCode;
use serde::Deserialize;

use crate::html::chrome::PageChrome;
use crate::html::render_template;
use crate::html::utils::Chrome;
use crate::orgs::extractor::{require_owner, ActiveOrg};
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
    chrome: PageChrome,
}

pub async fn handler(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    active: ActiveOrg,
) -> axum::response::Response {
    let org_filter = active.role.as_ref().map(|_| active.org_id);
    render_list(&state, org_filter, None, &chrome).await
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
    Chrome(chrome): Chrome,
    active: ActiveOrg,
    Form(form): Form<CreateForm>,
) -> axum::response::Response {
    if let Err(r) = require_owner(&active) {
        return r;
    }
    let org_filter = active.role.as_ref().map(|_| active.org_id);
    let name = form.name.trim().to_string();
    if name.is_empty() {
        return render_list(
            &state,
            org_filter,
            Some(chrome.t("flash-name-required")),
            &chrome,
        )
        .await;
    }
    let kind = form.kind.trim().to_string();
    if !["webhook", "slack", "email"].contains(&kind.as_str()) {
        return render_list(
            &state,
            org_filter,
            Some(chrome.t("flash-invalid-integration-kind")),
            &chrome,
        )
        .await;
    }
    // Email has no user-controlled endpoint, so `url` stays NULL and there's no
    // SSRF surface. A locked mailer ignores any submitted token.
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
                    return render_list(
                        &state,
                        org_filter,
                        Some(chrome.t("flash-invalid-email-provider")),
                        &chrome,
                    )
                    .await
                }
            };
            // Reject up front when the credential needed to send mail is missing;
            // otherwise the failure only surfaces at dispatch time. API providers
            // need a token (form or global); SMTP needs the global [email.smtp]
            // relay to be configured and carries no per-integration token.
            if provider.is_token_based() {
                let has_form_secret = form
                    .secret
                    .as_deref()
                    .map(str::trim)
                    .is_some_and(|s| !s.is_empty());
                if !has_form_secret && email_cfg.token.is_none() {
                    return render_list(
                        &state,
                        org_filter,
                        Some(chrome.t("flash-api-token-required")),
                        &chrome,
                    )
                    .await;
                }
            } else if email_cfg.smtp.host.is_none() {
                return render_list(
                    &state,
                    org_filter,
                    Some(chrome.t("flash-smtp-not-configured")),
                    &chrome,
                )
                .await;
            }
            let has_form_from = form
                .from_address
                .as_deref()
                .map(str::trim)
                .is_some_and(|s| !s.is_empty());
            if !has_form_from && email_cfg.from_address.is_none() {
                return render_list(
                    &state,
                    org_filter,
                    Some(chrome.t("flash-from-address-required")),
                    &chrome,
                )
                .await;
            }
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
            // SMTP has no per-integration token, so never store a submitted secret.
            (None, Some(cfg.to_string()), !provider.is_token_based())
        }
    } else {
        let url = form.url.trim().to_string();
        if url.is_empty() {
            return render_list(
                &state,
                org_filter,
                Some(chrome.t("flash-url-required")),
                &chrome,
            )
            .await;
        }
        // Block webhooks pointing at private/internal addresses. Validation only,
        // no request here, so no TOCTOU; the dispatcher does its own pinned resolution.
        if let Err(msg) = crate::util::ssrf::check_ssrf(&url).await {
            return render_list(&state, org_filter, Some(msg), &chrome).await;
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

    // Refuse to store plaintext: secrets are encrypted or not stored.
    let (secret, encrypted) = match raw_secret {
        Some(ref s) => match crate::util::crypto::encrypt_secret(s, state.encryptor.as_deref()) {
            Ok(val) => (Some(val), true),
            Err(e) => {
                tracing::warn!("refusing to store plaintext secret: {e}");
                return render_list(
                    &state,
                    org_filter,
                    Some(chrome.t("flash-secret-not-configured")),
                    &chrome,
                )
                .await;
            }
        },
        None => (None, false),
    };

    let result = queries::integrations::create_integration(
        &state.writer_pool,
        active.org_id,
        &name,
        &kind,
        url.as_deref(),
        secret.as_deref(),
        config.as_deref(),
        encrypted,
    )
    .await;
    match result {
        Ok(_) => {
            render_list(
                &state,
                org_filter,
                Some(chrome.t("flash-integration-created")),
                &chrome,
            )
            .await
        }
        Err(ref e) if is_name_conflict(e) => {
            render_list(
                &state,
                org_filter,
                Some(chrome.t("flash-integration-name-exists")),
                &chrome,
            )
            .await
        }
        Err(e) => render_list(&state, org_filter, Some(chrome.err(e)), &chrome).await,
    }
}

pub async fn delete(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    active: ActiveOrg,
    Path(id): Path<i64>,
) -> axum::response::Response {
    if let Err(r) = require_owner(&active) {
        return r;
    }
    let org_filter = active.role.as_ref().map(|_| active.org_id);
    let msg = match queries::integrations::delete_integration(&state.writer_pool, id, active.org_id)
        .await
    {
        Ok(0) => format!(
            "{} {}",
            chrome.t("common-error-prefix"),
            chrome.tv1("flash-not-found-integration", "id", &id.to_string())
        ),
        Ok(_) => chrome.t("flash-integration-deleted"),
        Err(e) => chrome.err(e),
    };
    render_list(&state, org_filter, Some(msg), &chrome).await
}

pub async fn test_integration(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    active: ActiveOrg,
    Path(id): Path<i64>,
) -> axum::response::Response {
    if let Err(r) = require_owner(&active) {
        return r;
    }
    let org_filter = active.role.as_ref().map(|_| active.org_id);
    let integration =
        match queries::integrations::get_integration(&state.pool, id, org_filter).await {
            Ok(Some(i)) => i,
            Ok(None) => {
                return render_list(
                    &state,
                    org_filter,
                    Some(chrome.t("flash-integration-not-found")),
                    &chrome,
                )
                .await
            }
            Err(e) => return render_list(&state, org_filter, Some(chrome.err(e)), &chrome).await,
        };

    let secret = match (&integration.secret, integration.encrypted, &state.encryptor) {
        (Some(s), true, Some(enc)) => enc.decrypt(s),
        (Some(s), false, _) => Some(s.clone()),
        _ => None,
    };

    let event = crate::notify::NotificationEvent {
        trigger: crate::notify::NotifyTrigger::NewIssue,
        project_id: 0,
        // Empty: a test notification has no real issue, so no (dead) link is added.
        fingerprint: String::new(),
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
            &state.config.server.web_base(),
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
                    org_filter,
                    Some(chrome.t("flash-integration-no-url")),
                    &chrome,
                )
                .await
            }
        };

        // Pin resolved DNS so reqwest can't re-resolve to a different (internal) IP.
        let resolved = match crate::util::ssrf::check_ssrf(url).await {
            Ok(r) => r,
            Err(msg) => return render_list(&state, org_filter, Some(msg), &chrome).await,
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
        Ok(()) => {
            render_list(
                &state,
                org_filter,
                Some(chrome.t("flash-test-notification-sent")),
                &chrome,
            )
            .await
        }
        Err(e) => {
            render_list(
                &state,
                org_filter,
                Some(chrome.tv1("flash-test-failed", "error", &e.to_string())),
                &chrome,
            )
            .await
        }
    }
}

#[derive(Template)]
#[template(path = "integration_new_webhook.html")]
struct NewWebhookTemplate {
    chrome: PageChrome,
}

pub async fn new_webhook(Chrome(chrome): Chrome) -> axum::response::Response {
    render_template(&NewWebhookTemplate { chrome })
}

#[derive(Template)]
#[template(path = "integration_new_slack.html")]
struct NewSlackTemplate {
    chrome: PageChrome,
}

pub async fn new_slack(Chrome(chrome): Chrome) -> axum::response::Response {
    render_template(&NewSlackTemplate { chrome })
}

#[derive(Template)]
#[template(path = "integration_new_email.html")]
struct NewEmailTemplate {
    chrome: PageChrome,
    lock: bool,
    default_provider: &'static str,
    from_placeholder: String,
    from_name_placeholder: String,
    /// Whether `[email] token` is set; if so the API-token form field is optional.
    has_default_token: bool,
    /// Whether `[email] from_address` is set; if so the From form field is optional.
    has_default_from: bool,
    /// Whether `[email.smtp] host` is set; gates the SMTP provider option.
    smtp_configured: bool,
}

pub async fn new_email(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
) -> axum::response::Response {
    let email = &state.config.email;
    render_template(&NewEmailTemplate {
        chrome,
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
        has_default_token: email.token.is_some(),
        has_default_from: email.from_address.is_some(),
        smtp_configured: email.smtp.host.is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use askama::Template;
    use unic_langid::langid;

    // Every integrations template renders in en and de without a Fluent
    // placeholder leaking. Empty list exercises the empty-state branch.
    #[test]
    fn integrations_templates_render_without_missing_keys() {
        for locale in [langid!("en"), langid!("de")] {
            let chrome = PageChrome::new("csrf".into(), locale.clone(), "/web/projects/".into());
            let pages = [
                IntegrationsTemplate {
                    integrations: Vec::new(),
                    message: None,
                    chrome: chrome.clone(),
                }
                .render()
                .expect("integrations list renders"),
                NewWebhookTemplate {
                    chrome: chrome.clone(),
                }
                .render()
                .expect("webhook form renders"),
                NewSlackTemplate {
                    chrome: chrome.clone(),
                }
                .render()
                .expect("slack form renders"),
                NewEmailTemplate {
                    chrome: chrome.clone(),
                    lock: false,
                    default_provider: "lettermint",
                    from_placeholder: "alerts@example.com".into(),
                    from_name_placeholder: "Stackpit Alerts".into(),
                    has_default_token: false,
                    has_default_from: false,
                    smtp_configured: true,
                }
                .render()
                .expect("email form renders"),
                NewEmailTemplate {
                    chrome: chrome.clone(),
                    lock: true,
                    default_provider: "lettermint",
                    from_placeholder: "alerts@example.com".into(),
                    from_name_placeholder: "Stackpit Alerts".into(),
                    has_default_token: true,
                    has_default_from: true,
                    smtp_configured: true,
                }
                .render()
                .expect("locked email form renders"),
            ];
            for html in pages {
                assert!(
                    !html.contains(crate::i18n::MISSING_PREFIX),
                    "integrations ({locale}) leaked a missing localization key: {html}"
                );
            }
        }
    }
}

/// Detect a duplicate integration name across SQLite and Postgres.
fn is_name_conflict(err: &anyhow::Error) -> bool {
    for cause in err.chain() {
        if let Some(sqlx::Error::Database(db_err)) = cause.downcast_ref::<sqlx::Error>() {
            // SQLite: "UNIQUE constraint failed: integrations.name"
            if db_err.message().contains("integrations.name") {
                return true;
            }
            // Postgres unique_violation (only one unique constraint on this table)
            if db_err.code().as_deref() == Some("23505") {
                return true;
            }
        }
    }
    false
}

async fn render_list(
    state: &AppState,
    org_id: Option<i64>,
    message: Option<String>,
    chrome: &PageChrome,
) -> axum::response::Response {
    let integrations = queries::integrations::list_integrations(&state.pool, org_id)
        .await
        .unwrap_or_default();

    let tmpl = IntegrationsTemplate {
        integrations,
        message,
        chrome: chrome.clone(),
    };

    render_template(&tmpl)
}
