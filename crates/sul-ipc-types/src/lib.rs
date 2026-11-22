//! Single source of truth for SUL zero-copy IPC (`rkyv`), `sul-wire` ring layout,
//! release classes, Tier-1 safety events, and failure taxonomy (Ch. 1 §2.6, Ch. 7 §2.3, Ch. 12).
//!
//! Stability rules (Ch. 7 §2.3): adding a field to a message type is backward compatible; removing a field or changing a
//! type needs a golden-bytes update (`tests/golden/ipc-v1.txt`, checked by `cargo xtask ipc-check`) and a migration step.

#![forbid(unsafe_code)]

use core::sync::atomic::{AtomicU64, Ordering};
use crossbeam_utils::CachePadded;
use rkyv::{Archive, Deserialize, Serialize};

// ---------------------------------------------------------------------------------------------------------------------
// Ring layout constants (Ch. 7 §2.3). `sul-ring` builds on these; the shared-memory ring (R0 layer 3) reuses them.
// ---------------------------------------------------------------------------------------------------------------------

/// Largest message the ring carries, in bytes (Ch. 7 §2.3, "Message size limits"). Larger payloads travel as address
/// pointers (Ch. 11 §11.6), not through the ring.
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;

/// Every slot starts on a 64-byte boundary, so slot headers never share a cache line with a neighbour.
pub const SLOT_ALIGN: usize = 64;

/// Size of the slot header: state, sequence number, timestamp and length (Ch. 7 §2.3, "Slot layout").
///
/// The spec's `Slot` struct lists fields that add up to 64 bytes (`_pad: [u8; 36]`) but sizes the payload as
/// `SLOT_SIZE - 128`. Those two cannot both hold; the field list is followed here and the payload is `slot - 64`.
/// Open question for the owner (docs/SESSION_STATE.md).
pub const SLOT_HEADER_BYTES: usize = 64;

/// Byte offsets of the slot header fields. Integers are little-endian.
pub const SLOT_STATE_OFFSET: usize = 0;
pub const SLOT_SEQ_OFFSET: usize = 8;
pub const SLOT_TIMESTAMP_OFFSET: usize = 16;
pub const SLOT_LEN_OFFSET: usize = 24;

/// Largest slot: the largest message plus its header.
pub const SLOT_MAX_BYTES: usize = MAX_MESSAGE_BYTES + SLOT_HEADER_BYTES;

/// Smallest slot: a header plus one aligned block of payload.
pub const SLOT_MIN_BYTES: usize = SLOT_HEADER_BYTES + SLOT_ALIGN;

/// Most slots in one ring, and most bytes one in-process ring may allocate. These stop a bad configuration value from
/// asking for an enormous allocation; they are limits of this implementation, not of the spec.
pub const RING_MAX_SLOTS: u32 = 1 << 20;
pub const RING_MAX_BYTES: usize = 256 * 1024 * 1024;

// `%` and not `usize::is_multiple_of`: the workspace MSRV is 1.85 and that method is newer.
const _: () = assert!(SLOT_MAX_BYTES % SLOT_ALIGN == 0);
const _: () = assert!(SLOT_MIN_BYTES % SLOT_ALIGN == 0);
const _: () = assert!(SLOT_HEADER_BYTES % SLOT_ALIGN == 0);
const _: () = assert!(SLOT_LEN_OFFSET + 4 <= SLOT_HEADER_BYTES);

/// State byte of a slot (Ch. 7 §2.3). A producer that dies in `Writing` leaves a slot the consumer must not trust.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SlotState {
    Empty = 0,
    Writing = 1,
    Full = 2,
    Reading = 3,
    Free = 4,
}

impl SlotState {
    #[inline]
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Empty),
            1 => Some(Self::Writing),
            2 => Some(Self::Full),
            3 => Some(Self::Reading),
            4 => Some(Self::Free),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Release classes, failure modes, Tier 1 events
// ---------------------------------------------------------------------------------------------------------------------

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

