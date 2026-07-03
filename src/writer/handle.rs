use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::Sender;

use crate::ingest::models::{StorableAttachment, StorableEvent};
use crate::util::stats::IngestStats;
use crate::util::throttle::Throttle;

use super::msg::{msg_bytes, WriteMsg};

type SendError = Box<tokio::sync::mpsc::error::TrySendError<WriteMsg>>;

/// Cap on total queued payload bytes (channel + in-flight + retry), set above the 200MB per-envelope cumulative cap so a single large-but-legal envelope is never permanently rejected while still bounding memory on a small VM.
const MAX_QUEUED_PAYLOAD_BYTES: usize = 512 * 1024 * 1024;

/// Public handle to the writer task, wrapping the channel sender with domain
/// methods so callers don't construct `WriteMsg` variants by hand.
#[derive(Clone)]
pub struct WriterHandle {
    tx: Sender<WriteMsg>,
    ingest_stats: Arc<IngestStats>,
    backpressure_warn_throttle: Arc<Throttle>,
    queued_bytes: Arc<AtomicUsize>,
    max_queued_bytes: usize,
}

impl WriterHandle {
    pub fn new(
        tx: Sender<WriteMsg>,
        ingest_stats: Arc<IngestStats>,
        queued_bytes: Arc<AtomicUsize>,
    ) -> Self {
        Self {
            tx,
            ingest_stats,
            backpressure_warn_throttle: Arc::new(Throttle::new()),
            queued_bytes,
            max_queued_bytes: MAX_QUEUED_PAYLOAD_BYTES,
        }
    }

    #[cfg(test)]
    fn new_with_byte_cap(
        tx: Sender<WriteMsg>,
        ingest_stats: Arc<IngestStats>,
        queued_bytes: Arc<AtomicUsize>,
        max_queued_bytes: usize,
    ) -> Self {
        Self {
            tx,
            ingest_stats,
            backpressure_warn_throttle: Arc::new(Throttle::new()),
            queued_bytes,
            max_queued_bytes,
        }
    }

    /// Reserve `size` bytes against the budget; over-admits slightly under concurrency but self-corrects (acceptable for a memory guard) and leaves the counter unchanged on rejection.
    fn try_reserve_bytes(&self, size: usize) -> bool {
        let prev = self.queued_bytes.fetch_add(size, Ordering::Relaxed);
        if prev + size > self.max_queued_bytes {
            self.queued_bytes.fetch_sub(size, Ordering::Relaxed);
            false
        } else {
            true
        }
    }

    #[cfg(any(test, feature = "integration-tests"))]
    pub fn raw_sender(&self) -> &Sender<WriteMsg> {
        &self.tx
    }

    #[cfg(any(test, feature = "integration-tests"))]
    pub fn queued_bytes(&self) -> usize {
        self.queued_bytes.load(Ordering::Relaxed)
    }

    // -- Event ingestion (fire-and-forget) -----------------------------------

