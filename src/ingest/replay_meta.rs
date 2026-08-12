//! Display metadata pulled out of a `replay_event` payload at ingest time.
//!
//! Field paths were read off live payloads (`sentry.javascript.react`): duration
//! is `timestamp - replay_start_timestamp`, the URL is `urls[0]`, and the user
//! comes from `user.username`/`email`/`id`. Browser and OS are *not* in the
//! payload for browser SDKs — `contexts` carries only framework entries — so
//! they fall back to classifying `request.headers["User-Agent"]`.

/// Everything the replay list renders per row, beyond what `events` already has.
#[derive(Debug, Default, PartialEq)]
pub struct ReplayMeta {
    pub duration_ms: Option<i64>,
    pub url: Option<String>,
    pub user_label: Option<String>,
    pub browser: Option<String>,
    pub os: Option<String>,
    pub error_count: i64,
}

fn non_empty(v: Option<&str>) -> Option<String> {
    v.filter(|s| !s.trim().is_empty()).map(str::to_string)
}

/// `timestamp - replay_start_timestamp`, both f64 unix seconds, as whole ms.
/// `None` when either is missing or the difference is negative (a clock skew or
/// a truncated payload should read blank, not as a nonsense duration).
fn duration_ms(payload: &serde_json::Value) -> Option<i64> {
    let end = payload.get("timestamp")?.as_f64()?;
    let start = payload.get("replay_start_timestamp")?.as_f64()?;
    if !end.is_finite() || !start.is_finite() {
        return None;
    }
    let ms = ((end - start) * 1000.0).round();
    (ms >= 0.0 && ms < i64::MAX as f64).then_some(ms as i64)
}

/// First URL in the session; `request.url` carries the same value and is the
/// fallback for payloads that omit the array.
fn url(payload: &serde_json::Value) -> Option<String> {
    let from_array = payload
        .get("urls")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_str());
    non_empty(from_array).or_else(|| {
        non_empty(
            payload
                .get("request")
                .and_then(|r| r.get("url"))
                .and_then(|v| v.as_str()),
        )
    })
}

/// The most human-readable identifier the SDK sent.
fn user_label(payload: &serde_json::Value) -> Option<String> {
    let user = payload.get("user")?;
    ["username", "email", "id"]
        .into_iter()
        .find_map(|k| non_empty(user.get(k).and_then(|v| v.as_str())))
}

/// Browser name (with major version when present).
///
/// Prefers `contexts.browser`, which some SDKs populate; browser-JS replays do
/// not, so the fallback classifies the User-Agent. Deliberately a small matcher
/// rather than a UA-parsing dependency: two display columns do not justify one,
/// and unknown agents are better left blank than guessed at.
fn browser(payload: &serde_json::Value) -> Option<String> {
    if let Some(ctx) = payload.get("contexts").and_then(|c| c.get("browser")) {
        let name = non_empty(ctx.get("name").and_then(|v| v.as_str()));
        if let Some(name) = name {
            return match non_empty(ctx.get("version").and_then(|v| v.as_str())) {
                Some(v) => Some(format!("{name} {}", major(&v))),
                None => Some(name),
            };
        }
    }
    user_agent(payload).and_then(browser_from_ua)
}

fn os(payload: &serde_json::Value) -> Option<String> {
    if let Some(ctx) = payload.get("contexts").and_then(|c| c.get("os")) {
        if let Some(name) = non_empty(ctx.get("name").and_then(|v| v.as_str())) {
            return Some(name);
        }
    }
    user_agent(payload).and_then(os_from_ua)
}

fn user_agent(payload: &serde_json::Value) -> Option<&str> {
    payload
        .get("request")?
        .get("headers")?
        .get("User-Agent")?
        .as_str()
}

/// Leading numeric component of a version string, e.g. `150.0.0.0` -> `150`.
fn major(version: &str) -> &str {
    version.split('.').next().unwrap_or(version)
}

/// Version that follows `token/` in a UA string, reduced to its major.
fn version_after<'a>(ua: &'a str, token: &str) -> Option<&'a str> {
    let rest = ua.split(token).nth(1)?;
    let v = rest.split([' ', ';', ')']).next()?;
    (!v.is_empty()).then(|| major(v))
}

/// Order matters: Edge and Chrome-derived agents both contain `Chrome/`, and
/// every Chromium agent contains `Safari/`.
fn browser_from_ua(ua: &str) -> Option<String> {
    for (token, name) in [
        ("Edg/", "Edge"),
        ("OPR/", "Opera"),
        ("Chrome/", "Chrome"),
        ("Firefox/", "Firefox"),
        ("Version/", "Safari"),
    ] {
        if ua.contains(token) {
            // Safari reports its own version under `Version/`, but only genuine
            // Safari has no Chromium token alongside it.
            if name == "Safari" && (ua.contains("Chrome/") || ua.contains("Chromium/")) {
                continue;
            }
            return Some(match version_after(ua, token) {
                Some(v) => format!("{name} {v}"),
                None => name.to_string(),
            });
        }
    }
    None
}

/// Checked before the desktop families: an Android agent also says `Linux`, and
/// an iOS one also says `Mac OS X`.
fn os_from_ua(ua: &str) -> Option<String> {
    for (needle, name) in [
        ("Android", "Android"),
        ("iPhone", "iOS"),
        ("iPad", "iPadOS"),
        ("Windows", "Windows"),
        ("Mac OS X", "macOS"),
        ("CrOS", "ChromeOS"),
        ("Linux", "Linux"),
    ] {
        if ua.contains(needle) {
            return Some(name.to_string());
        }
    }
    None
}

