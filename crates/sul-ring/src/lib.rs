//! In-process SPSC slot ring over the spec's slot layout (Ch. 7 §2.3), as a pure-Rust core so Miri/Kani/loom can check the
//! ring rules without OS calls (D-141, D-147).
//!
//! What this is and is not. [`LocalSpscRing`] is driven through `&mut self`, so one thread owns it at a time; it is the
//! single-owner model of the shared-memory ring that `sul-ipc` builds in R0 layer 3. The slot layout, the sequence rule
//! and the checks below are the ones that ring reuses. It is not a lock-free structure shared between threads or
//! processes, and nothing here has been run under Miri or loom (not available in the container, see
//! docs/SESSION_STATE.md).
//!
//! Slot layout (little-endian), `slot_bytes` long, 64-byte aligned:
//!
//! | offset | field | meaning |
//! |---|---|---|
//! | 0 | `state` (u8) | [`SlotState`]: `Full` while a message is readable |
//! | 8 | `seq` (u64) | the producer's position when it wrote the slot; the consumer checks it equals its own position |
//! | 16 | `timestamp` (u64) | reserved; the shared-memory ring stamps it for stale-slot detection |
//! | 24 | `len` (u32) | payload length in bytes |
//! | 64 | payload | starts on a 64-byte boundary, so an `rkyv` frame can be validated in place |
//!
//! Backpressure (Ch. 7 §2.3): a full ring returns [`RingError::WouldBlock`]. Nothing is dropped silently; the caller
//! retries, or drops the message and counts the loss.

#![forbid(unsafe_code)]

use core::sync::atomic::Ordering;
use sul_ipc_types::{
    RING_MAX_BYTES, RING_MAX_SLOTS, SLOT_ALIGN, SLOT_HEADER_BYTES, SLOT_LEN_OFFSET, SLOT_MAX_BYTES,
    SLOT_MIN_BYTES, SLOT_SEQ_OFFSET, SLOT_STATE_OFFSET, SlotState, WireRingHeader,
};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RingError {
    /// The ring is full (the spec's `WouldBlock`). The producer retries, or drops the message and counts the loss.
    #[error("ring is full (WouldBlock): retry, or drop the message and count the loss")]
    WouldBlock,
    #[error("ring buffer is empty")]
    Empty,
    #[error("payload exceeds slot capacity ({len} > {max})")]
    Oversized { len: usize, max: usize },
    #[error("invalid ring geometry: {0}")]
    BadGeometry(&'static str),
    /// A slot failed its checks. The slot has been released, so the ring is still usable; the message in it is lost.
    #[error("slot at position {index} is corrupt: {reason}")]
    Corrupt { index: u64, reason: &'static str },
}

/// A checked ring size: a power-of-two slot count and a slot size the layout can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RingGeometry {
    capacity: u32,
    slot_bytes: u32,
}

impl RingGeometry {
    /// Checks a geometry. `capacity` must be a power of two (the index is `position & (capacity - 1)`), `slot_bytes` a
    /// multiple of 64 between the minimum and the largest message plus header, and the whole ring must stay within
    /// [`RING_MAX_BYTES`].
    pub fn new(capacity: u32, slot_bytes: u32) -> Result<Self, RingError> {
        if capacity == 0 || !capacity.is_power_of_two() {
            return Err(RingError::BadGeometry(
                "capacity must be a non-zero power of two",
            ));
        }
        if capacity > RING_MAX_SLOTS {
            return Err(RingError::BadGeometry("capacity is above the slot limit"));
        }
        let slot = slot_bytes as usize;
        if slot % SLOT_ALIGN != 0 {
            return Err(RingError::BadGeometry(
                "slot size must be a multiple of 64 bytes",
            ));
        }
        if slot < SLOT_MIN_BYTES {
            return Err(RingError::BadGeometry(
                "slot size is below the minimum (a header plus one aligned block)",
            ));
        }
        if slot > SLOT_MAX_BYTES {
            return Err(RingError::BadGeometry(
                "slot size is above the largest message plus its header",
            ));
        }
        match (capacity as usize).checked_mul(slot) {
            Some(total) if total <= RING_MAX_BYTES => {}
            _ => {
                return Err(RingError::BadGeometry(
                    "ring would exceed the allocation limit",
                ));
            }
        }
        Ok(Self {
            capacity,
            slot_bytes,
        })
    }

    /// Checks the header read from a mapped section before any slot is touched: magic, version, then the geometry.
    pub fn from_header(header: &WireRingHeader) -> Result<Self, RingError> {
        if header.magic != WireRingHeader::MAGIC {
            return Err(RingError::BadGeometry("ring header magic does not match"));
        }
        if header.version != WireRingHeader::VERSION {
            return Err(RingError::BadGeometry("ring header version is unsupported"));
        }
        Self::new(header.capacity_pow2, header.slot_bytes)
    }

    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    pub fn slot_bytes(&self) -> u32 {
        self.slot_bytes
    }

    /// Largest payload one slot holds.
    pub fn payload_capacity(&self) -> usize {
        self.slot_bytes as usize - SLOT_HEADER_BYTES
    }

    /// Bytes of slot storage, header included.
    pub fn total_bytes(&self) -> usize {
        self.capacity as usize * self.slot_bytes as usize
    }
}

/// Counters a producer and consumer can report. `would_block` is the "counter" in the spec's
/// retry-then-drop-with-counter rule for high-frequency producers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RingStats {
    pub pushed: u64,
    pub popped: u64,
    pub would_block: u64,
    pub corrupt: u64,
}

