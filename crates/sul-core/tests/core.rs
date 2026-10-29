use serde_json::json;
use std::fs;
use std::path::Path;
use sul_core::Error;
use sul_core::capability::{CapabilityEntry, Status};
use sul_core::fsops::{ProtectedRoots, atomic_write, isolated_copy};
use sul_core::journal::Journal;
use sul_core::logging::{Logger, redact};
use sul_core::manifest::{BuildManifest, Variant};
use sul_core::safepath::{join_within, validate_relative};

fn write(p: &Path, s: &str) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, s).unwrap();
}

fn sample_tree(root: &Path) {
    write(&root.join("game.exe"), "binary-a");
    write(&root.join("data/assets.bin"), "assets-1");
    write(&root.join("data/sub/more.bin"), "more");
}

// ---- safepath ----
#[test]
fn rejects_unsafe_relative_paths() {
    for bad in [
        "../x",
        "a/../../x",
        "/etc/passwd",
        "C:\\Windows",
        "C:/x",
        "",
        "a\0b",
        "..",
    ] {
        assert!(validate_relative(bad).is_err(), "should reject {bad:?}");
    }
}

#[test]
fn accepts_normal_relative_paths() {
    assert_eq!(
        validate_relative("a/b/c.txt").unwrap(),
        Path::new("a").join("b").join("c.txt")
    );
    assert_eq!(
        validate_relative("a\\b.txt").unwrap(),
        Path::new("a").join("b.txt")
    );
    assert_eq!(
        validate_relative("./a//b").unwrap(),
        Path::new("a").join("b")
    );
}

#[test]
fn join_within_stays_in_root() {
    let d = tempfile::tempdir().unwrap();
    let p = join_within(d.path(), "x/y.txt").unwrap();
    assert!(p.starts_with(d.path().canonicalize().unwrap()));
    assert!(join_within(d.path(), "../escape").is_err());
}

#[cfg(unix)]
#[test]
fn join_within_rejects_symlink_escape() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("link")).unwrap();
    assert!(join_within(root.path(), "link/file.txt").is_err());
}

// ---- capability ----
#[test]
fn supported_tested_requires_evidence() {
    let mut c = CapabilityEntry::not_investigated("launch", "helldivers2");
    assert_eq!(c.status, Status::NotInvestigated);
    assert!(c.validate().is_ok());
    c.status = Status::SupportedTested;
    assert!(c.validate().is_err());
    c.test_date = Some("2026-10-22".into());
    assert!(c.validate().is_err());
    c.evidence = Some("logs/run1.jsonl".into());
    assert!(c.validate().is_ok());
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

// ---- fsops ----
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

#[test]
fn isolated_copy_refuses_protected_existing_and_overlapping_destinations() {
    let base = tempfile::tempdir().unwrap();
    let src = base.path().join("src");
    sample_tree(&src);
    let mut prot = ProtectedRoots::new();
    prot.add(&src).unwrap();
    let journal = Journal::open(&base.path().join("j.jsonl")).unwrap();

    // destination inside protected original
    let r = isolated_copy(&prot, &journal, "a", &src, &src.join("copy"));
    assert!(matches!(r, Err(Error::Protected(_))));

    // destination already exists
    let existing = base.path().join("existing");
    fs::create_dir_all(&existing).unwrap();
    let r = isolated_copy(&prot, &journal, "b", &src, &existing);
    assert!(matches!(r, Err(Error::DestinationExists(_))));

    // destination is parent of source (overlap) with no protection
    let open = ProtectedRoots::new();
    let r = isolated_copy(&open, &journal, "c", &src, base.path());
    assert!(r.is_err());
}

#[test]
fn atomic_write_replaces_and_respects_protection() {
    let d = tempfile::tempdir().unwrap();
    let mut prot = ProtectedRoots::new();
    let ok = d.path().join("cfg/settings.json");
    atomic_write(&prot, &ok, b"one").unwrap();
    atomic_write(&prot, &ok, b"two").unwrap();
    assert_eq!(fs::read_to_string(&ok).unwrap(), "two");

    prot.add(&d.path().join("cfg")).unwrap();
    let r = atomic_write(&prot, &d.path().join("cfg/other.json"), b"x");
    assert!(matches!(r, Err(Error::Protected(_))));
}

// ---- journal ----
#[test]
fn journal_reports_uncommitted_operations_and_survives_torn_line() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("j.jsonl");
    let j = Journal::open(&path).unwrap();
    j.begin("1", "copy", "a->b").unwrap();
    j.commit("1").unwrap();
    j.begin("2", "copy", "c->d").unwrap();
    j.begin("3", "copy", "e->f").unwrap();
    j.abort("3", "user cancelled").unwrap();
    // simulate a crash mid-write
    use std::io::Write;
    let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
    write!(f, "{{\"kind\":\"begin\",\"id\":\"4\",\"op\"").unwrap();

    let pending = j.pending().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, "2");
}

// ---- logging ----
#[test]
fn logger_redacts_secrets_on_disk() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("logs/sul.jsonl");
    let log = Logger::open(&path).unwrap();
    log.log(
        "info",
        "login",
        json!({"user": "soup", "access_token": "SUPERSECRET", "nested": {"Password": "hunter2"}}),
    )
    .unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(!text.contains("SUPERSECRET") && !text.contains("hunter2"));
    assert!(text.contains("[REDACTED]") && text.contains("soup"));
    assert_eq!(redact(&json!({"api_key": 1}))["api_key"], "[REDACTED]");
}

#[test]
fn hash_file_matches_the_published_sha256_vectors() {
    use sul_core::manifest::hash_file;
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
    use sul_core::manifest::hash_file;
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
    use sul_core::manifest::hash_file;
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope.bin");
    let error = hash_file(&missing).unwrap_err();
    assert!(error.to_string().contains("nope.bin"), "{error}");
}
