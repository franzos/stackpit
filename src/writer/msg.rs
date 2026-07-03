use crate::ingest::models::{StorableAttachment, StorableEvent};

pub enum WriteMsg {
    Event(StorableEvent),
    EventWithAttachments(StorableEvent, Vec<StorableAttachment>),
    Shutdown,
}

/// Uncompressed byte weight (event payload plus attachment data) for the queued-memory budget; the single source of truth for both increment and decrement, so callers must weigh a message before `flush_batch` compresses it in place.
pub(crate) fn msg_bytes(msg: &WriteMsg) -> usize {
    match msg {
        WriteMsg::Event(e) => e.payload.len(),
        WriteMsg::EventWithAttachments(e, atts) => {
            e.payload.len() + atts.iter().map(|a| a.data.len()).sum::<usize>()
        }
        WriteMsg::Shutdown => 0,
    }
}
