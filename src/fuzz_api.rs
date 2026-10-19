//! Entry points the `fuzz/` crate drives. Everything else stays crate-private.

pub use crate::ingest::parse_log::{log_entries_from_value, parse_log_entries, parse_log_entry};
pub use crate::ingest::parse_metric::parse_metric_payload;
pub use crate::ingest::parse_span::{extract_embedded_spans_from_value, extract_span_fields};
pub use crate::ingest::sourcemap::{decode_sourcemap, parse_artifact_bundle_sync};
pub use crate::locale::{accept, default_locale, from_accept_language};
