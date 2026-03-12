/// Tracks event throughput and failure modes on the ingestion hot path.
/// All counters are monotonically increasing -- take deltas for rates.
pub struct IngestStats {
    pub events_accepted: std::sync::atomic::AtomicU64,
    pub events_rejected: std::sync::atomic::AtomicU64,
    pub events_dropped: std::sync::atomic::AtomicU64,
}

impl IngestStats {
    pub fn new() -> Self {
        Self {
            events_accepted: std::sync::atomic::AtomicU64::new(0),
            events_rejected: std::sync::atomic::AtomicU64::new(0),
            events_dropped: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

/// Keeps a running count of discarded events by reason. This is intentionally
/// decoupled from the filter -- we just bump counters and flush to DB periodically.
pub struct DiscardStats {
    buffer: dashmap::DashMap<(u64, String, Option<i64>, String), u64>,
}

impl DiscardStats {
    pub fn new() -> Self {
        Self {
            buffer: dashmap::DashMap::new(),
        }
    }

    /// Bump the counter for a given project/reason/rule combo.
    pub fn record(&self, project_id: u64, reason: &str, rule_id: Option<i64>) {
        let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
        *self
            .buffer
            .entry((project_id, reason.to_string(), rule_id, date))
            .or_insert(0) += 1;
    }

    /// Writes accumulated counts to the database. We snapshot first, then subtract
    /// what we flushed -- so concurrent `record()` calls aren't lost.
    pub async fn flush(&self, pool: &crate::db::DbPool) -> anyhow::Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        // Grab a snapshot -- anything incremented after this is safe
        let entries: Vec<_> = self
            .buffer
            .iter()
            .map(|r| (r.key().clone(), *r.value()))
            .collect();

        let mut flushed = Vec::with_capacity(entries.len());
        for (key, count) in &entries {
            let (pid, reason, rule_id, date) = key;
            match crate::queries::filters::upsert_discard_stats(
                pool, *pid, reason, *rule_id, date, *count,
            )
            .await
            {
                Ok(()) => flushed.push((key.clone(), *count)),
                Err(e) => tracing::warn!("discard stats: failed to flush entry: {e}"),
            }
        }

        // Subtract only what we successfully wrote -- new increments are preserved
        for (key, count) in flushed {
            self.buffer
                .entry(key)
                .and_modify(|v| *v = v.saturating_sub(count));
        }
        self.buffer.retain(|_, v| *v > 0);

        Ok(())
    }
}
