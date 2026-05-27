use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc::Sender;

use crate::models::{StorableAttachment, StorableEvent};
use crate::stats::IngestStats;

use super::msg::WriteMsg;

type SendError = Box<tokio::sync::mpsc::error::TrySendError<WriteMsg>>;

/// The public face of the writer task. Wraps the channel sender so
/// callers can just call domain methods instead of constructing `WriteMsg`
/// variants by hand.
#[derive(Clone)]
pub struct WriterHandle {
    tx: Sender<WriteMsg>,
    ingest_stats: Arc<IngestStats>,
    backpressure_last_warn_secs: Arc<AtomicU64>,
}

impl WriterHandle {
    pub fn new(tx: Sender<WriteMsg>, ingest_stats: Arc<IngestStats>) -> Self {
        Self {
            tx,
            ingest_stats,
            backpressure_last_warn_secs: Arc::new(AtomicU64::new(0)),
        }
    }

    #[cfg(test)]
    pub fn raw_sender(&self) -> &Sender<WriteMsg> {
        &self.tx
    }

    // -- Event ingestion (fire-and-forget) -----------------------------------

    pub fn send_event(&self, event: StorableEvent) -> Result<(), SendError> {
        self.warn_if_backpressure();
        match self.tx.try_send(WriteMsg::Event(event)) {
            Ok(()) => {
                self.ingest_stats
                    .events_accepted
                    .fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(e) => {
                self.ingest_stats
                    .events_rejected
                    .fetch_add(1, Ordering::Relaxed);
                Err(Box::new(e))
            }
        }
    }

    fn warn_if_backpressure(&self) {
        let capacity = self.tx.capacity();
        let max = self.tx.max_capacity();
        let used = max - capacity;
        let pct = (used * 100) / max;
        if pct < 80 {
            return;
        }
        // Throttle to once per second so a sustained backup can't flood the log.
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let last = self.backpressure_last_warn_secs.load(Ordering::Relaxed);
        if now > last
            && self
                .backpressure_last_warn_secs
                .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            tracing::warn!(
                "writer channel at {pct}% capacity ({used}/{max}) — ingestion may be backing up"
            );
        }
    }

    pub fn send_event_with_attachments(
        &self,
        event: StorableEvent,
        attachments: Vec<StorableAttachment>,
    ) -> Result<(), SendError> {
        self.warn_if_backpressure();
        let msg = if attachments.is_empty() {
            WriteMsg::Event(event)
        } else {
            WriteMsg::EventWithAttachments(event, attachments)
        };
        match self.tx.try_send(msg) {
            Ok(()) => {
                self.ingest_stats
                    .events_accepted
                    .fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(e) => {
                self.ingest_stats
                    .events_rejected
                    .fetch_add(1, Ordering::Relaxed);
                Err(Box::new(e))
            }
        }
    }

    // -- Lifecycle -----------------------------------------------------------

    pub fn shutdown(&self) -> Result<(), Box<tokio::sync::mpsc::error::TrySendError<WriteMsg>>> {
        self.tx.try_send(WriteMsg::Shutdown).map_err(Box::new)
    }
}
