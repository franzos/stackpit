pub mod cidr;
pub mod data;
mod engine;
mod glob;
mod rate_limit;
pub mod rules;
mod verdict;

pub use data::FilterData;
pub use engine::{FilterEngine, PreFilterReject};
pub use verdict::FilterVerdict;

/// Case-insensitive substring search -- no allocations, just byte windows.
pub(crate) fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    if needle.len() > haystack.len() {
        return false;
    }
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

/// Case-insensitive `starts_with` -- same idea, no allocations.
pub(crate) fn starts_with_ignore_ascii_case(s: &str, prefix: &str) -> bool {
    s.len() >= prefix.len() && s.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}
