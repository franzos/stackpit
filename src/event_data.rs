//! Extraction helpers for pulling structured data out of raw Sentry JSON
//! payloads. Shared by the api, cli, and html layers.

use crate::forge;
use crate::queries::types::ProjectRepo;
use crate::sourcemap::ResolvedFrame;

// Structs

#[derive(Debug)]
pub struct SummaryTag {
    pub label: String,
    pub value: String,
}

#[derive(Debug)]
pub struct ExceptionData {
    pub exc_type: String,
    pub exc_value: String,
    pub mechanism_handled: Option<bool>,
    pub mechanism_type: Option<String>,
    pub frames: Vec<StackFrame>,
}

#[derive(Debug)]
pub struct SourceLink {
    pub label: String,
    pub url: String,
}

#[derive(Debug)]
pub struct StackFrame {
    pub filename: String,
    pub function: String,
    pub lineno: Option<u64>,
    pub colno: Option<u64>,
    pub context_line: Option<String>,
    pub pre_context: Vec<String>,
    pub post_context: Vec<String>,
    pub in_app: bool,
    pub vars: Vec<(String, String)>,
    pub source_links: Vec<SourceLink>,
}

impl StackFrame {
    pub fn has_detail(&self) -> bool {
        self.context_line.is_some()
            || !self.pre_context.is_empty()
            || !self.post_context.is_empty()
            || !self.vars.is_empty()
    }

    pub fn context_start_line(&self) -> u64 {
        self.lineno
            .unwrap_or(1)
            .saturating_sub(self.pre_context.len() as u64)
            .max(1)
    }
}

#[derive(Debug)]
pub struct Breadcrumb {
    pub timestamp: String,
    pub level: String,
    pub category: String,
    pub message: String,
    pub data: String,
}

#[derive(Debug)]
pub struct Tag {
    pub key: String,
    pub value: String,
}

#[derive(Debug)]
pub struct ContextGroup {
    pub name: String,
    pub entries: Vec<(String, String)>,
}

#[derive(Debug)]
pub struct RequestInfo {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub query_string: String,
    pub body: String,
    pub env: Vec<(String, String)>,
}

#[derive(Debug)]
pub struct Measurement {
    pub label: String,
    pub value: String,
    /// Core Web Vitals rating: "good" / "needs-improvement" / "poor", or None
    /// for measurements without a standard threshold.
    pub rating: Option<&'static str>,
}

impl Measurement {
    /// CSS class for the rating color, matching release-health classes.
    pub fn rating_class(&self) -> Option<&'static str> {
        match self.rating {
            Some("good") => Some("health-good"),
            Some("needs-improvement") => Some("health-warn"),
            Some("poor") => Some("health-bad"),
            _ => None,
        }
    }
}

/// Core Web Vitals rating from the raw (lowercased) measurement key and numeric
/// value. Units are ms except CLS, which is unitless. Returns None for keys
/// without a standard threshold.
fn vital_rating(key: &str, value: f64) -> Option<&'static str> {
    let bucket = |good: f64, ni: f64| {
        if value <= good {
            "good"
        } else if value <= ni {
            "needs-improvement"
        } else {
            "poor"
        }
    };
    Some(match key {
        "lcp" => bucket(2500.0, 4000.0),
        "fcp" | "fp" => bucket(1800.0, 3000.0),
        "fid" => bucket(100.0, 300.0),
        "inp" => bucket(200.0, 500.0),
        "cls" => bucket(0.1, 0.25),
        "ttfb" => bucket(800.0, 1800.0),
        _ => return None,
    })
}

#[derive(Debug, Default)]
pub struct UserInfo {
    pub id: Option<String>,
    pub email: Option<String>,
    pub username: Option<String>,
    pub ip_address: Option<String>,
}

impl UserInfo {
    pub fn has_any(&self) -> bool {
        self.id.is_some()
            || self.email.is_some()
            || self.username.is_some()
            || self.ip_address.is_some()
    }
}

// Extraction functions

