//! Admin browser-session store shared between framework-agnostic and axum sides.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use sha2::{Digest, Sha256};

/// Absolute lifetime of an admin browser session.
pub const ADMIN_SESSION_TTL_SECS: u64 = 24 * 60 * 60;

/// In-memory admin browser sessions: per-login random handle -> absolute
/// expiry. Keys are `SHA-256(handle)` so the map never holds raw cookie
/// values; a restart drops all sessions (admin re-login is acceptable).
#[derive(Default)]
pub struct AdminSessionStore {
    sessions: Mutex<HashMap<[u8; 32], Instant>>,
}

impl AdminSessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn key(handle: &str) -> [u8; 32] {
        Sha256::digest(handle.as_bytes()).into()
    }

    pub fn insert(&self, handle: &str, ttl: Duration) {
        let now = Instant::now();
        let mut sessions = self.sessions.lock();
        // Opportunistic prune so revoked-by-expiry entries don't accumulate.
        sessions.retain(|_, deadline| *deadline > now);
        sessions.insert(Self::key(handle), now + ttl);
    }

    /// True iff the handle exists and has not expired. Hashing the presented
    /// value first keeps the lookup independent of stored handle bytes.
    pub fn is_valid(&self, handle: &str) -> bool {
        self.sessions
            .lock()
            .get(&Self::key(handle))
            .is_some_and(|deadline| *deadline > Instant::now())
    }

    pub fn revoke(&self, handle: &str) {
        self.sessions.lock().remove(&Self::key(handle));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserted_handle_is_valid_until_revoked() {
        let store = AdminSessionStore::new();
        store.insert("handle-a", Duration::from_secs(60));
        assert!(store.is_valid("handle-a"));
        assert!(!store.is_valid("handle-b"), "unknown handle must fail");
        store.revoke("handle-a");
        assert!(!store.is_valid("handle-a"), "revoked handle must fail");
    }

    #[test]
    fn expired_handle_is_rejected() {
        let store = AdminSessionStore::new();
        store.insert("handle-a", Duration::ZERO);
        assert!(!store.is_valid("handle-a"));
    }

    #[test]
    fn insert_prunes_expired_entries() {
        let store = AdminSessionStore::new();
        store.insert("stale", Duration::ZERO);
        store.insert("fresh", Duration::from_secs(60));
        assert_eq!(store.sessions.lock().len(), 1);
        assert!(store.is_valid("fresh"));
    }
}
