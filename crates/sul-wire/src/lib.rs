//! Zero-copy `rkyv` (`bytecheck`-validated) IPC framing over `sul-ring` and
//! Win32 shared-memory adapter traits (`CreateFileMappingW` / `MapViewOfFile`, D-147).

use rkyv::rancor::Error as RkyvError;
use sul_ipc_types::IpcMessage;
use sul_ring::{LocalSpscRing, RingError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WireError {
    #[error("rkyv serialization failed: {0}")]
    Encode(String),
    #[error("rkyv validation failed: {0}")]
    Decode(String),
    #[error("ring transport error: {0}")]
    Ring(#[from] RingError),
}

/// Thin OS adapter trait for Win32 shared memory (`CreateFileMappingW` / `MapViewOfFile`)
/// so pure-Rust framing logic can be tested without Windows kernel handles (D-141, D-147).
pub trait ShmSectionAdapter {
    fn map_name(&self) -> &str;
    fn capacity_bytes(&self) -> usize;
}

pub fn encode_ipc(msg: &IpcMessage) -> Result<rkyv::util::AlignedVec, WireError> {
    rkyv::to_bytes::<RkyvError>(msg).map_err(|e| WireError::Encode(e.to_string()))
}

pub fn validate_archived_ipc(bytes: &[u8]) -> Result<&rkyv::Archived<IpcMessage>, WireError> {
    rkyv::access::<rkyv::Archived<IpcMessage>, RkyvError>(bytes)
        .map_err(|e| WireError::Decode(e.to_string()))
}

pub fn push_ipc(ring: &mut LocalSpscRing, msg: &IpcMessage) -> Result<(), WireError> {
    let bytes = encode_ipc(msg)?;
    ring.try_push(&bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_copy_ipc_roundtrip() {
        let mut ring = LocalSpscRing::new(8, 256);
        let msg = IpcMessage::Ping {
            seq: 42,
            timestamp_ns: 1_000_000,
        };
        push_ipc(&mut ring, &msg).unwrap();
        let mut buf = Vec::new();
        ring.try_pop(&mut buf).unwrap();
        let archived = validate_archived_ipc(&buf).unwrap();
        match archived {
            rkyv::Archived::<IpcMessage>::Ping { seq, .. } => assert_eq!(seq.to_native(), 42),
            _ => panic!("unexpected variant"),
        }
    }
}
