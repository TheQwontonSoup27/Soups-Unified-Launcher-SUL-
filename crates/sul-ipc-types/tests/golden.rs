//! Golden bytes for the IPC schema (Ch. 7 §2.3, "rkyv schema").
//!
//! The rule: adding a field to a message is backward compatible; removing a field or changing a type needs a golden-bytes
//! update and a migration step. This test is the "golden-bytes update" half. It serializes one sample of every message
//! variant (plus the enums whose discriminants travel inside messages, the Tier 1 identifiers stored in
//! `tier1-approvals.json`, and the ring layout constants) and compares the result with `tests/golden/ipc-v1.txt`.
//!
//! A failure here means the wire format changed. If that was intended, regenerate the file and review the diff:
//!
//! ```text
//! SUL_UPDATE_GOLDEN=1 cargo test -p sul-ipc-types --test golden
//! ```
//!
//! `cargo xtask ipc-check` runs this test. `rkyv`'s 32-bit relative pointers make the bytes the same on every platform.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;
use sul_ipc_types::{
    FailureMode, IpcMessage, MAX_MESSAGE_BYTES, RING_MAX_BYTES, RING_MAX_SLOTS, ReleaseClass,
    SLOT_ALIGN, SLOT_HEADER_BYTES, SLOT_LEN_OFFSET, SLOT_MAX_BYTES, SLOT_MIN_BYTES,
    SLOT_SEQ_OFFSET, SLOT_STATE_OFFSET, SLOT_TIMESTAMP_OFFSET, TelemetryMsg, Tier1Event,
    WireRingHeader,
};

const GOLDEN_FILE: &str = "tests/golden/ipc-v1.txt";

/// Every variant of [`IpcMessage`], by name. The `match` below has no wildcard arm, so adding a variant stops this file
/// compiling until the new variant is named here and given a sample in [`samples`].
const VARIANTS: [&str; 6] = [
    "Ping",
    "Pong",
    "TokenChunk",
    "Telemetry",
    "Tier1Event",
    "Error",
];

fn variant_name(message: &IpcMessage) -> &'static str {
    match message {
        IpcMessage::Ping { .. } => "Ping",
        IpcMessage::Pong { .. } => "Pong",
        IpcMessage::TokenChunk { .. } => "TokenChunk",
        IpcMessage::Telemetry(_) => "Telemetry",
        IpcMessage::Tier1Event { .. } => "Tier1Event",
        IpcMessage::Error { .. } => "Error",
    }
}

fn samples() -> Vec<(&'static str, IpcMessage)> {
    vec![
        (
            "Ping",
            IpcMessage::Ping {
                seq: 1,
                timestamp_ns: 2,
            },
        ),
        (
            "Pong",
            IpcMessage::Pong {
                seq: u64::MAX,
                timestamp_ns: 0,
            },
        ),
        (
            "TokenChunk",
            IpcMessage::TokenChunk {
                request_id: 9,
                seq: 3,
                is_final: true,
                utf8_bytes: "héllo".as_bytes().to_vec(),
            },
        ),
        (
            "TokenChunk.empty",
            IpcMessage::TokenChunk {
                request_id: 0,
                seq: 0,
                is_final: false,
                utf8_bytes: Vec::new(),
            },
        ),
        (
            "Telemetry",
            IpcMessage::Telemetry(TelemetryMsg {
                subsystem: "ai".into(),
                metric: "ttft".into(),
                value_us: 120,
                timestamp_ns: 5,
            }),
        ),
        (
            "Tier1Event",
            IpcMessage::Tier1Event {
                event_id: Tier1Event::AuthDisable.id().into(),
                approved: false,
            },
        ),
        (
            "Error",
            IpcMessage::Error {
                code: "E_WIRE_FRAME_CORRUPT".into(),
                mode: FailureMode::ModeC,
                detail: "dropped".into(),
            },
        ),
    ]
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(out, "{byte:02x}").unwrap();
    }
    out
}

fn bytes_of<T>(value: &T) -> Vec<u8>
where
    T: for<'a> rkyv::Serialize<
            rkyv::api::high::HighSerializer<
                rkyv::util::AlignedVec,
                rkyv::ser::allocator::ArenaHandle<'a>,
                rkyv::rancor::Error,
            >,
        >,
{
    rkyv::to_bytes::<rkyv::rancor::Error>(value)
        .expect("sample serializes")
        .to_vec()
}

