//! The trusted baseline: which files the watchdog guards, and the SHA-256 each one must have (Ch. 12 §2.8, Ch. 14 §14.13.2).
//!
//! "Baseline computed at install time." [`Baseline::compute`] walks a root, picks the files a [`Selection`] names and records
//! their digests. The baseline is then only as trustworthy as the way it is stored. Written plainly, anyone who can change a
//! guarded file can change the baseline beside it, so the guard would compare a tampered file with a tampered record and report
//! nothing. The defence is a signature made elsewhere (the installer, with a key SUL embeds) over the baseline's exact bytes:
//! [`Baseline::from_signed_json`] checks the signature first and only then reads the text, so unsigned or altered bytes are
//! never parsed into something that looks trusted.
//!
//! What is **not** built, and so what this does not give you yet: who signs and where the public key lives (the installer and
//! the update channel decide that), and therefore which files the installed product's baseline holds. Until then a baseline
//! can be computed, saved, checked and signed in tests, and an unsigned baseline is only as good as the folder it sits in.
//! This crate does not invent the install layout, so there is no built-in selection of "SUL's files".
//!
//! Paths are forward-slash, relative to the root, and are compared without regard to case on Windows (NTFS does not tell
//! `Sul.exe` from `sul.exe`).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use sul_core::fsops::{self, ProtectedRoots};
use sul_core::{Error, Result, error::io, safepath};
use sul_trust::manifest::{hash_file, now_unix};
use sul_trust::{TrustError, verify_ed25519};

pub const BASELINE_SCHEMA: u32 = 1;

/// What a guarded file is, which decides how it is reported and how often it is looked at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// SUL's own programs and libraries.
    Binary,
    /// A helper program SUL starts.
    Sidecar,
    /// A manifest of other files.
    Manifest,
    /// The code of the trust layer itself (`sul-trust`, `sul-quarantine`, `sul-watchdog`).
    TrustPrimitive,
    /// A model or adapter. Large, so it is verified at session start and when it changes, not on a timer.
    ModelFile,
    Other,
}

impl Role {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Binary => "binary",
            Self::Sidecar => "sidecar",
            Self::Manifest => "manifest",
            Self::TrustPrimitive => "trust primitive",
            Self::ModelFile => "model file",
            Self::Other => "file",
        }
    }

    /// Whether the periodic full check re-reads this kind of file. Model files are gigabytes; hashing them every few minutes
    /// would cost more than it protects, because the change events already catch a write the moment it happens.
    pub const fn in_periodic_rescan(self) -> bool {
        !matches!(self, Self::ModelFile)
    }
}

/// One guarded file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Watched {
    /// Forward-slash path relative to the baseline's root.
    pub path: String,
    pub sha256: String,
    pub size: u64,
    pub role: Role,
    /// The owning module's own code for a modified artifact (`E_VAULT_TAMPER_DETECTED`, ...), when it has one (Ch. 14
    /// §14.14.1: "or the artifact's specific error code"). The watchdog reports it beside `E_WATCHDOG_TAMPER`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Baseline {
    pub schema_version: u32,
    /// A short name for the root this describes ("sul-install"). Free text, no secrets.
    pub label: String,
    pub created_unix: u64,
    /// Sorted by path, no two the same.
    pub entries: Vec<Watched>,
}

/// How a rule recognises a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pattern {
    /// This one path.
    Exact(String),
    /// Every file with this extension, without the dot, in any case (`"dll"`).
    Extension(String),
    /// Every file under this folder (`"models/"`).
    Prefix(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub pattern: Pattern,
    pub role: Role,
    pub artifact_code: Option<String>,
}

/// Which files under a root are guarded. The first rule that matches a file decides its role; a file no rule matches is not
/// guarded at all.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    rules: Vec<Rule>,
}

/// The form in which two paths are compared: forward slashes, and lower case on Windows.
pub fn key(rel: &str) -> String {
    let rel = rel.replace('\\', "/");
    if cfg!(windows) {
        rel.to_lowercase()
    } else {
        rel
    }
}

fn extension_of(rel: &str) -> Option<&str> {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    match name.rfind('.') {
        Some(0) | None => None,
        Some(i) => Some(&name[i + 1..]),
    }
}

