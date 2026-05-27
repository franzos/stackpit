//! Admin token helpers shared between framework-agnostic and axum sides.

use sha2::{Digest, Sha256};

/// Derive a cookie-safe representation of the admin token. The raw token
/// never lands in a browser cookie jar; only its SHA-256 hex digest does.
pub fn hash_token_for_cookie(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}
