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
    GitHub,
    Forgejo,
    GitLab,
}

impl std::str::FromStr for IntegrationKind {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "webhook" => Ok(Self::Webhook),
            "slack" => Ok(Self::Slack),
            "email" => Ok(Self::Email),
            "github" => Ok(Self::GitHub),
            "forgejo" => Ok(Self::Forgejo),
            "gitlab" => Ok(Self::GitLab),
            other => anyhow::bail!("unknown integration kind: {other}"),
        }
    }
}

impl IntegrationKind {
    pub const ALL: [Self; 6] = [
        Self::Webhook,
        Self::Slack,
        Self::Email,
        Self::GitHub,
        Self::Forgejo,
        Self::GitLab,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Webhook => "webhook",
            Self::Slack => "slack",
            Self::Email => "email",
            Self::GitHub => "github",
            Self::Forgejo => "forgejo",
            Self::GitLab => "gitlab",
        }
    }

    // by value to match the existing as_str(self) convention on this Copy enum
    pub fn is_tracker(self) -> bool {
        matches!(self, Self::GitHub | Self::Forgejo | Self::GitLab)
    }

    /// Whether this kind sits behind [`crate::commercial::license::Feature::Integrations`].
    /// Email is the free baseline so an unlicensed install can still alert.
    pub fn requires_license(self) -> bool {
        !matches!(self, Self::Email)
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

/// One entry in a grouped stack trace: either a frame shown on its own, or a
/// run of consecutive library frames collapsed behind a single control.
pub enum FrameGroup<'a> {
    Single(&'a StackFrame),
    LibraryRun(&'a [StackFrame]),
}

impl ExceptionData {
    /// Group the frames so runs of two or more consecutive library frames
    /// collapse behind one control, leaving in-app frames and lone library
    /// frames as they are. Order is preserved.
    ///
    /// Without this an Android ANR renders forty `android::IPCThreadState` rows
    /// with the one app frame buried among them.
    pub fn frame_groups(&self) -> Vec<FrameGroup<'_>> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.frames.len() {
            if self.frames[i].in_app {
                out.push(FrameGroup::Single(&self.frames[i]));
                i += 1;
                continue;
            }
            let start = i;
            while i < self.frames.len() && !self.frames[i].in_app {
                i += 1;
            }
            let run = &self.frames[start..i];
            if run.len() >= 2 {
                out.push(FrameGroup::LibraryRun(run));
            } else {
                out.push(FrameGroup::Single(&run[0]));
            }
        }
        out
    }

    /// True when the stack looks like an un-symbolicated minified JS bundle:
    /// frames carry column numbers (a JS/minified tell) but no source context
    /// resolved, so no source map has been applied. Drives a hint linking to
    /// the project's source-map settings.
    pub fn looks_minified(&self) -> bool {
        !self.frames.is_empty()
            && self.frames.iter().all(|f| !f.has_detail())
            && self.frames.iter().any(|f| {
                f.colno.is_some()
                    || f.filename.ends_with(".js")
                    || f.filename.ends_with(".jsbundle")
            })
    }
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

    /// One stack-trace line, in the shape people paste into a bug report:
    /// `at function (file:line:col)`.
    pub fn copy_line(&self) -> String {
        let mut loc = self.filename.clone();
        if let Some(ln) = self.lineno {
            loc.push_str(&format!(":{ln}"));
            if let Some(cn) = self.colno {
                loc.push_str(&format!(":{cn}"));
            }
        }
        format!("at {} ({})", self.function, loc)
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

/// Distinct, non-empty breadcrumb categories, sorted — the option set for the
/// type filter above the crumb table.
pub fn breadcrumb_categories(crumbs: &[Breadcrumb]) -> Vec<String> {
    let set: std::collections::BTreeSet<&str> = crumbs
        .iter()
        .map(|c| c.category.as_str())
        .filter(|c| !c.is_empty())
        .collect();
    set.into_iter().map(String::from).collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn frame(filename: &str, colno: Option<u64>, context: Option<&str>) -> StackFrame {
        StackFrame {
            filename: filename.to_string(),
            function: "f".to_string(),
            lineno: Some(1),
            colno,
            context_line: context.map(String::from),
            pre_context: Vec::new(),
            post_context: Vec::new(),
            in_app: true,
            vars: Vec::new(),
            source_links: Vec::new(),
        }
    }

    fn lib_frame(filename: &str) -> StackFrame {
        StackFrame {
            in_app: false,
            ..frame(filename, None, None)
        }
    }

    /// `Single` if a lone frame, else the run length.
    fn shape(e: &ExceptionData) -> Vec<Option<usize>> {
        e.frame_groups()
            .iter()
            .map(|g| match g {
                FrameGroup::Single(_) => None,
                FrameGroup::LibraryRun(fs) => Some(fs.len()),
            })
            .collect()
    }

    #[test]
    fn frame_groups_collapses_runs_of_two_or_more() {
        // app, lib, lib, lib, app -> single, run(3), single
        let e = exc(vec![
            frame("a.rs", None, None),
            lib_frame("l1.rs"),
            lib_frame("l2.rs"),
            lib_frame("l3.rs"),
            frame("b.rs", None, None),
        ]);
        assert_eq!(shape(&e), vec![None, Some(3), None]);
    }

    #[test]
    fn frame_groups_leaves_lone_library_frames_alone() {
        // A single library frame between app frames is not worth a control.
        let e = exc(vec![
            frame("a.rs", None, None),
            lib_frame("l1.rs"),
            frame("b.rs", None, None),
        ]);
        assert_eq!(shape(&e), vec![None, None, None]);
    }

    // The case the feature exists for: an ANR whose app frame is buried in
    // framework rows. Exactly one collapsed run must render.
    #[test]
    fn frame_groups_anr_shape_collapses_to_one_control() {
        let mut frames: Vec<StackFrame> = (0..40)
            .map(|i| lib_frame(&format!("android::IPCThreadState{i}")))
            .collect();
        frames.insert(20, frame("com/app/Main.java", None, None));

        let e = exc(frames);
        let groups = e.frame_groups();
        assert_eq!(groups.len(), 3, "run, app frame, run");
        assert_eq!(shape(&e), vec![Some(20), None, Some(20)]);
        assert_eq!(
            e.frame_groups()
                .iter()
                .filter(|g| matches!(g, FrameGroup::Single(f) if f.in_app))
                .count(),
            1
        );
    }

    #[test]
    fn frame_groups_preserves_order_and_covers_every_frame() {
        let e = exc(vec![
            lib_frame("l1.rs"),
            lib_frame("l2.rs"),
            frame("a.rs", None, None),
            lib_frame("l3.rs"),
        ]);
        let seen: Vec<&str> = e
            .frame_groups()
            .iter()
            .flat_map(|g| match g {
                FrameGroup::Single(f) => vec![f.filename.as_str()],
                FrameGroup::LibraryRun(fs) => fs.iter().map(|f| f.filename.as_str()).collect(),
            })
            .collect();
        assert_eq!(seen, vec!["l1.rs", "l2.rs", "a.rs", "l3.rs"]);
    }

    #[test]
    fn frame_groups_handles_empty_and_all_app() {
        assert!(exc(Vec::new()).frame_groups().is_empty());
        let all_app = exc(vec![frame("a.rs", None, None), frame("b.rs", None, None)]);
        assert_eq!(shape(&all_app), vec![None, None]);
    }

    #[test]
    fn copy_line_reads_like_a_stack_trace_line() {
        let mut f = frame("src/main.rs", Some(9), None);
        f.function = "handle".into();
        assert_eq!(f.copy_line(), "at handle (src/main.rs:1:9)");

        f.colno = None;
        assert_eq!(f.copy_line(), "at handle (src/main.rs:1)");

        f.lineno = None;
        assert_eq!(f.copy_line(), "at handle (src/main.rs)");
    }

    fn exc(frames: Vec<StackFrame>) -> ExceptionData {
        ExceptionData {
            exc_type: "TypeError".into(),
            exc_value: "boom".into(),
            mechanism_handled: None,
            mechanism_type: None,
            frames,
        }
    }

    #[test]
    fn minified_js_without_source_is_flagged() {
        let e = exc(vec![
            frame("app:///main.jsbundle", Some(34), None),
            frame("app:///main.jsbundle", Some(29), None),
        ]);
        assert!(e.looks_minified());
    }

    #[test]
    fn symbolicated_frames_are_not_flagged() {
        // A frame with source context is not "minified", even with a colno.
        let e = exc(vec![frame("app.js", Some(10), Some("let x = 1;"))]);
        assert!(!e.looks_minified());
    }

    #[test]
    fn non_js_without_colno_is_not_flagged() {
        // A plain backend frame (no colno, no .js) shouldn't trigger the JS hint.
        let e = exc(vec![frame("app/models/user.rb", None, None)]);
        assert!(!e.looks_minified());
    }

    #[test]
    fn empty_frames_are_not_flagged() {
        assert!(!exc(Vec::new()).looks_minified());
    }

    #[test]
    fn tracker_kinds_roundtrip_and_flag() {
        for (s, k) in [
            ("github", IntegrationKind::GitHub),
            ("forgejo", IntegrationKind::Forgejo),
            ("gitlab", IntegrationKind::GitLab),
        ] {
            assert_eq!(IntegrationKind::from_str(s).unwrap(), k);
            assert_eq!(k.as_str(), s);
            assert!(k.is_tracker());
        }
        assert!(!IntegrationKind::Webhook.is_tracker());
    }
}