/// Extract everything the replay list needs. `error_count` is passed in rather
/// than read here: it comes from the caller's already-parsed `error_ids`.
pub fn extract(payload: &serde_json::Value, error_count: i64) -> ReplayMeta {
    ReplayMeta {
        duration_ms: duration_ms(payload),
        url: url(payload),
        user_label: user_label(payload),
        browser: browser(payload),
        os: os(payload),
        error_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const CHROME_LINUX: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/150.0.0.0 Safari/537.36";

    /// The live payload from project 33, trimmed to the fields we read.
    fn live_payload() -> serde_json::Value {
        json!({
            "event_id": "a474cee495e44eb7860f411bdb2732a9",
            "replay_start_timestamp": 1786290528.9180012_f64,
            "timestamp": 1786290615.471_f64,
            "urls": ["https://formshive.com/#/account/messages"],
            "user": {"id": "6abc1d7a-92e9-497c-a5c3-27928dca748d", "username": "18b72e6c...0c7003e7"},
            "request": {
                "url": "https://formshive.com/#/account/messages",
                "headers": {"User-Agent": CHROME_LINUX}
            },
            "contexts": {"react": {"version": "19.2.3"}},
            "error_ids": ["1428cf1c477b482b959a9d8e39cd6bd3"]
        })
    }

    #[test]
    fn extracts_a_real_replay_payload() {
        let m = extract(&live_payload(), 1);
        // 1786290615.471 - 1786290528.918 = 86.553s
        assert_eq!(m.duration_ms, Some(86553));
        assert_eq!(
            m.url.as_deref(),
            Some("https://formshive.com/#/account/messages")
        );
        assert_eq!(m.user_label.as_deref(), Some("18b72e6c...0c7003e7"));
        assert_eq!(m.browser.as_deref(), Some("Chrome 150"));
        assert_eq!(m.os.as_deref(), Some("Linux"));
        assert_eq!(m.error_count, 1);
    }

    #[test]
    fn everything_is_optional() {
        let m = extract(&json!({}), 0);
        assert_eq!(m, ReplayMeta::default());
    }

    #[test]
    fn duration_rejects_negative_and_missing_bounds() {
        assert_eq!(duration_ms(&json!({"timestamp": 10.0})), None);
        assert_eq!(duration_ms(&json!({"replay_start_timestamp": 10.0})), None);
        // Clock skew: end before start reads blank, not as a negative duration.
        assert_eq!(
            duration_ms(&json!({"timestamp": 5.0, "replay_start_timestamp": 10.0})),
            None
        );
        assert_eq!(
            duration_ms(&json!({"timestamp": 10.5, "replay_start_timestamp": 10.0})),
            Some(500)
        );
    }

    #[test]
    fn url_falls_back_from_the_array_to_request() {
        assert_eq!(
            url(&json!({"urls": [], "request": {"url": "/fallback"}})).as_deref(),
            Some("/fallback")
        );
        assert_eq!(
            url(&json!({"urls": ["/first", "/second"]})).as_deref(),
            Some("/first")
        );
        assert_eq!(url(&json!({"urls": [""]})).as_deref(), None);
    }

    #[test]
    fn user_label_prefers_the_readable_identifier() {
        let with = |u| user_label(&json!({"user": u}));
        assert_eq!(
            with(json!({"id": "1", "email": "a@b.c", "username": "alice"})).as_deref(),
            Some("alice")
        );
        assert_eq!(
            with(json!({"id": "1", "email": "a@b.c"})).as_deref(),
            Some("a@b.c")
        );
        assert_eq!(with(json!({"id": "1"})).as_deref(), Some("1"));
        assert_eq!(with(json!({"username": "  "})), None);
    }

    #[test]
    fn contexts_win_over_the_user_agent() {
        let payload = json!({
            "contexts": {"browser": {"name": "Firefox", "version": "141.0"}, "os": {"name": "Windows"}},
            "request": {"headers": {"User-Agent": CHROME_LINUX}}
        });
        assert_eq!(browser(&payload).as_deref(), Some("Firefox 141"));
        assert_eq!(os(&payload).as_deref(), Some("Windows"));
    }

    #[test]
    fn ua_classification_covers_the_common_agents() {
        let cases = [
            (CHROME_LINUX, Some("Chrome 150"), Some("Linux")),
            (
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/139.0.0.0 Safari/537.36 Edg/139.0.0.0",
                Some("Edge 139"),
                Some("Windows"),
            ),
            (
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:141.0) Gecko/20100101 Firefox/141.0",
                Some("Firefox 141"),
                Some("macOS"),
            ),
            (
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.5 Safari/605.1.15",
                Some("Safari 18"),
                Some("macOS"),
            ),
            (
                "Mozilla/5.0 (Linux; Android 15; Pixel 9) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/139.0.0.0 Mobile Safari/537.36",
                Some("Chrome 139"),
                // Android must beat the `Linux` its UA also carries.
                Some("Android"),
            ),
            (
                "Mozilla/5.0 (iPhone; CPU iPhone OS 18_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.5 Mobile/15E148 Safari/604.1",
                Some("Safari 18"),
                // iOS must beat the `Mac OS X` its UA also carries.
                Some("iOS"),
            ),
            ("some-cli/1.0", None, None),
        ];
        for (ua, want_browser, want_os) in cases {
            assert_eq!(browser_from_ua(ua).as_deref(), want_browser, "ua: {ua}");
            assert_eq!(os_from_ua(ua).as_deref(), want_os, "ua: {ua}");
        }
    }
}