/// Callback that resolves a minified frame using a sourcemap.
/// Takes (debug_id, line, col) and returns a resolved frame if possible.
pub type FrameResolver = dyn Fn(&str, u32, u32) -> Option<ResolvedFrame>;

pub fn extract_exceptions(
    payload: &serde_json::Value,
    commit_sha: Option<&str>,
    repos: &[ProjectRepo],
    resolver: Option<&FrameResolver>,
) -> Vec<ExceptionData> {
    let values = match payload
        .get("exception")
        .and_then(|e| e.get("values"))
        .and_then(|v| v.as_array())
    {
        Some(arr) => arr,
        None => return Vec::new(),
    };

    let mut result = Vec::new();
    for exc in values {
        let exc_type = exc
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("Exception")
            .to_string();
        let exc_value = exc
            .get("value")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mechanism_handled = exc
            .get("mechanism")
            .and_then(|m| m.get("handled"))
            .and_then(|v| v.as_bool());
        let mechanism_type = exc
            .get("mechanism")
            .and_then(|m| m.get("type"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let mut frames = Vec::new();
        if let Some(stacktrace) = exc.get("stacktrace").and_then(|s| s.get("frames")) {
            if let Some(frame_arr) = stacktrace.as_array() {
                // Sentry sends frames bottom-to-top; reverse for display.
                for frame in frame_arr.iter().rev() {
                    let filename = frame
                        .get("filename")
                        .or_else(|| frame.get("abs_path"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("<unknown>")
                        .to_string();
                    let function = frame
                        .get("function")
                        .and_then(|v| v.as_str())
                        .unwrap_or("<unknown>")
                        .to_string();
                    let lineno = frame.get("lineno").and_then(|v| v.as_u64());
                    let colno = frame.get("colno").and_then(|v| v.as_u64());
                    let context_line = frame
                        .get("context_line")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let pre_context = frame
                        .get("pre_context")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();
                    let post_context = frame
                        .get("post_context")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();
                    let in_app = frame
                        .get("in_app")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let vars = frame
                        .get("vars")
                        .and_then(|v| v.as_object())
                        .map(|obj| {
                            obj.iter()
                                .map(|(k, v)| {
                                    let val = match v {
                                        serde_json::Value::String(s) => s.clone(),
                                        other => other.to_string(),
                                    };
                                    (k.clone(), val)
                                })
                                .collect()
                        })
                        .unwrap_or_default();

                    let (
                        mut filename,
                        mut function,
                        mut lineno,
                        mut colno,
                        mut context_line,
                        mut pre_context,
                        mut post_context,
                    ) = (
                        filename,
                        function,
                        lineno,
                        colno,
                        context_line,
                        pre_context,
                        post_context,
                    );

                    if context_line.is_none() {
                        if let Some(resolver) = resolver {
                            if let Some(resolved) = try_resolve_with_debug_meta(
                                payload, &filename, lineno, colno, resolver,
                            ) {
                                filename = resolved.filename;
                                if let Some(f) = resolved.function {
                                    function = f;
                                }
                                lineno = Some(resolved.lineno as u64);
                                colno = Some(resolved.colno as u64);
                                context_line = resolved.context_line;
                                pre_context = resolved.pre_context;
                                post_context = resolved.post_context;
                            }
                        }
                    }

                    let source_links = match (commit_sha, lineno) {
                        (Some(sha), Some(ln)) => repos
                            .iter()
                            .filter_map(|repo| {
                                let ft = forge::ForgeType::from_tag(&repo.forge_type);
                                let (_, hostname) = forge::detect_forge(&repo.repo_url);
                                let url = forge::source_url(
                                    &ft,
                                    &repo.repo_url,
                                    repo.url_template.as_deref(),
                                    sha,
                                    &filename,
                                    ln,
                                )?;
                                // Only allow http/https.
                                if !url.starts_with("http://") && !url.starts_with("https://") {
                                    return None;
                                }
                                Some(SourceLink {
                                    label: forge::label_from_hostname(&hostname),
                                    url,
                                })
                            })
                            .collect(),
                        _ => Vec::new(),
                    };

                    frames.push(StackFrame {
                        filename,
                        function,
                        lineno,
                        colno,
                        context_line,
                        pre_context,
                        post_context,
                        in_app,
                        vars,
                        source_links,
                    });
                }
            }
        }

        result.push(ExceptionData {
            exc_type,
            exc_value,
            mechanism_handled,
            mechanism_type,
            frames,
        });
    }

    result
}

pub fn extract_breadcrumbs(payload: &serde_json::Value) -> Vec<Breadcrumb> {
    let values = match payload
        .get("breadcrumbs")
        .and_then(|b| b.get("values"))
        .and_then(|v| v.as_array())
    {
        Some(arr) => arr,
        None => return Vec::new(),
    };

    values
        .iter()
        .map(|crumb| {
            let timestamp = crumb
                .get("timestamp")
                .and_then(|v| v.as_f64())
                .map(|t| {
                    chrono::DateTime::from_timestamp(t as i64, 0)
                        .map(|dt| dt.format("%H:%M:%S").to_string())
                        .unwrap_or_default()
                })
                .or_else(|| {
                    crumb
                        .get("timestamp")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_default();
            let level = crumb
                .get("level")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let category = crumb
                .get("category")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let message = crumb
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let data = crumb
                .get("data")
                .filter(|v| v.is_object() || v.is_array())
                .map(|v| serde_json::to_string_pretty(v).unwrap_or_default())
                .unwrap_or_default();

            Breadcrumb {
                timestamp,
                level,
                category,
                message,
                data,
            }
        })
        .collect()
}

pub fn extract_tags(payload: &serde_json::Value) -> Vec<Tag> {
    let tags = match payload.get("tags") {
        Some(v) => v,
        None => return Vec::new(),
    };

    let mut result = Vec::new();
    match tags {
        serde_json::Value::Array(arr) => {
            for pair in arr {
                // SDK format: ["key", "value"]
                if let Some(inner) = pair.as_array() {
                    if inner.len() == 2 {
                        let key = inner[0].as_str().unwrap_or("").to_string();
                        let value = inner[1].as_str().unwrap_or("").to_string();
                        result.push(Tag { key, value });
                    }
                }
                // Sentry API format: {"key": "k", "value": "v"}
                else if let (Some(k), Some(v)) = (
                    pair.get("key").and_then(|v| v.as_str()),
                    pair.get("value").and_then(|v| v.as_str()),
                ) {
                    result.push(Tag {
                        key: k.to_string(),
                        value: v.to_string(),
                    });
                }
            }
        }
        serde_json::Value::Object(map) => {
            for (key, val) in map {
                let fallback = val.to_string();
                let value = val.as_str().unwrap_or(&fallback).to_string();
                result.push(Tag {
                    key: key.clone(),
                    value,
                });
            }
        }
        _ => {}
    }

    result
}

pub fn extract_contexts(payload: &serde_json::Value) -> Vec<ContextGroup> {
    let contexts = match payload.get("contexts").and_then(|c| c.as_object()) {
        Some(obj) => obj,
        None => return Vec::new(),
    };

    contexts
        .iter()
        .map(|(name, ctx)| {
            let entries = if let Some(obj) = ctx.as_object() {
                obj.iter()
                    .map(|(key, val)| {
                        let v = match val {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        (key.clone(), v)
                    })
                    .collect()
            } else {
                vec![("value".to_string(), ctx.to_string())]
            };
            ContextGroup {
                name: name.clone(),
                entries,
            }
        })
        .collect()
}

pub fn extract_request(payload: &serde_json::Value) -> Option<RequestInfo> {
    let request = payload.get("request").and_then(|r| r.as_object())?;

    let method = request
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let url = request
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let mut headers = Vec::new();
    if let Some(h) = request.get("headers") {
        match h {
            serde_json::Value::Object(map) => {
                for (key, val) in map {
                    let fallback = val.to_string();
                    let v = val.as_str().unwrap_or(&fallback).to_string();
                    headers.push((key.clone(), v));
                }
            }
            serde_json::Value::Array(arr) => {
                for pair in arr {
                    if let Some(inner) = pair.as_array() {
                        if inner.len() == 2 {
                            let key = inner[0].as_str().unwrap_or("").to_string();
                            let val = inner[1].as_str().unwrap_or("").to_string();
                            headers.push((key, val));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let query_string = request
        .get("query_string")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let body = if let Some(data) = request.get("data") {
        if let Some(s) = data.as_str() {
            s.to_string()
        } else {
            serde_json::to_string_pretty(data).unwrap_or_else(|_| data.to_string())
        }
    } else {
        String::new()
    };

    let mut env = Vec::new();
    if let Some(obj) = request.get("env").and_then(|v| v.as_object()) {
        for (key, val) in obj {
            let v = match val {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            env.push((key.clone(), v));
        }
    }

    Some(RequestInfo {
        method,
        url,
        headers,
        query_string,
        body,
        env,
    })
}

pub fn extract_user(payload: &serde_json::Value) -> UserInfo {
    let user = match payload.get("user").and_then(|u| u.as_object()) {
        Some(obj) => obj,
        None => return UserInfo::default(),
    };

    UserInfo {
        id: user
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                user.get("id")
                    .and_then(|v| v.as_u64())
                    .map(|n| n.to_string())
            }),
        email: user
            .get("email")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        username: user
            .get("username")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        ip_address: user
            .get("ip_address")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    }
}

/// Format a metric number: integer when whole, else up to 3 decimals with
/// trailing zeros trimmed.
fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        let s = format!("{n:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Pull web-vital measurements out of a transaction payload. Each metric is
/// `{value, unit}`; known vitals are ordered first, the rest alphabetically.
pub fn extract_measurements(payload: &serde_json::Value) -> Vec<Measurement> {
    const KNOWN_ORDER: &[&str] = &["lcp", "fcp", "fid", "inp", "cls", "ttfb", "fp", "duration"];

    let obj = match payload.get("measurements").and_then(|m| m.as_object()) {
        Some(o) if !o.is_empty() => o,
        _ => return Vec::new(),
    };

    let mut keys: Vec<&String> = obj.keys().collect();
    keys.sort_by(|a, b| {
        let rank = |k: &str| KNOWN_ORDER.iter().position(|x| *x == k);
        match (rank(a), rank(b)) {
            (Some(ra), Some(rb)) => ra.cmp(&rb),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.cmp(b),
        }
    });

    keys.into_iter()
        .filter_map(|key| {
            let metric = obj.get(key)?;
            let value = metric.get("value").and_then(|v| v.as_f64())?;
            let unit = metric.get("unit").and_then(|u| u.as_str()).unwrap_or("");
            let formatted = match unit {
                "millisecond" => format!("{} ms", fmt_num(value)),
                "second" => format!("{} s", fmt_num(value)),
                "byte" => format!("{} B", fmt_num(value)),
                "" => fmt_num(value),
                other => format!("{} {other}", fmt_num(value)),
            };
            Some(Measurement {
                rating: vital_rating(&key.to_lowercase(), value),
                label: key.to_uppercase(),
                value: formatted,
            })
        })
        .collect()
}

pub fn extract_summary_tags(tags: &[Tag], _contexts: &[ContextGroup]) -> Vec<SummaryTag> {
    tags.iter()
        .filter(|t| !t.value.is_empty())
        .map(|t| SummaryTag {
            label: t.key.clone(),
            value: t.value.clone(),
        })
        .collect()
}

// Sourcemap resolution helpers

/// Build a debug_id lookup from the event's `debug_meta.images` array.
/// Maps code_file (abs URL) → debug_id.
fn build_debug_id_map(payload: &serde_json::Value) -> Vec<(String, String)> {
    let images = match payload
        .get("debug_meta")
        .and_then(|dm| dm.get("images"))
        .and_then(|i| i.as_array())
    {
        Some(arr) => arr,
        None => return Vec::new(),
    };

    images
        .iter()
        .filter_map(|img| {
            let img_type = img.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if img_type != "sourcemap" {
                return None;
            }
            let code_file = img.get("code_file").and_then(|v| v.as_str())?;
            let debug_id = img.get("debug_id").and_then(|v| v.as_str())?;
            Some((code_file.to_string(), debug_id.to_lowercase()))
        })
        .collect()
}

/// Try to resolve a frame by matching its filename against debug_meta images.
fn try_resolve_with_debug_meta(
    payload: &serde_json::Value,
    filename: &str,
    lineno: Option<u64>,
    colno: Option<u64>,
    resolver: &FrameResolver,
) -> Option<ResolvedFrame> {
    let (line, col) = match (lineno, colno) {
        (Some(l), Some(c)) => (l as u32, c as u32),
        (Some(l), None) => (l as u32, 0),
        _ => return None,
    };

    let debug_map = build_debug_id_map(payload);
    if debug_map.is_empty() {
        return None;
    }

    // Match the frame's filename against code_file from debug_meta.
    for (code_file, debug_id) in &debug_map {
        if filename == code_file || code_file.ends_with(filename) || filename.ends_with(code_file) {
            if let Some(resolved) = resolver(debug_id, line, col) {
                return Some(resolved);
            }
        }
    }

    // Single sourcemap: use it (common for single-bundle apps).
    if debug_map.len() == 1 {
        let (_, debug_id) = &debug_map[0];
        return resolver(debug_id, line, col);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fmt_num_formats() {
        assert_eq!(fmt_num(3035.0), "3035");
        assert_eq!(fmt_num(0.349), "0.349");
        assert_eq!(fmt_num(98.0), "98");
    }

    #[test]
    fn extract_measurements_sample() {
        let payload = json!({"measurements": {
            "duration": {"value": 3035, "unit": "millisecond"},
            "lcp": {"value": 1116, "unit": "millisecond"},
            "cls": {"value": 0.349, "unit": ""},
            "ttfb": {"value": 188, "unit": "millisecond"},
            "fid": {"value": 98, "unit": "millisecond"},
            "fcp": {"value": 2183, "unit": "millisecond"},
            "fp": {"value": 2447, "unit": "millisecond"},
        }});
        let m = extract_measurements(&payload);
        let by_label: std::collections::HashMap<_, _> = m
            .iter()
            .map(|x| (x.label.as_str(), x.value.as_str()))
            .collect();
        assert_eq!(by_label["LCP"], "1116 ms");
        assert_eq!(by_label["CLS"], "0.349");
        assert_eq!(by_label["DURATION"], "3035 ms");
        assert_eq!(by_label["FID"], "98 ms");

        // Known vitals come first in fixed order: lcp, fcp, fid, cls, ttfb, fp, duration.
        let labels: Vec<&str> = m.iter().map(|x| x.label.as_str()).collect();
        let lcp = labels.iter().position(|l| *l == "LCP").unwrap();
        let ttfb = labels.iter().position(|l| *l == "TTFB").unwrap();
        let duration = labels.iter().position(|l| *l == "DURATION").unwrap();
        assert!(lcp < ttfb, "lcp before ttfb: {labels:?}");
        assert!(ttfb < duration, "ttfb before duration: {labels:?}");
    }

    #[test]
    fn extract_measurements_empty() {
        assert!(extract_measurements(&json!({})).is_empty());
        assert!(extract_measurements(&json!({"measurements": {}})).is_empty());
    }

    #[test]
    fn extract_measurements_unknown_unit() {
        let payload = json!({"measurements": {
            "frames_total": {"value": 42, "unit": "frame"},
        }});
        let m = extract_measurements(&payload);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].label, "FRAMES_TOTAL");
        assert_eq!(m[0].value, "42 frame");
    }

    #[test]
    fn extract_measurements_byte_and_second() {
        let payload = json!({"measurements": {
            "memory": {"value": 2048, "unit": "byte"},
            "wait": {"value": 1.5, "unit": "second"},
        }});
        let m = extract_measurements(&payload);
        let by_label: std::collections::HashMap<_, _> = m
            .iter()
            .map(|x| (x.label.as_str(), x.value.as_str()))
            .collect();
        assert_eq!(by_label["MEMORY"], "2048 B");
        assert_eq!(by_label["WAIT"], "1.5 s");
    }
}
