//! Bounding what a tool hands back. Every cut is reported with a count, so the
//! model can tell "there was nothing more" from "there was more and you were not
//! shown it", and narrow its next call.

use serde_json::{json, Value};

use crate::domain::StackFrame;

pub(super) const MAX_STRING_CHARS: usize = 500;
pub(super) const MAX_BREADCRUMBS: usize = 20;
const FRAME_HEAD: usize = 5;
const FRAME_TAIL: usize = 5;

pub(super) const DEFAULT_LIST_LIMIT: u64 = 25;
pub(super) const MAX_LIST_LIMIT: u64 = 50;

/// Server-side ceiling on page size, whatever the client asked for.
pub(super) fn clamp_limit(requested: Option<u64>) -> u64 {
    requested
        .unwrap_or(DEFAULT_LIST_LIMIT)
        .clamp(1, MAX_LIST_LIMIT)
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Report {
    strings_truncated: usize,
    list_items_omitted: usize,
    breadcrumbs_omitted: usize,
    stack_frames_omitted: usize,
    /// Frames outside the application were dropped in favour of in-app ones.
    non_app_frames_dropped: bool,
}

impl Report {
    /// Cap a string at [`MAX_STRING_CHARS`], counting the cut.
    pub(super) fn text(&mut self, s: &str) -> String {
        let chars = s.chars().count();
        if chars <= MAX_STRING_CHARS {
            return s.to_string();
        }
        self.strings_truncated += 1;
        let kept: String = s.chars().take(MAX_STRING_CHARS).collect();
        format!("{kept}[+{} chars truncated]", chars - MAX_STRING_CHARS)
    }

    pub(super) fn opt_text(&mut self, s: Option<&str>) -> Option<String> {
        s.map(|s| self.text(s))
    }

    pub(super) fn note_items_omitted(&mut self, n: usize) {
        self.list_items_omitted += n;
    }

    /// The most recent [`MAX_BREADCRUMBS`], oldest first, as the UI shows them.
    pub(super) fn breadcrumbs<T>(&mut self, all: Vec<T>) -> Vec<T> {
        let omitted = all.len().saturating_sub(MAX_BREADCRUMBS);
        self.breadcrumbs_omitted += omitted;
        all.into_iter().skip(omitted).collect()
    }

    /// First [`FRAME_HEAD`] plus last [`FRAME_TAIL`] frames. When the event
    /// distinguishes in-app frames, only those are candidates: they are where a
    /// reader looks, and vendor frames are what blows the budget.
    pub(super) fn frames<'a>(&mut self, frames: &'a [StackFrame]) -> Vec<&'a StackFrame> {
        let in_app: Vec<&StackFrame> = frames.iter().filter(|f| f.in_app).collect();
        let candidates: Vec<&StackFrame> = if in_app.is_empty() {
            frames.iter().collect()
        } else {
            if in_app.len() < frames.len() {
                self.non_app_frames_dropped = true;
            }
            in_app
        };

        self.stack_frames_omitted += frames.len() - candidates.len();
        if candidates.len() <= FRAME_HEAD + FRAME_TAIL {
            return candidates;
        }
        self.stack_frames_omitted += candidates.len() - FRAME_HEAD - FRAME_TAIL;
        candidates
            .iter()
            .take(FRAME_HEAD)
            .chain(candidates.iter().skip(candidates.len() - FRAME_TAIL))
            .copied()
            .collect()
    }

    fn applied(&self) -> bool {
        self.strings_truncated > 0
            || self.list_items_omitted > 0
            || self.breadcrumbs_omitted > 0
            || self.stack_frames_omitted > 0
    }

    pub(super) fn to_json(&self) -> Value {
        json!({
            "truncated": self.applied(),
            "strings_truncated": self.strings_truncated,
            "list_items_omitted": self.list_items_omitted,
            "breadcrumbs_omitted": self.breadcrumbs_omitted,
            "stack_frames_omitted": self.stack_frames_omitted,
            "non_app_frames_dropped": self.non_app_frames_dropped,
        })
    }
}

