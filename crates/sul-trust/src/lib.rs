//! Trust checks: cryptographic manifest verification (`blake3` + `ed25519-dalek`), SHA-256 build manifests and the checks
//! built on them, and the rules for when SUL may act without asking. Sits directly above `sul-core` (D-154).
//!
//! Two verification tiers, on purpose (Ch. 14 §14.13.1). Tier 1 is SUL's own code: the spec names Ed25519 Minisign signatures
//! and binary watermarks; this file holds a `blake3` digest check and a raw Ed25519 signature check ([`verify_ed25519`],
//! RFC 8032, strict). Not built: the Minisign file format, the binary watermarks, and the question of who signs and where
//! the public key is embedded (the installer and the update channel decide that). Tier 2 is an external game's files: SUL inventories them with read-only SHA-256 baseline manifests
//! ([`manifest`]), the digest Steam and the game vendors publish, and never modifies them.
//!
//! | What it gives                                                                      | Module        |
//! |------------------------------------------------------------------------------------|---------------|
//! | `blake3` digest check, the trust tiers (this file)                                 | crate root    |
//! | SHA-256 build manifests, read-only verification, schema migration                  | [`manifest`]  |
//! | What the checks report: findings, certainty, program kinds, the "not a guarantee" line | [`findings`]  |
//! | What a verification of a build against its manifest means                          | [`integrity`] |
//! | When SUL may act on a file without asking                                          | [`policy`]    |
//! | Discord webhook-address checking, with the secret kept out of every place it could leak | [`webhook`] |
//!
//! The quarantine is in `sul-quarantine` and package scanning is in `sul-mod`; both depend on this crate.

#![forbid(unsafe_code)]

pub mod findings;
pub mod integrity;
pub mod manifest;
pub mod policy;
pub mod webhook;

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrustTier {
    Tier1Verified,
    Tier2SignedPack,
    Tier3Untrusted,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TrustError {
    #[error("blake3 digest mismatch")]
    DigestMismatch,
    #[error("ed25519 signature verification failed")]
    InvalidSignature,
}

#[inline]
pub fn verify_blake3_digest(payload: &[u8], expected_hash: &[u8; 32]) -> Result<(), TrustError> {
    let actual = blake3::hash(payload);
    if actual.as_bytes() == expected_hash {
        Ok(())
    } else {
        Err(TrustError::DigestMismatch)
    }
}

/// Verify a detached Ed25519 signature (RFC 8032) over `payload`.
///
/// This is the strict check (`verify_strict`): besides a wrong signature it refuses a public key of small order (a "weak"
/// key that makes the equation hold for any message) and a signature whose `R` is of small order, both of which the plain
/// check of RFC 8032 accepts. Any failure, including a public key that is not a valid curve point, is
/// [`TrustError::InvalidSignature`]; the caller is never told which, because the answer is the same: do not trust it.
///
/// It checks a signature; it does not say whose key this is. Whether `public_key` is one SUL trusts is the caller's
/// question (the key is embedded in the signed build; Ch. 14 §14.13.1).
pub fn verify_ed25519(
    payload: &[u8],
    signature: &[u8; 64],
    public_key: &[u8; 32],
) -> Result<(), TrustError> {
    let key = ed25519_dalek::VerifyingKey::from_bytes(public_key)
        .map_err(|_| TrustError::InvalidSignature)?;
    let signature = ed25519_dalek::Signature::from_bytes(signature);
    key.verify_strict(payload, &signature)
        .map_err(|_| TrustError::InvalidSignature)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arr<const N: usize>(hex_text: &str) -> [u8; N] {
        hex::decode(hex_text)
            .unwrap()
            .try_into()
            .expect("wrong length")
    }

    // RFC 8032 section 7.1, TEST 1 and TEST 2. The keys and signatures were also reproduced with an independent
    // implementation (the Python `cryptography` package) when these tests were written.
    const RFC_PUB_1: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
    const RFC_SIG_1: &str = "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b";
    const RFC_PUB_2: &str = "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c";
    const RFC_SIG_2: &str = "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00";

    #[test]
    fn ed25519_accepts_the_rfc_8032_vectors() {
        assert_eq!(
            verify_ed25519(b"", &arr(RFC_SIG_1), &arr(RFC_PUB_1)),
            Ok(())
        );
        assert_eq!(
            verify_ed25519(&[0x72], &arr(RFC_SIG_2), &arr(RFC_PUB_2)),
            Ok(())
        );
    }

    #[test]
    fn ed25519_refuses_a_changed_message_signature_or_key() {
        let sig = arr::<64>(RFC_SIG_2);
        let key = arr::<32>(RFC_PUB_2);
        assert_eq!(
            verify_ed25519(&[0x73], &sig, &key),
            Err(TrustError::InvalidSignature),
            "message changed"
        );
        let mut bad_sig = sig;
        bad_sig[10] ^= 1;
        assert_eq!(
            verify_ed25519(&[0x72], &bad_sig, &key),
            Err(TrustError::InvalidSignature),
            "signature changed"
        );
        assert_eq!(
            verify_ed25519(&[0x72], &sig, &arr(RFC_PUB_1)),
            Err(TrustError::InvalidSignature),
            "another key"
        );
        // The same signature is not valid for the empty message either.
        assert_eq!(
            verify_ed25519(b"", &sig, &key),
            Err(TrustError::InvalidSignature)
        );
    }

    #[test]
    fn ed25519_refuses_a_key_that_is_not_a_curve_point() {
        // y = 2 is not the y coordinate of any point on the curve, so the key does not decode.
        let mut not_a_point = [0u8; 32];
        not_a_point[0] = 2;
        assert_eq!(
            verify_ed25519(b"x", &[0u8; 64], &not_a_point),
            Err(TrustError::InvalidSignature)
        );
    }

    #[test]
    fn ed25519_is_strict_about_a_weak_key() {
        // The identity point as the public key, with R = identity and S = 0: the plain RFC 8032 equation holds for every
        // message, so anyone could "sign" anything. The strict check must refuse it.
        let mut identity = [0u8; 32];
        identity[0] = 1;
        let mut forged = [0u8; 64];
        forged[0] = 1;
        for message in [&b""[..], b"anything", b"the whole build"] {
            assert_eq!(
                verify_ed25519(message, &forged, &identity),
                Err(TrustError::InvalidSignature)
            );
        }
        // Prove the test is not vacuous: the non-strict check really does accept this forgery.
        use ed25519_dalek::Verifier;
        let key = ed25519_dalek::VerifyingKey::from_bytes(&identity).unwrap();
        assert!(
            key.verify(b"anything", &ed25519_dalek::Signature::from_bytes(&forged))
                .is_ok(),
            "the plain check was expected to accept the weak-key forgery"
        );
    }

    #[test]
    fn ed25519_round_trips_a_signature_made_here() {
        use ed25519_dalek::Signer;
        let signing = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
        let public = signing.verifying_key().to_bytes();
        let payload = b"sulpack-header-v1";
        let signature = signing.sign(payload).to_bytes();
        assert_eq!(verify_ed25519(payload, &signature, &public), Ok(()));
        assert_eq!(
            verify_ed25519(b"sulpack-header-v2", &signature, &public),
            Err(TrustError::InvalidSignature)
        );
    }

    #[test]
    fn blake3_verification_works() {
        let data = b"sulpack-header-v1";
        let digest = *blake3::hash(data).as_bytes();
        assert_eq!(verify_blake3_digest(data, &digest), Ok(()));
        assert_eq!(
            verify_blake3_digest(b"tampered", &digest),
            Err(TrustError::DigestMismatch)
        );
    }
}
