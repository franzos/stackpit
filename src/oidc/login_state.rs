//! Encrypted cookie blob holding state, nonce, and PKCE verifier between
//! `/web/auth/login` and `/web/auth/callback`. AES-256-GCM with a fixed
//! domain-separator AAD; tampering fails on decrypt.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};

use crate::crypto::SecretEncryptor;

/// Domain-separator AAD: prevents cross-context blob reuse.
const AAD: &[u8] = b"stackpit:oidc-login:v1";

#[derive(Serialize, Deserialize)]
pub struct LoginState {
    pub state: String,
    pub nonce: String,
    pub pkce_verifier: String,
}

pub fn pack(enc: &SecretEncryptor, s: &LoginState) -> Option<String> {
    let json = serde_json::to_vec(s).ok()?;
    let ct = enc.encrypt_bytes_with_aad(&json, AAD)?;
    Some(URL_SAFE_NO_PAD.encode(ct))
}

/// `None` on any error -- callers treat missing/forged cookies as expired.
pub fn unpack(enc: &SecretEncryptor, blob_b64: &str) -> Option<LoginState> {
    let ct = URL_SAFE_NO_PAD.decode(blob_b64.trim()).ok()?;
    let pt = enc.decrypt_bytes_with_aad(&ct, AAD)?;
    serde_json::from_slice(&pt).ok()
}
