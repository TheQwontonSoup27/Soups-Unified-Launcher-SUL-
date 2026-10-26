use serde_json::{Value, json};
use std::collections::VecDeque;
use sul_core::Result;
use sul_core::capability::Status;
use sul_core::discord::*;

#[derive(Default)]
struct Mock {
    sent: Vec<(Opcode, Value)>,
    replies: VecDeque<(Opcode, Value)>,
}

impl Transport for Mock {
    fn write_frame(&mut self, op: Opcode, payload: &Value) -> Result<()> {
        self.sent.push((op, payload.clone()));
        Ok(())
    }
    fn read_frame(&mut self) -> Result<(Opcode, Value)> {
        self.replies
            .pop_front()
            .ok_or_else(|| sul_core::Error::Invalid("mock: no reply queued".into()))
    }
}

fn ready() -> (Opcode, Value) {
    (
        Opcode::Frame,
        json!({ "cmd": "DISPATCH", "evt": "READY", "data": {} }),
    )
}
fn ack() -> (Opcode, Value) {
    (
        Opcode::Frame,
        json!({ "cmd": "SET_ACTIVITY", "evt": null, "data": {} }),
    )
}
fn connected(extra: Vec<(Opcode, Value)>) -> RichPresence<Mock> {
    let mut m = Mock::default();
    m.replies.push_back(ready());
    m.replies.extend(extra);
    let mut rp = RichPresence::new("123456789", m);
    rp.connect().unwrap();
    rp
}
fn sample() -> Presence {
    Presence {
        details: "Browsing the launcher".into(),
        state: "Idle".into(),
        ..Default::default()
    }
}

#[test]
fn frame_roundtrip_and_partial_input() {
    let bytes = encode_frame(Opcode::Frame, b"{\"a\":1}").unwrap();
    assert_eq!(&bytes[0..4], &1u32.to_le_bytes());
    assert_eq!(&bytes[4..8], &7u32.to_le_bytes());
    let (op, payload, used) = decode_frame(&bytes).unwrap().unwrap();
    assert_eq!(
        (op, payload.as_slice(), used),
        (Opcode::Frame, &b"{\"a\":1}"[..], bytes.len())
    );
    assert!(decode_frame(&bytes[..5]).unwrap().is_none());
    assert!(decode_frame(&bytes[..bytes.len() - 1]).unwrap().is_none());
}

#[test]
fn frame_limits_and_bad_opcode() {
    assert!(encode_frame(Opcode::Frame, &vec![b'x'; MAX_FRAME]).is_err());
    let mut bad = 9u32.to_le_bytes().to_vec();
    bad.extend_from_slice(&0u32.to_le_bytes());
    assert!(decode_frame(&bad).is_err());
    let mut huge = 1u32.to_le_bytes().to_vec();
    huge.extend_from_slice(&(MAX_FRAME as u32).to_le_bytes());
    assert!(decode_frame(&huge).is_err());
}

#[test]
fn sanitize_rules() {
    assert_eq!(
        sanitize_text("  hello\u{0007}\n world "),
        Some("hello world".to_string())
    );
    assert_eq!(sanitize_text("x"), None);
    assert_eq!(sanitize_text(""), None);
    assert_eq!(
        sanitize_text(&"a".repeat(300)).unwrap().chars().count(),
        128
    );
}

#[test]
fn activity_only_contains_expected_fields() {
    let p = Presence {
        details: "Playing".into(),
        state: "x".into(),
        large_image: "logo".into(),
        large_text: "Team27".into(),
        start_unix: Some(1700000000),
        ..Default::default()
    };
    let a = activity_json(&p);
    assert_eq!(a["details"], "Playing");
    assert!(a.get("state").is_none(), "1-char state is dropped");
    assert_eq!(a["assets"]["large_image"], "logo");
    assert_eq!(a["assets"]["large_text"], "Team27");
    assert_eq!(a["timestamps"]["start"], 1700000000u64);
    let none = activity_json(&Presence::default());
    assert_eq!(none, json!({}));
    let no_image = activity_json(&Presence {
        large_text: "orphan".into(),
        ..Default::default()
    });
    assert!(no_image.get("assets").is_none());
}

