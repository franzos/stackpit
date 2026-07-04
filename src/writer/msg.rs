use crate::ingest::models::{StorableAttachment, StorableEvent};

pub enum WriteMsg {
    Event(StorableEvent),
    EventWithAttachments(StorableEvent, Vec<StorableAttachment>),
    Shutdown,
}

/// Byte weight (event payload, pre-extracted derived data, attachment data) for the queued-memory budget; the single source of truth for both increment and decrement, so callers must weigh a message before `flush_batch` compresses it in place.
pub(crate) fn msg_bytes(msg: &WriteMsg) -> usize {
    match msg {
        WriteMsg::Event(e) => e.queued_bytes(),
        WriteMsg::EventWithAttachments(e, atts) => {
            e.queued_bytes() + atts.iter().map(|a| a.data.len()).sum::<usize>()
        }
        WriteMsg::Shutdown => 0,
    }
}
