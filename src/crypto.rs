use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;

pub struct SecretEncryptor {
    cipher: Aes256Gcm,
}

impl SecretEncryptor {
    /// Grabs `STACKPIT_MASTER_KEY` from the environment. Expects 64 hex chars (32 bytes).
    /// Returns None if unset or malformed -- we don't crash, just warn and move on.
    pub fn from_env() -> Option<Self> {
        let hex_key = std::env::var("STACKPIT_MASTER_KEY").ok()?;
        let key_bytes = match hex::decode(hex_key.trim()) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("STACKPIT_MASTER_KEY: invalid hex encoding: {e}");
                return None;
            }
        };
        if key_bytes.len() != 32 {
            tracing::warn!("STACKPIT_MASTER_KEY must be 64 hex chars (32 bytes), ignoring");
            return None;
        }
        let cipher = Aes256Gcm::new_from_slice(&key_bytes).ok()?;
        Some(Self { cipher })
    }

    /// AES-256-GCM encrypt. Output is base64(nonce || ciphertext).
    pub fn encrypt(&self, plaintext: &str) -> Option<String> {
        use aes_gcm::aead::rand_core::RngCore;
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self.cipher.encrypt(nonce, plaintext.as_bytes()).ok()?;

        let mut combined = Vec::with_capacity(12 + ciphertext.len());
        combined.extend_from_slice(&nonce_bytes);
        combined.extend_from_slice(&ciphertext);
        Some(B64.encode(&combined))
    }

    /// Reverses what `encrypt` did -- base64-decode, split nonce, decrypt.
    pub fn decrypt(&self, encoded: &str) -> Option<String> {
        let combined = B64.decode(encoded).ok()?;
        if combined.len() < 12 {
            return None;
        }
        let (nonce_bytes, ciphertext) = combined.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = self.cipher.decrypt(nonce, ciphertext).ok()?;
        String::from_utf8(plaintext).ok()
    }
}

/// Encrypts a secret for storage. Returns an error if no encryptor is
/// configured or if encryption fails -- callers must decide whether to
/// proceed without encryption.
pub fn encrypt_secret(
    raw: &str,
    encryptor: Option<&SecretEncryptor>,
) -> Result<String, &'static str> {
    match encryptor {
        Some(enc) => enc
            .encrypt(raw)
            .ok_or("encryption failed — check STACKPIT_MASTER_KEY"),
        None => Err("no master key configured — set STACKPIT_MASTER_KEY to enable encryption"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_encryptor() -> SecretEncryptor {
        // All zeros -- fine for tests, obviously don't do this in prod
        let key = [0u8; 32];
        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        SecretEncryptor { cipher }
    }

    #[test]
    fn round_trip() {
        let enc = test_encryptor();
        let plaintext = "my-secret-webhook-token";
        let encrypted = enc.encrypt(plaintext).unwrap();
        assert_ne!(encrypted, plaintext);
        let decrypted = enc.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn decrypt_garbage_returns_none() {
        let enc = test_encryptor();
        assert!(enc.decrypt("not-valid-base64!!!").is_none());
        assert!(enc.decrypt("AAAA").is_none()); // too short
    }
}
