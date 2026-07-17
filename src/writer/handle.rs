use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

use crate::ingest::models::{StorableAttachment, StorableEvent};
use crate::util::stats::IngestStats;
use crate::util::throttle::Throttle;

use super::msg::{msg_bytes, WriteMsg};

type SendError = Box<tokio::sync::mpsc::error::TrySendError<WriteMsg>>;

/// Cap on total queued payload bytes (channel + in-flight + retry), set above the 200MB per-envelope cumulative cap so a single large-but-legal envelope is never permanently rejected while still bounding memory on a small VM.
const MAX_QUEUED_PAYLOAD_BYTES: usize = 512 * 1024 * 1024;

/// Public handle to the writer task(s), wrapping the channel senders with domain
/// methods so callers don't construct `WriteMsg` variants by hand. With more
/// than one writer, sends are distributed round-robin; an envelope always lands
/// on one writer so its all-or-nothing admission stays a single-channel reserve.
#[derive(Clone)]
pub struct WriterHandle {
    txs: Vec<Sender<WriteMsg>>,
    rr: Arc<AtomicUsize>,
    ingest_stats: Arc<IngestStats>,
    backpressure_warn_throttle: Arc<Throttle>,
    queued_bytes: Arc<AtomicUsize>,
    max_queued_bytes: usize,
    /// Out-of-band stop signal for the writer loops, delivered independently of
    /// the (possibly full) data channel so shutdown can't be lost under backpressure.
    shutdown: CancellationToken,
}

impl WriterHandle {
    pub fn new(
        txs: Vec<Sender<WriteMsg>>,
        ingest_stats: Arc<IngestStats>,
        queued_bytes: Arc<AtomicUsize>,
        shutdown: CancellationToken,
    ) -> Self {
        assert!(!txs.is_empty(), "WriterHandle needs at least one sender");
        Self {
            txs,
            rr: Arc::new(AtomicUsize::new(0)),
            ingest_stats,
            backpressure_warn_throttle: Arc::new(Throttle::new()),
            queued_bytes,
            max_queued_bytes: MAX_QUEUED_PAYLOAD_BYTES,
            shutdown,
        }
    }

    #[cfg(test)]
    fn new_with_byte_cap(
        txs: Vec<Sender<WriteMsg>>,
        ingest_stats: Arc<IngestStats>,
        queued_bytes: Arc<AtomicUsize>,
        max_queued_bytes: usize,
    ) -> Self {
        Self {
            txs,
            rr: Arc::new(AtomicUsize::new(0)),
            ingest_stats,
            backpressure_warn_throttle: Arc::new(Throttle::new()),
            queued_bytes,
            max_queued_bytes,
            shutdown: CancellationToken::new(),
        }
    }

    fn shard(&self) -> &Sender<WriteMsg> {
        if self.txs.len() == 1 {
            return &self.txs[0];
        }
        &self.txs[self.rr.fetch_add(1, Ordering::Relaxed) % self.txs.len()]
    }

