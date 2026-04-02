/// Error type for writer replies -- keeps things a bit more structured than raw strings.
#[derive(Debug, Clone)]
pub enum WriteError {
    NotFound(String),
    Internal(String),
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriteError::NotFound(msg) => write!(f, "not found: {msg}"),
            WriteError::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for WriteError {}

impl WriteError {
    pub fn is_not_found(&self) -> bool {
        matches!(self, WriteError::NotFound(_))
    }
}

/// A pending reply from the writer thread. Callers get this back and don't
/// need to care about the underlying channel plumbing.
pub type WriteReply<T> = tokio::sync::oneshot::Receiver<Result<T, WriteError>>;
