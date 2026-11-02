//! Build manifests: inventory + SHA-256 of a build directory, and read-only verification.
//! Scanning and verifying never modify the directory.
//!
//! # Schema versions and migration (owner decision, 2026-11-01)
//!
//! The manifest stays a plain JSON file with an integer `schema_version`. Migration is forward only:
//!
//! * a file with a **newer** version than this build knows is refused, never guessed at;
//! * a file with an **older** version is upgraded in memory one step at a time ([`migrate_value`]), and
//!   [`BuildManifest::load_file`] first keeps an exact copy of the old bytes beside it
//!   ([`fsops::backup_beside`]) and only then replaces the file atomically;
//! * a file with no readable version, or version 0, is refused.
//!
//! Version 1 had the file inventory, variant, base version, a free-text source, the state and notes.
//! Version 2 adds what spec item 1.3 asks a build record to hold: a source or manifest id, the patches
//! applied, the mod profiles that reference the build, compatibility notes, and verification and
//! update timestamps.

use crate::fsops::{self, ProtectedRoots};
use crate::{Error, Result, error::io, safepath};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Variant {
    Original,
    Optimized,
    Legacy,
    Experimental,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Being copied or assembled; not usable.
    Staging,
    Unverified,
    Ready,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// Forward-slash relative path.
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

/// A modification recorded against a build (spec 1.3: "patches/modifications").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchRef {
    pub id: String,
    pub description: String,
    pub applied_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildManifest {
    pub schema_version: u32,
    pub game_id: String,
    pub display_name: String,
    pub variant: Variant,
    pub base_version: String,
    /// Where the build came from (user-supplied description). Free text, no secrets.
    pub source: String,
    /// A store or depot manifest id for the source, when there is one. Added in schema 2.
    pub source_manifest_id: Option<String>,
    pub files: Vec<FileEntry>,
    /// Patches applied to the files after they were copied. Added in schema 2.
    pub patches: Vec<PatchRef>,
    /// Ids of the mod profiles that reference this build. Added in schema 2.
    pub profiles: Vec<String>,
    /// Free-text compatibility notes written by a person or a test; never a claim by itself. Added in schema 2.
    pub compatibility_notes: String,
    pub created_unix: u64,
    /// When the files were last compared with the inventory, if ever. Added in schema 2.
    pub verified_unix: Option<u64>,
    /// When this record last changed. Added in schema 2.
    pub updated_unix: u64,
    pub state: State,
    pub notes: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyReport {
    pub missing: Vec<String>,
    pub modified: Vec<String>,
    pub extra: Vec<String>,
}

impl VerifyReport {
    pub fn is_clean(&self) -> bool {
        self.missing.is_empty() && self.modified.is_empty() && self.extra.is_empty()
    }
}

pub fn hash_file(path: &Path) -> Result<(String, u64)> {
    let mut f = File::open(path).map_err(io(path))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut total = 0u64;
    loop {
        let n = f.read(&mut buf).map_err(io(path))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        total += n as u64;
    }
    Ok((hex::encode(h.finalize()), total))
}

/// Walk `root` and return sorted entries. Symlinks are rejected: managed builds must be plain
/// files so a build can't smuggle in content from elsewhere.
pub(crate) fn walk(root: &Path) -> Result<BTreeMap<String, std::path::PathBuf>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).map_err(io(&dir))? {
            let entry = entry.map_err(io(&dir))?;
            let p = entry.path();
            let ft = entry.file_type().map_err(io(&p))?;
            if ft.is_symlink() {
                return Err(Error::UnsafePath(format!(
                    "symlink in managed build: {}",
                    p.display()
                )));
            }
            if ft.is_dir() {
                stack.push(p);
            } else if ft.is_file() {
                let rel = p
                    .strip_prefix(root)
                    .unwrap()
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                out.insert(rel, p);
            }
        }
    }
    Ok(out)
}

