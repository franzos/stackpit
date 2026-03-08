//! Extraction helpers for pulling structured data out of raw Sentry JSON
//! payloads. I've found that api, cli, and html all need the same logic —
//! so it lives here instead of being buried in any one layer.

use crate::forge;
use crate::queries::types::ProjectRepo;
use crate::sourcemap::ResolvedFrame;

// ── Structs ──────────────────────────────────────────────────────────

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

// ── Extraction functions ─────────────────────────────────────────────

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
                // Sentry sends frames bottom-to-top — we reverse for display
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

                    // Try sourcemap resolution if no source context and we have a resolver
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

                    // Link back to the source if we've got a commit and line number
                    let source_links = match (commit_sha, lineno) {
                        (Some(sha), Some(ln)) => repos
                            .iter()
                            .filter_map(|repo| {
                                let ft = forge::ForgeType::from_str(&repo.forge_type);
                                let (_, hostname) = forge::detect_forge(&repo.repo_url);
                                let url = forge::source_url(
                                    &ft,
                                    &repo.repo_url,
                                    repo.url_template.as_deref(),
                                    sha,
                                    &filename,
                                    ln,
                                )?;
                                // Sanity check — only allow http/https
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

pub fn extract_summary_tags(tags: &[Tag], _contexts: &[ContextGroup]) -> Vec<SummaryTag> {
    tags.iter()
        .filter(|t| !t.value.is_empty())
        .map(|t| SummaryTag {
            label: t.key.clone(),
            value: t.value.clone(),
        })
        .collect()
}

// ── Sourcemap resolution helpers ────────────────────────────────────

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

    // Find the debug_id for this frame's filename.
    // The frame's abs_path or filename should match the code_file from debug_meta.
    for (code_file, debug_id) in &debug_map {
        if filename == code_file || code_file.ends_with(filename) || filename.ends_with(code_file) {
            if let Some(resolved) = resolver(debug_id, line, col) {
                return Some(resolved);
            }
        }
    }

    // If only one sourcemap is available, use it (common for single-bundle apps)
    if debug_map.len() == 1 {
        let (_, debug_id) = &debug_map[0];
        return resolver(debug_id, line, col);
    }

    None
}