impl Selection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn rule(mut self, pattern: Pattern, role: Role) -> Self {
        self.rules.push(Rule {
            pattern,
            role,
            artifact_code: None,
        });
        self
    }

    /// A rule whose files report the owning module's own code as well (see [`Watched::artifact_code`]).
    pub fn rule_with_code(
        mut self,
        pattern: Pattern,
        role: Role,
        artifact_code: impl Into<String>,
    ) -> Self {
        self.rules.push(Rule {
            pattern,
            role,
            artifact_code: Some(artifact_code.into()),
        });
        self
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// The first rule that matches `rel`, if any.
    pub fn matching(&self, rel: &str) -> Option<&Rule> {
        let norm = key(rel);
        self.rules.iter().find(|rule| match &rule.pattern {
            Pattern::Exact(p) => key(p) == norm,
            Pattern::Extension(ext) => extension_of(&norm)
                .is_some_and(|e| e.eq_ignore_ascii_case(ext.trim_start_matches('.'))),
            Pattern::Prefix(dir) => {
                let mut dir = key(dir);
                if !dir.ends_with('/') {
                    dir.push('/');
                }
                norm.starts_with(&dir)
            }
        })
    }
}

/// Every candidate file under `root` that `selection` matches, by normalised path, as (relative path, full path). A symlink
/// or junction is returned as a candidate too (it is a file name that matches a rule); callers decide what it means.
/// Folders that are links are not entered.
pub(crate) fn walk_selected(
    root: &Path,
    selection: &Selection,
) -> Result<BTreeMap<String, (String, PathBuf)>> {
    let mut found = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).map_err(io(&dir))? {
            let entry = entry.map_err(io(&dir))?;
            let path = entry.path();
            let file_type = entry.file_type().map_err(io(&path))?;
            if file_type.is_dir() {
                stack.push(path);
                continue;
            }
            let rel = match path.strip_prefix(root) {
                Ok(r) => r
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/"),
                Err(_) => continue,
            };
            if selection.matching(&rel).is_some() {
                found.insert(key(&rel), (rel, path));
            }
        }
    }
    Ok(found)
}

