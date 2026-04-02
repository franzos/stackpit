use crate::fingerprint;
use crate::models::{ItemType, StorableEvent};
use crate::sync::client::SentryEvent;
use anyhow::Result;
use serde_json::Value;

/// Takes a Sentry API event response and turns it into something we can actually store.
pub fn to_storable_event(sentry_event: &SentryEvent, project_id: u64) -> Result<StorableEvent> {
    let json = &sentry_event.json;

    let event_id = json
        .get("eventID")
        .and_then(|v| v.as_str())
        .or_else(|| json.get("id").and_then(|v| v.as_str()))
        .map(String::from)
        .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());

    let item_type = match json.get("type").and_then(|v| v.as_str()) {
        Some("transaction") => ItemType::Transaction,
        _ => ItemType::Event,
    };

    let timestamp = sentry_event
        .timestamp()
        .unwrap_or_else(|| chrono::Utc::now().timestamp());

    // The Sentry events list API omits most fields at the top level —
    // the actual values live in the tags array instead.
    let level = json
        .get("level")
        .and_then(|v| v.as_str())
        .or_else(|| json.get("tags").and_then(|t| find_tag(t, "level")))
        .map(|s| s.parse::<crate::models::Level>().unwrap());
    let platform = json
        .get("platform")
        .and_then(|v| v.as_str())
        .map(String::from);
    let release = json
        .get("release")
        .and_then(|v| match v {
            Value::String(s) => Some(s.clone()),
            Value::Object(obj) => obj
                .get("version")
                .and_then(|v| v.as_str())
                .map(String::from),
            _ => None,
        })
        .or_else(|| {
            json.get("tags")
                .and_then(|t| find_tag(t, "release"))
                .map(String::from)
        });
    let environment = json
        .get("environment")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| {
            json.get("tags")
                .and_then(|t| find_tag(t, "environment"))
                .map(String::from)
        });
    let server_name = json
        .get("server_name")
        .and_then(|v| v.as_str())
        .or_else(|| json.get("tags").and_then(|t| find_tag(t, "server_name")))
        .map(String::from);
    let transaction_name = json
        .get("transaction")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| {
            json.get("tags")
                .and_then(|t| find_tag(t, "transaction"))
                .map(String::from)
        });

    let api_title = json.get("title").and_then(|v| v.as_str()).map(String::from);

    let (sdk_name, sdk_version) = json
        .get("sdk")
        .map(|sdk| {
            (
                sdk.get("name").and_then(|v| v.as_str()).map(String::from),
                sdk.get("version")
                    .and_then(|v| v.as_str())
                    .map(String::from),
            )
        })
        .unwrap_or((None, None));

    let user_identifier = json.get("user").and_then(|u| {
        u.get("id")
            .and_then(|v| {
                v.as_str()
                    .map(String::from)
                    .or_else(|| v.as_u64().map(|n| n.to_string()))
            })
            .or_else(|| u.get("email").and_then(|v| v.as_str()).map(String::from))
            .or_else(|| u.get("username").and_then(|v| v.as_str()).map(String::from))
            .or_else(|| {
                u.get("ip_address")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            })
    });

    // The Sentry API response has a different shape than a raw SDK event —
    // we need to reshape it so fingerprinting and storage work correctly.
    let payload_json = build_payload_for_storage(json, &api_title);

    // Recompute the title from the payload — the API often returns poor titles
    // (e.g. just "error" instead of "Error: actual message")
    let title = crate::enrich::extract_title_from(&payload_json, &item_type, None).or(api_title);
    let fp = fingerprint::compute_fingerprint_from_value(project_id, &item_type, &payload_json);
    let compressed = zstd::encode_all(serde_json::to_vec(&payload_json)?.as_slice(), 3)?;

    Ok(StorableEvent {
        event_id,
        item_type,
        payload: compressed,
        project_id,
        public_key: "synced".to_string(),
        timestamp,
        level,
        platform,
        release,
        environment,
        server_name,
        transaction_name,
        title,
        sdk_name,
        sdk_version,
        fingerprint: fp,
        monitor_slug: None,
        session_status: None,
        parent_event_id: None,
        user_identifier,
        tags: extract_tags_from_api(json),
    })
}

