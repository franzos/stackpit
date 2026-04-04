use std::collections::HashMap;

use askama::Template;
use axum::extract::{Form, State};
use axum::http::header;
use serde::Deserialize;

use crate::extractors::BrowserDefaults;
use crate::html::render_template;
use crate::html::utils::{serialize_defaults_cookie, DEFAULTS_COOKIE};
use crate::server::AppState;

#[derive(Template)]
#[template(path = "browser_defaults.html")]
struct BrowserDefaultsTemplate {
    status: String,
    level: String,
    period: String,
    message: Option<String>,
}

pub async fn handler(BrowserDefaults(defaults): BrowserDefaults) -> axum::response::Response {
    render_page(&defaults, None)
}

#[derive(Deserialize)]
pub struct DefaultsForm {
    pub status: Option<String>,
    pub level: Option<String>,
    pub period: Option<String>,
}

/// Allowed values for each field -- anything else is silently dropped.
fn validated(key: &str, val: &str) -> bool {
    match key {
        "status" => matches!(val, "unresolved" | "resolved" | "ignored"),
        "level" => matches!(val, "fatal" | "error" | "warning" | "info" | "debug"),
        "period" => matches!(val, "1h" | "24h" | "7d" | "14d" | "30d" | "90d" | "365d"),
        _ => false,
    }
}

pub async fn save_defaults(
    State(state): State<AppState>,
    Form(form): Form<DefaultsForm>,
) -> axum::response::Response {
    let mut defaults = HashMap::new();

    for (key, val) in [
        ("status", form.status.as_deref()),
        ("level", form.level.as_deref()),
        ("period", form.period.as_deref()),
    ] {
        if let Some(v) = val {
            let v = v.trim();
            if !v.is_empty() && validated(key, v) {
                defaults.insert(key.to_string(), v.to_string());
            }
        }
    }

    let secure = secure_flag(&state);
    let cookie_value = serialize_defaults_cookie(&defaults);
    let cookie_header = if cookie_value.is_empty() {
        format!("{DEFAULTS_COOKIE}=; Path=/web; HttpOnly; SameSite=Strict{secure}; Max-Age=0")
    } else {
        format!("{DEFAULTS_COOKIE}={cookie_value}; Path=/web; HttpOnly; SameSite=Strict{secure}; Max-Age=31536000")
    };

    let message = if defaults.is_empty() {
        "Defaults cleared".to_string()
    } else {
        "Defaults saved".to_string()
    };

    let mut resp = render_page(&defaults, Some(message));
    resp.headers_mut()
        .insert(header::SET_COOKIE, cookie_header.parse().unwrap());
    resp
}

pub async fn clear_defaults(State(state): State<AppState>) -> axum::response::Response {
    let secure = secure_flag(&state);
    let cookie_header =
        format!("{DEFAULTS_COOKIE}=; Path=/web; HttpOnly; SameSite=Strict{secure}; Max-Age=0");
    let defaults = HashMap::new();
    let mut resp = render_page(&defaults, Some("Defaults cleared".to_string()));
    resp.headers_mut()
        .insert(header::SET_COOKIE, cookie_header.parse().unwrap());
    resp
}

fn secure_flag(state: &AppState) -> &'static str {
    if state
        .config
        .server
        .external_url
        .as_ref()
        .is_some_and(|u| u.starts_with("https://"))
    {
        "; Secure"
    } else {
        ""
    }
}

fn render_page(
    defaults: &HashMap<String, String>,
    message: Option<String>,
) -> axum::response::Response {
    let tmpl = BrowserDefaultsTemplate {
        status: defaults.get("status").cloned().unwrap_or_default(),
        level: defaults.get("level").cloned().unwrap_or_default(),
        period: defaults.get("period").cloned().unwrap_or_default(),
        message,
    };
    render_template(&tmpl)
}
