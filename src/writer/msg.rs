use crate::models::{StorableAttachment, StorableEvent};

pub enum WriteMsg {
    Event(StorableEvent),
    EventWithAttachments(StorableEvent, Vec<StorableAttachment>),
    Shutdown,
}