/// The text the golden file must hold.
fn render() -> String {
    let mut out = String::new();
    out.push_str("# SUL IPC golden bytes, schema v1. Regenerate with:\n");
    out.push_str("#   SUL_UPDATE_GOLDEN=1 cargo test -p sul-ipc-types --test golden\n");
    out.push_str("# Lines are `name length hex`. Review every diff: a change here is a wire-format change.\n");

    out.push_str("\n[ring]\n");
    for (name, value) in [
        ("MAX_MESSAGE_BYTES", MAX_MESSAGE_BYTES),
        ("SLOT_ALIGN", SLOT_ALIGN),
        ("SLOT_HEADER_BYTES", SLOT_HEADER_BYTES),
        ("SLOT_STATE_OFFSET", SLOT_STATE_OFFSET),
        ("SLOT_SEQ_OFFSET", SLOT_SEQ_OFFSET),
        ("SLOT_TIMESTAMP_OFFSET", SLOT_TIMESTAMP_OFFSET),
        ("SLOT_LEN_OFFSET", SLOT_LEN_OFFSET),
        ("SLOT_MIN_BYTES", SLOT_MIN_BYTES),
        ("SLOT_MAX_BYTES", SLOT_MAX_BYTES),
        ("RING_MAX_BYTES", RING_MAX_BYTES),
    ] {
        writeln!(out, "{name} = {value}").unwrap();
    }
    writeln!(out, "RING_MAX_SLOTS = {RING_MAX_SLOTS}").unwrap();
    writeln!(out, "MAGIC = {:#010x}", WireRingHeader::MAGIC).unwrap();
    writeln!(out, "VERSION = {}", WireRingHeader::VERSION).unwrap();

    out.push_str("\n[messages]\n");
    for (name, message) in samples() {
        let bytes = bytes_of(&message);
        writeln!(out, "{name} {} {}", bytes.len(), hex(&bytes)).unwrap();
    }

    // Enum discriminants are part of the wire format: `Error` carries a `FailureMode`.
    out.push_str("\n[enums]\n");
    for (name, mode) in [
        ("FailureMode::ModeA", FailureMode::ModeA),
        ("FailureMode::ModeB", FailureMode::ModeB),
        ("FailureMode::ModeC", FailureMode::ModeC),
        ("FailureMode::ModeD", FailureMode::ModeD),
        ("FailureMode::ModeE", FailureMode::ModeE),
        ("FailureMode::ModeF", FailureMode::ModeF),
    ] {
        writeln!(out, "{name} {}", hex(&bytes_of(&mode))).unwrap();
    }
    for (name, class) in [
        ("ReleaseClass::R0", ReleaseClass::R0),
        ("ReleaseClass::R1", ReleaseClass::R1),
        ("ReleaseClass::R2", ReleaseClass::R2),
        ("ReleaseClass::R3", ReleaseClass::R3),
        ("ReleaseClass::R4", ReleaseClass::R4),
    ] {
        writeln!(out, "{name} {}", hex(&bytes_of(&class))).unwrap();
    }

    // The identifiers are stored in tier1-approvals.json, so their text and their order are frozen.
    out.push_str("\n[tier1-core-events]\n");
    for event in Tier1Event::ALL {
        writeln!(out, "{}", event.id()).unwrap();
    }
    out
}

/// Line endings do not matter: git on Windows may check the file out with CRLF.
fn normalise(text: &str) -> String {
    text.replace("\r\n", "\n")
}

#[test]
fn ipc_bytes_match_the_golden_file() {
    let actual = render();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_FILE);
    if std::env::var_os("SUL_UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "cannot read {}: {error}. Generate it with `SUL_UPDATE_GOLDEN=1 cargo test -p sul-ipc-types --test golden`",
            path.display()
        )
    });
    assert_eq!(
        normalise(&expected),
        actual,
        "the IPC wire format changed. If that is intended, regenerate {GOLDEN_FILE} with `SUL_UPDATE_GOLDEN=1 cargo test -p sul-ipc-types --test golden`, review the diff, and add a migration step (Ch. 7 §2.3)"
    );
}

#[test]
fn every_message_variant_has_a_golden_sample() {
    let sampled: BTreeSet<&str> = samples()
        .iter()
        .map(|(_, message)| variant_name(message))
        .collect();
    let expected: BTreeSet<&str> = VARIANTS.into_iter().collect();
    assert_eq!(sampled, expected);
}

#[test]
fn the_golden_samples_decode_back_to_themselves() {
    // The golden file only means something if the bytes it holds are real messages.
    for (name, message) in samples() {
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&message).unwrap();
        let archived =
            rkyv::access::<rkyv::Archived<IpcMessage>, rkyv::rancor::Error>(&bytes).unwrap();
        let back: IpcMessage =
            rkyv::deserialize::<IpcMessage, rkyv::rancor::Error>(archived).unwrap();
        assert_eq!(back, message, "{name}");
    }
}
