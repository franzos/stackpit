use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc::Sender;

use crate::ingest::models::{StorableAttachment, StorableEvent};
use crate::util::stats::IngestStats;
use crate::util::throttle::Throttle;

use super::msg::WriteMsg;

type SendError = Box<tokio::sync::mpsc::error::TrySendError<WriteMsg>>;

/// Public handle to the writer task, wrapping the channel sender with domain
/// methods so callers don't construct `WriteMsg` variants by hand.
#[derive(Clone)]
pub struct WriterHandle {
    tx: Sender<WriteMsg>,
    ingest_stats: Arc<IngestStats>,
    backpressure_warn_throttle: Arc<Throttle>,
}

impl WriterHandle {
    pub fn new(tx: Sender<WriteMsg>, ingest_stats: Arc<IngestStats>) -> Self {
        Self {
            tx,
            ingest_stats,
            backpressure_warn_throttle: Arc::new(Throttle::new()),
        }
    }

    #[cfg(any(test, feature = "integration-tests"))]
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
        if self.backpressure_warn_throttle.allow(now, 1) {
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
