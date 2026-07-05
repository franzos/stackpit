//! Decode + verify a base64-encoded license blob against the baked-in
//! Ed25519 public key. Mirrors the encoder in `license-issuer`; the two MUST
//! stay in sync. Only the verification path lives here — Stackpit never signs
//! licenses.

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use chrono::{DateTime, TimeZone, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey, SIGNATURE_LENGTH};
use serde::Deserialize;

use crate::commercial::license::{Feature, License};
use crate::commercial::PUBLIC_KEY_BYTES;

/// Must match `license-issuer::claims::BLOB_MAGIC`.
const BLOB_MAGIC: &[u8; 4] = b"OPLB";
/// Must match `license-issuer::claims::BLOB_VERSION`.
const BLOB_VERSION: u8 = 1;
/// The `product` claim this binary accepts. Per-product signing keys are the
/// real gate; this is defense-in-depth for clearer "wrong product" errors.
const EXPECTED_PRODUCT: &str = "stackpit";

/// On-the-wire envelope (mirror of the issuer's `SignedBlob`).
// Wire-format freeze: any new envelope field requires a coordinated BLOB_VERSION bump.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedBlob {
    #[serde(rename = "c", with = "serde_bytes")]
    claims_cbor: Vec<u8>,
    #[serde(rename = "s", with = "serde_bytes")]
    signature: Vec<u8>,
}

/// Mirror of the issuer's `Claims`. Stringly-typed for forward compat —
/// translated into typed enums by [`into_license`].
// Additive forward-compat: unknown claim fields are ignored so a newer issuer
// can add benign claims without older binaries rejecting the blob. Security-
// relevant forward-compat is gated by BLOB_VERSION (the blob byte prefix), and
// structural strictness lives on the `SignedBlob` envelope above.
#[derive(Debug, Deserialize)]
struct Claims {
    #[allow(dead_code)] // wire-format claim; version gate is the blob byte prefix
    v: u8,
    license_id: String,
    customer: String,
    email: String,
    #[allow(dead_code)] // wire-format claim; single tier today, not branched on at runtime
    tier: String,
    /// Product the license was issued for. Defense-in-depth on top of the
    /// per-product signing key; empty on legacy blobs (treated as a match).
    #[serde(default)]
    product: String,
    issued_at: i64,
    expires_at: Option<i64>,
    #[serde(default)]
    features: Vec<String>,
    #[serde(default)]
    max_orgs: Option<u32>,
    #[allow(dead_code)] // Stackpit ignores seats; the wire may still carry the flag
    #[serde(default)]
    max_seats: Option<u32>,
    #[allow(dead_code)] // free-text issuer note; deserialized but unused at runtime
    #[serde(default)]
    note: String,
}

#[derive(Debug)]
pub enum VerifyError {
    /// Empty input. Surfaces nicer in the UI than "base64 error".
    Empty,
    /// Couldn't base64-decode, doesn't carry the magic, unknown version,
    /// or CBOR parse failure.
    Malformed(String),
    /// Parses fine, but the signature doesn't verify against
    /// [`PUBLIC_KEY_BYTES`]. Either tampered or signed with the wrong
    /// key (e.g. an old key after rotation).
    BadSignature,
    /// Signed correctly but carries a different product's `product` claim.
    WrongProduct,
    /// `issued_at` or `expires_at` couldn't be coerced into a UTC
    /// `DateTime`. Should never happen for issuer-emitted blobs but is
    /// recorded explicitly so we don't accidentally accept zero-stamp
    /// licenses.
    BadTimestamp,
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerifyError::Empty => write!(f, "no license provided"),
            VerifyError::Malformed(s) => write!(f, "malformed license blob: {s}"),
            VerifyError::BadSignature => {
                write!(
                    f,
                    "license signature did not verify (wrong key or tampered)"
                )
            }
            VerifyError::WrongProduct => write!(f, "license is for a different product"),
            VerifyError::BadTimestamp => write!(f, "license carries an invalid timestamp"),
        }
    }
}

impl std::error::Error for VerifyError {}

/// User-facing message for the activate page. Deliberately terse and
/// non-technical; the detailed `Display` form goes into `tracing::warn`
/// for the operator.
pub fn user_message(err: &VerifyError) -> &'static str {
    match err {
        VerifyError::Empty => "Paste your license key to activate.",
        VerifyError::BadSignature => {
            "This license isn't valid for this installation. Double-check you pasted the right key."
        }
        VerifyError::WrongProduct => "This license isn't for Stackpit.",
        _ => "We couldn't read that license. Please check it and try again.",
    }
}