#[test]
fn small_corner_image_is_sent_only_with_a_usable_asset_key() {
    let with_small = Presence {
        large_image: "logo".into(),
        small_image: "team27_corner".into(),
        small_text: "Team27".into(),
        ..Default::default()
    };
    let a = activity_json(&with_small);
    assert_eq!(a["assets"]["large_image"], "logo");
    assert_eq!(a["assets"]["small_image"], "team27_corner");
    assert_eq!(a["assets"]["small_text"], "Team27");

    // The small image works on its own, without a large one.
    let only_small = Presence {
        small_image: "corner".into(),
        ..Default::default()
    };
    let a = activity_json(&only_small);
    assert_eq!(a["assets"]["small_image"], "corner");
    assert!(a["assets"].get("large_image").is_none());

    // Empty, blank or control-character keys are dropped, and hover text never travels alone.
    for bad in ["", "   ", "bad\nkey", &"k".repeat(300)] {
        let p = Presence {
            small_image: bad.into(),
            small_text: "orphan".into(),
            ..Default::default()
        };
        assert_eq!(
            activity_json(&p),
            json!({}),
            "{bad:?} must not produce assets"
        );
    }
}

#[test]
fn asset_keys_are_trimmed_and_bounded() {
    assert_eq!(
        sanitize_asset_key("  team27_corner  "),
        Some("team27_corner".to_string())
    );
    assert_eq!(sanitize_asset_key(""), None);
    assert_eq!(sanitize_asset_key("a\u{0007}b"), None);
    assert!(sanitize_asset_key(&"k".repeat(256)).is_some());
    assert!(sanitize_asset_key(&"k".repeat(257)).is_none());
}

#[test]
fn set_activity_payload_shape() {
    let p = set_activity_payload(42, Some(&sample()), 7);
    assert_eq!(p["cmd"], "SET_ACTIVITY");
    assert_eq!(p["args"]["pid"], 42);
    assert_eq!(p["nonce"], "7");
    assert_eq!(p["args"]["activity"]["details"], "Browsing the launcher");
    let clear = set_activity_payload(42, None, 8);
    assert!(clear["args"].get("activity").is_none());
}

#[test]
fn handshake_sends_version_and_client_id_then_requires_ready() {
    let rp = connected(vec![]);
    assert!(rp.is_connected());
    assert!(!rp.is_enabled(), "presence is opt-in");

    let mut m = Mock::default();
    m.replies.push_back((
        Opcode::Frame,
        json!({ "evt": "ERROR", "data": { "code": 4000 } }),
    ));
    let mut rp = RichPresence::new("1", m);
    assert!(rp.connect().is_err());
    assert!(!rp.is_connected());

    let mut m = Mock::default();
    m.replies.push_back((
        Opcode::Close,
        json!({ "code": 4000, "message": "Invalid Client ID" }),
    ));
    assert!(RichPresence::new("1", m).connect().is_err());
}

#[test]
fn ping_during_handshake_is_answered_with_pong() {
    let mut m = Mock::default();
    m.replies.push_back((Opcode::Ping, json!({ "n": 1 })));
    m.replies.push_back(ready());
    let mut rp = RichPresence::new("1", m);
    rp.connect().unwrap();
}

#[test]
fn disabled_by_default_publishes_nothing() {
    let mut rp = connected(vec![]);
    assert_eq!(
        rp.publish(Status::Experimental, &sample()).unwrap(),
        PublishOutcome::DisabledByUser
    );
}

#[test]
fn unsupported_and_uninvestigated_games_never_publish() {
    let mut rp = connected(vec![]);
    rp.set_enabled(true).unwrap();
    assert_eq!(
        rp.publish(Status::Unsupported, &sample()).unwrap(),
        PublishOutcome::UnsupportedGame
    );
    assert_eq!(
        rp.publish(Status::NotInvestigated, &sample()).unwrap(),
        PublishOutcome::UnsupportedGame
    );
}

#[test]
fn enabled_and_supported_publishes() {
    let mut rp = connected(vec![ack()]);
    rp.set_enabled(true).unwrap();
    assert_eq!(
        rp.publish(Status::Experimental, &sample()).unwrap(),
        PublishOutcome::Sent
    );
}

#[test]
fn turning_off_clears_activity_and_stops_publishing() {
    let mut rp = connected(vec![ack(), ack()]);
    rp.set_enabled(true).unwrap();
    rp.publish(Status::Partial, &sample()).unwrap();
    rp.set_enabled(false).unwrap();
    assert!(!rp.is_enabled());
    assert_eq!(
        rp.publish(Status::Partial, &sample()).unwrap(),
        PublishOutcome::DisabledByUser
    );
}

#[test]
fn discord_error_reply_is_reported() {
    let mut rp = connected(vec![(
        Opcode::Frame,
        json!({ "evt": "ERROR", "data": { "code": 4000, "message": "bad" } }),
    )]);
    rp.set_enabled(true).unwrap();
    assert!(rp.publish(Status::Experimental, &sample()).is_err());
}

#[test]
fn publish_before_connect_is_an_error_not_a_panic() {
    let mut rp = RichPresence::new("1", Mock::default());
    rp.set_enabled(true).unwrap();
    assert!(rp.publish(Status::Experimental, &sample()).is_err());
}