/// The sorted inventory (path, SHA-256, size) of every plain file under `root`. A symlink anywhere is an error.
pub(crate) fn inventory(root: &Path) -> Result<Vec<FileEntry>> {
    let mut files = Vec::new();
    for (rel, p) in walk(root)? {
        let (sha256, size) = hash_file(&p)?;
        files.push(FileEntry {
            path: rel,
            sha256,
            size,
        });
    }
    Ok(files)
}

impl BuildManifest {
    pub fn scan(
        root: &Path,
        game_id: &str,
        display_name: &str,
        variant: Variant,
        base_version: &str,
        source: &str,
    ) -> Result<Self> {
        let mut files = Vec::new();
        for (rel, p) in walk(root)? {
            let (sha256, size) = hash_file(&p)?;
            files.push(FileEntry {
                path: rel,
                sha256,
                size,
            });
        }
        let created_unix = now_unix();
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            game_id: game_id.into(),
            display_name: display_name.into(),
            variant,
            base_version: base_version.into(),
            source: source.into(),
            source_manifest_id: None,
            files,
            patches: Vec::new(),
            profiles: Vec::new(),
            compatibility_notes: String::new(),
            created_unix,
            verified_unix: None,
            updated_unix: created_unix,
            state: State::Unverified,
            notes: String::new(),
        })
    }

    /// Record that the record changed now.
    pub fn touch(&mut self) {
        self.updated_unix = now_unix();
    }

    /// Compare a directory against this manifest. Read-only.
    pub fn verify(&self, root: &Path) -> Result<VerifyReport> {
        let mut report = VerifyReport::default();
        let on_disk = walk(root)?;
        for f in &self.files {
            safepath::validate_relative(&f.path)?; // manifests from outside are untrusted
            match on_disk.get(&f.path) {
                None => report.missing.push(f.path.clone()),
                Some(p) => {
                    let (h, size) = hash_file(p)?;
                    if h != f.sha256 || size != f.size {
                        report.modified.push(f.path.clone());
                    }
                }
            }
        }
        let known: std::collections::HashSet<&str> =
            self.files.iter().map(|f| f.path.as_str()).collect();
        for k in on_disk.keys() {
            if !known.contains(k.as_str()) {
                report.extra.push(k.clone());
            }
        }
        Ok(report)
    }

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Parse manifest text of any known schema version. Older versions are upgraded in memory only;
    /// nothing is written. A newer or unreadable version is refused.
    pub fn from_json(s: &str) -> Result<Self> {
        Ok(Self::from_json_migrating(s)?.0)
    }

    /// Like [`Self::from_json`], also returning the version the text had when it was older than the
    /// current one (`None` when no migration was needed).
    pub fn from_json_migrating(s: &str) -> Result<(Self, Option<u32>)> {
        let mut value: serde_json::Value = serde_json::from_str(s)?;
        let found = migrate_value(&mut value)?;
        let m: Self = serde_json::from_value(value)?;
        m.validate()?;
        Ok((m, found))
    }

    /// Checks that hold for every manifest, whatever produced it: manifests from outside are untrusted.
    ///
    /// Every path must be a safe relative path, no two paths may be the same file on Windows (the
    /// comparison ignores letter case), and every hash must be 64 lowercase hex digits.
    pub fn validate(&self) -> Result<()> {
        let mut seen = HashSet::new();
        for f in &self.files {
            safepath::validate_relative(&f.path)?;
            if !seen.insert(f.path.to_ascii_lowercase()) {
                return Err(Error::Invalid(format!(
                    "manifest lists {} twice (paths that differ only by letter case are one file on Windows)",
                    f.path
                )));
            }
            let hex_ok = f.sha256.len() == 64
                && f.sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
            if !hex_ok {
                return Err(Error::Invalid(format!(
                    "manifest hash for {} is not 64 lowercase hex digits",
                    f.path
                )));
            }
        }
        Ok(())
    }

    /// Read a manifest file, upgrading it to the current schema on disk when it is older.
    ///
    /// Order of events for an older file: parse and migrate in memory (nothing written yet); keep the
    /// exact old bytes in a new `.v<N>.bak` file beside it; replace the file atomically. A failure at any
    /// point before the last step leaves the original file as it was. Returns the version it was upgraded
    /// from, if any.
    pub fn load_file(protected: &ProtectedRoots, path: &Path) -> Result<(Self, Option<u32>)> {
        let bytes = std::fs::read(path).map_err(io(path))?;
        let text = String::from_utf8(bytes.clone())
            .map_err(|_| Error::Invalid(format!("{} is not valid UTF-8", path.display())))?;
        let (m, from) = Self::from_json_migrating(&text).map_err(|e| match e {
            Error::Serde(source) => Error::json(path, source),
            other => other,
        })?;
        if let Some(old) = from {
            fsops::backup_beside(protected, path, &format!("v{old}.bak"), &bytes)?;
            fsops::atomic_write(protected, path, m.to_json()?.as_bytes())?;
        }
        Ok((m, from))
    }

    /// Write this manifest atomically (temp file in the same folder, then rename).
    pub fn save_file(&self, protected: &ProtectedRoots, path: &Path) -> Result<()> {
        self.validate()?;
        fsops::atomic_write(protected, path, self.to_json()?.as_bytes())
    }
}

