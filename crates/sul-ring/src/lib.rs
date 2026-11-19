//! Wait-free SPSC ring buffer over 64-byte cache-line-padded `WireRingHeader`.
//! Pure-Rust core so Miri/Kani can verify ring invariants without OS syscalls (D-141, D-147).

use core::sync::atomic::Ordering;
use sul_ipc_types::WireRingHeader;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RingError {
    #[error("ring buffer is full (needed {needed} bytes, available {available})")]
    Full { needed: usize, available: usize },
    #[error("ring buffer is empty")]
    Empty,
    #[error("payload exceeds slot capacity ({len} > {max})")]
    Oversized { len: usize, max: usize },
}

/// Fixed-capacity in-memory SPSC byte ring for unit testing and intra-process channels.
pub struct LocalSpscRing {
    header: WireRingHeader,
    buf: Vec<u8>,
}

impl LocalSpscRing {
    pub fn new(capacity_pow2: u32, slot_bytes: u32) -> Self {
        assert!(
            capacity_pow2.is_power_of_two(),
            "capacity must be power of two"
        );
        Self {
            header: WireRingHeader {
                magic: WireRingHeader::MAGIC,
                version: 1,
                capacity_pow2,
                slot_bytes,
                head: crossbeam_utils::CachePadded::new(core::sync::atomic::AtomicU64::new(0)),
                tail: crossbeam_utils::CachePadded::new(core::sync::atomic::AtomicU64::new(0)),
            },
            buf: vec![0u8; (capacity_pow2 as usize) * (slot_bytes as usize)],
        }
    }

    #[inline]
    pub fn try_push(&mut self, payload: &[u8]) -> Result<(), RingError> {
        let max = self.header.slot_bytes as usize;
        if payload.len() > max.saturating_sub(4) {
            return Err(RingError::Oversized {
                len: payload.len(),
                max: max.saturating_sub(4),
            });
        }
        let avail = self.header.available_write() as usize;
        if avail == 0 {
            return Err(RingError::Full {
                needed: 1,
                available: 0,
            });
        }
        let head = self.header.head.load(Ordering::Relaxed);
        let idx = (head as usize) & ((self.header.capacity_pow2 as usize) - 1);
        let offset = idx * max;
        let len_bytes = (payload.len() as u32).to_le_bytes();
        self.buf[offset..offset + 4].copy_from_slice(&len_bytes);
        self.buf[offset + 4..offset + 4 + payload.len()].copy_from_slice(payload);
        self.header
            .head
            .store(head.wrapping_add(1), Ordering::Release);
        Ok(())
    }

    #[inline]
    pub fn try_pop(&mut self, out: &mut Vec<u8>) -> Result<usize, RingError> {
        if self.header.available_read() == 0 {
            return Err(RingError::Empty);
        }
        let tail = self.header.tail.load(Ordering::Relaxed);
        let max = self.header.slot_bytes as usize;
        let idx = (tail as usize) & ((self.header.capacity_pow2 as usize) - 1);
        let offset = idx * max;
        let len = u32::from_le_bytes(self.buf[offset..offset + 4].try_into().unwrap()) as usize;
        out.clear();
        out.extend_from_slice(&self.buf[offset + 4..offset + 4 + len]);
        self.header
            .tail
            .store(tail.wrapping_add(1), Ordering::Release);
        Ok(len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_spsc_slots() {
        let mut ring = LocalSpscRing::new(4, 64);
        ring.try_push(b"sul-ok").unwrap();
        let mut out = Vec::new();
        let n = ring.try_pop(&mut out).unwrap();
        assert_eq!(n, 6);
        assert_eq!(&out, b"sul-ok");
        assert_eq!(ring.try_pop(&mut out), Err(RingError::Empty));
    }
}