/// Fixed-capacity in-process ring using the spec's slot layout. See the module notes for what it is not.
pub struct LocalSpscRing {
    header: WireRingHeader,
    geometry: RingGeometry,
    /// Slot storage plus `SLOT_ALIGN - 1` spare bytes, so the slots can start on a 64-byte boundary wherever the
    /// allocator put the block. Never resized, so `base` stays valid.
    buf: Vec<u8>,
    base: usize,
    stats: RingStats,
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    let mut raw = [0u8; 8];
    raw.copy_from_slice(&bytes[offset..offset + 8]);
    u64::from_le_bytes(raw)
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    let mut raw = [0u8; 4];
    raw.copy_from_slice(&bytes[offset..offset + 4]);
    u32::from_le_bytes(raw)
}

impl LocalSpscRing {
    /// Builds a ring. Bad geometry is an error, not a panic: these numbers come from configuration.
    pub fn new(capacity: u32, slot_bytes: u32) -> Result<Self, RingError> {
        let geometry = RingGeometry::new(capacity, slot_bytes)?;
        let buf = vec![0u8; geometry.total_bytes() + SLOT_ALIGN - 1];
        let addr = buf.as_ptr() as usize;
        let base = (SLOT_ALIGN - addr % SLOT_ALIGN) % SLOT_ALIGN;
        Ok(Self {
            header: WireRingHeader::new(capacity, slot_bytes),
            geometry,
            buf,
            base,
            stats: RingStats::default(),
        })
    }

    pub fn geometry(&self) -> RingGeometry {
        self.geometry
    }

    pub fn capacity(&self) -> usize {
        self.geometry.capacity() as usize
    }

    /// Largest payload `try_push` accepts.
    pub fn payload_capacity(&self) -> usize {
        self.geometry.payload_capacity()
    }