    pub fn send_event(&self, event: StorableEvent) -> Result<(), SendError> {
        self.warn_if_backpressure();
        let msg = WriteMsg::Event(event);
        let size = msg_bytes(&msg);
        if !self.try_reserve_bytes(size) {
            self.ingest_stats
                .events_rejected
                .fetch_add(1, Ordering::Relaxed);
            return Err(Box::new(TrySendError::Full(msg)));
        }
        match self.tx.try_send(msg) {
            Ok(()) => {
                self.ingest_stats
                    .events_accepted
                    .fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(e) => {
                self.queued_bytes.fetch_sub(size, Ordering::Relaxed);
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
        let size = msg_bytes(&msg);
        if !self.try_reserve_bytes(size) {
            self.ingest_stats
                .events_rejected
                .fetch_add(1, Ordering::Relaxed);
            return Err(Box::new(TrySendError::Full(msg)));
        }
        match self.tx.try_send(msg) {
            Ok(()) => {
                self.ingest_stats
                    .events_accepted
                    .fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(e) => {
                self.queued_bytes.fetch_sub(size, Ordering::Relaxed);
                self.ingest_stats
                    .events_rejected
                    .fetch_add(1, Ordering::Relaxed);
                Err(Box::new(e))
            }
        }
    }

    /// All-or-nothing envelope send: reserve the byte budget then one channel permit per event before sending any, so a full or closed channel rejects the whole envelope up front (503) without queuing a prefix, which stops a retrying SDK from re-sending already-accepted events; any rejection returns `false` and leaves the counter unchanged.
    pub fn send_envelope(&self, events: Vec<(StorableEvent, Vec<StorableAttachment>)>) -> bool {
        self.warn_if_backpressure();
        let n = events.len();
        if n == 0 {
            return true;
        }
        let total: usize = events
            .iter()
            .map(|(e, atts)| e.payload.len() + atts.iter().map(|a| a.data.len()).sum::<usize>())
            .sum();
        if !self.try_reserve_bytes(total) {
            self.ingest_stats
                .events_rejected
                .fetch_add(n as u64, Ordering::Relaxed);
            return false;
        }
        let permits = match self.tx.try_reserve_many(n) {
            Ok(p) => p,
            Err(_) => {
                self.queued_bytes.fetch_sub(total, Ordering::Relaxed);
                self.ingest_stats
                    .events_rejected
                    .fetch_add(n as u64, Ordering::Relaxed);
                return false;
            }
        };
        for (permit, (event, attachments)) in permits.zip(events) {
            let msg = if attachments.is_empty() {
                WriteMsg::Event(event)
            } else {
                WriteMsg::EventWithAttachments(event, attachments)
            };
            permit.send(msg);
        }
        self.ingest_stats
            .events_accepted
            .fetch_add(n as u64, Ordering::Relaxed);
        true
    }

    // -- Lifecycle -----------------------------------------------------------

    pub fn shutdown(&self) -> Result<(), Box<tokio::sync::mpsc::error::TrySendError<WriteMsg>>> {
        self.tx.try_send(WriteMsg::Shutdown).map_err(Box::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::models::ItemType;

    fn ev(id: &str) -> StorableEvent {
        StorableEvent::new(id.to_string(), ItemType::Event, vec![0], 1, "k".to_string())
    }

    fn ev_bytes(id: &str, len: usize) -> StorableEvent {
        StorableEvent::new(
            id.to_string(),
            ItemType::Event,
            vec![0u8; len],
            1,
            "k".to_string(),
        )
    }

    fn handle_with_capacity(cap: usize) -> (WriterHandle, tokio::sync::mpsc::Receiver<WriteMsg>) {
        let (tx, rx) = tokio::sync::mpsc::channel::<WriteMsg>(cap);
        (
            WriterHandle::new(
                tx,
                Arc::new(IngestStats::new()),
                Arc::new(AtomicUsize::new(0)),
            ),
            rx,
        )
    }

    fn handle_with_byte_cap(
        msg_cap: usize,
        byte_cap: usize,
    ) -> (WriterHandle, tokio::sync::mpsc::Receiver<WriteMsg>) {
        let (tx, rx) = tokio::sync::mpsc::channel::<WriteMsg>(msg_cap);
        (
            WriterHandle::new_with_byte_cap(
                tx,
                Arc::new(IngestStats::new()),
                Arc::new(AtomicUsize::new(0)),
                byte_cap,
            ),
            rx,
        )
    }

    #[tokio::test]
    async fn send_envelope_rejects_and_queues_nothing_when_it_wont_fit() {
        let (handle, rx) = handle_with_capacity(2);
        let events = vec![(ev("a"), vec![]), (ev("b"), vec![]), (ev("c"), vec![])];
        assert!(!handle.send_envelope(events));
        assert_eq!(rx.len(), 0, "a rejected envelope must queue nothing");
        assert_eq!(
            handle.ingest_stats.events_accepted.load(Ordering::Relaxed),
            0
        );
    }

    #[tokio::test]
    async fn send_envelope_queues_all_when_it_fits() {
        let (handle, rx) = handle_with_capacity(4);
        let events = vec![(ev("a"), vec![]), (ev("b"), vec![])];
        assert!(handle.send_envelope(events));
        assert_eq!(rx.len(), 2, "an accepted envelope must queue every event");
        assert_eq!(
            handle.ingest_stats.events_accepted.load(Ordering::Relaxed),
            2
        );
    }

    // Message-count capacity is generous; only the byte cap should reject here.
    #[tokio::test]
    async fn send_event_rejected_when_byte_cap_exceeded() {
        let (handle, rx) = handle_with_byte_cap(1000, 100);
        assert!(handle.send_event(ev_bytes("a", 150)).is_err());
        assert_eq!(rx.len(), 0, "a byte-rejected send must queue nothing");
        assert_eq!(
            handle.queued_bytes(),
            0,
            "a rejected send must leave the counter unchanged"
        );
        assert_eq!(
            handle.ingest_stats.events_rejected.load(Ordering::Relaxed),
            1
        );
    }

    #[tokio::test]
    async fn send_event_accepted_again_after_counter_decremented() {
        let (handle, mut rx) = handle_with_byte_cap(1000, 100);
        assert!(handle.send_event(ev_bytes("a", 80)).is_ok());
        assert_eq!(handle.queued_bytes(), 80);
        // A second 80-byte send would exceed the 100-byte cap.
        assert!(handle.send_event(ev_bytes("b", 80)).is_err());
        assert_eq!(handle.queued_bytes(), 80, "rejection must not leak bytes");

        // Simulate the writer loop draining the first message.
        let msg = rx.try_recv().unwrap();
        handle
            .queued_bytes
            .fetch_sub(msg_bytes(&msg), Ordering::Relaxed);
        assert_eq!(handle.queued_bytes(), 0);

        // Same-size send now fits again.
        assert!(handle.send_event(ev_bytes("c", 80)).is_ok());
        assert_eq!(handle.queued_bytes(), 80);
    }

    // Reserving then freeing the same message returns the counter to its start,
    // proving msg_bytes matches on both the increment and decrement side.
    #[tokio::test]
    async fn reserve_then_free_balances() {
        let (handle, mut rx) = handle_with_byte_cap(1000, 10_000);
        let start = handle.queued_bytes();
        let att = StorableAttachment {
            event_id: "z".to_string(),
            filename: "f".to_string(),
            content_type: None,
            data: vec![0u8; 300],
        };
        assert!(handle
            .send_event_with_attachments(ev_bytes("z", 200), vec![att])
            .is_ok());
        assert_eq!(handle.queued_bytes(), start + 500);
        let msg = rx.try_recv().unwrap();
        handle
            .queued_bytes
            .fetch_sub(msg_bytes(&msg), Ordering::Relaxed);
        assert_eq!(
            handle.queued_bytes(),
            start,
            "increment and decrement must cancel"
        );
    }
}
