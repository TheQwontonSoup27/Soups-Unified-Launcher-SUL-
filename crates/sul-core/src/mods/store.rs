//! The mod store: mods the owner imported, each one a verified copy in a library root of kind `Mods`.
//!
//! Layout inside the root: `<game id>/<mod id>/<version>/<the mod's files>`. A version is never imported over
//! another. An import is built in a hidden staging folder beside its final place, compared with the source
//! file by file, and only then renamed into place; an interrupted import leaves a staging folder that the index
//! does not mention. The folder the mod came from is only read.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::builds::{COPY_MARGIN_BYTES, Ctx};
use crate::error::io;
use crate::fsops;
use crate::journal::fresh_id;
use crate::manifest::{FileEntry, VerifyReport, inventory, now_unix};
use crate::roots::RootKind;
use crate::safepath;
use crate::{Error, Result};

use super::{
    MAX_LIST_LEN, MAX_TEXT_LEN, validate_mod_path, validate_name, validate_note, validate_text,
};

pub const STORE_SCHEMA: u32 = 1;
/// Most files one mod may have. A limit chosen here to keep an import bounded, not a measured figure.
pub const MAX_MOD_FILES: usize = 50_000;

/// A mod this mod needs, loaded before it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Requirement {
    pub mod_id: String,
    /// The oldest version of that mod that will do, when the author said so.
    pub min_version: Option<String>,
}

