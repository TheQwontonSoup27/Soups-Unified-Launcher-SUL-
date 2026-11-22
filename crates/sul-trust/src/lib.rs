//! Cryptographic manifest verification (`blake3` + `ed25519-dalek`) and structural refusal gates.

#![forbid(unsafe_code)]

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
