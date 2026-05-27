use aes_gcm::aead::{Aead, KeyInit, OsRng, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use zeroize::Zeroizing;

/// AES-GCM standard nonce length, prepended to every ciphertext.
pub const NONCE_LEN: usize = 12;

/// `N` random bytes from the OS RNG, hex-encoded (yields `2*N` chars).
pub fn random_hex<const N: usize>() -> String {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).expect("OS RNG must be available");
    hex::encode(buf)
}

pub struct SecretEncryptor {
    cipher: Aes256Gcm,
}

impl SecretEncryptor {
    /// Read `STACKPIT_MASTER_KEY` (64 hex chars = 32 bytes).
    ///
    /// - unset → `Ok(None)` (no at-rest encryption)
    /// - set but malformed → `Err(_)` so startup fails fast. Silently
    ///   disabling encryption on a typo'd key would be a footgun for OAuth.
    pub fn from_env() -> Result<Option<Self>, &'static str> {
        let Ok(raw) = std::env::var("STACKPIT_MASTER_KEY") else {
            return Ok(None);
        };
        // Zeroize the heap-allocated String so the key doesn't linger.
        let hex_key = Zeroizing::new(raw);
        let key_bytes: Zeroizing<Vec<u8>> = match hex::decode(hex_key.trim()) {
            Ok(b) => Zeroizing::new(b),
            Err(_) => {
                return Err(
                    "STACKPIT_MASTER_KEY is set but is not valid hex; expected 64 hex chars",
                );
            }
        };
        if key_bytes.len() != 32 {
            return Err("STACKPIT_MASTER_KEY is set but is not 32 bytes (64 hex chars)");
        }
        let cipher = Aes256Gcm::new_from_slice(&key_bytes)
            .map_err(|_| "STACKPIT_MASTER_KEY: AES-256-GCM cipher init failed")?;
        Ok(Some(Self { cipher }))
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

    /// AES-256-GCM encrypt with AAD. Output is `nonce || ciphertext || tag`
    /// (raw bytes, BLOB-friendly). Bind `aad` to the row's PK so blob-swap
    /// attacks across rows surface as decryption failures.
    pub fn encrypt_bytes_with_aad(&self, plaintext: &[u8], aad: &[u8]) -> Option<Vec<u8>> {
        use aes_gcm::aead::rand_core::RngCore;
        let mut nonce_bytes = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self
            .cipher
            .encrypt(
                nonce,
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .ok()?;

        let mut combined = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        combined.extend_from_slice(&nonce_bytes);
        combined.extend_from_slice(&ciphertext);
        Some(combined)
    }

    /// Reverses [`Self::encrypt_bytes_with_aad`]. `aad` mismatch fails
    /// decryption (it's covered by the GCM tag).
    pub fn decrypt_bytes_with_aad(&self, blob: &[u8], aad: &[u8]) -> Option<Vec<u8>> {
        if blob.len() < NONCE_LEN {
            return None;
        }
        let (nonce_bytes, ciphertext) = blob.split_at(NONCE_LEN);
        let nonce = Nonce::from_slice(nonce_bytes);
        self.cipher
            .decrypt(
                nonce,
                Payload {
                    msg: ciphertext,
                    aad,
                },
            )
            .ok()
    }
}

/// Encrypt secret for storage (error if unconfigured or encryption fails).
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
