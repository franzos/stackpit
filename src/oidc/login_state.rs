//! Encrypted cookie blob holding state, nonce, and PKCE verifier between
//! `/web/auth/login` and `/web/auth/callback`. AES-256-GCM with a fixed
//! domain-separator AAD; tampering fails on decrypt.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};

use crate::util::crypto::SecretEncryptor;

/// Domain-separator AAD: prevents cross-context blob reuse.
const AAD: &[u8] = b"stackpit:oidc-login:v1";

/// Mirrors Hydra's auth-code lifetime. Sealed into the blob too: cookie `Max-Age` is only advisory.
pub const LOGIN_TTL_SECONDS: i64 = 600;

#[derive(Serialize, Deserialize)]
pub struct LoginState {
    pub state: String,
    pub nonce: String,
    pub pkce_verifier: String,
    /// Absent reads as the epoch, i.e. expired - deliberate.
    #[serde(default)]
    pub issued_at: i64,
}

impl LoginState {
    pub fn new(state: String, nonce: String, pkce_verifier: String) -> Self {
        Self {
            state,
            nonce,
            pkce_verifier,
            issued_at: chrono::Utc::now().timestamp(),
        }
    }

    #[must_use]
    pub fn is_fresh(&self) -> bool {
        chrono::Utc::now()
            .timestamp()
            .saturating_sub(self.issued_at)
            <= LOGIN_TTL_SECONDS
    }
}

pub fn pack(enc: &SecretEncryptor, s: &LoginState) -> Option<String> {
    let json = serde_json::to_vec(s).ok()?;
    let ct = enc.encrypt_bytes_with_aad(&json, AAD)?;
    Some(URL_SAFE_NO_PAD.encode(ct))
}

/// `None` on any error - callers treat missing/forged/stale cookies as expired.
pub fn unpack(enc: &SecretEncryptor, blob_b64: &str) -> Option<LoginState> {
    let ct = URL_SAFE_NO_PAD.decode(blob_b64.trim()).ok()?;
    let pt = enc.decrypt_bytes_with_aad(&ct, AAD)?;
    let state: LoginState = serde_json::from_slice(&pt).ok()?;
    state.is_fresh().then_some(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> LoginState {
        LoginState::new(
            "state-value".to_string(),
            "nonce-value".to_string(),
            "verifier-value".to_string(),
        )
    }

    #[test]
    fn a_login_state_round_trips() {
        let enc = SecretEncryptor::for_tests();
        let unpacked = unpack(&enc, &pack(&enc, &state()).expect("pack")).expect("unpack");
        assert_eq!(unpacked.state, "state-value");
        assert_eq!(unpacked.nonce, "nonce-value");
        assert_eq!(unpacked.pkce_verifier, "verifier-value");
    }

    #[test]
    fn the_verifier_is_not_readable_from_the_blob() {
        let packed = pack(&SecretEncryptor::for_tests(), &state()).expect("pack");
        assert!(!packed.contains("verifier-value"));
        assert!(!packed.contains("nonce-value"));
    }

    #[test]
    fn a_tampered_blob_does_not_open() {
        let enc = SecretEncryptor::for_tests();
        let mut broken = pack(&enc, &state()).expect("pack");
        broken.push('x');
        assert!(unpack(&enc, &broken).is_none());
        assert!(unpack(&enc, "nonsense").is_none());
    }

    #[test]
    fn an_expired_blob_is_refused_even_though_the_cookie_was_presented() {
        let enc = SecretEncryptor::for_tests();
        let mut stale = state();
        stale.issued_at -= LOGIN_TTL_SECONDS + 1;
        assert!(unpack(&enc, &pack(&enc, &stale).expect("pack")).is_none());
    }

    #[test]
    fn a_blob_written_before_issued_at_existed_reads_as_expired() {
        let enc = SecretEncryptor::for_tests();
        let legacy = serde_json::json!({
            "state": "state-value",
            "nonce": "nonce-value",
            "pkce_verifier": "verifier-value",
        });
        let ct = enc
            .encrypt_bytes_with_aad(&serde_json::to_vec(&legacy).expect("json"), AAD)
            .expect("encrypt");
        assert!(unpack(&enc, &URL_SAFE_NO_PAD.encode(ct)).is_none());
    }
}