    /// Compress on the accept path so the CPU cost spreads across request tasks
    /// instead of serializing on the writer. Large payloads go through
    /// `block_in_place` (multi-thread runtime only) to not stall the worker.
    fn compress_event(event: &mut StorableEvent) {
        if event.compressed {
            return;
        }
        if event.payload.len() > crate::util::INLINE_CPU_MAX_BYTES {
            super::block_in_place_if_multi_thread(|| event.compress_payload());
        } else {
            event.compress_payload();
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
        &self.txs[0]
    }

    #[cfg(any(test, feature = "integration-tests"))]
    pub fn queued_bytes(&self) -> usize {
        self.queued_bytes.load(Ordering::Relaxed)
    }

    // -- Event ingestion (fire-and-forget) -----------------------------------

    pub fn send_event(&self, mut event: StorableEvent) -> Result<(), SendError> {
        // Once shutdown starts, reject instead of falsely acking into a queue nobody drains.
        if self.shutdown.is_cancelled() {
            self.ingest_stats
                .events_rejected
                .fetch_add(1, Ordering::Relaxed);
            return Err(Box::new(TrySendError::Closed(WriteMsg::Event(event))));
        }
        Self::compress_event(&mut event);
        let tx = self.shard();
        self.warn_if_backpressure(tx);
        let msg = WriteMsg::Event(event);
        let size = msg_bytes(&msg);
        if !self.try_reserve_bytes(size) {
            self.ingest_stats
                .events_rejected
                .fetch_add(1, Ordering::Relaxed);
            return Err(Box::new(TrySendError::Full(msg)));
        }
        match tx.try_send(msg) {
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

    fn warn_if_backpressure(&self, tx: &Sender<WriteMsg>) {
        let capacity = tx.capacity();
        let max = tx.max_capacity();
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
        mut event: StorableEvent,
        attachments: Vec<StorableAttachment>,
    ) -> Result<(), SendError> {
        if self.shutdown.is_cancelled() {
            self.ingest_stats
                .events_rejected
                .fetch_add(1, Ordering::Relaxed);
            let msg = if attachments.is_empty() {
                WriteMsg::Event(event)
            } else {
                WriteMsg::EventWithAttachments(event, attachments)
            };
            return Err(Box::new(TrySendError::Closed(msg)));
        }
        Self::compress_event(&mut event);
        let tx = self.shard();
        self.warn_if_backpressure(tx);
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
        match tx.try_send(msg) {
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
    pub fn send_envelope(&self, mut events: Vec<(StorableEvent, Vec<StorableAttachment>)>) -> bool {
        if self.shutdown.is_cancelled() {
            self.ingest_stats
                .events_rejected
                .fetch_add(events.len() as u64, Ordering::Relaxed);
            return false;
        }
        for (event, _) in events.iter_mut() {
            Self::compress_event(event);
        }
        let tx = self.shard();
        self.warn_if_backpressure(tx);
        let n = events.len();
        if n == 0 {
            return true;
        }
        // Must mirror msg_bytes so the drain-side decrement balances this reserve.
        let total: usize = events
            .iter()
            .map(|(e, atts)| e.queued_bytes() + atts.iter().map(|a| a.data.len()).sum::<usize>())
            .sum();
        if !self.try_reserve_bytes(total) {
            self.ingest_stats
                .events_rejected
                .fetch_add(n as u64, Ordering::Relaxed);
            return false;
        }
        let permits = match tx.try_reserve_many(n) {
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
        // Reliable path: the loops observe this even when the data channel is full.
        self.shutdown.cancel();
        // Best-effort sentinel too, so a non-full queue drains in FIFO order before exit.
        let mut result = Ok(());
        for tx in &self.txs {
            if let Err(e) = tx.try_send(WriteMsg::Shutdown) {
                if result.is_ok() {
                    result = Err(Box::new(e));
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::models::ItemType;

    fn ev(id: &str) -> StorableEvent {
        StorableEvent::new(id.to_string(), ItemType::Event, vec![0], 1, "k".to_string())
    }

    // Pre-marked compressed so send paths don't shrink the payload; these tests
    // assert byte-budget accounting against exact sizes.
    fn ev_bytes(id: &str, len: usize) -> StorableEvent {
        let mut e = StorableEvent::new(
            id.to_string(),
            ItemType::Event,
            vec![0u8; len],
            1,
            "k".to_string(),
        );
        e.compressed = true;
        e
    }

    fn handle_with_capacity(cap: usize) -> (WriterHandle, tokio::sync::mpsc::Receiver<WriteMsg>) {
        let (tx, rx) = tokio::sync::mpsc::channel::<WriteMsg>(cap);
        (
            WriterHandle::new(
                vec![tx],
                Arc::new(IngestStats::new()),
                Arc::new(AtomicUsize::new(0)),
                CancellationToken::new(),
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
                vec![tx],
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

    // After shutdown() cancels the token, every send path must reject instead of
    // acking into a queue the writer will never drain.
    #[tokio::test]
    async fn sends_rejected_after_shutdown_cancel() {
        let (handle, rx) = handle_with_capacity(10);
        let _ = handle.shutdown();
        assert!(handle.send_event(ev("a")).is_err());
        assert!(handle.send_event_with_attachments(ev("b"), vec![]).is_err());
        assert!(!handle.send_envelope(vec![(ev("c"), vec![])]));
        assert_eq!(rx.len(), 1, "only the shutdown sentinel may be queued");
        assert_eq!(
            handle.queued_bytes(),
            0,
            "rejected sends must not reserve bytes"
        );
        assert_eq!(
            handle.ingest_stats.events_rejected.load(Ordering::Relaxed),
            3
        );
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