pub(crate) fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Bring a manifest JSON value up to [`SCHEMA_VERSION`], one version step at a time.
///
/// Returns the version the value had if it had to be upgraded, `None` if it was already current.
/// Refuses a value that is not an object, has no whole-number `schema_version`, has version 0, or
/// has a version newer than this build knows.
pub fn migrate_value(value: &mut serde_json::Value) -> Result<Option<u32>> {
    let obj = value
        .as_object_mut()
        .ok_or_else(|| Error::Invalid("manifest is not a JSON object".into()))?;
    let found = obj
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| Error::Invalid("manifest has no readable schema_version".into()))?;
    if found == 0 {
        return Err(Error::Invalid("manifest schema 0 does not exist".into()));
    }
    if found > SCHEMA_VERSION {
        return Err(Error::Invalid(format!(
            "manifest schema {found} is newer than supported {SCHEMA_VERSION}"
        )));
    }
    let mut at = found;
    while at < SCHEMA_VERSION {
        match at {
            1 => v1_to_v2(obj),
            // A new schema adds an arm here and raises SCHEMA_VERSION; the loop never skips a step.
            _ => {
                return Err(Error::Invalid(format!(
                    "no migration from manifest schema {at}"
                )));
            }
        }
        at += 1;
        obj.insert("schema_version".into(), at.into());
    }
    Ok((found < SCHEMA_VERSION).then_some(found))
}

/// Schema 1 to 2: add the 1.3 fields with empty values. Existing values are never changed.
fn v1_to_v2(obj: &mut serde_json::Map<String, serde_json::Value>) {
    use serde_json::json;
    let created = obj.get("created_unix").cloned().unwrap_or_else(|| json!(0));
    obj.entry("source_manifest_id").or_insert(json!(null));
    obj.entry("patches").or_insert(json!([]));
    obj.entry("profiles").or_insert(json!([]));
    obj.entry("compatibility_notes").or_insert(json!(""));
    obj.entry("verified_unix").or_insert(json!(null));
    obj.entry("updated_unix").or_insert(created);
}

