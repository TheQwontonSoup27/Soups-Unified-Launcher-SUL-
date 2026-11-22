//! Zero-copy `rkyv` (`bytecheck`-validated) framing of [`IpcMessage`] over `sul-ring` (Ch. 7 §2.3, §2.5), plus the
//! adapter trait for the Win32 shared-memory section (`CreateFileMappingW` / `MapViewOfFile`, D-141, D-147).
//!
//! The alignment rule. `rkyv` reads its archived types in place, so the bytes it validates must start on the archive's
//! alignment ([`FRAME_ALIGN`]). A plain `Vec<u8>` copy of a frame does not promise that. This crate therefore has two ways
//! to read a frame, and refuses the unsafe third:
//!
//! - [`pop_ipc_with`] / [`pop_ipc`] read the frame where the ring put it. `sul-ring` starts every payload on a 64-byte
//!   boundary, so the frame is aligned and nothing is copied.
//! - [`decode_ipc_copy`] copies the bytes into an `AlignedVec` first, for bytes that came from anywhere else.
//! - [`validate_archived_ipc`] and [`decode_ipc`] read the bytes they are given and return [`WireError::Misaligned`] when
//!   those are not aligned. They never read misaligned memory and never panic on it.
//!
//! Backpressure (Ch. 7 §2.3): [`push_ipc`] goes through `LocalSpscRing::try_push` and nothing else. A full ring comes back
//! as `WireError::Ring(RingError::WouldBlock)` with the queued messages untouched; the caller decides whether to retry or
//! drop the message and count the loss. This crate holds no unsafe code: `#![forbid(unsafe_code)]` makes the compiler refuse
//! any `unsafe` block in it. No xtask gate checks that the attribute itself is still there.
//!
//! Not done yet (R0 layer 3): the shared-memory ring across processes, the Tauri binary framing and the ring view for the
//! window. Miri is not available in the container, so nothing here has been run under it (docs/SESSION_STATE.md).

#![forbid(unsafe_code)]

use core::mem::align_of;
use rkyv::{Archived, rancor::Error as RkyvError, util::AlignedVec};
use sul_ipc_types::{IpcMessage, MAX_MESSAGE_BYTES};
use sul_ring::{LocalSpscRing, RingError};
use thiserror::Error;

/// Failure code for a frame that cannot be trusted (Ch. 7 §2.5 failure taxonomy). The spec's recovery is "drop the frame,
/// log, continue": the ring has already released the slot by the time this is returned.
pub const E_WIRE_FRAME_CORRUPT: &str = "E_WIRE_FRAME_CORRUPT";

/// Alignment `rkyv` needs for the archived envelope. The ring's 64-byte payload alignment is a multiple of this.
pub const FRAME_ALIGN: usize = align_of::<Archived<IpcMessage>>();

/// Alignment of the buffers `encode_ipc` and `decode_ipc_copy` allocate (`rkyv`'s default `AlignedVec`).
const ALIGNED_VEC_ALIGN: usize = 16;

const _: () = assert!(ALIGNED_VEC_ALIGN % FRAME_ALIGN == 0);
const _: () = assert!(sul_ipc_types::SLOT_ALIGN % FRAME_ALIGN == 0);

#[derive(Debug, Error)]
pub enum WireError {
    #[error("rkyv serialization failed: {0}")]
    Encode(String),
    #[error("message is {len} bytes encoded, above the {max}-byte message limit")]
    TooLarge { len: usize, max: usize },
    #[error("frame is not {align}-byte aligned (it starts {offset} bytes past an aligned address)")]
    Misaligned { align: usize, offset: usize },
    #[error("rkyv validation failed: {0}")]
    Decode(String),
    #[error("ring transport error: {0}")]
    Ring(#[from] RingError),
}

impl WireError {
    /// The spec's failure code, for the errors that have one. `WouldBlock`, `Empty` and size errors are flow control or
    /// caller mistakes, not corruption, so they have none.
    pub fn code(&self) -> Option<&'static str> {
        match self {
            Self::Decode(_) | Self::Misaligned { .. } | Self::Ring(RingError::Corrupt { .. }) => {
                Some(E_WIRE_FRAME_CORRUPT)
            }
            _ => None,
        }
    }
}

/// Thin OS adapter trait for Win32 shared memory (`CreateFileMappingW` / `MapViewOfFile`)
/// so pure-Rust framing logic can be tested without Windows kernel handles (D-141, D-147).
pub trait ShmSectionAdapter {
    fn map_name(&self) -> &str;
    fn capacity_bytes(&self) -> usize;
}

/// Serializes a message into an aligned buffer, refusing anything above the 64 KB message limit (Ch. 7 §2.3).
pub fn encode_ipc(msg: &IpcMessage) -> Result<AlignedVec, WireError> {
    let bytes = rkyv::to_bytes::<RkyvError>(msg).map_err(|e| WireError::Encode(e.to_string()))?;
    check_len(bytes.len())?;
    Ok(bytes)
}

