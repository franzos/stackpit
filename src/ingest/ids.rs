//! Validation for SDK-supplied id fields (`event_id`, `trace_id`, `span_id`).
//!
//! These arrive verbatim from untrusted SDK JSON and are rendered into every
//! list page, so anything non-printable, non-ASCII or unbounded is dropped and
//! the caller falls back to a generated id.

/// Upper bound on an SDK-supplied id. Sentry ids are 32 hex chars; the slack
/// covers SDKs that send a hyphenated UUID or a short custom id.
const MAX_ID_LEN: usize = 64;

/// `Some` for an id safe to store and display, `None` for one that is empty,
/// over-long, or carries anything outside printable ASCII.
pub(crate) fn sanitize_id(s: &str) -> Option<String> {
    let ok = !s.is_empty() && s.len() <= MAX_ID_LEN && s.chars().all(|c| c.is_ascii_graphic());
    ok.then(|| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ordinary_sdk_ids() {
        assert_eq!(
            sanitize_id("a474cee495e44eb7860f411bdb2732a9").as_deref(),
            Some("a474cee495e44eb7860f411bdb2732a9")
        );
        assert_eq!(
            sanitize_id("6abc1d7a-92e9-497c-a5c3-27928dca748d").as_deref(),
            Some("6abc1d7a-92e9-497c-a5c3-27928dca748d")
        );
        // Short non-hex ids are legal today and stay legal.
        assert_eq!(sanitize_id("aaa").as_deref(), Some("aaa"));
    }

    #[test]
    fn rejects_multibyte_and_control_chars() {
        assert!(sanitize_id("aaaaaaaaaaaé").is_none());
        assert!(sanitize_id("event\u{0}id").is_none());
        assert!(sanitize_id("event id").is_none());
    }

    #[test]
    fn rejects_empty_and_overlong() {
        assert!(sanitize_id("").is_none());
        assert!(sanitize_id(&"a".repeat(MAX_ID_LEN)).is_some());
        assert!(sanitize_id(&"a".repeat(MAX_ID_LEN + 1)).is_none());
    }
}
