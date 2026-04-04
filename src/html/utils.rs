use std::collections::HashMap;

use serde::Deserialize;

use crate::writer::msg::WriteMsg;
use crate::writer::types::{WriteError, WriteReply};
use tokio::sync::mpsc::error::TrySendError;

/// Waits for the writer thread to reply. If the channel dies on us,
/// we turn that into an error page instead of panicking.
pub async fn await_writer<T>(
    send_result: Result<WriteReply<T>, Box<TrySendError<WriteMsg>>>,
) -> Result<Result<T, WriteError>, axum::response::Response> {
    let rx = match send_result {
        Ok(rx) => rx,
        Err(_) => {
            return Err(super::html_error(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "writer unavailable",
            ))
        }
    };
    match rx.await {
        Ok(result) => Ok(result),
        Err(_) => Err(super::html_error(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "writer dropped reply",
        )),
    }
}

/// Sends a writer command, waits for the result, and renders a page with a
/// success or error message. Cuts out the boilerplate that every POST handler
/// was duplicating.
pub async fn writer_then_render(
    send_result: Result<WriteReply<()>, Box<TrySendError<WriteMsg>>>,
    success_msg: &str,
    render: impl AsyncRenderFn,
) -> axum::response::Response {
    let result = match await_writer(send_result).await {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    match result {
        Ok(()) => render.call(Some(success_msg.to_string())).await,
        Err(e) => render.call(Some(format!("Error: {e}"))).await,
    }
}

/// Trait for the render callback so we can pass async closures ergonomically.
pub trait AsyncRenderFn {
    fn call(
        self,
        message: Option<String>,
    ) -> impl std::future::Future<Output = axum::response::Response> + Send;
}

/// Blanket impl for any closure that takes Option<String> and returns a future of Response.
impl<F, Fut> AsyncRenderFn for F
where
    F: FnOnce(Option<String>) -> Fut,
    Fut: std::future::Future<Output = axum::response::Response> + Send,
{
    fn call(
        self,
        message: Option<String>,
    ) -> impl std::future::Future<Output = axum::response::Response> + Send {
        self(message)
    }
}

/// Shared query params for all the list pages. Serde drops unknown fields,
/// so unused `Option`s just stay `None` -- no harm done.
#[derive(Deserialize)]
pub struct ListParams {
    pub query: Option<String>,
    pub level: Option<String>,
    pub status: Option<String>,
    pub sort: Option<String>,
    pub release: Option<String>,
    pub tag: Option<String>,
    pub period: Option<String>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub project_id: Option<u64>,
    pub item_type: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

/// Treats empty strings as `None` for numeric query params -- browsers love sending those.
pub fn empty_string_as_none<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    match s.as_deref() {
        None | Some("") => Ok(None),
        Some(v) => v.parse().map(Some).map_err(serde::de::Error::custom),
    }
}

/// Turns period strings like "1h", "24h", "7d" into a Unix timestamp cutoff.
pub fn period_to_timestamp(period: &str) -> Option<i64> {
    let now = chrono::Utc::now().timestamp();
    let seconds = match period {
        "1h" => 3600,
        "24h" => 86400,
        "7d" => 7 * 86400,
        "14d" => 14 * 86400,
        "30d" => 30 * 86400,
        "90d" => 90 * 86400,
        "365d" => 365 * 86400,
        _ => return None,
    };
    Some(now - seconds)
}

/// Builds the query strings for pagination and filtering. The thing is,
/// `sort` only belongs in filter_qs -- we don't want it leaking into
/// pagination links.
pub fn build_filter_qs(params: &[(&str, &str)], sort: &str) -> (String, String) {
    let mut base_parts = Vec::new();
    for (name, value) in params {
        if !value.is_empty() {
            base_parts.push(format!("&{}={}", name, urlencoded(value)));
        }
    }
    let base_qs = base_parts.join("");
    let mut filter_qs = base_qs.clone();
    if !sort.is_empty() {
        filter_qs.push_str(&format!("&sort={}", urlencoded(sort)));
    }
    (base_qs, filter_qs)
}

/// Bare-minimum percent-encoding for query string values. Good enough for our use case.
pub fn urlencoded(s: &str) -> String {
    s.replace('%', "%25")
        .replace('&', "%26")
        .replace('=', "%3D")
        .replace('+', "%2B")
        .replace(' ', "+")
        .replace('#', "%23")
}

pub const DEFAULTS_COOKIE: &str = "sp_defaults";

/// Parses the `sp_defaults` cookie value (format: `status:resolved|period:7d`)
/// into a map. Invalid segments are silently skipped.
pub fn parse_defaults_cookie(value: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for segment in value.split('|') {
        if let Some((k, v)) = segment.split_once(':') {
            let k = k.trim();
            let v = v.trim();
            if !k.is_empty() && !v.is_empty() {
                map.insert(k.to_string(), v.to_string());
            }
        }
    }
    map
}

/// Serializes a defaults map back into cookie format: `status:resolved|period:7d`.
pub fn serialize_defaults_cookie(defaults: &HashMap<String, String>) -> String {
    let mut parts: Vec<String> = defaults
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("{k}:{v}"))
        .collect();
    parts.sort(); // deterministic order
    parts.join("|")
}