fn check_len(len: usize) -> Result<(), WireError> {
    if len > MAX_MESSAGE_BYTES {
        return Err(WireError::TooLarge {
            len,
            max: MAX_MESSAGE_BYTES,
        });
    }
    Ok(())
}

/// Validates a frame in place and returns the archived message. The bytes must already be aligned to [`FRAME_ALIGN`]
/// (use [`decode_ipc_copy`] for bytes that might not be); anything else is an error, not undefined behaviour.
pub fn validate_archived_ipc(bytes: &[u8]) -> Result<&Archived<IpcMessage>, WireError> {
    check_len(bytes.len())?;
    let offset = bytes.as_ptr() as usize % FRAME_ALIGN;
    if offset != 0 {
        return Err(WireError::Misaligned {
            align: FRAME_ALIGN,
            offset,
        });
    }
    rkyv::access::<Archived<IpcMessage>, RkyvError>(bytes)
        .map_err(|e| WireError::Decode(e.to_string()))
}

/// Validates an aligned frame and returns an owned message.
pub fn decode_ipc(bytes: &[u8]) -> Result<IpcMessage, WireError> {
    let archived = validate_archived_ipc(bytes)?;
    rkyv::deserialize::<IpcMessage, RkyvError>(archived)
        .map_err(|e| WireError::Decode(e.to_string()))
}

/// Like [`decode_ipc`] but for bytes with no alignment promise: the frame is copied into an aligned buffer first.
pub fn decode_ipc_copy(bytes: &[u8]) -> Result<IpcMessage, WireError> {
    check_len(bytes.len())?;
    let mut aligned = AlignedVec::<ALIGNED_VEC_ALIGN>::with_capacity(bytes.len());
    aligned.extend_from_slice(bytes);
    decode_ipc(&aligned)
}

/// Encodes a message and writes it to the ring with `try_push`. A full ring is `Ring(WouldBlock)`; nothing is dropped.
pub fn push_ipc(ring: &mut LocalSpscRing, msg: &IpcMessage) -> Result<(), WireError> {
    let bytes = encode_ipc(msg)?;
    ring.try_push(&bytes)?;
    Ok(())
}

/// Reads the oldest message straight from its ring slot (no copy), validates it and hands the archived form to `read`.
///
/// The slot is released whether or not the frame is valid, so one bad frame cannot wedge the ring; a bad frame comes back
/// as [`WireError::Decode`] (code [`E_WIRE_FRAME_CORRUPT`]) and the next call reads the next message.
pub fn pop_ipc_with<R>(
    ring: &mut LocalSpscRing,
    read: impl FnOnce(&Archived<IpcMessage>) -> R,
) -> Result<R, WireError> {
    ring.try_pop_with(|bytes| validate_archived_ipc(bytes).map(read))?
}

/// Reads the oldest message and returns it owned.
pub fn pop_ipc(ring: &mut LocalSpscRing) -> Result<IpcMessage, WireError> {
    ring.try_pop_with(decode_ipc)?
}

#[cfg(test)]
mod tests {
    use super::*;
    use sul_ipc_types::{FailureMode, SLOT_ALIGN, TelemetryMsg, Tier1Event};

    fn all_variants() -> Vec<IpcMessage> {
        vec![
            IpcMessage::Ping {
                seq: 42,
                timestamp_ns: 1_000_000,
            },
            IpcMessage::Pong {
                seq: u64::MAX,
                timestamp_ns: 0,
            },
            IpcMessage::TokenChunk {
                request_id: 7,
                seq: 1,
                is_final: false,
                utf8_bytes: "héllo, wörld".as_bytes().to_vec(),
            },
            IpcMessage::TokenChunk {
                request_id: 7,
                seq: 2,
                is_final: true,
                utf8_bytes: Vec::new(),
            },
            IpcMessage::Telemetry(TelemetryMsg {
                subsystem: "ai".into(),
                metric: "ttft".into(),
                value_us: 120,
                timestamp_ns: 5,
            }),
            IpcMessage::Tier1Event {
                event_id: Tier1Event::AuthDisable.id().into(),
                approved: false,
            },
            IpcMessage::Error {
                code: E_WIRE_FRAME_CORRUPT.into(),
                mode: FailureMode::ModeC,
                detail: "dropped".into(),
            },
        ]
    }

    fn ring() -> LocalSpscRing {
        LocalSpscRing::new(8, 512).unwrap()
    }

    /// A tiny deterministic generator, so the damage test needs no extra dependency and fails the same way every run.
    struct XorShift(u64);
    impl XorShift {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }

    #[test]
    fn zero_copy_ipc_roundtrip() {
        let mut ring = ring();
        let msg = IpcMessage::Ping {
            seq: 42,
            timestamp_ns: 1_000_000,
        };
        push_ipc(&mut ring, &msg).unwrap();
        let seq = pop_ipc_with(&mut ring, |archived| match archived {
            rkyv::Archived::<IpcMessage>::Ping { seq, .. } => seq.to_native(),
            _ => panic!("unexpected variant"),
        })
        .unwrap();
        assert_eq!(seq, 42);
    }

    #[test]
    fn every_variant_round_trips_in_place_and_owned() {
        for msg in all_variants() {
            let mut ring = ring();
            push_ipc(&mut ring, &msg).unwrap();
            let matches = pop_ipc_with(&mut ring, |archived| *archived == msg).unwrap();
            assert!(matches, "in-place read differs for {msg:?}");

            push_ipc(&mut ring, &msg).unwrap();
            assert_eq!(pop_ipc(&mut ring).unwrap(), msg);
            assert!(ring.is_empty());
        }
    }

    #[test]
    fn messages_keep_their_order_across_wraps() {
        let mut ring = LocalSpscRing::new(4, 256).unwrap();
        let mut next_push = 0u64;
        let mut next_pop = 0u64;
        for _ in 0..40 {
            for _ in 0..3 {
                let msg = IpcMessage::Ping {
                    seq: next_push,
                    timestamp_ns: 0,
                };
                if push_ipc(&mut ring, &msg).is_ok() {
                    next_push += 1;
                }
            }
            for _ in 0..2 {
                if let Ok(IpcMessage::Ping { seq, .. }) = pop_ipc(&mut ring) {
                    assert_eq!(seq, next_pop);
                    next_pop += 1;
                }
            }
        }
        while let Ok(IpcMessage::Ping { seq, .. }) = pop_ipc(&mut ring) {
            assert_eq!(seq, next_pop);
            next_pop += 1;
        }
        assert_eq!(next_pop, next_push);
        assert!(next_push > 20);
    }

    #[test]
    fn the_frame_the_ring_hands_out_is_aligned() {
        let mut ring = ring();
        // More pushes than slots, so every slot is checked whatever the allocator did with the buffer.
        for round in 0..(ring.capacity() * 2 + 1) as u64 {
            let msg = IpcMessage::Ping {
                seq: round,
                timestamp_ns: 0,
            };
            push_ipc(&mut ring, &msg).unwrap();
            let addr = ring
                .try_pop_with(|bytes| {
                    assert!(validate_archived_ipc(bytes).is_ok());
                    bytes.as_ptr() as usize
                })
                .unwrap();
            assert_eq!(addr % SLOT_ALIGN, 0);
            assert_eq!(addr % FRAME_ALIGN, 0);
        }
    }

    #[test]
    fn a_misaligned_frame_is_refused_not_read() {
        let frame = encode_ipc(&all_variants()[2]).unwrap();
        // One junk byte in front of the frame puts it one byte off the aligned start of the buffer.
        let mut shifted = AlignedVec::<ALIGNED_VEC_ALIGN>::new();
        shifted.push(0xAA);
        shifted.extend_from_slice(&frame);
        let misaligned = &shifted[1..];
        assert_eq!(misaligned, &frame[..]);

        let err = validate_archived_ipc(misaligned).unwrap_err();
        assert!(matches!(
            err,
            WireError::Misaligned { align, offset } if align == FRAME_ALIGN && offset == 1
        ));
        assert_eq!(err.code(), Some(E_WIRE_FRAME_CORRUPT));
        assert!(matches!(
            decode_ipc(misaligned),
            Err(WireError::Misaligned { .. })
        ));

        // The copying path accepts the very same bytes.
        assert_eq!(decode_ipc_copy(misaligned).unwrap(), all_variants()[2]);
    }

    #[test]
    fn a_full_ring_says_would_block_and_loses_nothing() {
        let mut ring = LocalSpscRing::new(4, 256).unwrap();
        let msgs: Vec<IpcMessage> = (0..4)
            .map(|i| IpcMessage::Ping {
                seq: i,
                timestamp_ns: 0,
            })
            .collect();
        for msg in &msgs {
            push_ipc(&mut ring, msg).unwrap();
        }
        let extra = IpcMessage::Pong {
            seq: 99,
            timestamp_ns: 0,
        };
        let err = push_ipc(&mut ring, &extra).unwrap_err();
        assert!(matches!(err, WireError::Ring(RingError::WouldBlock)));
        assert_eq!(err.code(), None);
        for msg in &msgs {
            assert_eq!(&pop_ipc(&mut ring).unwrap(), msg);
        }
        push_ipc(&mut ring, &extra).unwrap();
        assert_eq!(pop_ipc(&mut ring).unwrap(), extra);
    }

