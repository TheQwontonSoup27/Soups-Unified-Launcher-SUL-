//! Tripwires for the Security & Trust page's claims about what SUL's own code writes and keeps.
//!
//! These read SUL's own Rust source as text (the part before each file's test code). They are an inventory that fails
//! when something new appears, so a person has to look at it and update the claim. They do not prove what the code does
//! at run time, and they cannot see inside other crates, the Rust standard library, Windows, or the WebView2 runtime.
//!
//! 1. Every place that creates, changes, moves or deletes a file is listed, with the file it is in. A new one fails the
//!    test until it is added here, which is where a reviewer is asked to decide whether it is safe.
//! 2. No field or JSON key in SUL's source is named like a secret, except the one type built to hold a webhook address.
//! 3. That type cannot be written to a file by accident: it does not derive `Serialize` or `Deserialize`.
//! 4. No crate for a credential store or a network client is declared, because neither is built yet. Whoever adds one
//!    must update the page's wording in the same change.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Every production source file in the app: `crates/sul-core/src` and `src-tauri/src`, as (path under the repo, text).
fn sources() -> Vec<(String, String)> {
    let root = repo();
    let mut files = Vec::new();
    rust_files(
        &root.join("crates").join("sul-core").join("src"),
        &mut files,
    );
    rust_files(&root.join("src-tauri").join("src"), &mut files);
    files.sort();
    files
        .into_iter()
        .map(|p| {
            let name = p
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            (name, fs::read_to_string(&p).unwrap().replace("\r\n", "\n"))
        })
        .collect()
}

