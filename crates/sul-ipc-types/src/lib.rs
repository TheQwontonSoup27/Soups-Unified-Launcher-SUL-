//! Single source of truth for SUL zero-copy IPC (`rkyv`), `sul-wire` ring layout,
//! release classes, Tier-1 safety events, and failure taxonomy (Ch. 1 §2.6, Ch. 7, Ch. 12).

use core::sync::atomic::{AtomicU64, Ordering};
use crossbeam_utils::CachePadded;
use rkyv::{Archive, Deserialize, Serialize};

/// Product release milestones (one-product rule: strict supersets).
#[derive(Archive, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[rkyv(compare(PartialEq), derive(Debug))]
pub enum ReleaseClass {
    R0, // v0.1 "the launcher"
    R1, // v0.2-0.3 "everything mainstream"
    R2, // v0.4-0.5 "the deep technical release"
    R3, // v0.6+ "sensing and security" (requires A.152)
    R4, // research only
}

/// Deterministic failure recovery modes (SUL-LEDGER/failures.md).
#[derive(Archive, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[rkyv(compare(PartialEq), derive(Debug))]
pub enum FailureMode {
    ModeA, // Retry with backoff
    ModeB, // Degrade / switch fallback backend
    ModeC, // Disable optional feature & continue
    ModeD, // Quarantine & safe-mode recovery
    ModeE, // Hard refusal / security halt
    ModeF, // Escalate to diagnostic / Coder review
}

/// Cache-line-padded wait-free SPSC ring header for `sul-wire` token streaming & telemetry.
/// Eliminates false sharing between producer (`sul-ai-native`) and consumer (`sul.exe` / `sul-overlay`).
#[repr(C, align(64))]
pub struct WireRingHeader {
    pub magic: u32,
    pub version: u16,
    pub capacity_pow2: u32,
    pub slot_bytes: u32,
    pub head: CachePadded<AtomicU64>,
    pub tail: CachePadded<AtomicU64>,
}

impl WireRingHeader {
    pub const MAGIC: u32 = 0x5355_4C57; // "SULW"

    #[inline]
    pub fn available_read(&self) -> u64 {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        head.saturating_sub(tail)
    }

    #[inline]
    pub fn available_write(&self) -> u64 {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        (self.capacity_pow2 as u64).saturating_sub(head.saturating_sub(tail))
    }
}

/// Core IPC message envelope routed across `sul-ipc` and `sul-wire`.
#[derive(Archive, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[rkyv(compare(PartialEq), derive(Debug))]
pub enum IpcMessage {
    Ping {
        seq: u64,
        timestamp_ns: u64,
    },
    Pong {
        seq: u64,
        timestamp_ns: u64,
    },
    TokenChunk {
        request_id: u64,
        seq: u32,
        is_final: bool,
        utf8_bytes: Vec<u8>,
    },
    Telemetry(TelemetryMsg),
    Tier1Event {
        event_id: String,
        approved: bool,
    },
    Error {
        code: String,
        mode: FailureMode,
        detail: String,
    },
}

/// Telemetry event variant consumed by `sul-metrics` (Ch. 12 §2.2).
#[derive(Archive, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[rkyv(compare(PartialEq), derive(Debug))]
pub struct TelemetryMsg {
    pub subsystem: String,
    pub metric: String,
    pub value_us: u64,
    pub timestamp_ns: u64,
}
