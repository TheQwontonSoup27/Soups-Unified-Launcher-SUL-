//! Trust checks: cryptographic manifest verification (`blake3` + `ed25519-dalek`), SHA-256 build manifests and the checks
//! built on them, and the rules for when SUL may act without asking. Sits directly above `sul-core` (D-154).
//!
//! Two verification tiers, on purpose (Ch. 14 §14.13.1). Tier 1 is SUL's own code: the spec names Ed25519 Minisign signatures
//! and binary watermarks; this file holds a `blake3` digest check only, and the signature check is declared in [`TrustError`]
//! but not built yet. Tier 2 is an external game's files: SUL inventories them with read-only SHA-256 baseline manifests
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

#[cfg(test)]
mod tests {
    use super::*;

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
