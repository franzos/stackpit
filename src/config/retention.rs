use serde::Deserialize;
use std::fmt;
use std::str::FromStr;

/// Upper bound for a per-project override, in days (100 years).
pub const MAX_OVERRIDE_DAYS: u32 = 36_500;

/// `[storage.retention]`: per-category retention in days. Unset falls back to
/// `storage.retention_days`; 0 means forever.
#[derive(Debug, Default, Clone, Copy, Deserialize, PartialEq, Eq)]
pub struct RetentionConfig {
    pub errors: Option<u32>,
    pub traces: Option<u32>,
    pub logs: Option<u32>,
    pub metrics: Option<u32>,
    pub releases: Option<u32>,
}

impl RetentionConfig {
    pub(crate) fn all_set(&self) -> bool {
        self.errors.is_some()
            && self.traces.is_some()
            && self.logs.is_some()
            && self.metrics.is_some()
            && self.releases.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Errors,
    Traces,
    Logs,
    Metrics,
    Releases,
}

impl Category {
    pub const ALL: [Category; 5] = [
        Category::Errors,
        Category::Traces,
        Category::Logs,
        Category::Metrics,
        Category::Releases,
    ];

    /// Categories a project owner may shorten.
    pub const OVERRIDABLE: [Category; 4] = [
        Category::Errors,
        Category::Traces,
        Category::Logs,
        Category::Metrics,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Category::Errors => "errors",
            Category::Traces => "traces",
            Category::Logs => "logs",
            Category::Metrics => "metrics",
            Category::Releases => "releases",
        }
    }

    pub fn is_overridable(self) -> bool {
        self != Category::Releases
    }
}

impl FromStr for Category {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Category::ALL
            .into_iter()
            .find(|c| c.as_str() == s)
            .ok_or(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverrideError {
    Zero,
    NotOverridable,
    NotShorter,
    TooLarge,
}

impl fmt::Display for OverrideError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            OverrideError::Zero => "retention override must be at least 1 day",
            OverrideError::NotOverridable => "this category cannot be overridden per project",
            OverrideError::NotShorter => {
                "retention override must be shorter than the server retention"
            }
            OverrideError::TooLarge => "retention override exceeds 36500 days",
        })
    }
}

impl std::error::Error for OverrideError {}

/// Effective server retention in days per category; `None` means forever.
/// `base` covers everything outside the categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolved {
    pub base: Option<u32>,
    pub errors: Option<u32>,
    pub traces: Option<u32>,
    pub logs: Option<u32>,
    pub metrics: Option<u32>,
    pub releases: Option<u32>,
}

impl Resolved {
    /// Every category and the base set to `n` days; 0 means forever.
    pub fn uniform(n: u32) -> Self {
        let d = (n != 0).then_some(n);
        Self {
            base: d,
            errors: d,
            traces: d,
            logs: d,
            metrics: d,
            releases: d,
        }
    }

    pub fn days(&self, cat: Category) -> Option<u32> {
        match cat {
            Category::Errors => self.errors,
            Category::Traces => self.traces,
            Category::Logs => self.logs,
            Category::Metrics => self.metrics,
            Category::Releases => self.releases,
        }
    }

    /// True when every `events` row shares one cutoff, so the events sweep
    /// needs no `item_type` predicate.
    pub fn events_uniform(&self) -> bool {
        self.errors == self.base && self.traces == self.base
    }

    /// `min(server, override)` with a forever server value treated as infinite.
    pub fn effective_for_override(&self, cat: Category, override_days: u32) -> u32 {
        self.days(cat)
            .map_or(override_days, |server| server.min(override_days))
    }

    /// Validates a per-project override against the server value.
    pub fn check_override(&self, cat: Category, days: u32) -> Result<(), OverrideError> {
        if !cat.is_overridable() {
            return Err(OverrideError::NotOverridable);
        }
        if days == 0 {
            return Err(OverrideError::Zero);
        }
        if days > MAX_OVERRIDE_DAYS {
            return Err(OverrideError::TooLarge);
        }
        if self.days(cat).is_some_and(|server| days >= server) {
            return Err(OverrideError::NotShorter);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_strings_round_trip() {
        for cat in Category::ALL {
            assert_eq!(cat.as_str().parse::<Category>(), Ok(cat));
        }
        assert_eq!(Category::Errors.as_str(), "errors");
        assert_eq!(Category::Traces.as_str(), "traces");
        assert_eq!(Category::Logs.as_str(), "logs");
        assert_eq!(Category::Metrics.as_str(), "metrics");
        assert!("spans".parse::<Category>().is_err());
        assert!("Errors".parse::<Category>().is_err());
    }

    #[test]
    fn overridable_excludes_releases() {
        assert!(!Category::OVERRIDABLE.contains(&Category::Releases));
        assert_eq!(Category::OVERRIDABLE.len(), Category::ALL.len() - 1);
    }

    #[test]
    fn uniform_zero_is_forever() {
        let r = Resolved::uniform(0);
        assert_eq!(r.base, None);
        for cat in Category::ALL {
            assert_eq!(r.days(cat), None);
        }
        assert!(r.events_uniform());
    }

    #[test]
    fn uniform_sets_every_category() {
        let r = Resolved::uniform(30);
        assert_eq!(r.base, Some(30));
        for cat in Category::ALL {
            assert_eq!(r.days(cat), Some(30));
        }
        assert!(r.events_uniform());
    }

    #[test]
    fn events_uniform_ignores_logs_metrics_releases() {
        let r = Resolved {
            logs: Some(7),
            metrics: None,
            releases: Some(1),
            ..Resolved::uniform(90)
        };
        assert!(r.events_uniform());
        let r = Resolved {
            traces: Some(30),
            ..Resolved::uniform(90)
        };
        assert!(!r.events_uniform());
        let r = Resolved {
            errors: None,
            ..Resolved::uniform(90)
        };
        assert!(!r.events_uniform());
    }

    #[test]
    fn effective_for_override_clamps_to_server() {
        let r = Resolved {
            traces: None,
            ..Resolved::uniform(30)
        };
        assert_eq!(r.effective_for_override(Category::Errors, 7), 7);
        assert_eq!(r.effective_for_override(Category::Errors, 60), 30);
        assert_eq!(r.effective_for_override(Category::Errors, 30), 30);
        assert_eq!(r.effective_for_override(Category::Traces, 5000), 5000);
    }

    #[test]
    fn check_override_cases() {
        let r = Resolved {
            traces: None,
            ..Resolved::uniform(30)
        };
        assert_eq!(r.check_override(Category::Errors, 29), Ok(()));
        assert_eq!(r.check_override(Category::Errors, 1), Ok(()));
        assert_eq!(
            r.check_override(Category::Errors, 30),
            Err(OverrideError::NotShorter)
        );
        assert_eq!(
            r.check_override(Category::Errors, 31),
            Err(OverrideError::NotShorter)
        );
        assert_eq!(
            r.check_override(Category::Logs, 0),
            Err(OverrideError::Zero)
        );
        assert_eq!(
            r.check_override(Category::Releases, 1),
            Err(OverrideError::NotOverridable)
        );
        assert_eq!(
            r.check_override(Category::Traces, MAX_OVERRIDE_DAYS),
            Ok(())
        );
        assert_eq!(
            r.check_override(Category::Traces, MAX_OVERRIDE_DAYS + 1),
            Err(OverrideError::TooLarge)
        );
        assert_eq!(
            r.check_override(Category::Traces, 0),
            Err(OverrideError::Zero)
        );
    }
}
