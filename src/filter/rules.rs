use crate::models::StorableEvent;

use super::{contains_ignore_ascii_case, starts_with_ignore_ascii_case};

/// A user-defined rule -- match an event field, then drop or sample it.
#[derive(Clone)]
pub struct FilterRule {
    pub field: FilterField,
    pub operator: FilterOperator,
    pub value: String,
    pub action: FilterAction,
    pub sample_rate: Option<f64>,
}

/// Which event field to match on. Supports `tags.*` for arbitrary tag keys.
#[derive(Clone)]
pub enum FilterField {
    Level,
    Platform,
    SdkName,
    SdkVersion,
    Title,
    TransactionName,
    ServerName,
    Environment,
    Release,
    Tag(String),
}

impl FilterField {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "level" => Some(Self::Level),
            "platform" => Some(Self::Platform),
            "sdk_name" => Some(Self::SdkName),
            "sdk_version" => Some(Self::SdkVersion),
            "title" => Some(Self::Title),
            "transaction_name" => Some(Self::TransactionName),
            "server_name" => Some(Self::ServerName),
            "environment" => Some(Self::Environment),
            "release" => Some(Self::Release),
            other => other
                .strip_prefix("tags.")
                .map(|tag_key| Self::Tag(tag_key.to_string())),
        }
    }

    /// Quick validation -- is this a field name we actually know about?
    pub fn is_valid(s: &str) -> bool {
        matches!(
            s,
            "level"
                | "platform"
                | "sdk_name"
                | "sdk_version"
                | "title"
                | "transaction_name"
                | "server_name"
                | "environment"
                | "release"
        ) || s.starts_with("tags.")
    }

    pub fn extract<'a>(&self, event: &'a StorableEvent) -> Option<&'a str> {
        match self {
            Self::Level => event.level.as_ref().map(|l| l.as_str()),
            Self::Platform => event.platform.as_deref(),
            Self::SdkName => event.sdk_name.as_deref(),
            Self::SdkVersion => event.sdk_version.as_deref(),
            Self::Title => event.title.as_deref(),
            Self::TransactionName => event.transaction_name.as_deref(),
            Self::ServerName => event.server_name.as_deref(),
            Self::Environment => event.environment.as_deref(),
            Self::Release => event.release.as_deref(),
            Self::Tag(key) => event
                .tags
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str()),
        }
    }
}

/// How to compare the field value against the rule value.
#[derive(Clone, Debug, PartialEq)]
pub enum FilterOperator {
    Equals,
    NotEquals,
    Contains,
    NotContains,
    StartsWith,
    In,
    NotIn,
}

impl FilterOperator {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "equals" => Some(Self::Equals),
            "not_equals" => Some(Self::NotEquals),
            "contains" => Some(Self::Contains),
            "not_contains" => Some(Self::NotContains),
            "starts_with" => Some(Self::StartsWith),
            "in" => Some(Self::In),
            "not_in" => Some(Self::NotIn),
            _ => None,
        }
    }

    /// Validation helper -- is this an operator we support?
    pub fn is_valid(s: &str) -> bool {
        matches!(
            s,
            "equals" | "not_equals" | "contains" | "not_contains" | "starts_with" | "in" | "not_in"
        )
    }

    pub fn matches(&self, field_value: &str, rule_value: &str) -> bool {
        match self {
            Self::Equals => field_value.eq_ignore_ascii_case(rule_value),
            Self::NotEquals => !field_value.eq_ignore_ascii_case(rule_value),
            Self::Contains => contains_ignore_ascii_case(field_value, rule_value),
            Self::NotContains => !contains_ignore_ascii_case(field_value, rule_value),
            Self::StartsWith => starts_with_ignore_ascii_case(field_value, rule_value),
            Self::In => rule_value
                .split(',')
                .any(|v| field_value.eq_ignore_ascii_case(v.trim())),
            Self::NotIn => !rule_value
                .split(',')
                .any(|v| field_value.eq_ignore_ascii_case(v.trim())),
        }
    }
}

/// What to do when a rule matches -- either drop outright or sample.
#[derive(Clone)]
pub enum FilterAction {
    Drop,
    Sample,
}

impl FilterAction {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "drop" => Some(Self::Drop),
            "sample" => Some(Self::Sample),
            _ => None,
        }
    }

    /// Is this an action we know how to handle?
    pub fn is_valid(s: &str) -> bool {
        matches!(s, "drop" | "sample")
    }
}

impl FilterRule {
    pub fn matches(&self, event: &StorableEvent) -> bool {
        match self.field.extract(event) {
            Some(field_val) => self.operator.matches(field_val, &self.value),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ItemType;

    #[test]
    fn operator_equals() {
        assert!(FilterOperator::Equals.matches("error", "Error"));
        assert!(!FilterOperator::Equals.matches("error", "warning"));
    }

    #[test]
    fn operator_not_equals() {
        assert!(FilterOperator::NotEquals.matches("error", "warning"));
        assert!(!FilterOperator::NotEquals.matches("error", "Error"));
    }

    #[test]
    fn operator_contains() {
        assert!(FilterOperator::Contains.matches("NullPointerException", "null"));
        assert!(!FilterOperator::Contains.matches("TypeError", "null"));
    }

    #[test]
    fn operator_not_contains() {
        assert!(FilterOperator::NotContains.matches("TypeError", "null"));
        assert!(!FilterOperator::NotContains.matches("NullPointerException", "null"));
    }

    #[test]
    fn operator_starts_with() {
        assert!(FilterOperator::StartsWith.matches("ErrorHandler", "error"));
        assert!(!FilterOperator::StartsWith.matches("MyError", "error"));
    }

    #[test]
    fn operator_in() {
        assert!(FilterOperator::In.matches("error", "error, warning, info"));
        assert!(FilterOperator::In.matches("WARNING", "error, warning, info"));
        assert!(!FilterOperator::In.matches("debug", "error, warning, info"));
    }

    #[test]
    fn operator_not_in() {
        assert!(FilterOperator::NotIn.matches("debug", "error, warning, info"));
        assert!(!FilterOperator::NotIn.matches("error", "error, warning, info"));
    }

    #[test]
    fn field_extraction() {
        let event = StorableEvent {
            event_id: "test".to_string(),
            item_type: ItemType::Event,
            payload: vec![],
            project_id: 1,
            public_key: "key".to_string(),
            timestamp: 0,
            level: Some(crate::models::Level::Error),
            platform: Some("python".to_string()),
            release: Some("1.0.0".to_string()),
            environment: Some("production".to_string()),
            server_name: Some("web-01".to_string()),
            transaction_name: Some("/api/health".to_string()),
            title: Some("TypeError: null".to_string()),
            sdk_name: Some("sentry.python".to_string()),
            sdk_version: Some("1.0.0".to_string()),
            fingerprint: None,
            monitor_slug: None,
            session_status: None,
            parent_event_id: None,
            user_identifier: None,
            tags: vec![("browser".to_string(), "Chrome".to_string())],
        };

        assert_eq!(FilterField::Level.extract(&event), Some("error"));
        assert_eq!(FilterField::Platform.extract(&event), Some("python"));
        assert_eq!(
            FilterField::Tag("browser".to_string()).extract(&event),
            Some("Chrome")
        );
        assert_eq!(FilterField::Tag("os".to_string()).extract(&event), None);
    }
}