/// Sentry's API wraps exception/breadcrumb data inside `entries[]` — we
/// unwrap that back into the standard event shape for storage and fingerprinting.
fn build_payload_for_storage(api_json: &Value, title: &Option<String>) -> Value {
    let mut payload = serde_json::json!({});

    // Carry over the standard top-level fields as-is
    for key in &[
        "event_id",
        "eventID",
        "level",
        "platform",
        "release",
        "environment",
        "server_name",
        "transaction",
        "message",
        "logentry",
        "fingerprint",
        "tags",
        "contexts",
        "user",
        "request",
        "sdk",
        "timestamp",
    ] {
        if let Some(v) = api_json.get(*key) {
            payload[*key] = v.clone();
        }
    }

    // The API omits many fields at the top level — fill them in from tags
    for tag_key in &[
        "level",
        "release",
        "environment",
        "server_name",
        "transaction",
    ] {
        if payload.get(*tag_key).is_none() || payload[*tag_key].is_null() {
            if let Some(v) = api_json.get("tags").and_then(|t| find_tag(t, tag_key)) {
                payload[*tag_key] = Value::String(v.to_string());
            }
        }
    }

    // Copy dateCreated as timestamp if the API didn't provide one
    if payload.get("timestamp").is_none() {
        if let Some(v) = api_json.get("dateCreated") {
            payload["timestamp"] = v.clone();
        }
    }

    // Pull exception/breadcrumbs out of the entries[] wrapper
    if let Some(entries) = api_json.get("entries").and_then(|v| v.as_array()) {
        for entry in entries {
            let entry_type = entry.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match entry_type {
                "exception" => {
                    if let Some(data) = entry.get("data") {
                        let mut exc = data.clone();
                        normalize_exception_frames(&mut exc);
                        payload["exception"] = exc;
                    }
                }
                "breadcrumbs" => {
                    if let Some(data) = entry.get("data") {
                        payload["breadcrumbs"] = data.clone();
                    }
                }
                "request" => {
                    if let Some(data) = entry.get("data") {
                        payload["request"] = data.clone();
                    }
                }
                "message" => {
                    if let Some(data) = entry.get("data") {
                        if let Some(msg) = data.get("formatted").and_then(|v| v.as_str()) {
                            if payload.get("message").is_none() {
                                payload["message"] = Value::String(msg.to_string());
                            }
                        }
                    }
                }
                "threads" => {
                    if let Some(data) = entry.get("data") {
                        payload["threads"] = data.clone();
                    }
                }
                _ => {}
            }
        }
    }

    // If there's no exception or message but we do have a title, use that
    if payload.get("exception").is_none()
        && payload.get("message").is_none()
        && payload.get("logentry").is_none()
    {
        if let Some(t) = title {
            payload["message"] = Value::String(t.clone());
        }
    }

    payload
}

/// Sentry's API returns frames in camelCase with a `context` array. We need
/// snake_case keys and the standard `context_line`/`pre_context`/`post_context` split.
fn normalize_exception_frames(exc: &mut Value) {
    let values = match exc.get_mut("values").and_then(|v| v.as_array_mut()) {
        Some(v) => v,
        None => return,
    };

    for value in values {
        // Both stacktrace and rawStacktrace need the same treatment
        for key in &["stacktrace", "rawStacktrace"] {
            if let Some(st) = value.get_mut(*key).and_then(|v| v.as_object_mut()) {
                if let Some(frames) = st.get_mut("frames").and_then(|v| v.as_array_mut()) {
                    for frame in frames {
                        normalize_frame(frame);
                    }
                }
            }
        }
    }
}

