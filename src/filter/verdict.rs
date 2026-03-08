/// Why we dropped an event. Useful for metrics and debugging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // all variants used in tests; some only via pre_filter_check at runtime
pub enum DropReason {
    DiscardedFingerprint,
    BrowserExtension,
    Localhost,
    MessageFilter,
    ExcludedEnvironment,
    ReleaseFilter,
    FilterRule,
    Sampled,
    HealthCheckUserAgent,
    BlockedUserAgent,
    IpBlocked,
}

impl DropReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DiscardedFingerprint => "discarded fingerprint",
            Self::BrowserExtension => "browser extension",
            Self::Localhost => "localhost",
            Self::MessageFilter => "message filter",
            Self::ExcludedEnvironment => "excluded environment",
            Self::ReleaseFilter => "release filter",
            Self::FilterRule => "filter rule",
            Self::Sampled => "sampled out",
            Self::HealthCheckUserAgent => "health check user-agent",
            Self::BlockedUserAgent => "blocked user-agent",
            Self::IpBlocked => "IP blocked",
        }
    }
}

impl std::fmt::Display for DropReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Accept or drop -- that's all the filter engine has to say.
#[derive(Debug)]
pub enum FilterVerdict {
    Accept,
    Drop { reason: DropReason },
}

impl FilterVerdict {
    #[allow(dead_code)] // used in tests
    pub fn is_drop(&self) -> bool {
        matches!(self, FilterVerdict::Drop { .. })
    }
}