/// Deterministic failure recovery modes, A to F (SUL-LEDGER/failures.md, Ch. 1 §6.1).
///
/// The meaning of each letter is defined once, in `SUL-LEDGER/failures.md` ("Mode definitions") and Ch. 1 §6.1, and is
/// deliberately not repeated here. The one-line meanings that used to sit beside each variant ("retry with backoff",
/// "degrade", "hard refusal"...) disagreed with that definition (A is an inference timeout, B invalid structured output,
/// C IPC channel death, D model-file corruption, E a Rust panic, F a cascading failure), and the spec's per-error
/// tables use the letters in yet another sense. Which reading wins is an open question for the owner
/// (docs/SESSION_STATE.md); until then these are bare letters.
#[derive(Archive, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[rkyv(compare(PartialEq), derive(Debug))]
pub enum FailureMode {
    ModeA,
    ModeB,
    ModeC,
    ModeD,
    ModeE,
    ModeF,
}

/// The 21 Tier 1-core events: safety-relevant changes that always need an OS-level approval modal, whatever the release
/// (Ch. 1 §6.8, `SUL-LEDGER/releases.md`). The string forms are stored in `tier1-approvals.json`, so they must never
/// change; `cargo xtask release-check` compares [`Tier1Event::ALL`] with the list in `releases.md`.
///
/// Events that are Tier 1 only by release class (R1 to R3) are not listed here: [`IpcMessage::Tier1Event`] carries the
/// event as a string so it can carry those too.
#[derive(Archive, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[rkyv(compare(PartialEq), derive(Debug))]
pub enum Tier1Event {
    VaultDisableAuthFactor,
    VaultDisableAntiDebug,
    VaultDisableDecoy,
    VaultChangeKdfProfile,
    VaultChangeAeadCipher,
    VaultEnableSearchableIndex,
    VaultAddExtraPathViaConfig,
    AuthDisable,
    AuthDisableFactor,
    AuthChangeReauthForVault,
    AuthDisableAntiDebug,
    AuthDisableProcessHardening,
    AuthDisableMemoryLock,
    InvariantDisableDestructiveActionAlwaysThinking,
    InvariantEnableUncensoredModel,
    WebviewDisableIsolationPattern,
    WebviewDisableStrictEsm,
    WebviewLowerRuntimeFloor,
    WebviewLowerTauriVersionFloor,
    WebviewGrantShellAllowExecute,
    RefusalVoiceCloneAttempt,
}

impl Tier1Event {
    /// Every Tier 1-core event, in the order `releases.md` lists them.
    pub const ALL: [Tier1Event; 21] = [
        Self::VaultDisableAuthFactor,
        Self::VaultDisableAntiDebug,
        Self::VaultDisableDecoy,
        Self::VaultChangeKdfProfile,
        Self::VaultChangeAeadCipher,
        Self::VaultEnableSearchableIndex,
        Self::VaultAddExtraPathViaConfig,
        Self::AuthDisable,
        Self::AuthDisableFactor,
        Self::AuthChangeReauthForVault,
        Self::AuthDisableAntiDebug,
        Self::AuthDisableProcessHardening,
        Self::AuthDisableMemoryLock,
        Self::InvariantDisableDestructiveActionAlwaysThinking,
        Self::InvariantEnableUncensoredModel,
        Self::WebviewDisableIsolationPattern,
        Self::WebviewDisableStrictEsm,
        Self::WebviewLowerRuntimeFloor,
        Self::WebviewLowerTauriVersionFloor,
        Self::WebviewGrantShellAllowExecute,
        Self::RefusalVoiceCloneAttempt,
    ];

    /// The event's stable identifier, as written in `releases.md` and in the approvals file.
    pub const fn id(self) -> &'static str {
        match self {
            Self::VaultDisableAuthFactor => "vault:disable_auth_factor",
            Self::VaultDisableAntiDebug => "vault:disable_anti_debug",
            Self::VaultDisableDecoy => "vault:disable_decoy",
            Self::VaultChangeKdfProfile => "vault:change_kdf_profile",
            Self::VaultChangeAeadCipher => "vault:change_aead_cipher",
            Self::VaultEnableSearchableIndex => "vault:enable_searchable_index",
            Self::VaultAddExtraPathViaConfig => "vault:add_extra_path_via_config",
            Self::AuthDisable => "auth:disable",
            Self::AuthDisableFactor => "auth:disable_factor",
            Self::AuthChangeReauthForVault => "auth:change_reauth_for_vault",
            Self::AuthDisableAntiDebug => "auth:disable_anti_debug",
            Self::AuthDisableProcessHardening => "auth:disable_process_hardening",
            Self::AuthDisableMemoryLock => "auth:disable_memory_lock",
            Self::InvariantDisableDestructiveActionAlwaysThinking => {
                "invariant:disable_destructive_action_always_thinking"
            }
            Self::InvariantEnableUncensoredModel => "invariant:enable_uncensored_model",
            Self::WebviewDisableIsolationPattern => "webview:disable_isolation_pattern",
            Self::WebviewDisableStrictEsm => "webview:disable_strict_esm",
            Self::WebviewLowerRuntimeFloor => "webview:lower_runtime_floor",
            Self::WebviewLowerTauriVersionFloor => "webview:lower_tauri_version_floor",
            Self::WebviewGrantShellAllowExecute => "webview:grant_shell_allow_execute",
            Self::RefusalVoiceCloneAttempt => "refusal:voice_clone_attempt",
        }
    }

    /// Looks an identifier up. Unknown identifiers (including release-class Tier 1 events) give `None`.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|event| event.id() == id)
    }
}

