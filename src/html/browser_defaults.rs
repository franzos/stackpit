use std::collections::HashMap;

use askama::Template;
use axum::extract::{Form, State};
use axum::http::header;
use serde::Deserialize;

use crate::extractors::BrowserDefaults;
use crate::html::chrome::PageChrome;
use crate::html::flash::Flash;
use crate::html::render_template;
use crate::html::utils::{serialize_defaults_cookie, Chrome, DEFAULTS_COOKIE};
use crate::server::AppState;

#[derive(Template)]
#[template(path = "browser_defaults.html")]
struct BrowserDefaultsTemplate {
    status: String,
    level: String,
    period: String,
    message: Option<Flash>,
    chrome: PageChrome,
}

pub async fn handler(
    BrowserDefaults(defaults): BrowserDefaults,
    Chrome(chrome): Chrome,
) -> axum::response::Response {
    render_page(&defaults, None, &chrome)
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
    Chrome(chrome): Chrome,
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

    let message = Flash::ok(if defaults.is_empty() {
        chrome.t("flash-defaults-cleared")
    } else {
        chrome.t("flash-defaults-saved")
    });

    let mut resp = render_page(&defaults, Some(message), &chrome);
    resp.headers_mut()
        .insert(header::SET_COOKIE, cookie_header.parse().unwrap());
    resp
}

pub async fn clear_defaults(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
) -> axum::response::Response {
    let secure = secure_flag(&state);
    let cookie_header =
        format!("{DEFAULTS_COOKIE}=; Path=/web; HttpOnly; SameSite=Strict{secure}; Max-Age=0");
    let defaults = HashMap::new();
    let mut resp = render_page(
        &defaults,
        Some(Flash::ok(chrome.t("flash-defaults-cleared"))),
        &chrome,
    );
    resp.headers_mut()
        .insert(header::SET_COOKIE, cookie_header.parse().unwrap());
    resp
}

fn secure_flag(state: &AppState) -> &'static str {
    if state.config.server.cookies_should_be_secure() {
        "; Secure"
    } else {
        ""
    }
}

fn render_page(
    defaults: &HashMap<String, String>,
    message: Option<Flash>,
    chrome: &PageChrome,
) -> axum::response::Response {
    let tmpl = BrowserDefaultsTemplate {
        status: defaults.get("status").cloned().unwrap_or_default(),
        level: defaults.get("level").cloned().unwrap_or_default(),
        period: defaults.get("period").cloned().unwrap_or_default(),
        message,
        chrome: chrome.clone(),
    };
    render_template(&tmpl)
}

#[cfg(test)]
mod tests {
    use super::*;
    use askama::Template;
    use unic_langid::langid;

    #[test]
    fn browser_defaults_renders_without_missing_keys() {
        for locale in [langid!("en"), langid!("de")] {
            let tmpl = BrowserDefaultsTemplate {
                status: String::new(),
                level: String::new(),
                period: String::new(),
                message: None,
                chrome: PageChrome::new("csrf".into(), locale.clone(), "/web/projects/".into()),
            };
            let html = tmpl.render().expect("browser defaults renders");
            assert!(
                !html.contains(crate::i18n::MISSING_PREFIX),
                "browser defaults ({locale}) leaked a missing localization key: {html}"
            );
        }
    }
}
