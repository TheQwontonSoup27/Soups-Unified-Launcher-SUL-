//! The rule in `sul_trust::policy` and the quarantine agree, end to end.
//!
//! This test used to sit in `policy.rs` when both lived in `sul-core`. The quarantine depends on `sul-trust` now, so the one
//! test that needs both lives here, where both can be used.

use sul_core::fsops::ProtectedRoots;
use sul_quarantine::quarantine::Quarantine;
use sul_trust::findings::Certainty;
use sul_trust::manifest::hash_file;
use sul_trust::policy::{
    Action, GameModStatus, KnownHarmful, ListSource, Observation, RiskKind, Tiering, decide,
};

#[test]
fn the_rule_and_the_quarantine_agree_end_to_end() {
    let tmp = tempfile::tempdir().unwrap();
    let own = tmp.path().join("sul");
    let game = tmp.path().join("game");
    std::fs::create_dir_all(&own).unwrap();
    std::fs::create_dir_all(&game).unwrap();
    let mut external = ProtectedRoots::new();
    external.add(&game).unwrap();
    let tiering = Tiering::new(&[own.as_path()], external.clone()).unwrap();
    let q = Quarantine::open(&tmp.path().join("quarantine"), external).unwrap();

    let ours = own.join("sidecar.exe");
    let theirs = game.join("anticheat.dll");
    std::fs::write(&ours, b"tampered sidecar").unwrap();
    std::fs::write(&theirs, b"tampered anticheat").unwrap();
    let ours_hash = hash_file(&ours).unwrap().0;
    let theirs_hash = hash_file(&theirs).unwrap().0;
    let mut known = KnownHarmful::empty();
    known
        .add(&ours_hash, ListSource::HiveMind, "tampered sidecar", 1)
        .unwrap();
    known
        .add(&theirs_hash, ListSource::Jarvis, "tampered anticheat", 2)
        .unwrap();

    // SUL's own file with known-harmful content: automatic, and the quarantine can undo it.
    let o = Observation::new(tiering.classify(&ours), Certainty::Confirmed).with_sha256(&ours_hash);
    let d = decide(&o, &known);
    assert_eq!(
        (d.action, d.source),
        (Action::QuarantineAutomatically, Some(ListSource::HiveMind))
    );
    let rec = q
        .quarantine_file(&ours, "known-harmful content (automatic)")
        .unwrap();
    assert!(!ours.exists());
    q.restore(&rec.id).unwrap();
    assert_eq!(std::fs::read(&ours).unwrap(), b"tampered sidecar");

    // The same list entry on a game's file: the game is not launched, nothing is moved, and the quarantine refuses on its own too.
    let o = Observation::new(tiering.classify(&theirs), Certainty::Confirmed)
        .with_sha256(&theirs_hash)
        .with_risk(RiskKind::AntiCheatFlag);
    assert_eq!(decide(&o, &known).action, Action::BlockLaunch);
    assert!(q.quarantine_file(&theirs, "must be refused").is_err());
    assert_eq!(std::fs::read(&theirs).unwrap(), b"tampered anticheat");
    // Owner-modded: a warning they can override, and still nothing is moved.
    let modded = o.with_game(GameModStatus::OwnerModded);
    assert_eq!(decide(&modded, &known).action, Action::WarnOwnerOverridable);
    assert_eq!(std::fs::read(&theirs).unwrap(), b"tampered anticheat");
}