/// Converts a single stack frame from Sentry's API format to the SDK format we store.
fn normalize_frame(frame: &mut Value) {
    let obj = match frame.as_object_mut() {
        Some(o) => o,
        None => return,
    };

    // camelCase → snake_case
    if let Some(v) = obj.remove("lineNo") {
        obj.entry("lineno").or_insert(v);
    }
    if let Some(v) = obj.remove("colNo") {
        obj.entry("colno").or_insert(v);
    }
    if let Some(v) = obj.remove("absPath") {
        obj.entry("abs_path").or_insert(v);
    }
    if let Some(v) = obj.remove("inApp") {
        obj.entry("in_app").or_insert(v);
    }

    // The API gives us context as [[lineNo, text], ...] — split that into
    // pre_context, context_line, and post_context based on the current lineno
    if obj.contains_key("context_line") {
        return; // already in SDK format, nothing to do
    }

    let context = match obj.get("context").and_then(|v| v.as_array()) {
        Some(c) if !c.is_empty() => c.clone(),
        _ => return,
    };

    let lineno = obj.get("lineno").and_then(|v| v.as_u64()).unwrap_or(0);

    let mut pre = Vec::new();
    let mut post = Vec::new();
    let mut ctx_line: Option<String> = None;

    for entry in &context {
        let arr = match entry.as_array() {
            Some(a) if a.len() == 2 => a,
            _ => continue,
        };
        let line_num = arr[0].as_u64().unwrap_or(0);
        let line_text = arr[1].as_str().unwrap_or("").to_string();

        if lineno > 0 && line_num == lineno {
            ctx_line = Some(line_text);
        } else if ctx_line.is_none() {
            pre.push(Value::String(line_text));
        } else {
            post.push(Value::String(line_text));
        }
    }

    if let Some(cl) = ctx_line {
        obj.insert("context_line".to_string(), Value::String(cl));
    }
    if !pre.is_empty() {
        obj.insert("pre_context".to_string(), Value::Array(pre));
    }
    if !post.is_empty() {
        obj.insert("post_context".to_string(), Value::Array(post));
    }

    obj.remove("context");
}

/// Pulls tags out of a Sentry API response. The thing is, the API returns
/// `[{"key": "k", "value": "v"}]` while SDKs use `[["k", "v"]]` or `{"k": "v"}` —
/// so we handle all three shapes here.
fn extract_tags_from_api(json: &Value) -> Vec<(String, String)> {
    let tags = match json.get("tags") {
        Some(v) => v,
        None => return Vec::new(),
    };

    let mut result = Vec::new();
    match tags {
        Value::Array(arr) => {
            for item in arr {
                // API format: {"key": ..., "value": ...}
                if let (Some(k), Some(v)) = (
                    item.get("key").and_then(|v| v.as_str()),
                    item.get("value").and_then(|v| v.as_str()),
                ) {
                    if !k.is_empty() {
                        result.push((k.to_string(), v.to_string()));
                    }
                }
                // SDK format: ["k", "v"] tuple
                else if let Some(inner) = item.as_array() {
                    if inner.len() == 2 {
                        let k = inner[0].as_str().unwrap_or("").to_string();
                        let v = inner[1].as_str().unwrap_or("").to_string();
                        if !k.is_empty() {
                            result.push((k, v));
                        }
                    }
                }
            }
        }
        Value::Object(map) => {
            for (k, v) in map {
                let val = v
                    .as_str()
                    .map(String::from)
                    .unwrap_or_else(|| v.to_string());
                result.push((k.clone(), val));
            }
        }
        _ => {}
    }

    result
}

