//! Decode + verify a base64-encoded license blob against the baked-in
//! Ed25519 public keys (offline root, and the licence shop's web key).
//! The OPLB/CBOR/Ed25519 wire-format decode and signature
//! check are delegated to the MIT `signetlib` crate; everything here is
//! Stackpit's entitlement policy (product check, timestamp coercion, and the
//! typed [`License`] mapping). Stackpit never signs licenses.

use chrono::{DateTime, TimeZone, Utc};
use ed25519_dalek::VerifyingKey;
use signetlib::codec::DecodeError;

use crate::commercial::license::{Feature, License};
use crate::commercial::{PUBLIC_KEY_BYTES, WEB_PUBLIC_KEY_BYTES};

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
    /// Parses fine, but the signature doesn't verify against either
    /// baked-in key. Either tampered or signed with the wrong key
    /// (e.g. an old key after rotation).
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

/// Flash key for the activate page. A key rather than English text because the
/// activate handler redirects and the banner is resolved against the request
/// locale on the other side; the detailed `Display` form goes into
/// `tracing::warn` for the operator.
pub fn user_message(err: &VerifyError) -> &'static str {
    match err {
        VerifyError::Empty => crate::html::flash::LICENSE_EMPTY,
        VerifyError::BadSignature => crate::html::flash::LICENSE_BAD_SIGNATURE,
        VerifyError::WrongProduct => crate::html::flash::LICENSE_WRONG_PRODUCT,
        _ => crate::html::flash::LICENSE_UNREADABLE,
    }
}

/// Decode a base64 license blob, verify its signature against either
/// baked-in public key, and convert it into the typed [`License`].
///
/// Both the offline root key and the shop's web key are accepted; see
/// [`WEB_PUBLIC_KEY_BYTES`] for why the split exists.
pub fn decode_and_verify(b64: &str) -> Result<License, VerifyError> {
    let trimmed = b64.trim();
    if trimmed.is_empty() {
        return Err(VerifyError::Empty);
    }

    let root = VerifyingKey::from_bytes(PUBLIC_KEY_BYTES)
        .map_err(|e| VerifyError::Malformed(format!("baked-in pubkey: {e}")))?;
    let web = VerifyingKey::from_bytes(WEB_PUBLIC_KEY_BYTES)
        .map_err(|e| VerifyError::Malformed(format!("baked-in web pubkey: {e}")))?;

    let claims =
        signetlib::codec::decode_and_verify_any(trimmed, &[root, web]).map_err(|e| match e {
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
        tier: claims.tier,
        product: claims.product,
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

    /// Same, but signed with the licence shop's *web* key rather than the
    /// offline root, and carrying the claim set the shop's SKU sells. This is
    /// the blob shape a real purchase at licenses.gofranz.com produces.
    const WEB_FIXTURE: &str = "T1BMQgGiYWNY3axhdgFqbGljZW5zZV9pZHggOTgwNWRhMGVmMzZkM2EzMmUyNWRjNTU1Nzg0MzA4MGNoY3VzdG9tZXJxU3RhY2twaXQgV2ViIFRlc3RlZW1haWxxd2ViQHN0YWNrcGl0LnRlc3RkdGllcmNwcm9ncHJvZHVjdGhzdGFja3BpdGlpc3N1ZWRfYXQaaoICfmpleHBpcmVzX2F0GnDdKf9oZmVhdHVyZXOCbW9ic2VydmFiaWxpdHlsaW50ZWdyYXRpb25zaG1heF9vcmdz9mltYXhfc2VhdHP2ZG5vdGVgYXNYQNBhj3HPFrfIMjMxxDEqBhPZC+dSTB6Q4hWxj+mwh9FnC3681p+t4TjLF3DTj+ERlQV0qaczwgEVV0n96yq5vQg=";

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

    #[test]
    fn web_signed_blob_verifies() {
        let license = decode_and_verify(WEB_FIXTURE).expect("web-signed blob verifies");
        assert_eq!(license.customer, "Stackpit Web Test");
        assert!(license.has_feature(Feature::Observability));
        assert!(license.has_feature(Feature::Integrations));
    }

    /// The two fixtures carry *different* tiers, which is what proves the value
    /// is read from the blob. The persisted row used to hardcode `"business"`,
    /// so a `pro` licence recorded itself as the top tier.
    #[test]
    fn the_tier_and_product_come_from_the_blob() {
        let root = decode_and_verify(FIXTURE).expect("fixture verifies");
        assert_eq!(root.tier, "business");
        assert_eq!(root.product, "stackpit");

        let web = decode_and_verify(WEB_FIXTURE).expect("web fixture verifies");
        assert_eq!(web.tier, "pro", "a pro licence must not read as business");
        assert_eq!(web.product, "stackpit");

        assert_ne!(
            root.tier, web.tier,
            "the fixtures must differ or this proves nothing"
        );
    }

    /// Guards the key material itself: a truncated file, or web-pubkey.bin
    /// accidentally holding a copy of the root key, would silently collapse
    /// this back to single-key verification.
    #[test]
    fn both_baked_in_keys_are_valid_and_distinct() {
        let root = VerifyingKey::from_bytes(PUBLIC_KEY_BYTES).expect("root pubkey parses");
        let web = VerifyingKey::from_bytes(WEB_PUBLIC_KEY_BYTES).expect("web pubkey parses");
        assert_ne!(
            root.to_bytes(),
            web.to_bytes(),
            "web-pubkey.bin must not be a copy of pubkey.bin"
        );
    }
}
