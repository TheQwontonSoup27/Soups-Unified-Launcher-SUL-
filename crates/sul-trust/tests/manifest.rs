//! The build manifest (`sul_trust::manifest`): scan, verify, JSON round trip, and the SHA-256 file hash.
//!
//! These tests lived in `sul-core/tests/core.rs` when the manifest was part of `sul-core` (D-154). The manifest is here now,
//! and `isolated_copy_never_touches_source_and_verifies_clean` came with it: it checks `sul_core::fsops::isolated_copy`
//! with a manifest as the witness, so it needs both crates, and `sul-trust` is the lowest crate that has both.

use serde_json::json;
use std::fs;
use std::path::Path;
use sul_core::Error;
use sul_core::fsops::{ProtectedRoots, isolated_copy};
use sul_core::journal::Journal;
use sul_trust::manifest::{BuildManifest, Variant};

fn write(p: &Path, s: &str) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, s).unwrap();
}

fn sample_tree(root: &Path) {
    write(&root.join("game.exe"), "binary-a");
    write(&root.join("data/assets.bin"), "assets-1");
    write(&root.join("data/sub/more.bin"), "more");
}

// ---- manifest ----
#[test]
fn manifest_verify_detects_changes() {
    let d = tempfile::tempdir().unwrap();
    sample_tree(d.path());
    let m = BuildManifest::scan(
        d.path(),
        "terraria",
        "Terraria",
        Variant::Original,
        "1.4.4",
        "test",
    )
    .unwrap();
    assert_eq!(m.files.len(), 3);
    assert!(m.verify(d.path()).unwrap().is_clean());

    write(&d.path().join("data/assets.bin"), "tampered!");
    fs::remove_file(d.path().join("game.exe")).unwrap();
    write(&d.path().join("new.txt"), "extra");
    let r = m.verify(d.path()).unwrap();
    assert_eq!(r.modified, vec!["data/assets.bin"]);
    assert_eq!(r.missing, vec!["game.exe"]);
    assert_eq!(r.extra, vec!["new.txt"]);
}

#[test]
fn manifest_json_roundtrip_and_future_schema_rejected() {
    let d = tempfile::tempdir().unwrap();
    sample_tree(d.path());
    let m =
        BuildManifest::scan(d.path(), "g", "G", Variant::Custom("mine".into()), "1", "s").unwrap();
    let back = BuildManifest::from_json(&m.to_json().unwrap()).unwrap();
    assert_eq!(m, back);
    let mut v: serde_json::Value = serde_json::from_str(&m.to_json().unwrap()).unwrap();
    v["schema_version"] = json!(999);
    assert!(BuildManifest::from_json(&v.to_string()).is_err());
}

#[test]
fn manifest_with_traversal_path_is_rejected_on_verify() {
    let d = tempfile::tempdir().unwrap();
    sample_tree(d.path());
    let mut m = BuildManifest::scan(d.path(), "g", "G", Variant::Original, "1", "s").unwrap();
    m.files[0].path = "../evil".into();
    assert!(matches!(m.verify(d.path()), Err(Error::UnsafePath(_))));
}

// ---- fsops, witnessed by a manifest ----
#[test]
fn isolated_copy_never_touches_source_and_verifies_clean() {
    let base = tempfile::tempdir().unwrap();
    let src = base.path().join("steam_install");
    sample_tree(&src);
    let before = BuildManifest::scan(&src, "g", "G", Variant::Original, "1", "s").unwrap();

    let mut prot = ProtectedRoots::new();
    prot.add(&src).unwrap();
    let journal = Journal::open(&base.path().join("state/journal.jsonl")).unwrap();
    let dst = base.path().join("builds/legacy");
    isolated_copy(&prot, &journal, "op1", &src, &dst).unwrap();

    assert!(
        before.verify(&src).unwrap().is_clean(),
        "source must be unchanged"
    );
    assert!(
        before.verify(&dst).unwrap().is_clean(),
        "copy must match source"
    );
    assert!(journal.pending().unwrap().is_empty());
    assert!(!base.path().join("builds/legacy.partial").exists());
}

// ---- hash_file ----
#[test]
fn hash_file_matches_the_published_sha256_vectors() {
    use sul_trust::manifest::hash_file;
    let dir = tempfile::tempdir().unwrap();

    let empty = dir.path().join("empty");
    fs::write(&empty, b"").unwrap();
    assert_eq!(
        hash_file(&empty).unwrap(),
        (
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_owned(),
            0
        )
    );

    let abc = dir.path().join("abc");
    fs::write(&abc, b"abc").unwrap();
    assert_eq!(
        hash_file(&abc).unwrap(),
        (
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".to_owned(),
            3
        )
    );
}

#[test]
fn hash_file_reads_files_larger_than_its_buffer_in_pieces() {
    use sul_trust::manifest::hash_file;
    // 2 MiB + 12345 bytes: more than two reads of the 1 MiB buffer, with a short last read.
    let data: Vec<u8> = (0..(2usize << 20) + 12_345)
        .map(|i| ((i * 31 + 7) % 256) as u8)
        .collect();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("big.bin");
    fs::write(&file, &data).unwrap();
    assert_eq!(
        hash_file(&file).unwrap(),
        (
            "8a7235b1459d77c512e367b38b2a4170a6d5bdb240b5d577b56726bc6f1a8b48".to_owned(),
            2_109_497
        )
    );
}

#[test]
fn hash_file_of_a_missing_file_is_an_error_that_names_the_file() {
    use sul_trust::manifest::hash_file;
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope.bin");
    let error = hash_file(&missing).unwrap_err();
    assert!(error.to_string().contains("nope.bin"), "{error}");
}
