//! Lenient coercion of string-typed SDK fields that arrive as something else.
//!
//! `exception.values[].value` and `message` are strings in the event schema, but
//! SDKs send other shapes: the React Native console integration forwards the raw
//! `console.error(...)` argument list, so the value lands as a JSON array. Sentry
//! coerces rather than drops; so do we, otherwise the field reads as absent and
//! every such event groups under a bare type with no description.

use serde_json::Value;

/// Read a nominally-string JSON field, coercing the shapes SDKs actually send.
/// Strings pass through; numbers and booleans stringify; arrays join their
/// elements with a space (console-argument semantics); objects and anything
/// nested fall back to compact JSON. `null` and absent both yield `None`.
pub fn lenient_string(value: Option<&Value>) -> Option<String> {
    let out = match value? {
        Value::Null => return None,
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .collect::<Vec<_>>()
            .join(" "),
        obj @ Value::Object(_) => obj.to_string(),
    };
    Some(out)
}

/// `lenient_string` on a named child of `parent`, for the common `x.get("k")` case.
pub fn lenient_field(parent: &Value, key: &str) -> Option<String> {
    lenient_string(parent.get(key))
}

/// The event's message text, from `logentry` first and top-level `message`
/// second. Either may be a bare string or a LogEntry object; the unformatted
/// `message` template wins over `formatted` so grouping stays stable across
/// events that only differ in their interpolated parameters.
pub fn message_text(json: &Value) -> Option<String> {
    ["logentry", "message"]
        .into_iter()
        .filter_map(|key| json.get(key))
        .find_map(|node| match node {
            Value::Object(_) => {
                lenient_field(node, "message").or_else(|| lenient_field(node, "formatted"))
            }
            other => lenient_string(Some(other)),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn string_passes_through() {
        assert_eq!(
            lenient_field(&json!({"value": "boom"}), "value").as_deref(),
            Some("boom")
        );
    }

    #[test]
    fn absent_and_null_are_none() {
        assert_eq!(lenient_field(&json!({}), "value"), None);
        assert_eq!(lenient_field(&json!({"value": null}), "value"), None);
    }

    // The React Native console integration ships the console argument list, so
    // a single-argument error arrives as a one-element array.
    #[test]
    fn single_element_array_reads_as_the_message() {
        assert_eq!(
            lenient_field(
                &json!({"value": ["+ STATE: Unknown consent status"]}),
                "value"
            )
            .as_deref(),
            Some("+ STATE: Unknown consent status")
        );
    }

    #[test]
    fn multi_element_array_joins_with_space() {
        assert_eq!(
            lenient_field(&json!({"value": ["failed for user", 42]}), "value").as_deref(),
            Some("failed for user 42")
        );
    }

    #[test]
    fn scalars_stringify() {
        assert_eq!(
            lenient_field(&json!({"value": 500}), "value").as_deref(),
            Some("500")
        );
        assert_eq!(
            lenient_field(&json!({"value": true}), "value").as_deref(),
            Some("true")
        );
    }

    #[test]
    fn object_falls_back_to_json() {
        assert_eq!(
            lenient_field(&json!({"value": {"code": 1}}), "value").as_deref(),
            Some(r#"{"code":1}"#)
        );
    }

    #[test]
    fn message_text_reads_bare_string_and_logentry() {
        assert_eq!(
            message_text(&json!({"message": "something broke"})).as_deref(),
            Some("something broke")
        );
        assert_eq!(
            message_text(&json!({"logentry": {"message": "log msg"}})).as_deref(),
            Some("log msg")
        );
    }

    #[test]
    fn message_text_prefers_logentry_over_top_level() {
        let json = json!({"logentry": {"message": "template"}, "message": "formatted"});
        assert_eq!(message_text(&json).as_deref(), Some("template"));
    }

    #[test]
    fn message_text_handles_object_valued_top_level_message() {
        // What the React Native SDK sends: message is a LogEntry whose own
        // `message` is the console argument list.
        let json = json!({"message": {"message": ["+ STATE: Unknown consent status"]}});
        assert_eq!(
            message_text(&json).as_deref(),
            Some("+ STATE: Unknown consent status")
        );
    }

    #[test]
    fn message_text_falls_back_to_formatted() {
        let json = json!({"logentry": {"formatted": "user 7 not found"}});
        assert_eq!(message_text(&json).as_deref(), Some("user 7 not found"));
    }

    #[test]
    fn empty_array_is_empty_string_not_none() {
        // Distinguishable from absent: callers treat "" as "no description".
        assert_eq!(
            lenient_field(&json!({"value": []}), "value").as_deref(),
            Some("")
        );
    }
}