impl core::fmt::Display for Tier1Event {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.id())
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Ring header
// ---------------------------------------------------------------------------------------------------------------------

/// Wait-free SPSC ring header for `sul-wire` token streaming and telemetry: the producer's `head` and the consumer's
/// `tail` each sit in their own [`CachePadded`] block (128 bytes on x86-64, crossbeam's choice), so the two sides never
/// write to one cache line. This removes false sharing between the producer (`sul-ai-native`) and the consumer
/// (`sul.exe` / `sul-overlay`).
///
/// `capacity_pow2` is the slot count itself (a power of two such as 8), not its base-2 exponent.
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
    pub const VERSION: u16 = 1;

    /// An empty ring header. Geometry is not checked here; `sul_ring::RingGeometry` does that.
    pub fn new(capacity_pow2: u32, slot_bytes: u32) -> Self {
        Self {
            magic: Self::MAGIC,
            version: Self::VERSION,
            capacity_pow2,
            slot_bytes,
            head: CachePadded::new(AtomicU64::new(0)),
            tail: CachePadded::new(AtomicU64::new(0)),
        }
    }

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

// ---------------------------------------------------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------------------------------------------------

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
    /// A Tier 1 approval result. `event_id` is a string so that release-class Tier 1 events can travel too; for the 21
    /// Tier 1-core events it is [`Tier1Event::id`].
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