/// Checks which applicable keys are missing from the query string but present
/// in cookie defaults. Returns a redirect URL with those defaults appended,
/// preserving all existing params. Returns `None` when nothing needs adding.
pub fn defaults_redirect_url(
    path: &str,
    raw_qs: Option<&str>,
    defaults: &HashMap<String, String>,
    applicable_keys: &[&str],
) -> Option<String> {
    let qs = raw_qs.unwrap_or("");

    // Collect keys already present in the query string (even if empty-valued).
    let existing_keys: std::collections::HashSet<&str> = qs
        .split('&')
        .filter_map(|pair| pair.split_once('=').map(|(k, _)| k))
        .collect();

    let mut additions = Vec::new();
    for &key in applicable_keys {
        if !existing_keys.contains(key) {
            if let Some(val) = defaults.get(key) {
                if !val.is_empty() {
                    additions.push(format!("{key}={}", urlencoded(val)));
                }
            }
        }
    }
    if additions.is_empty() {
        return None;
    }

    let merged = if qs.is_empty() {
        additions.join("&")
    } else {
        format!("{qs}&{}", additions.join("&"))
    };
    Some(format!("{path}?{merged}"))
}

/// Strips characters that'd break SVG text elements.
pub fn sanitize_svg_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Strips `<script>` tags and on* event handlers from SVG output. Defense in depth.
pub fn sanitize_svg_output(svg: &str) -> String {
    // Nuke any <script>...</script> blocks (case-insensitive).
    // We lowercase once and walk byte offsets so this is O(n) not O(n²).
    let lower = svg.to_lowercase();
    let mut kept: Vec<&str> = Vec::new();
    let mut cursor = 0;

    while let Some(rel) = lower[cursor..].find("<script") {
        let start = cursor + rel;
        if let Some(end_rel) = lower[start..].find("</script>") {
            kept.push(&svg[cursor..start]);
            cursor = start + end_rel + 9; // skip past </script>
        } else {
            // Malformed script tag -- keep everything before it, drop the rest
            kept.push(&svg[cursor..start]);
            cursor = svg.len();
            break;
        }
    }
    kept.push(&svg[cursor..]);

    let result = kept.concat();

    // Also strip on* event handlers (onclick, onload, etc.)
    regex_lite_on_handler(&result)
}

/// Hand-rolled on* attribute stripper -- avoids pulling in a regex crate just for this.
fn regex_lite_on_handler(svg: &str) -> String {
    let mut result = String::with_capacity(svg.len());
    let mut chars = svg.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == ' ' || ch == '\t' || ch == '\n' {
            // Peek ahead to see if this is an on* event handler attribute
            let rest: String = chars.clone().take(40).collect();
            if rest.starts_with("on") && rest.contains('=') {
                // Walk past the attribute value
                let eq_pos = match rest.find('=') {
                    Some(p) => p,
                    None => continue,
                };
                // Skip past "on...="
                for _ in 0..eq_pos + 1 {
                    chars.next();
                }
                // Eat whitespace
                while chars.peek() == Some(&' ') {
                    chars.next();
                }
                // Consume the attribute value (quoted or unquoted)
                if let Some(&quote) = chars.peek() {
                    if quote == '"' || quote == '\'' {
                        chars.next(); // opening quote
                        for c in chars.by_ref() {
                            if c == quote {
                                break;
                            }
                        }
                    } else {
                        // Unquoted value -- consume until whitespace or tag end
                        while let Some(&c) = chars.peek() {
                            if c == ' ' || c == '\t' || c == '\n' || c == '>' || c == '/' {
                                break;
                            }
                            chars.next();
                        }
                    }
                }
                continue;
            }
        }
        result.push(ch);
    }

    result
}