/// What SUL records about an imported mod. Text fields are written by a person and never checked against
/// the mod itself: SUL does not verify a licence or a claim of compatibility.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModManifest {
    pub game_id: String,
    pub mod_id: String,
    pub name: String,
    pub version: String,
    pub requires: Vec<Requirement>,
    /// Game versions this mod is said to work with. Empty means nobody said; it is never read as "any".
    pub compatible_versions: Vec<String>,
    pub files: Vec<FileEntry>,
    /// Where the mod came from, in the owner's words. No secrets.
    pub source: String,
    /// The licence or permission, in the owner's words. SUL does not check it.
    pub license_note: String,
    pub imported_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModRecord {
    pub manifest: ModManifest,
    pub root_id: String,
    /// Folder of the mod's files, relative to the root, forward slashes.
    pub dir: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoreData {
    schema: u32,
    mods: Vec<ModRecord>,
}

#[derive(Debug, Clone)]
pub struct ModImport {
    pub game_id: String,
    pub mod_id: String,
    pub name: String,
    pub version: String,
    pub requires: Vec<Requirement>,
    pub compatible_versions: Vec<String>,
    pub source: String,
    pub license_note: String,
    /// The folder holding the mod's files. Only read.
    pub src: PathBuf,
    /// A library root of kind `Mods`.
    pub root_id: String,
}

pub struct ModStore {
    file: PathBuf,
    data: StoreData,
}

impl ModStore {
    /// Load the index. A missing file is an empty store; a corrupt or newer one is an error, never a reset.
    pub fn open(file: &Path) -> Result<Self> {
        let data = match std::fs::read_to_string(file) {
            Ok(text) => {
                let d: StoreData = serde_json::from_str(&text).map_err(|e| Error::json(file, e))?;
                if d.schema > STORE_SCHEMA {
                    return Err(Error::Invalid(format!(
                        "mod store schema {} is newer than supported {STORE_SCHEMA}",
                        d.schema
                    )));
                }
                d
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => StoreData {
                schema: STORE_SCHEMA,
                mods: vec![],
            },
            Err(e) => return Err(io(file)(e)),
        };
        Ok(Self {
            file: file.to_path_buf(),
            data,
        })
    }

    fn save(&self, ctx: &Ctx) -> Result<()> {
        let body = serde_json::to_string_pretty(&self.data)?;
        fsops::atomic_write(ctx.protected, &self.file, body.as_bytes())
    }

    pub fn mods(&self) -> &[ModRecord] {
        &self.data.mods
    }

    pub fn get(&self, game_id: &str, mod_id: &str, version: &str) -> Option<&ModRecord> {
        self.data.mods.iter().find(|m| {
            m.manifest.game_id == game_id
                && m.manifest.mod_id == mod_id
                && m.manifest.version == version
        })
    }

    /// The folder holding a mod's files.
    pub fn mod_dir(&self, ctx: &Ctx, rec: &ModRecord) -> Result<PathBuf> {
        safepath::join_within(ctx.roots.usable_path(&rec.root_id)?, &rec.dir)
    }

    /// Compare the stored files with the mod's own inventory. Read only.
    pub fn verify(&self, ctx: &Ctx, rec: &ModRecord) -> Result<VerifyReport> {
        let dir = self.mod_dir(ctx, rec)?;
        let mut report = VerifyReport::default();
        let on_disk = inventory(&dir)?;
        for f in &rec.manifest.files {
            match on_disk.iter().find(|d| d.path == f.path) {
                None => report.missing.push(f.path.clone()),
                Some(d) if d.sha256 != f.sha256 || d.size != f.size => {
                    report.modified.push(f.path.clone())
                }
                Some(_) => {}
            }
        }
        for d in &on_disk {
            if !rec.manifest.files.iter().any(|f| f.path == d.path) {
                report.extra.push(d.path.clone());
            }
        }
        Ok(report)
    }

    /// Copy a mod folder into the store as a new, verified version. Returns `<mod id>@<version>`.
    pub fn import_folder(&mut self, ctx: &Ctx, req: &ModImport) -> Result<String> {
        validate_name("game id", &req.game_id)?;
        validate_name("mod id", &req.mod_id)?;
        validate_name("version", &req.version)?;
        validate_text("mod name", &req.name, MAX_TEXT_LEN)?;
        validate_note("source", &req.source, 1000)?;
        validate_note("licence note", &req.license_note, 1000)?;
        if req.requires.len() > MAX_LIST_LEN || req.compatible_versions.len() > MAX_LIST_LEN {
            return Err(Error::Invalid("too many requirements or versions".into()));
        }
        for r in &req.requires {
            validate_name("required mod id", &r.mod_id)?;
            if r.mod_id == req.mod_id {
                return Err(Error::Invalid("a mod cannot require itself".into()));
            }
            if let Some(v) = &r.min_version {
                validate_name("required version", v)?;
            }
        }
        for v in &req.compatible_versions {
            validate_name("compatible game version", v)?;
        }
        if self.get(&req.game_id, &req.mod_id, &req.version).is_some() {
            return Err(Error::Invalid(format!(
                "{}@{} is already in the store; import it as a new version instead",
                req.mod_id, req.version
            )));
        }
        let root = ctx
            .roots
            .get(&req.root_id)
            .ok_or_else(|| Error::Invalid(format!("no library root with id {:?}", req.root_id)))?;
        if root.kind != RootKind::Mods {
            return Err(Error::Invalid(format!(
                "library root {:?} is not a mod library",
                req.root_id
            )));
        }
        let root_path = ctx.roots.usable_path(&req.root_id)?.to_path_buf();

        let src = req.src.canonicalize().map_err(io(&req.src))?;
        if !src.is_dir() {
            return Err(Error::Invalid(format!("{} is not a folder", src.display())));
        }
        let root_canon = root_path.canonicalize().map_err(io(&root_path))?;
        if src.starts_with(&root_canon) || root_canon.starts_with(&src) {
            return Err(Error::Invalid(
                "the mod folder and the mod library overlap".into(),
            ));
        }

        let files = inventory(&src)?;
        if files.is_empty() {
            return Err(Error::Invalid("the mod folder has no files".into()));
        }
        if files.len() > MAX_MOD_FILES {
            return Err(Error::Invalid(format!(
                "the mod has {} files; the limit is {MAX_MOD_FILES}",
                files.len()
            )));
        }
        let mut seen = HashSet::new();
        let mut checked = Vec::with_capacity(files.len());
        for f in &files {
            let path = validate_mod_path(&f.path)?;
            if !seen.insert(path.to_ascii_lowercase()) {
                return Err(Error::Invalid(format!(
                    "{path} appears twice (names that differ only by letter case are one file on Windows)"
                )));
            }
            checked.push(FileEntry {
                path,
                sha256: f.sha256.clone(),
                size: f.size,
            });
        }
        let total: u64 = checked.iter().map(|f| f.size).sum();
        ctx.roots
            .ensure_space(ctx.disk, &req.root_id, total, COPY_MARGIN_BYTES)?;

        let rel_dir = format!("{}/{}/{}", req.game_id, req.mod_id, req.version);
        let final_dir = safepath::join_within(&root_path, &rel_dir)?;
        if std::fs::symlink_metadata(&final_dir).is_ok() {
            return Err(Error::DestinationExists(final_dir));
        }
        let op = fresh_id("import-mod");
        let parent = final_dir
            .parent()
            .ok_or_else(|| Error::Invalid("mod folder has no parent".into()))?
            .to_path_buf();
        let staging = parent.join(format!(".staging-{op}"));
        ctx.protected.check_writable(&staging)?;

        ctx.journal.run(
            &op,
            "import_mod",
            &format!("{}@{}", req.mod_id, req.version),
            || {
                for f in &checked {
                    let from = safepath::join_within(&src, &f.path)?;
                    let to = staging.join(&f.path);
                    fsops::copy_file_atomic(ctx.protected, &from, &to, false)?;
                }
                if inventory(&staging)? != checked {
                    return Err(Error::Invalid(
                        "the copy does not match the source; it was left in the staging folder"
                            .into(),
                    ));
                }
                fsops::rename_new(ctx.protected, &staging, &final_dir)
            },
        )?;

        let manifest = ModManifest {
            game_id: req.game_id.clone(),
            mod_id: req.mod_id.clone(),
            name: req.name.trim().to_owned(),
            version: req.version.clone(),
            requires: req.requires.clone(),
            compatible_versions: req.compatible_versions.clone(),
            files: checked,
            source: req.source.clone(),
            license_note: req.license_note.clone(),
            imported_unix: now_unix(),
        };
        self.data.mods.push(ModRecord {
            manifest,
            root_id: req.root_id.clone(),
            dir: rel_dir,
        });
        if let Err(e) = self.save(ctx) {
            self.data.mods.pop();
            return Err(e);
        }
        Ok(format!("{}@{}", req.mod_id, req.version))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::testkit::{GAME, World, put};

    pub(crate) fn mod_world() -> World {
        let mut w = World::new();
        std::fs::create_dir_all(w.base.join("mods")).unwrap();
        w.roots
            .add(
                &w.protected,
                "mods",
                "mods",
                &w.base.join("mods"),
                RootKind::Mods,
            )
            .unwrap();
        w
    }

    pub(crate) fn make_mod(w: &World, name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = w.base.join("incoming").join(name);
        for (p, t) in files {
            put(&dir.join(p), t);
        }
        dir
    }

    pub(crate) fn req(w: &World, mod_id: &str, version: &str, files: &[(&str, &str)]) -> ModImport {
        ModImport {
            game_id: GAME.into(),
            mod_id: mod_id.into(),
            name: mod_id.to_uppercase(),
            version: version.into(),
            requires: vec![],
            compatible_versions: vec!["1.0.0".into()],
            source: "made up for a test".into(),
            license_note: "mine".into(),
            src: make_mod(w, &fresh_id(&format!("{mod_id}-{version}")), files),
            root_id: "mods".into(),
        }
    }

    #[test]
    fn an_import_is_a_verified_copy_and_the_source_is_left_alone() {
        let w = mod_world();
        let mut store = ModStore::open(&w.base.join("state/mods.json")).unwrap();
        let r = req(
            &w,
            "ui",
            "1.0",
            &[("data/a.bin", "NEW-A"), ("mods/x.txt", "x")],
        );
        let key = store.import_folder(&w.ctx(), &r).unwrap();
        assert_eq!(key, "ui@1.0");

        let rec = store.get(GAME, "ui", "1.0").unwrap().clone();
        assert_eq!(rec.manifest.files.len(), 2);
        assert!(store.verify(&w.ctx(), &rec).unwrap().is_clean());
        let dir = store.mod_dir(&w.ctx(), &rec).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("data/a.bin")).unwrap(),
            "NEW-A"
        );
        assert_eq!(
            std::fs::read_to_string(r.src.join("data/a.bin")).unwrap(),
            "NEW-A"
        );
        // The index survives a restart.
        let again = ModStore::open(&w.base.join("state/mods.json")).unwrap();
        assert_eq!(again.mods(), store.mods());
        assert!(w.journal.pending().unwrap().is_empty());
    }

    #[test]
    fn the_same_version_is_never_imported_over_itself() {
        let w = mod_world();
        let mut store = ModStore::open(&w.base.join("state/mods.json")).unwrap();
        let r = req(&w, "ui", "1.0", &[("data/a.bin", "ONE")]);
        store.import_folder(&w.ctx(), &r).unwrap();
        let mut r2 = req(&w, "ui", "1.0", &[("data/a.bin", "TWO")]);
        r2.src = make_mod(&w, "other", &[("data/a.bin", "TWO")]);
        assert!(store.import_folder(&w.ctx(), &r2).is_err());
        let rec = store.get(GAME, "ui", "1.0").unwrap().clone();
        let dir = store.mod_dir(&w.ctx(), &rec).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("data/a.bin")).unwrap(),
            "ONE"
        );
        // A new version is fine.
        let r3 = req(&w, "ui", "1.1", &[("data/a.bin", "THREE")]);
        store.import_folder(&w.ctx(), &r3).unwrap();
        assert_eq!(store.mods().len(), 2);
    }

    #[test]
    fn bad_requests_change_nothing() {
        let w = mod_world();
        let mut store = ModStore::open(&w.base.join("state/mods.json")).unwrap();
        let good = [("data/a.bin", "A")];
        let mut cases: Vec<(&str, ModImport)> = vec![];
        let mut r = req(&w, "ui", "1.0", &good);
        r.mod_id = "../evil".into();
        cases.push(("unsafe mod id", r));
        let mut r = req(&w, "ui", "1.0", &good);
        r.root_id = "ssd".into();
        cases.push(("a builds root is not a mod library", r));
        let mut r = req(&w, "ui", "1.0", &good);
        r.root_id = "nope".into();
        cases.push(("unknown root", r));
        let mut r = req(&w, "ui", "1.0", &good);
        r.requires = vec![Requirement {
            mod_id: "ui".into(),
            min_version: None,
        }];
        cases.push(("requires itself", r));
        let mut r = req(&w, "ui", "1.0", &good);
        r.name = " ".into();
        cases.push(("blank name", r));
        let mut r = req(&w, "ui", "1.0", &good);
        r.src = w.base.join("does-not-exist");
        cases.push(("missing source", r));
        let mut r = req(&w, "ui", "1.0", &good);
        r.src = w.base.join("mods");
        cases.push(("source is the library", r));
        let mut r = req(&w, "ui", "1.0", &good);
        std::fs::create_dir_all(w.base.join("incoming/empty")).unwrap();
        r.src = w.base.join("incoming/empty");
        cases.push(("no files", r));
        let mut r = req(&w, "ui", "1.0", &good);
        r.compatible_versions = vec!["a/b".into()];
        cases.push(("unsafe version text", r));
        for (why, r) in cases {
            assert!(store.import_folder(&w.ctx(), &r).is_err(), "{why}");
        }
        assert!(store.mods().is_empty());
        assert!(!w.base.join("mods").join(GAME).exists());
    }

    #[test]
    fn names_that_differ_only_by_case_are_refused() {
        let w = mod_world();
        let mut store = ModStore::open(&w.base.join("state/mods.json")).unwrap();
        let r = req(&w, "ui", "1.0", &[("Data/A.bin", "1"), ("data/a.bin", "2")]);
        // On a case-insensitive disk the second write would replace the first; either the folder cannot
        // even be made (Windows) or the import refuses it. Neither may produce a store entry.
        if inventory(&r.src).unwrap().len() == 2 {
            let err = store.import_folder(&w.ctx(), &r).unwrap_err();
            assert!(err.to_string().contains("appears twice"), "{err}");
        }
        assert!(store.mods().is_empty());
    }

    #[test]
    fn an_import_that_does_not_fit_changes_nothing() {
        let mut w = mod_world();
        w.disk = crate::roots::fake::FakeDisk::with(10);
        let mut store = ModStore::open(&w.base.join("state/mods.json")).unwrap();
        let r = req(&w, "ui", "1.0", &[("data/a.bin", "A")]);
        assert!(matches!(
            store.import_folder(&w.ctx(), &r),
            Err(Error::InsufficientSpace { .. })
        ));
        assert!(store.mods().is_empty());
    }

    #[test]
    fn a_changed_store_file_is_reported_and_an_unreadable_index_is_an_error() {
        let w = mod_world();
        let mut store = ModStore::open(&w.base.join("state/mods.json")).unwrap();
        store
            .import_folder(
                &w.ctx(),
                &req(&w, "ui", "1.0", &[("data/a.bin", "A"), ("b.txt", "B")]),
            )
            .unwrap();
        let rec = store.get(GAME, "ui", "1.0").unwrap().clone();
        let dir = store.mod_dir(&w.ctx(), &rec).unwrap();
        std::fs::write(dir.join("data/a.bin"), "tampered").unwrap();
        put(&dir.join("extra.txt"), "extra");
        std::fs::remove_file(dir.join("b.txt")).unwrap();
        let r = store.verify(&w.ctx(), &rec).unwrap();
        assert_eq!(r.modified, vec!["data/a.bin"]);
        assert_eq!(r.missing, vec!["b.txt"]);
        assert_eq!(r.extra, vec!["extra.txt"]);

        std::fs::write(w.base.join("state/mods.json"), "{ not json").unwrap();
        assert!(ModStore::open(&w.base.join("state/mods.json")).is_err());
        std::fs::write(
            w.base.join("state/mods.json"),
            r#"{"schema": 99, "mods": []}"#,
        )
        .unwrap();
        assert!(ModStore::open(&w.base.join("state/mods.json")).is_err());
    }
}