    /// Messages waiting to be read.
    pub fn len(&self) -> usize {
        self.header.available_read() as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn is_full(&self) -> bool {
        self.header.available_write() == 0
    }

    pub fn stats(&self) -> RingStats {
        self.stats
    }

    /// The slot a position maps to. The counters are 64-bit and never wrap in practice (centuries at a million messages a
    /// second); `saturating_sub` in the header also makes a corrupted `tail > head` read as empty.
    fn slot_range(&self, position: u64) -> core::ops::Range<usize> {
        let index = (position as usize) & (self.capacity() - 1);
        let slot = self.geometry.slot_bytes() as usize;
        let start = self.base + index * slot;
        start..start + slot
    }

    /// Writes one message into the next slot, or says why it cannot.
    #[inline]
    pub fn try_push(&mut self, payload: &[u8]) -> Result<(), RingError> {
        let max = self.payload_capacity();
        if payload.len() > max {
            return Err(RingError::Oversized {
                len: payload.len(),
                max,
            });
        }
        if self.header.available_write() == 0 {
            self.stats.would_block += 1;
            return Err(RingError::WouldBlock);
        }
        let head = self.header.head.load(Ordering::Relaxed);
        let range = self.slot_range(head);
        let slot = &mut self.buf[range];
        slot[SLOT_STATE_OFFSET] = SlotState::Writing as u8;
        slot[SLOT_SEQ_OFFSET..SLOT_SEQ_OFFSET + 8].copy_from_slice(&head.to_le_bytes());
        slot[SLOT_LEN_OFFSET..SLOT_LEN_OFFSET + 4]
            .copy_from_slice(&(payload.len() as u32).to_le_bytes());
        slot[SLOT_HEADER_BYTES..SLOT_HEADER_BYTES + payload.len()].copy_from_slice(payload);
        slot[SLOT_STATE_OFFSET] = SlotState::Full as u8;
        self.header
            .head
            .store(head.wrapping_add(1), Ordering::Release);
        self.stats.pushed += 1;
        Ok(())
    }

    /// Reads the oldest message in place and releases its slot. `read` sees exactly the payload bytes, starting on a
    /// 64-byte boundary, which is what lets `sul-wire` validate an `rkyv` frame without copying it.
    ///
    /// A slot that fails its checks (not `Full`, wrong sequence number, impossible length) is released all the same and
    /// reported as [`RingError::Corrupt`], so one bad slot cannot wedge the ring (Ch. 7 §2.3, "Recovery").
    pub fn try_pop_with<R>(&mut self, read: impl FnOnce(&[u8]) -> R) -> Result<R, RingError> {
        if self.header.available_read() == 0 {
            return Err(RingError::Empty);
        }
        let tail = self.header.tail.load(Ordering::Relaxed);
        let range = self.slot_range(tail);
        let max = self.payload_capacity();
        let verdict = {
            let slot = &self.buf[range.clone()];
            match check_slot(slot, tail, max) {
                Ok(len) => Ok(read(&slot[SLOT_HEADER_BYTES..SLOT_HEADER_BYTES + len])),
                Err(reason) => Err(reason),
            }
        };
        self.buf[range.start + SLOT_STATE_OFFSET] = SlotState::Free as u8;
        self.header
            .tail
            .store(tail.wrapping_add(1), Ordering::Release);
        match verdict {
            Ok(value) => {
                self.stats.popped += 1;
                Ok(value)
            }
            Err(reason) => {
                self.stats.corrupt += 1;
                Err(RingError::Corrupt {
                    index: tail,
                    reason,
                })
            }
        }
    }

    /// Copies the oldest message into `out` and returns its length. The copy has no alignment guarantee, so it is not
    /// the way to read an `rkyv` frame; use [`LocalSpscRing::try_pop_with`] for that.
    #[inline]
    pub fn try_pop(&mut self, out: &mut Vec<u8>) -> Result<usize, RingError> {
        self.try_pop_with(|bytes| {
            out.clear();
            out.extend_from_slice(bytes);
            bytes.len()
        })
    }
}

/// Checks one slot before its payload is exposed. Returns the payload length, or why the slot is not trusted.
fn check_slot(slot: &[u8], expected_seq: u64, max_len: usize) -> Result<usize, &'static str> {
    match SlotState::from_u8(slot[SLOT_STATE_OFFSET]) {
        Some(SlotState::Full) => {}
        Some(SlotState::Writing) => {
            return Err("the producer stopped mid-write (state is Writing)");
        }
        _ => return Err("the slot is not in the Full state"),
    }
    if read_u64(slot, SLOT_SEQ_OFFSET) != expected_seq {
        return Err("the sequence number does not match the read position");
    }
    let len = read_u32(slot, SLOT_LEN_OFFSET) as usize;
    if len > max_len {
        return Err("the length field exceeds the slot's payload capacity");
    }
    Ok(len)
}