fn valid_artifact_code(code: &str) -> bool {
    code.starts_with("E_")
        && code.len() > 2
        && code.len() <= 64
        && code
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

impl Baseline {
    /// Record the digests of every file under `root` that `selection` matches. A symlink that matches is an error: a guarded
    /// file must be a plain file, or the baseline would describe whatever the link pointed at on that day.
    pub fn compute(
        root: &Path,
        label: &str,
        selection: &Selection,
        created_unix: u64,
    ) -> Result<Self> {
        if selection.is_empty() {
            return Err(Error::Invalid(
                "a baseline needs at least one selection rule".into(),
            ));
        }
        let mut entries = Vec::new();
        for (_, (rel, path)) in walk_selected(root, selection)? {
            let meta = std::fs::symlink_metadata(&path).map_err(io(&path))?;
            if !meta.is_file() {
                return Err(Error::UnsafePath(format!(
                    "{rel} is a link or not a plain file, so it cannot be guarded"
                )));
            }
            let rule = selection
                .matching(&rel)
                .expect("walk_selected only returns matches");
            let (sha256, size) = hash_file(&path)?;
            entries.push(Watched {
                path: rel,
                sha256,
                size,
                role: rule.role,
                artifact_code: rule.artifact_code.clone(),
            });
        }
        let baseline = Self {
            schema_version: BASELINE_SCHEMA,
            label: label.to_owned(),
            created_unix,
            entries,
        };
        baseline.validate()?;
        Ok(baseline)
    }

    /// [`compute`](Self::compute) stamped with the current time.
    pub fn compute_now(root: &Path, label: &str, selection: &Selection) -> Result<Self> {
        Self::compute(root, label, selection, now_unix())
    }

    pub fn get(&self, rel: &str) -> Option<&Watched> {
        let wanted = key(rel);
        self.entries.iter().find(|e| key(&e.path) == wanted)
    }

    /// Check everything a reader relies on: the schema, safe relative paths, sorted and unique, real digests, sane codes.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != BASELINE_SCHEMA {
            return Err(Error::Invalid(format!(
                "baseline schema {} is not the supported schema {BASELINE_SCHEMA}",
                self.schema_version
            )));
        }
        if self.label.is_empty()
            || self.label.len() > 64
            || self.label.chars().any(char::is_control)
        {
            return Err(Error::Invalid(
                "baseline label must be 1 to 64 printable characters".into(),
            ));
        }
        let mut previous: Option<String> = None;
        for entry in &self.entries {
            safepath::validate_relative(&entry.path)?;
            if entry.path.contains('\\') {
                return Err(Error::UnsafePath(format!(
                    "baseline path {:?} must use forward slashes",
                    entry.path
                )));
            }
            if entry.sha256.len() != 64
                || !entry
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(Error::Invalid(format!(
                    "baseline entry {:?} has a digest that is not 64 lower-case hex digits",
                    entry.path
                )));
            }
            // (No let-chain here: they need Rust 1.88 and the workspace's MSRV is 1.85.)
            if let Some(code) = &entry.artifact_code {
                if !valid_artifact_code(code) {
                    return Err(Error::Invalid(format!(
                        "baseline entry {:?} has an artifact code that is not an E_ code: {code:?}",
                        entry.path
                    )));
                }
            }
            let this = key(&entry.path);
            if previous.as_ref().is_some_and(|p| *p >= this) {
                return Err(Error::Invalid(format!(
                    "baseline entries must be sorted by path with no repeats (at {:?})",
                    entry.path
                )));
            }
            previous = Some(this);
        }
        Ok(())
    }

    /// The exact bytes a signer signs and [`from_signed_json`](Self::from_signed_json) checks: compact JSON, fields in
    /// declaration order, entries in sorted order. The same baseline always gives the same bytes.
    pub fn to_json(&self) -> Result<String> {
        self.validate()?;
        Ok(serde_json::to_string(self)?)
    }

    /// Read a baseline that is **not** signed. It is checked for shape ([`validate`](Self::validate)) and for nothing else:
    /// use it for a baseline that was only just computed here, never for one found on disk that should be trusted.
    pub fn from_json_unsigned(text: &str) -> Result<Self> {
        let baseline: Self = serde_json::from_str(text)?;
        baseline.validate()?;
        Ok(baseline)
    }

    /// Check the Ed25519 signature over `raw` first, and only then read it. Any failure is
    /// [`TrustError::InvalidSignature`] (as an error here, so nothing is parsed from bytes that were not signed).
    pub fn from_signed_json(
        raw: &[u8],
        signature: &[u8; 64],
        public_key: &[u8; 32],
    ) -> Result<Self> {
        verify_ed25519(raw, signature, public_key)
            .map_err(|e: TrustError| Error::Invalid(format!("baseline signature refused: {e}")))?;
        let text = std::str::from_utf8(raw)
            .map_err(|_| Error::Invalid("signed baseline is not UTF-8".into()))?;
        Self::from_json_unsigned(text)
    }

    /// Save atomically (temp file, then rename) under a root that `protected` does not forbid.
    pub fn save(&self, protected: &ProtectedRoots, path: &Path) -> Result<()> {
        fsops::atomic_write(protected, path, self.to_json()?.as_bytes())
    }

    /// Read a saved baseline without checking a signature (see [`from_json_unsigned`](Self::from_json_unsigned)).
    pub fn load_unsigned(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(io(path))?;
        Self::from_json_unsigned(&text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::fs;

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::create_dir_all(root.join("models")).unwrap();
        fs::write(root.join("sul.exe"), b"the program").unwrap();
        fs::write(root.join("bin/helper.dll"), b"a library").unwrap();
        fs::write(root.join("bin/notes.txt"), b"not guarded").unwrap();
        fs::write(root.join("models/jarvis.gguf"), b"weights").unwrap();
        dir
    }

    fn selection() -> Selection {
        Selection::new()
            .rule(Pattern::Prefix("models/".into()), Role::ModelFile)
            .rule(Pattern::Exact("sul.exe".into()), Role::TrustPrimitive)
            .rule(Pattern::Extension("dll".into()), Role::Binary)
    }

    #[test]
    fn compute_picks_only_what_the_rules_name_and_sorts_it() {
        let dir = tree();
        let b = Baseline::compute(dir.path(), "test", &selection(), 1).unwrap();
        let paths: Vec<_> = b.entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["bin/helper.dll", "models/jarvis.gguf", "sul.exe"]);
        assert_eq!(b.get("sul.exe").unwrap().role, Role::TrustPrimitive);
        assert_eq!(b.get("sul.exe").unwrap().size, 11);
        assert_eq!(b.get("models/jarvis.gguf").unwrap().role, Role::ModelFile);
        assert!(b.get("bin/notes.txt").is_none());
        // The digest is the real SHA-256 of "the program" (computed with Python's hashlib, not with this code).
        assert_eq!(
            b.get("sul.exe").unwrap().sha256,
            "50fda23bf0a5115d4f893dc10d122c10a4705f47688f5620e80611c092d4053e"
        );
    }

    #[test]
    fn the_first_matching_rule_wins() {
        let sel = Selection::new()
            .rule(
                Pattern::Exact("bin/special.dll".into()),
                Role::TrustPrimitive,
            )
            .rule(Pattern::Extension("dll".into()), Role::Binary);
        assert_eq!(
            sel.matching("bin/special.dll").unwrap().role,
            Role::TrustPrimitive
        );
        assert_eq!(sel.matching("bin/other.dll").unwrap().role, Role::Binary);
        assert_eq!(sel.matching("bin/other.DLL").unwrap().role, Role::Binary);
        assert!(sel.matching("bin/other.dll.txt").is_none());
        assert!(
            sel.matching(".dll").is_none(),
            "a dot-file has no extension"
        );
    }

    #[test]
    fn a_prefix_means_a_folder_not_a_name_that_starts_the_same() {
        let sel = Selection::new().rule(Pattern::Prefix("models".into()), Role::ModelFile);
        assert!(sel.matching("models/a.gguf").is_some());
        assert!(sel.matching("models-old/a.gguf").is_none());
        assert!(sel.matching("models").is_none());
    }

    #[test]
    fn an_empty_selection_and_a_linked_file_are_refused() {
        let dir = tree();
        assert!(Baseline::compute(dir.path(), "t", &Selection::new(), 1).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path().join("sul.exe"), dir.path().join("link.dll"))
                .unwrap();
            let err = Baseline::compute(dir.path(), "t", &selection(), 1).unwrap_err();
            assert!(matches!(err, Error::UnsafePath(_)), "{err}");
        }
    }

    #[test]
    fn artifact_codes_are_carried_and_checked() {
        let dir = tree();
        let sel = Selection::new().rule_with_code(
            Pattern::Prefix("models/".into()),
            Role::ModelFile,
            "E_MODEL_TAMPER_DETECTED",
        );
        let b = Baseline::compute(dir.path(), "t", &sel, 1).unwrap();
        assert_eq!(
            b.entries[0].artifact_code.as_deref(),
            Some("E_MODEL_TAMPER_DETECTED")
        );
        let mut bad = b.clone();
        bad.entries[0].artifact_code = Some("tamper".into());
        assert!(bad.validate().is_err());
        bad.entries[0].artifact_code = Some("E_".into());
        assert!(bad.validate().is_err());
    }

    #[test]
    fn validate_refuses_every_malformed_baseline() {
        let dir = tree();
        let good = Baseline::compute(dir.path(), "t", &selection(), 1).unwrap();
        assert!(good.validate().is_ok());

        let mut b = good.clone();
        b.schema_version = 2;
        assert!(b.validate().is_err(), "newer schema");
        let mut b = good.clone();
        b.label = String::new();
        assert!(b.validate().is_err(), "empty label");
        let mut b = good.clone();
        b.entries[0].path = "../escape.dll".into();
        assert!(b.validate().is_err(), "path climbs out");
        let mut b = good.clone();
        b.entries[0].path = "C:/Windows/x.dll".into();
        assert!(b.validate().is_err(), "absolute path");
        let mut b = good.clone();
        b.entries[0].sha256 = "ABC".into();
        assert!(b.validate().is_err(), "short digest");
        let mut b = good.clone();
        b.entries[0].sha256 = "G".repeat(64);
        assert!(b.validate().is_err(), "non-hex digest");
        let mut b = good.clone();
        b.entries[0].sha256 = good.entries[0].sha256.to_uppercase();
        assert!(b.validate().is_err(), "upper-case digest");
        let mut b = good.clone();
        b.entries.swap(0, 1);
        assert!(b.validate().is_err(), "unsorted");
        let mut b = good.clone();
        b.entries.push(good.entries[0].clone());
        assert!(b.validate().is_err(), "a repeat at the end");
        // Found by a deliberate-bug check, 2026-11-26: the case above is also out of order, so the sort check refused it
        // whether or not repeats were checked. A repeat right beside its original is in order and only a repeat check sees it.
        let mut b = good.clone();
        b.entries.insert(1, good.entries[0].clone());
        assert!(b.validate().is_err(), "a repeat right beside the original");
    }

    #[test]
    fn the_json_is_stable_and_reads_back() {
        let dir = tree();
        let b = Baseline::compute(dir.path(), "t", &selection(), 7).unwrap();
        let text = b.to_json().unwrap();
        assert_eq!(
            text,
            b.to_json().unwrap(),
            "the same baseline gives the same bytes"
        );
        assert!(
            text.starts_with(
                "{\"schema_version\":1,\"label\":\"t\",\"created_unix\":7,\"entries\":["
            )
        );
        assert_eq!(Baseline::from_json_unsigned(&text).unwrap(), b);
        assert!(Baseline::from_json_unsigned("{}").is_err());
    }

    #[test]
    fn a_signed_baseline_is_read_only_after_its_signature_checks() {
        let dir = tree();
        let b = Baseline::compute(dir.path(), "t", &selection(), 7).unwrap();
        let json = b.to_json().unwrap();
        let signing = SigningKey::from_bytes(&[9u8; 32]);
        let public = signing.verifying_key().to_bytes();
        let signature = signing.sign(json.as_bytes()).to_bytes();

        assert_eq!(
            Baseline::from_signed_json(json.as_bytes(), &signature, &public).unwrap(),
            b
        );

        // One changed digit in a guarded file's record: the signature no longer matches, so nothing is parsed.
        let tampered = json.replacen(&b.entries[0].sha256[..1], "0", 1);
        assert_ne!(tampered, json);
        let err = Baseline::from_signed_json(tampered.as_bytes(), &signature, &public).unwrap_err();
        assert!(err.to_string().contains("signature refused"), "{err}");

        // Whitespace added: the bytes differ, so the signature fails even though the JSON means the same thing.
        let spaced = format!("{json} ");
        assert!(Baseline::from_signed_json(spaced.as_bytes(), &signature, &public).is_err());

        // Another signer's key.
        let other = SigningKey::from_bytes(&[10u8; 32])
            .verifying_key()
            .to_bytes();
        assert!(Baseline::from_signed_json(json.as_bytes(), &signature, &other).is_err());

        // A valid signature over bytes that are not a valid baseline is still refused, after the signature check.
        let junk = b"{\"schema_version\":1}";
        let junk_sig = signing.sign(junk).to_bytes();
        let err = Baseline::from_signed_json(junk, &junk_sig, &public).unwrap_err();
        assert!(!err.to_string().contains("signature refused"), "{err}");
    }

    #[test]
    fn save_and_load_round_trip_and_respect_protected_roots() {
        let dir = tree();
        let b = Baseline::compute(dir.path(), "t", &selection(), 7).unwrap();
        let out = tempfile::tempdir().unwrap();
        let file = out.path().join("baseline.json");
        b.save(&ProtectedRoots::new(), &file).unwrap();
        assert_eq!(Baseline::load_unsigned(&file).unwrap(), b);

        let mut protected = ProtectedRoots::new();
        protected.add(out.path()).unwrap();
        let err = b
            .save(&protected, &out.path().join("other.json"))
            .unwrap_err();
        assert!(matches!(err, Error::Protected(_)), "{err}");
    }

    #[test]
    fn keys_ignore_case_only_on_windows() {
        assert_eq!(
            key("Bin\\Sul.exe"),
            if cfg!(windows) {
                "bin/sul.exe"
            } else {
                "Bin/Sul.exe"
            }
        );
    }
}