fn find_tag<'a>(tags: &'a Value, key: &str) -> Option<&'a str> {
    // Sentry's tags come in two shapes — array of objects or flat object
    match tags {
        Value::Array(arr) => arr.iter().find_map(|tag| {
            if tag.get("key").and_then(|v| v.as_str()) == Some(key) {
                tag.get("value").and_then(|v| v.as_str())
            } else {
                None
            }
        }),
        Value::Object(obj) => obj.get(key).and_then(|v| v.as_str()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_basic_event() {
        let json = serde_json::json!({
            "eventID": "abc123",
            "dateCreated": "2025-01-15T12:00:00Z",
            "title": "TypeError: null is not an object",
            "level": "error",
            "platform": "javascript",
            "entries": [
                {
                    "type": "exception",
                    "data": {
                        "values": [{"type": "TypeError", "value": "null is not an object"}]
                    }
                }
            ]
        });

        let event = SentryEvent { json };
        let storable = to_storable_event(&event, 42).unwrap();

        assert_eq!(storable.event_id, "abc123");
        assert_eq!(storable.project_id, 42);
        assert_eq!(storable.level, Some(crate::models::Level::Error));
        assert_eq!(storable.platform.as_deref(), Some("javascript"));
        assert_eq!(
            storable.title.as_deref(),
            Some("TypeError: null is not an object")
        );
        assert!(storable.fingerprint.is_some());
        assert_eq!(storable.public_key, "synced");
    }

    #[test]
    fn transform_event_with_release_object() {
        let json = serde_json::json!({
            "eventID": "rel1",
            "dateCreated": "2025-01-15T12:00:00Z",
            "title": "test",
            "release": {"version": "1.2.3"}
        });

        let event = SentryEvent { json };
        let storable = to_storable_event(&event, 1).unwrap();
        assert_eq!(storable.release.as_deref(), Some("1.2.3"));
    }

    #[test]
    fn normalize_sentry_api_frames() {
        let json = serde_json::json!({
            "eventID": "frame1",
            "dateCreated": "2025-01-15T12:00:00Z",
            "title": "Error",
            "entries": [
                {
                    "type": "exception",
                    "data": {
                        "values": [{
                            "type": "Error",
                            "value": "fail",
                            "stacktrace": {
                                "frames": [{
                                    "filename": "app.js",
                                    "function": "main",
                                    "lineNo": 5,
                                    "colNo": 10,
                                    "absPath": "/src/app.js",
                                    "inApp": true,
                                    "context": [
                                        [3, "let a = 1;"],
                                        [4, "let b = 2;"],
                                        [5, "throw new Error('fail');"],
                                        [6, "let c = 3;"],
                                        [7, "let d = 4;"]
                                    ]
                                }]
                            }
                        }]
                    }
                }
            ]
        });

        let event = SentryEvent { json };
        let storable = to_storable_event(&event, 1).unwrap();
        let payload: Value =
            serde_json::from_slice(&zstd::decode_all(storable.payload.as_slice()).unwrap())
                .unwrap();

        let frame = &payload["exception"]["values"][0]["stacktrace"]["frames"][0];
        assert_eq!(frame["lineno"], 5);
        assert_eq!(frame["colno"], 10);
        assert_eq!(frame["abs_path"], "/src/app.js");
        assert_eq!(frame["in_app"], true);
        assert_eq!(frame["context_line"], "throw new Error('fail');");
        assert_eq!(
            frame["pre_context"],
            serde_json::json!(["let a = 1;", "let b = 2;"])
        );
        assert_eq!(
            frame["post_context"],
            serde_json::json!(["let c = 3;", "let d = 4;"])
        );
        assert!(frame.get("context").is_none()); // should be gone after normalization
    }

    #[test]
    fn transform_entries_unwrapped() {
        let json = serde_json::json!({
            "eventID": "e1",
            "dateCreated": "2025-01-15T12:00:00Z",
            "title": "test",
            "entries": [
                {
                    "type": "exception",
                    "data": {"values": [{"type": "Error", "value": "fail"}]}
                },
                {
                    "type": "breadcrumbs",
                    "data": {"values": [{"message": "clicked"}]}
                }
            ]
        });

        let event = SentryEvent { json };
        let storable = to_storable_event(&event, 1).unwrap();

        // Verify the entries[] wrapper got flattened into top-level keys
        let payload: Value =
            serde_json::from_slice(&zstd::decode_all(storable.payload.as_slice()).unwrap())
                .unwrap();
        assert!(payload.get("exception").is_some());
        assert!(payload.get("breadcrumbs").is_some());
    }
}