#[cfg(test)]
impl LocalSpscRing {
    /// Test-only access to the raw bytes of the slot a position maps to, to damage it on purpose.
    fn slot_bytes_mut(&mut self, position: u64) -> &mut [u8] {
        let range = self.slot_range(position);
        &mut self.buf[range]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    const CAP: u32 = 4;
    const SLOT: u32 = 128; // 64-byte header + 64 bytes of payload

    fn ring() -> LocalSpscRing {
        LocalSpscRing::new(CAP, SLOT).unwrap()
    }

    /// A small deterministic generator, so the model test needs no extra dependency and fails the same way every run.
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
    fn roundtrip_spsc_slots() {
        let mut ring = ring();
        ring.try_push(b"sul-ok").unwrap();
        let mut out = Vec::new();
        let n = ring.try_pop(&mut out).unwrap();
        assert_eq!(n, 6);
        assert_eq!(&out, b"sul-ok");
        assert_eq!(ring.try_pop(&mut out), Err(RingError::Empty));
    }

    #[test]
    fn an_empty_payload_round_trips() {
        let mut ring = ring();
        ring.try_push(&[]).unwrap();
        let got = ring.try_pop_with(|bytes| bytes.len()).unwrap();
        assert_eq!(got, 0);
    }

    #[test]
    fn a_full_ring_says_would_block_and_keeps_its_messages() {
        let mut ring = ring();
        for i in 0..CAP as u8 {
            ring.try_push(&[i]).unwrap();
        }
        assert!(ring.is_full());
        assert_eq!(ring.try_push(b"x"), Err(RingError::WouldBlock));
        assert_eq!(ring.try_push(b"y"), Err(RingError::WouldBlock));
        assert_eq!(ring.stats().would_block, 2);
        // Nothing was lost or overwritten by the refused pushes.
        let mut out = Vec::new();
        for i in 0..CAP as u8 {
            ring.try_pop(&mut out).unwrap();
            assert_eq!(out, [i]);
        }
        // Space is back once the consumer has read.
        ring.try_push(b"z").unwrap();
    }

    #[test]
    fn an_empty_ring_says_empty() {
        let mut ring = ring();
        let mut out = Vec::new();
        assert_eq!(ring.try_pop(&mut out), Err(RingError::Empty));
        assert!(ring.is_empty());
        assert_eq!(ring.stats(), RingStats::default());
    }

    #[test]
    fn the_payload_limit_is_exact() {
        let mut ring = ring();
        let max = ring.payload_capacity();
        assert_eq!(max, 64);
        ring.try_push(&vec![7u8; max]).unwrap();
        assert_eq!(
            ring.try_push(&vec![7u8; max + 1]),
            Err(RingError::Oversized { len: max + 1, max })
        );
        // The refused oversized push did not use a slot.
        assert_eq!(ring.len(), 1);
    }

    #[test]
    fn messages_come_out_in_order_across_many_wraps() {
        let mut ring = ring();
        let mut out = Vec::new();
        let mut next_push = 0u32;
        let mut next_pop = 0u32;
        for _ in 0..(CAP * 10) {
            // Keep the ring between one and three messages deep so every slot is reused many times.
            for _ in 0..3 {
                if !ring.is_full() {
                    ring.try_push(&next_push.to_le_bytes()).unwrap();
                    next_push += 1;
                }
            }
            for _ in 0..2 {
                if ring.try_pop(&mut out).is_ok() {
                    assert_eq!(out, next_pop.to_le_bytes());
                    next_pop += 1;
                }
            }
        }
        while ring.try_pop(&mut out).is_ok() {
            assert_eq!(out, next_pop.to_le_bytes());
            next_pop += 1;
        }
        assert_eq!(next_pop, next_push);
        assert!(next_push > CAP * 5);
    }

    #[test]
    fn every_payload_starts_on_a_64_byte_boundary() {
        let mut ring = ring();
        // Go round the ring more than twice so every slot is checked, whatever the allocator did with the buffer.
        for round in 0..(CAP * 2 + 1) {
            ring.try_push(&round.to_le_bytes()).unwrap();
            let addr = ring.try_pop_with(|bytes| bytes.as_ptr() as usize).unwrap();
            assert_eq!(addr % SLOT_ALIGN, 0, "slot {round} is misaligned");
        }
    }

    #[test]
    fn behaves_like_a_bounded_fifo_under_random_use() {
        let mut ring = LocalSpscRing::new(8, 128).unwrap();
        let mut model: VecDeque<Vec<u8>> = VecDeque::new();
        let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
        let max = ring.payload_capacity();
        let (mut pushes, mut pops, mut blocked, mut oversized) = (0u32, 0u32, 0u32, 0u32);
        for _ in 0..50_000 {
            if rng.next() % 2 == 0 {
                let len = (rng.next() % (max as u64 + 6)) as usize; // some of these are too big
                let payload: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
                match ring.try_push(&payload) {
                    Ok(()) => {
                        assert!(len <= max && model.len() < 8);
                        model.push_back(payload);
                        pushes += 1;
                    }
                    Err(RingError::Oversized { .. }) => {
                        assert!(len > max);
                        oversized += 1;
                    }
                    Err(RingError::WouldBlock) => {
                        assert_eq!(model.len(), 8);
                        blocked += 1;
                    }
                    Err(other) => panic!("unexpected push error: {other}"),
                }
            } else {
                let mut out = Vec::new();
                match ring.try_pop(&mut out) {
                    Ok(_) => {
                        assert_eq!(Some(out), model.pop_front());
                        pops += 1;
                    }
                    Err(RingError::Empty) => assert!(model.is_empty()),
                    Err(other) => panic!("unexpected pop error: {other}"),
                }
            }
            assert_eq!(ring.len(), model.len());
        }
        // The run really did exercise every outcome.
        assert!(pushes > 1000 && pops > 1000 && blocked > 10 && oversized > 10);
    }

    #[test]
    fn bad_geometry_is_an_error_not_a_panic() {
        let cases: [(u32, u32); 9] = [
            (0, 128),                          // no slots
            (3, 128),                          // not a power of two
            (4, 0),                            // no slot bytes (used to panic on the first push)
            (4, 64),                           // header only, no payload
            (4, 100),                          // not a multiple of 64
            (4, 3), // smaller than the length prefix the old layout needed
            (4, (SLOT_MAX_BYTES + 64) as u32), // bigger than the largest message plus header
            (RING_MAX_SLOTS * 2, 128), // too many slots
            (1 << 14, SLOT_MAX_BYTES as u32), // each field is fine, the product is over the byte limit
        ];
        for (capacity, slot_bytes) in cases {
            assert!(
                matches!(
                    LocalSpscRing::new(capacity, slot_bytes),
                    Err(RingError::BadGeometry(_))
                ),
                "({capacity}, {slot_bytes}) should be refused"
            );
        }
    }

    #[test]
    fn the_largest_slot_is_accepted() {
        let ring = LocalSpscRing::new(2, SLOT_MAX_BYTES as u32).unwrap();
        assert_eq!(ring.payload_capacity(), 64 * 1024);
        let geometry = RingGeometry::new(1, SLOT_MIN_BYTES as u32).unwrap();
        assert_eq!(geometry.payload_capacity(), SLOT_ALIGN);
    }

    #[test]
    fn a_header_is_checked_before_it_is_trusted() {
        let good = WireRingHeader::new(8, 256);
        assert_eq!(
            RingGeometry::from_header(&good).unwrap(),
            RingGeometry::new(8, 256).unwrap()
        );

        let mut bad_magic = WireRingHeader::new(8, 256);
        bad_magic.magic ^= 1;
        assert!(matches!(
            RingGeometry::from_header(&bad_magic),
            Err(RingError::BadGeometry(m)) if m.contains("magic")
        ));

        let mut bad_version = WireRingHeader::new(8, 256);
        bad_version.version += 1;
        assert!(matches!(
            RingGeometry::from_header(&bad_version),
            Err(RingError::BadGeometry(m)) if m.contains("version")
        ));

        let bad_capacity = WireRingHeader::new(6, 256);
        assert!(RingGeometry::from_header(&bad_capacity).is_err());
    }

    /// Damage the slot the consumer reads next, then check that it is refused, counted, and released.
    fn assert_refused(damage: impl FnOnce(&mut [u8]), expected_reason: &str) {
        let mut ring = ring();
        ring.try_push(b"first").unwrap();
        ring.try_push(b"second").unwrap();
        damage(ring.slot_bytes_mut(0));
        let mut out = Vec::new();
        match ring.try_pop(&mut out) {
            Err(RingError::Corrupt { index, reason }) => {
                assert_eq!(index, 0);
                assert!(reason.contains(expected_reason), "got: {reason}");
            }
            other => panic!("expected Corrupt, got {other:?}"),
        }
        assert_eq!(ring.stats().corrupt, 1);
        // The bad slot was released and the next message is intact.
        ring.try_pop(&mut out).unwrap();
        assert_eq!(out, b"second");
    }

    #[test]
    fn a_slot_left_in_the_writing_state_is_refused() {
        assert_refused(
            |slot| slot[SLOT_STATE_OFFSET] = SlotState::Writing as u8,
            "mid-write",
        );
    }

    #[test]
    fn a_slot_that_is_not_full_is_refused() {
        assert_refused(
            |slot| slot[SLOT_STATE_OFFSET] = SlotState::Free as u8,
            "not in the Full state",
        );
        assert_refused(
            |slot| slot[SLOT_STATE_OFFSET] = 0xEE,
            "not in the Full state",
        );
    }

    #[test]
    fn a_wrong_sequence_number_is_refused() {
        assert_refused(
            |slot| slot[SLOT_SEQ_OFFSET..SLOT_SEQ_OFFSET + 8].copy_from_slice(&99u64.to_le_bytes()),
            "sequence",
        );
    }

    #[test]
    fn an_impossible_length_is_refused_without_panicking() {
        assert_refused(
            |slot| {
                slot[SLOT_LEN_OFFSET..SLOT_LEN_OFFSET + 4].copy_from_slice(&u32::MAX.to_le_bytes())
            },
            "length",
        );
    }

    #[test]
    fn a_ring_of_corrupt_slots_does_not_wedge() {
        let mut ring = ring();
        for i in 0..CAP as u8 {
            ring.try_push(&[i]).unwrap();
        }
        for position in 0..CAP as u64 {
            ring.slot_bytes_mut(position)[SLOT_STATE_OFFSET] = SlotState::Free as u8;
        }
        let mut out = Vec::new();
        for _ in 0..CAP {
            assert!(matches!(
                ring.try_pop(&mut out),
                Err(RingError::Corrupt { .. })
            ));
        }
        assert!(ring.is_empty());
        for i in 0..CAP as u8 {
            ring.try_push(&[i]).unwrap();
        }
        assert_eq!(ring.len(), CAP as usize);
        assert_eq!(ring.stats().corrupt, CAP as u64);
    }

    #[test]
    fn stats_count_pushes_and_pops() {
        let mut ring = ring();
        ring.try_push(b"a").unwrap();
        ring.try_push(b"b").unwrap();
        let mut out = Vec::new();
        ring.try_pop(&mut out).unwrap();
        assert_eq!(
            ring.stats(),
            RingStats {
                pushed: 2,
                popped: 1,
                would_block: 0,
                corrupt: 0
            }
        );
    }
}