// The spec's compile-time size check (Ch. 7 §2.3). It bounds the fixed part of a message only (strings and byte vectors
// live behind pointers); the encoded length is checked against `MAX_MESSAGE_BYTES` when a message is encoded.
const _: () = assert!(core::mem::size_of::<IpcMessage>() <= MAX_MESSAGE_BYTES);
const _: () = assert!(core::mem::size_of::<TelemetryMsg>() <= MAX_MESSAGE_BYTES);

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{align_of, offset_of, size_of};
    use std::collections::HashSet;

    /// The 21 ids as `releases.md` writes them. A second copy on purpose: renaming one by accident must fail a test.
    const EXPECTED_TIER1_IDS: [&str; 21] = [
        "vault:disable_auth_factor",
        "vault:disable_anti_debug",
        "vault:disable_decoy",
        "vault:change_kdf_profile",
        "vault:change_aead_cipher",
        "vault:enable_searchable_index",
        "vault:add_extra_path_via_config",
        "auth:disable",
        "auth:disable_factor",
        "auth:change_reauth_for_vault",
        "auth:disable_anti_debug",
        "auth:disable_process_hardening",
        "auth:disable_memory_lock",
        "invariant:disable_destructive_action_always_thinking",
        "invariant:enable_uncensored_model",
        "webview:disable_isolation_pattern",
        "webview:disable_strict_esm",
        "webview:lower_runtime_floor",
        "webview:lower_tauri_version_floor",
        "webview:grant_shell_allow_execute",
        "refusal:voice_clone_attempt",
    ];

    #[test]
    fn tier1_core_events_match_the_spec_list_in_order() {
        let ids: Vec<&str> = Tier1Event::ALL.iter().map(|e| e.id()).collect();
        assert_eq!(ids, EXPECTED_TIER1_IDS);
    }

    #[test]
    fn tier1_ids_are_unique_and_round_trip() {
        let unique: HashSet<&str> = Tier1Event::ALL.iter().map(|e| e.id()).collect();
        assert_eq!(unique.len(), Tier1Event::ALL.len());
        for event in Tier1Event::ALL {
            assert_eq!(Tier1Event::from_id(event.id()), Some(event));
            assert_eq!(event.to_string(), event.id());
        }
    }

    #[test]
    fn unknown_tier1_ids_are_not_core_events() {
        assert_eq!(Tier1Event::from_id(""), None);
        assert_eq!(Tier1Event::from_id("stt:change_model"), None); // Tier 1 by release class, not core
        assert_eq!(Tier1Event::from_id("AUTH:DISABLE"), None); // ids are case-sensitive
        assert_eq!(Tier1Event::from_id("auth:disable "), None);
    }

    #[test]
    fn the_structural_refusal_is_a_tier1_core_event() {
        assert_eq!(
            Tier1Event::from_id("refusal:voice_clone_attempt"),
            Some(Tier1Event::RefusalVoiceCloneAttempt)
        );
    }

    #[test]
    fn slot_state_round_trips_and_rejects_unknown_bytes() {
        for state in [
            SlotState::Empty,
            SlotState::Writing,
            SlotState::Full,
            SlotState::Reading,
            SlotState::Free,
        ] {
            assert_eq!(SlotState::from_u8(state as u8), Some(state));
        }
        assert_eq!(SlotState::from_u8(5), None);
        assert_eq!(SlotState::from_u8(255), None);
    }

    #[test]
    fn slot_header_fields_fit_and_do_not_overlap() {
        // These are constants, so the compiler checks them; a layout change that overlaps two fields fails the build.
        const {
            assert!(SLOT_STATE_OFFSET < SLOT_SEQ_OFFSET);
            assert!(SLOT_SEQ_OFFSET + 8 <= SLOT_TIMESTAMP_OFFSET);
            assert!(SLOT_TIMESTAMP_OFFSET + 8 <= SLOT_LEN_OFFSET);
            assert!(SLOT_LEN_OFFSET + 4 <= SLOT_HEADER_BYTES);
        }
        assert_eq!(SLOT_MAX_BYTES, 64 * 1024 + SLOT_HEADER_BYTES);
    }

    #[test]
    fn ring_header_keeps_head_and_tail_on_separate_cache_lines() {
        assert_eq!(align_of::<WireRingHeader>() % 64, 0);
        assert_eq!(size_of::<WireRingHeader>() % 64, 0);
        let head = offset_of!(WireRingHeader, head);
        let tail = offset_of!(WireRingHeader, tail);
        assert!(
            head.abs_diff(tail) >= 64,
            "head and tail share a cache line"
        );
        assert_eq!(head % 64, 0);
        assert_eq!(tail % 64, 0);
        // The fixed fields must not share a line with the producer's counter either.
        assert!(head >= 64, "head shares a cache line with the fixed fields");
    }

    #[test]
    fn ring_header_counts_available_space() {
        let header = WireRingHeader::new(4, 128);
        assert_eq!(header.magic, WireRingHeader::MAGIC);
        assert_eq!(header.version, WireRingHeader::VERSION);
        assert_eq!(header.available_read(), 0);
        assert_eq!(header.available_write(), 4);
        header.head.store(3, Ordering::Release);
        assert_eq!(header.available_read(), 3);
        assert_eq!(header.available_write(), 1);
        header.tail.store(3, Ordering::Release);
        assert_eq!(header.available_read(), 0);
        assert_eq!(header.available_write(), 4);
    }

    #[test]
    fn every_message_variant_round_trips_through_rkyv() {
        let messages = vec![
            IpcMessage::Ping {
                seq: 1,
                timestamp_ns: 2,
            },
            IpcMessage::Pong {
                seq: u64::MAX,
                timestamp_ns: 0,
            },
            IpcMessage::TokenChunk {
                request_id: 9,
                seq: 3,
                is_final: true,
                utf8_bytes: "héllo".as_bytes().to_vec(),
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
                code: "E_WIRE_FRAME_CORRUPT".into(),
                mode: FailureMode::ModeC,
                detail: String::new(),
            },
        ];
        for message in messages {
            let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&message).unwrap();
            let archived =
                rkyv::access::<rkyv::Archived<IpcMessage>, rkyv::rancor::Error>(&bytes).unwrap();
            let back: IpcMessage =
                rkyv::deserialize::<IpcMessage, rkyv::rancor::Error>(archived).unwrap();
            assert_eq!(back, message);
        }
    }
}