/// Decode a base64 license blob, verify its signature against the
/// baked-in public key, and convert it into the typed [`License`].
pub fn decode_and_verify(b64: &str) -> Result<License, VerifyError> {
    let trimmed = b64.trim();
    if trimmed.is_empty() {
        return Err(VerifyError::Empty);
    }
    let bytes = B64
        .decode(trimmed)
        .map_err(|e| VerifyError::Malformed(format!("base64: {e}")))?;

    if bytes.len() < BLOB_MAGIC.len() + 1 {
        return Err(VerifyError::Malformed("blob too short".into()));
    }
    if &bytes[..BLOB_MAGIC.len()] != BLOB_MAGIC {
        return Err(VerifyError::Malformed("magic mismatch".into()));
    }
    let version = bytes[BLOB_MAGIC.len()];
    if version != BLOB_VERSION {
        return Err(VerifyError::Malformed(format!(
            "unsupported version {version}, expected {BLOB_VERSION}"
        )));
    }

    let cbor_start = BLOB_MAGIC.len() + 1;
    let blob: SignedBlob = ciborium::from_reader(&bytes[cbor_start..])
        .map_err(|e| VerifyError::Malformed(format!("envelope cbor: {e}")))?;

    if blob.signature.len() != SIGNATURE_LENGTH {
        return Err(VerifyError::Malformed(format!(
            "signature length {}, expected {SIGNATURE_LENGTH}",
            blob.signature.len()
        )));
    }
    let sig_bytes: [u8; SIGNATURE_LENGTH] = blob
        .signature
        .as_slice()
        .try_into()
        .map_err(|_| VerifyError::Malformed("signature length".into()))?;
    let signature = Signature::from_bytes(&sig_bytes);

    let verifying = VerifyingKey::from_bytes(PUBLIC_KEY_BYTES)
        .map_err(|e| VerifyError::Malformed(format!("baked-in pubkey: {e}")))?;
    verifying
        .verify(&blob.claims_cbor, &signature)
        .map_err(|_| VerifyError::BadSignature)?;

    let claims: Claims = ciborium::from_reader(blob.claims_cbor.as_slice())
        .map_err(|e| VerifyError::Malformed(format!("claims cbor: {e}")))?;

    if !claims.product.is_empty() && claims.product != EXPECTED_PRODUCT {
        return Err(VerifyError::WrongProduct);
    }

    into_license(claims)
}

fn into_license(claims: Claims) -> Result<License, VerifyError> {
    let issued_at = unix_to_utc(claims.issued_at)?;
    let expires_at = match claims.expires_at {
        Some(ts) => Some(unix_to_utc(ts)?),
        None => None,
    };
    let features = claims
        .features
        .iter()
        .filter_map(|s| Feature::from_wire(s))
        .collect();
    Ok(License {
        license_id: claims.license_id,
        customer: claims.customer,
        email: claims.email,
        issued_at,
        expires_at,
        features,
        max_orgs: claims.max_orgs,
    })
}

fn unix_to_utc(ts: i64) -> Result<DateTime<Utc>, VerifyError> {
    match Utc.timestamp_opt(ts, 0).single() {
        Some(dt) => Ok(dt),
        None => Err(VerifyError::BadTimestamp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real signed blob issued by `license-issuer` for product=stackpit,
    /// customer "Stackpit Test", expiring 2030. Verifies against the baked-in
    /// `pubkey.bin`, exercising the full decode + signature + product path.
    const FIXTURE: &str = "T1BMQgGiYWNYxKxhdgFqbGljZW5zZV9pZHggZTg0NjQ3MzAwNTAzYmU4Nzc5MmM3OGMxZmM5YjZkMzFoY3VzdG9tZXJtU3RhY2twaXQgVGVzdGVlbWFpbHJ0ZXN0QHN0YWNrcGl0LnRlc3RkdGllcmhidXNpbmVzc2dwcm9kdWN0aHN0YWNrcGl0aWlzc3VlZF9hdBpqSpSiamV4cGlyZXNfYXQacr0L/2hmZWF0dXJlc4BobWF4X29yZ3P2aW1heF9zZWF0c/Zkbm90ZWBhc1hAr9JqsTMXdMOfBt5J+uB5zSitJG3wwVyaTy66Cb7d8HrVqKt0oqhX9ygkaoroJBgNo7ZlHMRN486SIBq7tuyGCA==";

    #[test]
    fn empty_input_rejected() {
        assert!(matches!(decode_and_verify(""), Err(VerifyError::Empty)));
    }

    #[test]
    fn garbage_rejected() {
        assert!(matches!(
            decode_and_verify("not-a-license"),
            Err(VerifyError::Malformed(_))
        ));
    }

    #[test]
    fn real_fixture_blob_verifies() {
        let license = decode_and_verify(FIXTURE).expect("fixture blob verifies");
        assert_eq!(license.customer, "Stackpit Test");
        assert_eq!(license.email, "test@stackpit.test");
        assert!(license.expires_at.is_some());
    }
}