    #[test]
    fn an_empty_ring_says_empty() {
        let mut ring = ring();
        assert!(matches!(
            pop_ipc(&mut ring),
            Err(WireError::Ring(RingError::Empty))
        ));
        assert!(matches!(
            pop_ipc_with(&mut ring, |_| ()),
            Err(WireError::Ring(RingError::Empty))
        ));
    }

    #[test]
    fn oversized_messages_are_refused_before_they_reach_the_ring() {
        let huge = IpcMessage::TokenChunk {
            request_id: 1,
            seq: 0,
            is_final: false,
            utf8_bytes: vec![b'x'; MAX_MESSAGE_BYTES],
        };
        let err = encode_ipc(&huge).unwrap_err();
        assert!(matches!(err, WireError::TooLarge { max, .. } if max == MAX_MESSAGE_BYTES));
        assert_eq!(err.code(), None);

        let mut ring = ring();
        assert!(matches!(
            push_ipc(&mut ring, &huge),
            Err(WireError::TooLarge { .. })
        ));
        assert!(ring.is_empty(), "a refused message must not use a slot");

        // A message under the global limit but over this ring's slot is the ring's refusal, and also uses no slot.
        let big = IpcMessage::TokenChunk {
            request_id: 1,
            seq: 0,
            is_final: false,
            utf8_bytes: vec![b'x'; 2000],
        };
        assert!(matches!(
            push_ipc(&mut ring, &big),
            Err(WireError::Ring(RingError::Oversized { .. }))
        ));
        assert!(ring.is_empty());

        // Oversized input to the decoder is refused before any parsing.
        assert!(matches!(
            decode_ipc_copy(&vec![0u8; MAX_MESSAGE_BYTES + 1]),
            Err(WireError::TooLarge { .. })
        ));
    }

    #[test]
    fn garbage_truncated_and_empty_frames_are_errors_and_do_not_wedge_the_ring() {
        let good = IpcMessage::Pong {
            seq: 5,
            timestamp_ns: 6,
        };
        let frame = encode_ipc(&good).unwrap();
        let truncated = &frame[..frame.len() - 1];
        let garbage = [0xFFu8; 48];

        let mut ring = ring();
        ring.try_push(&garbage).unwrap();
        ring.try_push(truncated).unwrap();
        ring.try_push(&[]).unwrap();
        push_ipc(&mut ring, &good).unwrap();

        for _ in 0..3 {
            let err = pop_ipc(&mut ring).unwrap_err();
            assert!(matches!(err, WireError::Decode(_)), "got {err:?}");
            assert_eq!(err.code(), Some(E_WIRE_FRAME_CORRUPT));
        }
        // Each bad frame was dropped and its slot released; the good message behind them is intact.
        assert_eq!(pop_ipc(&mut ring).unwrap(), good);
        assert!(ring.is_empty());
    }

    #[test]
    fn a_slot_the_ring_itself_refuses_carries_the_frame_corrupt_code() {
        // `Corrupt` comes from the ring (a slot left in the wrong state); the wire layer reports it with the same code.
        let err = WireError::from(RingError::Corrupt {
            index: 3,
            reason: "test",
        });
        assert_eq!(err.code(), Some(E_WIRE_FRAME_CORRUPT));
        assert_eq!(WireError::from(RingError::Empty).code(), None);
        assert_eq!(WireError::Encode("x".into()).code(), None);
    }

    #[test]
    fn damaged_frames_never_panic() {
        // Flip bytes in valid frames and read them back. Each result must be a message or an error, never a panic, and the
        // run must see both outcomes, otherwise the damage is too weak or too strong to mean anything.
        let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
        let frames: Vec<AlignedVec> = all_variants()
            .iter()
            .map(|m| encode_ipc(m).unwrap())
            .collect();
        let (mut accepted, mut refused) = (0u32, 0u32);
        for i in 0..6000 {
            let mut bytes = frames[i % frames.len()].to_vec();
            for _ in 0..=(rng.next() % 3) {
                let at = (rng.next() as usize) % bytes.len();
                bytes[at] ^= 1 << (rng.next() % 8);
            }
            let mut ring = ring();
            ring.try_push(&bytes).unwrap();
            match pop_ipc(&mut ring) {
                Ok(_) => accepted += 1,
                Err(WireError::Decode(_)) => refused += 1,
                Err(other) => panic!("unexpected error: {other}"),
            }
        }
        assert!(
            accepted > 100 && refused > 100,
            "accepted {accepted}, refused {refused}"
        );
    }

    #[test]
    fn error_messages_name_the_numbers() {
        let text = WireError::TooLarge {
            len: 70000,
            max: 65536,
        }
        .to_string();
        assert!(text.contains("70000") && text.contains("65536"));
        let text = WireError::Misaligned {
            align: 8,
            offset: 3,
        }
        .to_string();
        assert!(text.contains('8') && text.contains('3'));
    }
}
