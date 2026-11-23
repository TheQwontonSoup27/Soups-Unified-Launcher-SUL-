use serde_json::json;
use std::fs;
use std::path::Path;
use sul_core::Error;
use sul_core::fsops::{ProtectedRoots, atomic_write, isolated_copy};
use sul_core::journal::Journal;
use sul_core::logging::{Logger, redact};
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

/// A junction is a folder link Windows lets anyone create (a symlink needs a privilege). One inside the
/// root that points outside it must not be followed out.
#[cfg(windows)]
#[test]
fn join_within_rejects_junction_escape() {
    use std::os::windows::process::CommandExt;
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let link = root.path().join("link");
    let made = std::process::Command::new("cmd")
        .raw_arg(format!(
            r#"/C mklink /J "{}" "{}""#,
            link.display(),
            outside.path().display()
        ))
        .output()
        .unwrap();
    assert!(
        made.status.success(),
        "mklink failed: {}",
        String::from_utf8_lossy(&made.stdout)
    );
    assert!(join_within(root.path(), "link/file.txt").is_err());
    assert!(join_within(root.path(), "link").is_err());
    assert!(join_within(root.path(), "plain/file.txt").is_ok());
}

// ---- fsops ----
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

/// The whole path to the disk, not just the function: a webhook URL put in a field of its own, in a message, in an
/// error string or in a nested value must not be in the file afterwards.
#[test]
fn logger_never_writes_a_discord_webhook_url_to_disk() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("logs/sul.jsonl");
    let log = Logger::open(&path).unwrap();
    let id = "987654321098765432";
    let token = "Zz9_Yy8-Xx7WwVvUuTtSsRrQqPpOoNnMmLlKkJjIiHhGgFfEeDdCcBbAa0123456789abcdefghij";
    let url = format!("https://discord.com/api/webhooks/{id}/{token}");
    log.log(
        "error",
        "webhook post failed",
        json!({
            "webhook_url": url,
            "error": format!("error sending request for url ({url}): connection refused"),
            "details": {"attempts": [format!("POST {url}")], "status": 0},
            "channel": "clips",
        }),
    )
    .unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(!text.contains(id) && !text.contains(token), "{text}");
    assert!(
        text.contains("error sending request for url ([REDACTED]): connection refused"),
        "the words around the address are kept: {text}"
    );
    assert!(text.contains("clips") && text.contains("connection refused"));
    let line: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
    assert_eq!(line["fields"]["webhook_url"], "[REDACTED]");
}
