//! Admin-side filter operations: the persist-then-reload invariant that keeps
//! the in-memory [`FilterEngine`] in sync with the filter tables after a write.
//! Kept out of the hot-path engine and out of the presentation layer.

use std::sync::atomic::{AtomicU32, Ordering};

use crate::db::DbPool;
use crate::filter::FilterEngine;
use crate::queries::filters::load_filter_data;

/// Tracks consecutive reload failures so we can escalate logging.
static RELOAD_FAILURES: AtomicU32 = AtomicU32::new(0);

/// Reloads the engine from the DB after a filter mutation. On success resets
/// the failure counter (logging recovery); on failure increments it and logs
/// so stale rules surface in operations.
pub async fn reload(pool: &DbPool, engine: &FilterEngine) {
    match load_filter_data(pool).await {
        Ok(data) => {
            let prev = RELOAD_FAILURES.swap(0, Ordering::Relaxed);
            if prev > 0 {
                tracing::info!("filter engine recovered after {prev} consecutive reload failures");
            }
            engine.apply_data(data);
        }
        Err(e) => {
            let count = RELOAD_FAILURES.fetch_add(1, Ordering::Relaxed) + 1;
            tracing::error!(
                consecutive_failures = count,
                "filter engine reload failed: {e} — filter rules may be stale"
            );
        }
    }
}

/// Persist-then-reload: when `result` is `Ok`, reload the engine; otherwise
/// leave it untouched. Returns `result` so callers can branch for rendering.
pub async fn persist_and_reload(
    pool: &DbPool,
    engine: &FilterEngine,
    result: anyhow::Result<()>,
) -> anyhow::Result<()> {
    if result.is_ok() {
        reload(pool, engine).await;
    }
    result
}
