//! Decode + verify a base64-encoded license blob against the baked-in
//! Ed25519 public key. The OPLB/CBOR/Ed25519 wire-format decode and signature
//! check are delegated to the MIT `signetlib` crate; everything here is
//! Stackpit's entitlement policy (product check, timestamp coercion, and the
//! typed [`License`] mapping). Stackpit never signs licenses.

use chrono::{DateTime, TimeZone, Utc};
use ed25519_dalek::VerifyingKey;
use signetlib::codec::DecodeError;

use crate::commercial::license::{Feature, License};
use crate::commercial::PUBLIC_KEY_BYTES;

/// The `product` claim this binary accepts. Per-product signing keys are the
/// real gate; this is defense-in-depth for clearer "wrong product" errors.
const EXPECTED_PRODUCT: &str = "stackpit";

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

    let vk = VerifyingKey::from_bytes(PUBLIC_KEY_BYTES)
        .map_err(|e| VerifyError::Malformed(format!("baked-in pubkey: {e}")))?;

    let claims = signetlib::codec::decode_and_verify(trimmed, &vk).map_err(|e| match e {
        DecodeError::Malformed(s) => VerifyError::Malformed(s),
        DecodeError::BadSignature => VerifyError::BadSignature,
    })?;

    if !claims.product.is_empty() && claims.product != EXPECTED_PRODUCT {
        return Err(VerifyError::WrongProduct);
    }

    into_license(claims)
}

fn into_license(claims: signetlib::claims::Claims) -> Result<License, VerifyError> {
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