/// The text before the file's test code, with `//` comments removed. A file with no test code is all production.
///
/// Test code starts at the first `#[cfg(test)]` or `#[cfg(all(test, ...))]` at the start of a line. Anything after that
/// point that is not a test module, an attribute, a closing brace or the one test helper is a mistake in this
/// convention (production code hiding in the test section), and fails here so it cannot escape the scan.
fn production(name: &str, text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let cut = lines
        .iter()
        .position(|l| l.starts_with("#[cfg(test)]") || l.starts_with("#[cfg(all(test"))
        .unwrap_or(lines.len());
    for (i, line) in lines.iter().enumerate().skip(cut) {
        let ok = line.is_empty()
            || line.starts_with(' ')
            || line.starts_with('\t')
            || line.starts_with("#[")
            || line.starts_with("mod ")
            || line.starts_with("pub(crate) mod ")
            || line.starts_with("pub(crate) use ")
            || line.starts_with("use ")
            || line.starts_with('}')
            || line.starts_with("//")
            || line.starts_with("pub(crate) fn make_junction");
        assert!(
            ok,
            "{name}:{}: production code after the test section starts, so the scan would skip it: {line}",
            i + 1
        );
    }
    lines[..cut]
        .iter()
        .map(|l| match l.find("//") {
            Some(i) if i == 0 || l[..i].ends_with(' ') => &l[..i],
            _ => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

const KINDS: &[(&str, &[&str])] = &[
    (
        "create or overwrite a file",
        &["fs::write", "File::create", "File::create_new"],
    ),
    ("open a file or pipe with options", &["OpenOptions"]),
    ("copy a file", &["fs::copy"]),
    ("move or rename", &["fs::rename"]),
    ("create a folder", &["create_dir_all", "fs::create_dir("]),
    ("delete a file", &["remove_file"]),
    ("delete a folder", &["remove_dir_all", "remove_dir("]),
    (
        "change permissions, links or times",
        &[
            "set_permissions",
            "hard_link",
            "symlink_file",
            "symlink_dir",
            "set_times",
            "set_len",
        ],
    ),
];

/// The kinds of change one file's production code makes. A name inside a longer name (`overwrite_x`, `my_remove_file`)
/// and a function being declared (`fn remove_file(`) are not counted; a method call (`f.set_len(0)`) is.
fn kinds_in(code: &str) -> BTreeSet<&'static str> {
    let mut found = BTreeSet::new();
    for (kind, needles) in KINDS {
        for needle in *needles {
            for (at, _) in code.match_indices(needle) {
                let inside_longer_name = code[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_');
                let is_declaration = code[..at].trim_end().ends_with("fn");
                if !inside_longer_name && !is_declaration {
                    found.insert(*kind);
                }
            }
        }
    }
    found
}

/// Where SUL's own code may change files, and what it does there. Updated by hand, on purpose.
const WRITE_SITES: &[(&str, &[&str], &str)] = &[
    (
        "crates/sul-core/src/discord.rs",
        &["open a file or pipe with options"],
        "opens Discord's local named pipe (\\\\.\\pipe\\discord-ipc-N) to send rich presence to the Discord app on this computer; it is not a disk file",
    ),
    (
        "crates/sul-core/src/fsops.rs",
        &[
            "copy a file",
            "create a folder",
            "create or overwrite a file",
            "move or rename",
        ],
        "the safe file operations: atomic write (temp file then rename), verified isolated copy; it deletes nothing",
    ),
    (
        "crates/sul-core/src/journal.rs",
        &["create a folder", "open a file or pipe with options"],
        "appends one line per operation to the journal",
    ),
    (
        "crates/sul-core/src/library.rs",
        &[
            "create a folder",
            "create or overwrite a file",
            "move or rename",
        ],
        "saves the game library index atomically (temp file then rename)",
    ),
    (
        "crates/sul-core/src/logging.rs",
        &["create a folder", "open a file or pipe with options"],
        "appends to the log file; every line passes the redactor first",
    ),
    (
        "crates/sul-core/src/service.rs",
        &["move or rename"],
        "sets an unreadable library file aside under a new name instead of deleting it",
    ),
    (
        "crates/sul-core/src/templates.rs",
        &["create a folder", "create or overwrite a file"],
        "writes a game-compatibility template the user asked for",
    ),
    (
        "crates/sul-core/src/trust/quarantine.rs",
        &[
            "create a folder",
            "delete a file",
            "delete a folder",
            "open a file or pipe with options",
        ],
        "the quarantine: a verified copy into its own folder (created with create-new, so it never overwrites), the original removed only after the copy is proven, a restore that never overwrites; the folder removal is only for a copy that did not finish; no purge",
    ),
    (
        "crates/sul-core/src/updater.rs",
        &["create a folder", "create or overwrite a file"],
        "saves which updater version was last seen",
    ),
    (
        "src-tauri/src/lib.rs",
        &["create a folder"],
        "creates the app's own data folder at start-up",
    ),
];

#[test]
fn the_scan_covers_the_source_it_is_meant_to() {
    let all = sources();
    assert!(all.len() >= 25, "found {} files", all.len());
    for must in [
        "crates/sul-core/src/fsops.rs",
        "crates/sul-core/src/trust/quarantine.rs",
        "src-tauri/src/lib.rs",
        "crates/sul-core/src/logging.rs",
    ] {
        assert!(all.iter().any(|(n, _)| n == must), "{must} was not scanned");
    }
}

#[test]
fn the_detector_sees_each_kind_and_skips_look_alikes() {
    let samples = [
        ("create or overwrite a file", "std::fs::write(p, b)?;"),
        (
            "create or overwrite a file",
            "let f = std::fs::File::create(&tmp)?;",
        ),
        (
            "create or overwrite a file",
            "let f = File::create_new(p)?;",
        ),
        (
            "open a file or pipe with options",
            "std::fs::OpenOptions::new().append(true)",
        ),
        ("copy a file", "fs::copy(&a, &b)?;"),
        ("move or rename", "std::fs::rename(&a, &b)"),
        ("create a folder", "fs::create_dir_all(dir)"),
        ("create a folder", "std::fs::create_dir(dir)"),
        ("delete a file", "let _ = fs::remove_file(p);"),
        ("delete a file", "remove_file(p)"),
        ("delete a folder", "fs::remove_dir_all(p)"),
        ("delete a folder", "fs::remove_dir(p)"),
        (
            "change permissions, links or times",
            "fs::set_permissions(p, perms)",
        ),
        ("change permissions, links or times", "f.set_len(0)?;"),
    ];
    for (kind, code) in samples {
        assert!(kinds_in(code).contains(kind), "{kind}: {code}");
    }
    for harmless in [
        "f.write(b)",
        "fn write(&self)",
        "self.io.write_all(&bytes)",
        "overwrite(x)",
        "rewrite(x)",
        "fn remove_file(&self, p: &Path)",
        "my_remove_file(p)",
        "fs::read(p)",
        "fs::read_to_string(p)",
        "fs::metadata(p)",
        "fs::read_dir(p)",
        "File::open(p)",
    ] {
        assert!(kinds_in(harmless).is_empty(), "{harmless} was counted");
    }
    assert_eq!(
        production("x.rs", "let a = 1; // fs::write(p)\nfs::remove_file(p);\n"),
        "let a = 1; \nfs::remove_file(p);"
    );
    assert_eq!(
        production(
            "x.rs",
            "a();\n#[cfg(test)]\nmod tests {\n    fs::write(p);\n}\n"
        ),
        "a();"
    );
}

#[test]
fn std_fs_is_only_imported_by_names_the_scan_understands() {
    // `use std::fs::write;` would let a bare `write(p, b)` through the scan above, so such imports are refused.
    let known = [
        "self",
        "File",
        "OpenOptions",
        "read_dir",
        "read_to_string",
        "read",
        "metadata",
        "symlink_metadata",
        "canonicalize",
        "Metadata",
        "ReadDir",
        "DirEntry",
    ];
    for (name, text) in sources() {
        for line in production(&name, &text).lines().filter(|l| {
            l.trim_start().starts_with("use std::fs::")
                || l.trim_start().starts_with("use std::fs;")
        }) {
            let names = line
                .trim()
                .trim_end_matches(';')
                .trim_start_matches("use std::fs")
                .trim_start_matches("::")
                .trim_matches(|c| c == '{' || c == '}');
            for item in names.split(',').map(str::trim).filter(|i| !i.is_empty()) {
                assert!(
                    known.contains(&item),
                    "{name}: imports std::fs::{item}; add it to the scan before using it"
                );
            }
        }
    }
}

#[test]
fn only_the_steam_launcher_starts_another_program() {
    for (name, text) in sources() {
        let code = production(&name, &text);
        let spawns = code.contains("Command::new")
            || code.contains(".spawn(")
            || code.contains("ShellExecute");
        assert_eq!(spawns, name == "crates/sul-core/src/steam.rs", "{name}");
    }
}

#[test]
fn only_the_listed_files_change_files_and_only_in_the_listed_ways() {
    let mut found: Vec<(String, BTreeSet<&str>)> = Vec::new();
    for (name, text) in sources() {
        let kinds = kinds_in(&production(&name, &text));
        if !kinds.is_empty() {
            found.push((name, kinds));
        }
    }
    let mut problems = Vec::new();
    for (name, kinds) in &found {
        match WRITE_SITES.iter().find(|(n, _, _)| n == name) {
            None => problems.push(format!(
                "{name} changes files ({kinds:?}) and is not in WRITE_SITES"
            )),
            Some((_, allowed, _)) => {
                for k in kinds {
                    if !allowed.contains(k) {
                        problems.push(format!(
                            "{name} now also does \"{k}\", which WRITE_SITES does not allow"
                        ));
                    }
                }
                for k in *allowed {
                    if !kinds.contains(k) {
                        problems.push(format!("{name} no longer does \"{k}\"; remove it from WRITE_SITES so the list stays true"));
                    }
                }
            }
        }
    }
    for (name, allowed, _) in WRITE_SITES {
        if !allowed.is_empty() && !found.iter().any(|(n, _)| n == name) {
            problems.push(format!(
                "{name} is in WRITE_SITES but does not change any file"
            ));
        }
    }
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}

#[test]
fn every_write_site_says_why() {
    for (name, kinds, why) in WRITE_SITES {
        assert!(why.len() >= 20, "{name}: say what it is for");
        assert!(
            Path::new(&repo()).join(name).is_file(),
            "{name} does not exist"
        );
        let mut sorted = kinds.to_vec();
        sorted.sort_unstable();
        assert_eq!(
            &sorted, kinds,
            "{name}: keep the kinds in alphabetical order"
        );
    }
}

#[test]
fn only_the_quarantine_deletes_anything() {
    // Deletion is the dangerous operation. Only the quarantine may do it, and only after a verified copy (its own tests
    // and its guard test list the exact four places).
    for (name, text) in sources() {
        let kinds = kinds_in(&production(&name, &text));
        let deletes = kinds.contains("delete a file") || kinds.contains("delete a folder");
        assert!(
            !deletes || name == "crates/sul-core/src/trust/quarantine.rs",
            "{name} deletes"
        );
    }
}

/// Names that look like a secret when used as a field or key (`name:` or `"name"`).
const SECRET_WORDS: &[&str] = &[
    "token",
    "secret",
    "secrets",
    "password",
    "passwords",
    "passwd",
    "credential",
    "credentials",
    "apikey",
    "api_key",
    "bearer",
    "authorization",
    "webhook_url",
    "webhookurl",
];

/// True when `word` appears in `line` as a field (`word: Type`, not `Word::Variant`) or a quoted key (`"word"`).
fn looks_like_field(line: &str, word: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.match_indices(word).any(|(at, _)| {
        let before = lower[..at].chars().next_back();
        let rest = &lower[at + word.len()..];
        let own_word = !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
            && !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_');
        let field = rest.starts_with(':') && !rest.starts_with("::");
        let key = before == Some('"') && rest.starts_with('"');
        let binding = lower[..at].trim_end().ends_with("let")
            || lower[..at].trim_end().ends_with("pub")
            || lower[..at].trim_end().ends_with("mut");
        own_word && (field || key || binding)
    })
}

#[test]
fn the_secret_name_detector_sees_fields_and_keys_and_skips_look_alikes() {
    for yes in [
        "pub token: String,",
        "    secret: Option<String>,",
        "let password = read();",
        "json!({\"api_key\": 1})",
        "struct A { webhook_url: String }",
        "pub authorization: x",
    ] {
        assert!(
            SECRET_WORDS.iter().any(|w| looks_like_field(yes, w)),
            "{yes}"
        );
    }
    for no in [
        "tokens.push(Token::Open)",
        "Some(Token::Str(key))",
        "let tokens = tokenize(text)?;",
        "// the token",
        "fn tokenize(t: &str)",
        "let credentials_ok = true;",
        "Err(Error::Unauthorized)",
    ] {
        assert!(
            !SECRET_WORDS.iter().any(|w| looks_like_field(no, w)),
            "{no}"
        );
    }
}

#[test]
fn no_field_or_key_is_named_like_a_secret_except_the_webhook_type() {
    let mut hits = Vec::new();
    for (name, text) in sources() {
        // The webhook type is the one place built to hold a secret. The log redactor lists secret-like names so it can
        // hide their values; that is its job.
        if name == "crates/sul-core/src/trust/webhook.rs"
            || name == "crates/sul-core/src/logging.rs"
        {
            continue;
        }
        for (n, line) in production(&name, &text).lines().enumerate() {
            if SECRET_WORDS.iter().any(|w| looks_like_field(line, w)) {
                hits.push(format!("{name}:{}: {}", n + 1, line.trim()));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "a field or key looks like a secret: {hits:#?}"
    );
}

#[test]
fn the_webhook_type_cannot_be_written_to_a_file_by_accident() {
    let (name, text) = sources()
        .into_iter()
        .find(|(n, _)| n.ends_with("trust/webhook.rs"))
        .unwrap();
    let code = production(&name, &text);
    assert!(
        !code.contains("Serialize") && !code.contains("Deserialize"),
        "WebhookUrl must not derive serde traits"
    );
    assert!(
        !code.contains("fs::") && !code.contains("File"),
        "the webhook module must not touch files"
    );
    // Its derive list is exactly what the module's own tests (Debug and Display never show the token) rely on.
    for derive in code
        .lines()
        .filter(|l| l.trim_start().starts_with("#[derive("))
    {
        assert!(
            !derive.contains("Debug") && !derive.contains("Display"),
            "Debug and Display are written by hand so they can mask: {derive}"
        );
    }
}

#[test]
fn no_credential_store_or_network_client_is_declared_yet() {
    // Webhook sending and Windows Credential Manager storage are decided but not built (docs/decisions/0004). When
    // either is built this test must be changed together with the Security & Trust page text and the capability matrix.
    let root = repo();
    for manifest in [
        "Cargo.toml",
        "crates/sul-core/Cargo.toml",
        "src-tauri/Cargo.toml",
    ] {
        let text = fs::read_to_string(root.join(manifest)).unwrap();
        for line in text.lines().map(str::trim).filter(|l| !l.starts_with('#')) {
            let name = line
                .split(['=', '.', ' '])
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            for banned in [
                "keyring",
                "wincred",
                "credential",
                "secret-service",
                "security-framework",
                "reqwest",
                "ureq",
                "hyper",
                "curl",
                "isahc",
                "attohttpc",
                "minreq",
                "surf",
            ] {
                assert!(
                    !name.contains(banned),
                    "{manifest} declares \"{name}\": update the page and the matrix with it"
                );
            }
        }
    }
}