pub(super) fn truncation_schema() -> Value {
    json!({
        "type": "object",
        "description": "What was cut from this response. Narrow the query if `truncated` is true.",
        "properties": {
            "truncated": { "type": "boolean" },
            "strings_truncated": { "type": "integer" },
            "list_items_omitted": { "type": "integer" },
            "breadcrumbs_omitted": { "type": "integer" },
            "stack_frames_omitted": { "type": "integer" },
            "non_app_frames_dropped": { "type": "boolean" },
        },
        "required": ["truncated"],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(name: &str, in_app: bool) -> StackFrame {
        StackFrame {
            filename: name.to_string(),
            function: name.to_string(),
            lineno: Some(1),
            colno: None,
            context_line: None,
            pre_context: Vec::new(),
            post_context: Vec::new(),
            in_app,
            vars: Vec::new(),
            source_links: Vec::new(),
        }
    }

    #[test]
    fn a_long_string_is_cut_and_says_by_how_much() {
        let mut report = Report::default();
        let out = report.text(&"x".repeat(MAX_STRING_CHARS + 40));
        assert!(out.ends_with("[+40 chars truncated]"), "got {out}");
        assert_eq!(report.to_json()["strings_truncated"], 1);
        assert_eq!(report.to_json()["truncated"], true);
    }

    #[test]
    fn a_short_string_is_left_alone() {
        let mut report = Report::default();
        assert_eq!(report.text("fine"), "fine");
        assert_eq!(report.to_json()["truncated"], false);
    }

    // Multi-byte input must not be sliced mid-character.
    #[test]
    fn truncation_counts_characters_not_bytes() {
        let mut report = Report::default();
        let out = report.text(&"é".repeat(MAX_STRING_CHARS + 1));
        assert!(out.contains("[+1 chars truncated]"), "got {out}");
    }

    #[test]
    fn breadcrumbs_keep_the_most_recent_and_count_the_rest() {
        let mut report = Report::default();
        let kept = report.breadcrumbs((0..30).collect::<Vec<i32>>());
        assert_eq!(kept.len(), MAX_BREADCRUMBS);
        assert_eq!(kept[0], 10, "the oldest are the ones dropped");
        assert_eq!(kept[MAX_BREADCRUMBS - 1], 29);
        assert_eq!(report.to_json()["breadcrumbs_omitted"], 10);
    }

    #[test]
    fn frames_keep_the_head_and_the_tail() {
        let mut report = Report::default();
        let frames: Vec<StackFrame> = (0..30).map(|i| frame(&format!("f{i}"), false)).collect();
        let kept = report.frames(&frames);
        assert_eq!(kept.len(), FRAME_HEAD + FRAME_TAIL);
        assert_eq!(kept[0].function, "f0");
        assert_eq!(
            kept[FRAME_HEAD].function, "f25",
            "the tail is the last five"
        );
        assert_eq!(report.to_json()["stack_frames_omitted"], 20);
        assert_eq!(report.to_json()["non_app_frames_dropped"], false);
    }

    #[test]
    fn in_app_frames_win_when_the_event_marks_them() {
        let mut report = Report::default();
        let mut frames: Vec<StackFrame> = (0..20).map(|i| frame(&format!("v{i}"), false)).collect();
        frames.push(frame("app", true));
        let kept = report.frames(&frames);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].function, "app");
        assert_eq!(report.to_json()["stack_frames_omitted"], 20);
        assert_eq!(report.to_json()["non_app_frames_dropped"], true);
    }

    #[test]
    fn a_client_limit_cannot_exceed_the_server_ceiling() {
        assert_eq!(clamp_limit(None), DEFAULT_LIST_LIMIT);
        assert_eq!(clamp_limit(Some(0)), 1);
        assert_eq!(clamp_limit(Some(10)), 10);
        assert_eq!(clamp_limit(Some(10_000)), MAX_LIST_LIMIT);
    }
}
