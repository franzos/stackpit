//! Shared domain types: status/kind enums and extracted event-shape structs.
//! Lives below both the query layer and the event-payload extraction layer so
//! neither has to depend on the other.

use serde::{Deserialize, Serialize};

/// Project/key status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub enum ProjectStatus {
    #[default]
    Active,
    Archived,
}

impl std::str::FromStr for ProjectStatus {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "archived" => Self::Archived,
            _ => Self::Active,
        })
    }
}

impl ProjectStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
        }
    }

    pub fn is_archived(&self) -> bool {
        matches!(self, Self::Archived)
    }
}

impl std::fmt::Display for ProjectStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum IssueStatus {
    #[default]
    Unresolved,
    Resolved,
    Ignored,
}

impl std::str::FromStr for IssueStatus {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "resolved" => Self::Resolved,
            "ignored" => Self::Ignored,
            _ => Self::Unresolved,
        })
    }
}

impl IssueStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unresolved => "unresolved",
            Self::Resolved => "resolved",
            Self::Ignored => "ignored",
        }
    }
}

impl std::fmt::Display for IssueStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialEq<str> for IssueStatus {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for IssueStatus {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

/// Integration transport -- parsed once at the DB boundary so the dispatcher
/// can match exhaustively instead of comparing raw strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IntegrationKind {
    Webhook,
    Slack,
    Email,
}

impl std::str::FromStr for IntegrationKind {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "webhook" => Ok(Self::Webhook),
            "slack" => Ok(Self::Slack),
            "email" => Ok(Self::Email),
            other => anyhow::bail!("unknown integration kind: {other}"),
        }
    }
}

impl IntegrationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Webhook => "webhook",
            Self::Slack => "slack",
            Self::Email => "email",
        }
    }
}

impl std::fmt::Display for IntegrationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialEq<str> for IntegrationKind {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for IntegrationKind {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

// Extracted event-shape structs. Produced by `ingest::event_data`, consumed by
// the query and html layers.

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
