use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use secrecy::{ExposeSecret, SecretString};
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
    /// Resolve the master key (64 hex chars = 32 bytes) from, in order:
    /// `STACKPIT_MASTER_KEY` env var, then `[server].master_key` in config.
    ///
    /// - neither set → `Ok(None)` (no at-rest encryption)
    /// - set but malformed → `Err(_)` so startup fails fast. Silently
    ///   disabling encryption on a typo'd key would be a footgun for OAuth.
    pub fn from_config_or_env(config_key: Option<&SecretString>) -> Result<Option<Self>, String> {
        if let Ok(raw) = std::env::var("STACKPIT_MASTER_KEY") {
            return Self::parse_key(&Zeroizing::new(raw), "STACKPIT_MASTER_KEY").map(Some);
        }
        match config_key {
            Some(secret) => {
                let raw = Zeroizing::new(secret.expose_secret().to_string());
                Self::parse_key(&raw, "server.master_key").map(Some)
            }
            None => Ok(None),
        }
    }

    fn parse_key(raw: &str, source: &str) -> Result<Self, String> {
        let key_bytes: Zeroizing<Vec<u8>> = match hex::decode(raw.trim()) {
            Ok(b) => Zeroizing::new(b),
            Err(_) => return Err(format!("{source} is not valid hex; expected 64 hex chars")),
        };
        if key_bytes.len() != 32 {
            return Err(format!("{source} is not 32 bytes (64 hex chars)"));
        }
        let cipher = Aes256Gcm::new_from_slice(&key_bytes)
            .map_err(|_| format!("{source}: AES-256-GCM cipher init failed"))?;
        Ok(Self { cipher })
    }

    /// AES-256-GCM encrypt. Output is base64(nonce || ciphertext).
    pub fn encrypt(&self, plaintext: &str) -> Option<String> {
        // Empty AAD is byte-identical to no AAD in GCM, so this stays
        // wire-compatible with data written by the older no-AAD path.
        Some(B64.encode(self.encrypt_bytes_with_aad(plaintext.as_bytes(), b"")?))
    }

    /// Reverses what `encrypt` did -- base64-decode, split nonce, decrypt.
    pub fn decrypt(&self, encoded: &str) -> Option<String> {
        String::from_utf8(self.decrypt_bytes_with_aad(&B64.decode(encoded).ok()?, b"")?).ok()
    }

    /// AES-256-GCM encrypt with AAD. Output is `nonce || ciphertext || tag`
    /// (raw bytes, BLOB-friendly). Bind `aad` to the row's PK so blob-swap
    /// attacks across rows surface as decryption failures.
    pub fn encrypt_bytes_with_aad(&self, plaintext: &[u8], aad: &[u8]) -> Option<Vec<u8>> {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        getrandom::fill(&mut nonce_bytes).expect("OS RNG must be available");
        let nonce = Nonce::from(nonce_bytes);

        let ciphertext = self
            .cipher
            .encrypt(
                &nonce,
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

    /// Fixed all-zero key, so tests don't race on `STACKPIT_MASTER_KEY`.
    #[cfg(test)]
    pub(crate) fn for_tests() -> Self {
        Self {
            cipher: Aes256Gcm::new_from_slice(&[0u8; 32]).expect("fixed test key is 32 bytes"),
        }
    }

    /// Reverses [`Self::encrypt_bytes_with_aad`]. `aad` mismatch fails
    /// decryption (it's covered by the GCM tag).
    pub fn decrypt_bytes_with_aad(&self, blob: &[u8], aad: &[u8]) -> Option<Vec<u8>> {
        if blob.len() < NONCE_LEN {
            return None;
        }
        let (nonce_bytes, ciphertext) = blob.split_at(NONCE_LEN);
        let nonce_arr: [u8; NONCE_LEN] = nonce_bytes.try_into().ok()?;
        let nonce = Nonce::from(nonce_arr);
        self.cipher
            .decrypt(
                &nonce,
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
        SecretEncryptor::for_tests()
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

    /// GCM spec test case 16 (AES-256). Pins the at-rest blob layout
    /// (`nonce || ciphertext || tag`) against a known-answer vector so a
    /// future aes-gcm/aead bump can't silently change what's on disk.
    #[test]
    fn decrypts_known_answer_vector() {
        let key = hex::decode("feffe9928665731c6d6a8f9467308308feffe9928665731c6d6a8f9467308308")
            .unwrap();
        let enc = SecretEncryptor {
            cipher: Aes256Gcm::new_from_slice(&key).unwrap(),
        };
        let mut blob = hex::decode("cafebabefacedbaddecaf888").unwrap();
        blob.extend_from_slice(&hex::decode("522dc1f099567d07f47f37a32a84427d643a8cdcbfe5c0c97598a2bd2555d1aa8cb08e48590dbb3da7b08b1056828838c5f61e6393ba7a0abcc9f66276fc6ece0f4e1768cddf8853bb2d551b").unwrap());
        let aad = hex::decode("feedfacedeadbeeffeedfacedeadbeefabaddad2").unwrap();

        let plaintext = enc.decrypt_bytes_with_aad(&blob, &aad).unwrap();
        assert_eq!(hex::encode(plaintext), "d9313225f88406e5a55909c5aff5269a86a7a9531534f7da2e4c303d8a318a721c3c0c95956809532fcf0e2449a6b525b16aedf5aa0de657ba637b39");
        assert!(enc.decrypt_bytes_with_aad(&blob, b"wrong-aad").is_none());
    }

    #[test]
    fn decrypt_garbage_returns_none() {
        let enc = test_encryptor();
        assert!(enc.decrypt("not-valid-base64!!!").is_none());
        assert!(enc.decrypt("AAAA").is_none()); // too short
    }
}
