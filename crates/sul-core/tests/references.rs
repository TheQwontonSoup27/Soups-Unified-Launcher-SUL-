//! `docs/references.md` is the one list of links and named sources. These tests keep it complete.
//!
//! They read the repository's Markdown documents as text. A document that carries a web address which the list does not
//! contain fails the test, so whoever writes the link has to say what it is and whether anyone opened it. They do not
//! check that an address still works (nothing here reaches the network), and they cannot see a link that was never written
//! down.

use std::fs;
use std::path::{Path, PathBuf};

/// The links the owner sent (eight on 2026-11-05, ten on 2026-11-09). Each must stay in the list with its status and a
/// plain description of what it is and does.
const OWNER_LINKS: [&str; 18] = [
    "https://github.com/morluto/rea",
    "https://github.com/Zagitta/gpu-unlock",
    "https://github.com/Raphire/Win11Debloat",
    "https://github.com/Gathin23/cleanyourtv",
    "https://github.com/yuliskov/SmartTube",
    "https://github.com/yuliskov/SmartTube/releases",
    "https://github.com/ReVanced/revanced-manager",
    "https://github.com/pi-hole/pi-hole",
    "https://github.com/yt-dlp/yt-dlp",
    "https://github.com/ollama/ollama",
    "https://github.com/immich-app/immich",
    "https://github.com/openai/whisper",
    "https://github.com/plausible/analytics",
    "https://github.com/appflowy-io/appflowy",
    "https://github.com/penpot/penpot",
    "https://github.com/n8n-io",
    "https://github.com/calendly",
    "https://gist.github.com/aamiaa/204cd9d42013ded9faf646fae7f89fbb",
];

/// Addresses that are made-up examples or local development addresses. They are described in section 8 of the list and
/// are not links to follow.
const NOT_REFERENCES: [&str; 6] = [
    "https://discord.com/api/webhooks",
    "https://relay.example",
    "https://example.com",
    "https://github.com/owner/repo",
    "http://localhost",
    "http://tauri.localhost",
];

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn references() -> String {
    fs::read_to_string(repo().join("docs").join("references.md"))
        .expect("docs/references.md exists")
}

fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            markdown_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
}

/// The documents that are written by hand: the root documents and everything under `docs/`.
fn documents() -> Vec<PathBuf> {
    let root = repo();
    let mut files: Vec<PathBuf> = [
        "README.md",
        "CLAUDE.md",
        "PROJECT_SPEC.md",
        "PROJECT_STATUS.md",
        "ROADMAP.md",
    ]
    .iter()
    .map(|f| root.join(f))
    .collect();
    markdown_files(&root.join("docs"), &mut files);
    files
}

/// Every web address in `text`, without the punctuation that ends a sentence or closes a link.
fn addresses(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in text.split(|c: char| c.is_whitespace() || "()<>\"'`[]|".contains(c)) {
        for scheme in ["https://", "http://"] {
            if let Some(start) = token.find(scheme) {
                let address = token[start..].trim_end_matches(['.', ',', ';', ':', '*', '!', '?']);
                if address.len() > scheme.len() {
                    out.push(address.to_owned());
                }
                break;
            }
        }
    }
    out
}

fn is_example(address: &str) -> bool {
    NOT_REFERENCES
        .iter()
        .any(|prefix| address.starts_with(prefix))
}

#[test]
fn the_scan_finds_addresses_and_trims_what_is_not_part_of_them() {
    let text = "See https://rustup.rs. Also [x](https://github.com/a/b/releases), <http://localhost:5173/> and \"https://discord.com/api/webhooks/1/x\"; **https://a.example/p**: end";
    assert_eq!(
        addresses(text),
        [
            "https://rustup.rs",
            "https://github.com/a/b/releases",
            "http://localhost:5173/",
            "https://discord.com/api/webhooks/1/x",
            "https://a.example/p"
        ]
    );
    assert!(
        is_example("http://localhost:5173/") && is_example("https://discord.com/api/webhooks/1/x")
    );
    assert!(!is_example("https://github.com/morluto/rea") && !is_example("https://rustup.rs"));
    assert!(addresses("no links here, only https:// and http://").is_empty());
}

#[test]
fn the_scan_reaches_the_documents_it_is_meant_to() {
    let files = documents();
    let names: Vec<String> = files
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    for expected in [
        "PROJECT_SPEC.md",
        "CLAUDE.md",
        "steamcmd-acquisition-2026-11-03.md",
        "audio-modding-2026-11-01.md",
        "discord.md",
        "SETUP-WINDOWS.md",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "{expected} is not scanned: {names:?}"
        );
    }
    let found: usize = files
        .iter()
        .filter(|p| !p.ends_with("references.md"))
        .map(|p| {
            addresses(&fs::read_to_string(p).unwrap())
                .into_iter()
                .filter(|a| !is_example(a))
                .count()
        })
        .sum();
    assert!(
        found >= 6,
        "only {found} addresses found in the documents; the scan is probably reading the wrong place"
    );
}

#[test]
fn every_web_address_in_the_documents_is_in_the_list() {
    let list = references();
    let mut missing = Vec::new();
    for file in documents() {
        if file.ends_with("references.md") {
            continue;
        }
        let text = fs::read_to_string(&file).unwrap();
        for address in addresses(&text) {
            if !is_example(&address) && !list.contains(&address) {
                missing.push(format!(
                    "{} (in {})",
                    address,
                    file.strip_prefix(repo()).unwrap().display()
                ));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "add these to docs/references.md with their status:\n{}",
        missing.join("\n")
    );
}

#[test]
fn the_owner_links_are_listed_with_a_status_and_none_is_called_investigated() {
    let list = references();
    for link in OWNER_LINKS {
        let line = list
            .lines()
            .find(|l| l.contains(link))
            .unwrap_or_else(|| panic!("{link} is missing from docs/references.md"));
        assert!(
            line.contains("Not investigated") || line.contains("Front page only"),
            "{link} needs its status on its row: {line}"
        );
        assert!(
            !line.contains("Supported and tested"),
            "{link} must not be claimed as tested: {line}"
        );
    }
}

#[test]
fn every_owner_link_row_has_a_plain_description_of_what_it_is_and_does() {
    let list = references();
    for link in OWNER_LINKS {
        let line = list
            .lines()
            .find(|l| l.starts_with('|') && l.contains(link))
            .unwrap_or_else(|| panic!("{link} has no table row in docs/references.md"));
        // | Link | tagline | What it is and does | Licence | Status | Notes for SUL |
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        assert_eq!(
            cells.len(),
            8,
            "{link}: the row must have the six columns of its table: {line}"
        );
        assert!(
            cells[3].chars().count() >= 60,
            "{link} needs a plain description of what it is and does (column 3): {line}"
        );
    }
}

#[test]
fn the_list_says_what_its_status_words_mean_and_that_no_code_is_copied() {
    let list = references();
    for needle in [
        "Status words",
        "Not investigated",
        "Listed, not read",
        "no code is copied",
        "undecided",
    ] {
        assert!(
            list.contains(needle),
            "docs/references.md lost its explanation: {needle}"
        );
    }
}