/// Where a manifest file for build `id` lives next to its build folder: `<dir>/<id>.manifest.json`.
pub fn manifest_path_for(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.manifest.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A schema 1 manifest exactly as the previous version of SUL wrote it.
    fn v1_text() -> String {
        json!({
            "schema_version": 1,
            "game_id": "terraria",
            "display_name": "Terraria",
            "variant": "legacy",
            "base_version": "1.4.4",
            "source": "my own copy",
            "files": [
                {"path": "game.exe", "sha256": "a".repeat(64), "size": 10},
                {"path": "data/x.bin", "sha256": "b".repeat(64), "size": 20}
            ],
            "created_unix": 1700000000u64,
            "state": "ready",
            "notes": "keep this"
        })
        .to_string()
    }

    fn open_protected() -> ProtectedRoots {
        ProtectedRoots::new()
    }

    #[test]
    fn a_schema_1_file_is_upgraded_and_every_old_field_survives() {
        let (m, from) = BuildManifest::from_json_migrating(&v1_text()).unwrap();
        assert_eq!(from, Some(1));
        assert_eq!(m.schema_version, 2);
        assert_eq!(m.game_id, "terraria");
        assert_eq!(m.display_name, "Terraria");
        assert_eq!(m.variant, Variant::Legacy);
        assert_eq!(m.base_version, "1.4.4");
        assert_eq!(m.source, "my own copy");
        assert_eq!(m.files.len(), 2);
        assert_eq!(m.files[1].path, "data/x.bin");
        assert_eq!(m.files[1].size, 20);
        assert_eq!(m.created_unix, 1_700_000_000);
        assert_eq!(m.state, State::Ready);
        assert_eq!(m.notes, "keep this");
    }

    #[test]
    fn the_new_fields_start_empty_and_updated_starts_at_created() {
        let (m, _) = BuildManifest::from_json_migrating(&v1_text()).unwrap();
        assert_eq!(m.source_manifest_id, None);
        assert!(m.patches.is_empty());
        assert!(m.profiles.is_empty());
        assert_eq!(m.compatibility_notes, "");
        assert_eq!(m.verified_unix, None);
        assert_eq!(m.updated_unix, m.created_unix);
    }

    #[test]
    fn a_current_file_needs_no_migration_and_round_trips() {
        let (m, _) = BuildManifest::from_json_migrating(&v1_text()).unwrap();
        let (again, from) = BuildManifest::from_json_migrating(&m.to_json().unwrap()).unwrap();
        assert_eq!(from, None);
        assert_eq!(again, m);
    }

    #[test]
    fn a_schema_1_file_is_not_accepted_without_the_migration_step() {
        // Proves the new fields are filled by the migration and not by quiet serde defaults.
        assert!(serde_json::from_str::<BuildManifest>(&v1_text()).is_err());
    }

    #[test]
    fn newer_zero_missing_and_odd_versions_are_refused() {
        // A body that is otherwise a valid current manifest, so only the version can be the reason.
        let current = BuildManifest::from_json(&v1_text())
            .unwrap()
            .to_json()
            .unwrap();
        let base: serde_json::Value = serde_json::from_str(&current).unwrap();
        assert!(BuildManifest::from_json(&base.to_string()).is_ok());
        for bad in [
            json!(3),
            json!(999),
            json!(0),
            json!(-1),
            json!("2"),
            json!(1.5),
            json!(null),
        ] {
            let mut v = base.clone();
            v["schema_version"] = bad.clone();
            assert!(
                BuildManifest::from_json(&v.to_string()).is_err(),
                "{bad} should be refused"
            );
        }
        let mut v = base.clone();
        v.as_object_mut().unwrap().remove("schema_version");
        assert!(BuildManifest::from_json(&v.to_string()).is_err());
        assert!(BuildManifest::from_json("[]").is_err());
        assert!(BuildManifest::from_json("not json").is_err());
    }

    #[test]
    fn the_migration_step_never_changes_a_value_that_is_already_there() {
        let mut v: serde_json::Value = serde_json::from_str(&v1_text()).unwrap();
        v["patches"] = json!([{"id": "p", "description": "d", "applied_unix": 5}]);
        v["compatibility_notes"] = json!("kept");
        migrate_value(&mut v).unwrap();
        assert_eq!(v["patches"][0]["id"], "p");
        assert_eq!(v["compatibility_notes"], "kept");
    }

    #[test]
    fn bad_paths_duplicates_and_hashes_are_refused() {
        let mut v: serde_json::Value = serde_json::from_str(&v1_text()).unwrap();
        v["files"][0]["path"] = json!("../evil");
        assert!(matches!(
            BuildManifest::from_json(&v.to_string()),
            Err(Error::UnsafePath(_))
        ));

        let mut v: serde_json::Value = serde_json::from_str(&v1_text()).unwrap();
        v["files"][1]["path"] = json!("GAME.EXE");
        assert!(matches!(
            BuildManifest::from_json(&v.to_string()),
            Err(Error::Invalid(m)) if m.contains("twice")
        ));

        for bad_hash in ["00", &"A".repeat(64), &"g".repeat(64), &"a".repeat(65)] {
            let mut v: serde_json::Value = serde_json::from_str(&v1_text()).unwrap();
            v["files"][0]["sha256"] = json!(bad_hash);
            assert!(
                BuildManifest::from_json(&v.to_string()).is_err(),
                "{bad_hash:?} should be refused"
            );
        }
    }

    #[test]
    fn load_file_keeps_the_old_bytes_then_upgrades_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("b.manifest.json");
        std::fs::write(&path, v1_text()).unwrap();

        let (m, from) = BuildManifest::load_file(&open_protected(), &path).unwrap();
        assert_eq!(from, Some(1));

        let backup = dir.path().join("b.manifest.json.v1.bak");
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), v1_text());
        let on_disk: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(on_disk["schema_version"], 2);
        assert_eq!(
            BuildManifest::from_json(&std::fs::read_to_string(&path).unwrap()).unwrap(),
            m
        );

        // A second load has nothing to do and makes no new backup.
        let (_, again) = BuildManifest::load_file(&open_protected(), &path).unwrap();
        assert_eq!(again, None);
        let backups: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".bak"))
            .collect();
        assert_eq!(backups, vec!["b.manifest.json.v1.bak"]);
    }

    #[test]
    fn an_existing_backup_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("b.manifest.json");
        let earlier = dir.path().join("b.manifest.json.v1.bak");
        std::fs::write(&earlier, "an earlier backup").unwrap();
        std::fs::write(&path, v1_text()).unwrap();

        BuildManifest::load_file(&open_protected(), &path).unwrap();

        assert_eq!(
            std::fs::read_to_string(&earlier).unwrap(),
            "an earlier backup"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("b.manifest.json.v1.bak.1")).unwrap(),
            v1_text()
        );
    }

    #[test]
    fn a_file_that_cannot_be_migrated_is_left_exactly_as_it_was() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("b.manifest.json");
        let mut v: serde_json::Value = serde_json::from_str(&v1_text()).unwrap();
        v["files"][0]["path"] = json!("../evil");
        let text = v.to_string();
        std::fs::write(&path, &text).unwrap();

        assert!(BuildManifest::load_file(&open_protected(), &path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            1,
            "no backup, no temp file"
        );

        let newer = dir.path().join("newer.json");
        std::fs::write(&newer, json!({"schema_version": 99}).to_string()).unwrap();
        assert!(BuildManifest::load_file(&open_protected(), &newer).is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn nothing_is_written_into_a_protected_root_not_even_a_backup() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("original_install");
        std::fs::create_dir_all(&original).unwrap();
        let path = original.join("b.manifest.json");
        std::fs::write(&path, v1_text()).unwrap();
        let mut protected = ProtectedRoots::new();
        protected.add(&original).unwrap();

        assert!(matches!(
            BuildManifest::load_file(&protected, &path),
            Err(Error::Protected(_))
        ));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), v1_text());
        assert_eq!(std::fs::read_dir(&original).unwrap().count(), 1);

        let (m, _) = BuildManifest::from_json_migrating(&v1_text()).unwrap();
        assert!(matches!(
            m.save_file(&protected, &path),
            Err(Error::Protected(_))
        ));
    }

    #[test]
    fn save_file_refuses_an_invalid_manifest() {
        let (mut m, _) = BuildManifest::from_json_migrating(&v1_text()).unwrap();
        m.files[0].sha256 = "short".into();
        let dir = tempfile::tempdir().unwrap();
        assert!(
            m.save_file(&open_protected(), &dir.path().join("x.json"))
                .is_err()
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn the_backup_tag_cannot_climb_out_of_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("b.json");
        for bad in ["", "../x", "a/b", "a\\b"] {
            assert!(
                fsops::backup_beside(&open_protected(), &path, bad, b"x").is_err(),
                "{bad:?}"
            );
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
