use serde::{Deserialize, Serialize};

/// I've seen SDKs send hundreds of tags per event — this keeps things sane.
pub const MAX_TAGS_PER_EVENT: usize = 200;

/// HLL-12 precision — 4096 registers per sketch, good enough for our counts.
pub const HLL_REGISTER_COUNT: usize = 1 << 12;

#[derive(Debug, Clone, PartialEq)]
pub struct StorableEvent {
    pub event_id: String,
    pub item_type: ItemType,
    pub payload: Vec<u8>,
    pub project_id: u64,
    pub public_key: String,
    pub timestamp: i64,
    pub level: Option<String>,
    pub platform: Option<String>,
    pub release: Option<String>,
    pub environment: Option<String>,
    pub server_name: Option<String>,
    pub transaction_name: Option<String>,
    pub title: Option<String>,
    pub sdk_name: Option<String>,
    pub sdk_version: Option<String>,
    pub fingerprint: Option<String>,
    pub monitor_slug: Option<String>,
    pub session_status: Option<String>,
    pub parent_event_id: Option<String>,
    pub user_identifier: Option<String>,
    pub tags: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct StorableAttachment {
    pub event_id: String,
    pub filename: String,
    pub content_type: Option<String>,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ItemType {
    Event,
    Transaction,
    Session,
    Sessions,
    Attachment,
    ClientReport,
    CheckIn,
    Profile,
    ProfileChunk,
    ReplayEvent,
    ReplayRecording,
    ReplayVideo,
    UserReport,
    Log,
    Span,
    Metric,
    #[default]
    Unknown,
}

impl ItemType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Event => "event",
            Self::Transaction => "transaction",
            Self::Session => "session",
            Self::Sessions => "sessions",
            Self::Attachment => "attachment",
            Self::ClientReport => "client_report",
            Self::CheckIn => "check_in",
            Self::Profile => "profile",
            Self::ProfileChunk => "profile_chunk",
            Self::ReplayEvent => "replay_event",
            Self::ReplayRecording => "replay_recording",
            Self::ReplayVideo => "replay_video",
            Self::UserReport => "user_report",
            Self::Log => "log",
            Self::Span => "span",
            Self::Metric => "metric",
            Self::Unknown => "unknown",
        }
    }
}

impl std::str::FromStr for ItemType {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "event" => Self::Event,
            "transaction" => Self::Transaction,
            "session" => Self::Session,
            "sessions" => Self::Sessions,
            "attachment" => Self::Attachment,
            "client_report" => Self::ClientReport,
            "check_in" => Self::CheckIn,
            "profile" => Self::Profile,
            "profile_chunk" => Self::ProfileChunk,
            "replay_event" => Self::ReplayEvent,
            "replay_recording" => Self::ReplayRecording,
            "replay_video" => Self::ReplayVideo,
            "user_report" | "user_feedback" => Self::UserReport,
            "log" | "otel_log" => Self::Log,
            "span" | "otel_span" => Self::Span,
            "metric" | "statsd" | "metric_buckets" => Self::Metric,
            _ => Self::Unknown,
        })
    }
}

impl std::fmt::Display for ItemType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl StorableEvent {
    /// New event with the required fields — everything optional defaults to None.
    pub fn new(
        event_id: String,
        item_type: ItemType,
        payload: Vec<u8>,
        project_id: u64,
        public_key: String,
    ) -> Self {
        Self {
            event_id,
            item_type,
            payload,
            project_id,
            public_key,
            timestamp: chrono::Utc::now().timestamp(),
            level: None,
            platform: None,
            release: None,
            environment: None,
            server_name: None,
            transaction_name: None,
            title: None,
            sdk_name: None,
            sdk_version: None,
            fingerprint: None,
            monitor_slug: None,
            session_status: None,
            parent_event_id: None,
            user_identifier: None,
            tags: Vec::new(),
        }
    }
}

#[cfg(test)]
impl StorableEvent {
    pub fn test_default(event_id: &str) -> Self {
        Self {
            event_id: event_id.to_string(),
            item_type: ItemType::Event,
            payload: vec![0],
            project_id: 1,
            public_key: "test-key".to_string(),
            timestamp: 1000,
            level: Some("error".to_string()),
            platform: None,
            release: None,
            environment: None,
            server_name: None,
            transaction_name: None,
            title: Some("test".to_string()),
            sdk_name: None,
            sdk_version: None,
            fingerprint: None,
            monitor_slug: None,
            session_status: None,
            parent_event_id: None,
            user_identifier: None,
            tags: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_VARIANTS: &[(&str, ItemType)] = &[
        ("event", ItemType::Event),
        ("transaction", ItemType::Transaction),
        ("session", ItemType::Session),
        ("sessions", ItemType::Sessions),
        ("attachment", ItemType::Attachment),
        ("client_report", ItemType::ClientReport),
        ("check_in", ItemType::CheckIn),
        ("profile", ItemType::Profile),
        ("profile_chunk", ItemType::ProfileChunk),
        ("replay_event", ItemType::ReplayEvent),
        ("replay_recording", ItemType::ReplayRecording),
        ("replay_video", ItemType::ReplayVideo),
        ("user_report", ItemType::UserReport),
        ("log", ItemType::Log),
        ("span", ItemType::Span),
        ("metric", ItemType::Metric),
    ];

    #[test]
    fn from_str_as_str_round_trip() {
        for (s, variant) in ALL_VARIANTS {
            assert_eq!(s.parse::<ItemType>().unwrap(), *variant);
            assert_eq!(variant.as_str(), *s);
        }
    }

    #[test]
    fn from_str_unknown_input() {
        assert_eq!("garbage".parse::<ItemType>().unwrap(), ItemType::Unknown);
        assert_eq!("".parse::<ItemType>().unwrap(), ItemType::Unknown);
    }

    #[test]
    fn unknown_as_str() {
        assert_eq!(ItemType::Unknown.as_str(), "unknown");
    }

    #[test]
    fn from_str_aliases() {
        assert_eq!("otel_log".parse::<ItemType>().unwrap(), ItemType::Log);
        assert_eq!("otel_span".parse::<ItemType>().unwrap(), ItemType::Span);
        assert_eq!("statsd".parse::<ItemType>().unwrap(), ItemType::Metric);
        assert_eq!(
            "metric_buckets".parse::<ItemType>().unwrap(),
            ItemType::Metric
        );
        assert_eq!(
            "profile_chunk".parse::<ItemType>().unwrap(),
            ItemType::ProfileChunk
        );
        assert_eq!(
            "replay_video".parse::<ItemType>().unwrap(),
            ItemType::ReplayVideo
        );
    }
    #[test]
    fn display_matches_as_str() {
        for (_, variant) in ALL_VARIANTS {
            assert_eq!(format!("{variant}"), variant.as_str());
        }
        assert_eq!(format!("{}", ItemType::Unknown), "unknown");
    }
}
