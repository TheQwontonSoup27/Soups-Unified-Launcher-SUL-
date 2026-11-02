//! Build registry (spec 1.2 and 1.3): the managed builds of each game, which one is active in each
//! slot, and the operations that keep them safe.
//!
//! * **Import** copies an existing install into a managed root as a new build, then proves the copy
//!   matches the source before the build may be used. The source is only read.
//! * **Register in place** records an install that stays where it is (the user's original Steam copy)
//!   as a read-only baseline. It is accepted only when the folder is already a protected location, so
//!   SUL cannot write into it by mistake.
//! * **Slots** are the variants (original, optimized, legacy, experimental, or a named one). Each slot
//!   of a game has one active build plus a history, and switching is one atomic write of the registry.
//! * **Rollback** returns a slot to the most recent earlier build that still verifies.
//! * **Repair** restores missing or changed files of a managed build from a named source, but only with
//!   content whose hash matches the build's own manifest; a changed file is moved aside first.
//! * **Migrate** copies a build to another root, verifies the copy, switches the registry, and leaves
//!   the old copy in place, listed as retired.
//!
//! Nothing here deletes anything. A file that has to go out of the way is moved into the build's
//! recovery folder; an old copy is only listed. Deleting is the owner's decision, made outside SUL.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::io;
use crate::fsops::{self, ProtectedRoots};
use crate::journal::{Journal, fresh_id};
use crate::manifest::{
    BuildManifest, State, Variant, VerifyReport, hash_file, manifest_path_for, now_unix,
};
use crate::roots::{DiskSpace, LibraryRoots, RootKind};
use crate::safepath;
use crate::storage::validate_game_id;
use crate::{Error, Result};

pub const REGISTRY_SCHEMA: u32 = 1;
/// Free space to leave on a disk after a copy: 512 MiB. A choice made here, not a measured figure.
pub const COPY_MARGIN_BYTES: u64 = 512 * 1024 * 1024;
/// How many earlier builds a slot remembers for rollback.
pub const HISTORY_LIMIT: usize = 20;

const RESERVED_SLOTS: [&str; 4] = ["original", "optimized", "legacy", "experimental"];

/// Where a build's files are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Location {
    /// Inside a library root, in a folder SUL created. SUL may write here.
    Managed { root_id: String, dir: String },
    /// An install that stays where it is. SUL only reads it.
    ExternalInstall { path: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildRecord {
    pub id: String,
    pub game_id: String,
    pub slot: String,
    pub variant: Variant,
    pub location: Location,
    pub state: State,
}

/// The active build of one slot of one game, and the ones it replaced (most recent last).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveSlot {
    pub game_id: String,
    pub slot: String,
    pub build_id: String,
    pub history: Vec<String>,
}

/// A build SUL no longer manages whose files are still on disk. Listed so they can be found and dealt
/// with by the owner; SUL never deletes them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetiredCopy {
    pub build_id: String,
    pub game_id: String,
    pub location: Location,
    pub retired_unix: u64,
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct RegistryData {
    schema: u32,
    builds: Vec<BuildRecord>,
    active: Vec<ActiveSlot>,
    retired: Vec<RetiredCopy>,
}

/// What an operation needs from the rest of the app.
pub struct Ctx<'a> {
    pub roots: &'a LibraryRoots,
    pub protected: &'a ProtectedRoots,
    pub journal: &'a Journal,
    pub disk: &'a dyn DiskSpace,
}

#[derive(Debug, Clone)]
pub struct ImportRequest {
    pub game_id: String,
    pub display_name: String,
    pub variant: Variant,
    pub base_version: String,
    pub source_description: String,
    pub source_manifest_id: Option<String>,
    pub src: PathBuf,
    pub root_id: String,
}

#[derive(Debug, Clone)]
pub struct RegisterRequest {
    pub game_id: String,
    pub display_name: String,
    pub variant: Variant,
    pub base_version: String,
    pub source_description: String,
    pub path: PathBuf,
}

/// The outcome of comparing a build with its manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuildCheck {
    pub state: State,
    pub report: VerifyReport,
}

#[derive(Debug, Clone)]
pub enum RepairSource {
    /// Another managed build or a registered install, by id.
    Build(String),
    /// A folder, read only.
    Folder(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairReport {
    pub restored: Vec<String>,
    /// Changed files that were moved aside before being restored: (path in build, where it went).
    pub moved_aside: Vec<(String, PathBuf)>,
    /// Files that could not be restored, with the reason.
    pub unrepairable: Vec<(String, String)>,
    /// Files in the build that its manifest does not list. Reported, never touched.
    pub extra: Vec<String>,
    pub state_after: State,
}

#[derive(Debug)]
pub struct BuildRegistry {
    file: PathBuf,
    data: RegistryData,
}

/// The slot name for a variant. Named variants must be plain names and may not imitate a built-in slot.
pub fn slot_of(v: &Variant) -> Result<String> {
    Ok(match v {
        Variant::Original => "original".into(),
        Variant::Optimized => "optimized".into(),
        Variant::Legacy => "legacy".into(),
        Variant::Experimental => "experimental".into(),
        Variant::Custom(name) => {
            validate_game_id(name)?;
            let lower = name.to_ascii_lowercase();
            if RESERVED_SLOTS.contains(&lower.as_str()) {
                return Err(Error::Invalid(format!(
                    "{name:?} is the name of a built-in slot; use that variant instead"
                )));
            }
            lower
        }
    })
}

/// A build is usable when nothing it lists is missing or changed. Extra files (a game's caches, logs
/// and screenshots) are reported but do not make a build unusable.
fn sound(r: &VerifyReport) -> bool {
    r.missing.is_empty() && r.modified.is_empty()
}

impl BuildRegistry {
    /// Load the registry. A missing file is an empty registry; a corrupt or newer one is an error, never a reset.
    pub fn open(file: &Path) -> Result<Self> {
        let data = match std::fs::read_to_string(file) {
            Ok(text) => {
                let d: RegistryData =
                    serde_json::from_str(&text).map_err(|e| Error::json(file, e))?;
                if d.schema > REGISTRY_SCHEMA {
                    return Err(Error::Invalid(format!(
                        "build registry schema {} is newer than supported {REGISTRY_SCHEMA}",
                        d.schema
                    )));
                }
                d
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => RegistryData {
                schema: REGISTRY_SCHEMA,
                builds: vec![],
                active: vec![],
                retired: vec![],
            },
            Err(e) => return Err(io(file)(e)),
        };
        Ok(Self {
            file: file.to_path_buf(),
            data,
        })
    }

    fn save(&self, protected: &ProtectedRoots) -> Result<()> {
        let body = serde_json::to_string_pretty(&self.data)?;
        fsops::atomic_write(protected, &self.file, body.as_bytes())
    }

    pub fn builds(&self) -> &[BuildRecord] {
        &self.data.builds
    }

    pub fn retired(&self) -> &[RetiredCopy] {
        &self.data.retired
    }

    pub fn get(&self, id: &str) -> Option<&BuildRecord> {
        self.data.builds.iter().find(|b| b.id == id)
    }

    fn record(&self, id: &str) -> Result<&BuildRecord> {
        self.get(id)
            .ok_or_else(|| Error::Invalid(format!("no build with id {id:?}")))
    }

    fn record_mut(&mut self, id: &str) -> Result<&mut BuildRecord> {
        self.data
            .builds
            .iter_mut()
            .find(|b| b.id == id)
            .ok_or_else(|| Error::Invalid(format!("no build with id {id:?}")))
    }

    pub fn active(&self, game_id: &str, slot: &str) -> Option<&ActiveSlot> {
        self.data
            .active
            .iter()
            .find(|a| a.game_id == game_id && a.slot == slot)
    }

    fn is_active_anywhere(&self, id: &str) -> bool {
        self.data.active.iter().any(|a| a.build_id == id)
    }

    /// Ids of builds stored in root `root_id`, for [`LibraryRoots::forget`].
    pub fn builds_in_root(&self, root_id: &str) -> Vec<String> {
        self.data
            .builds
            .iter()
            .filter(|b| matches!(&b.location, Location::Managed { root_id: r, .. } if r == root_id))
            .map(|b| b.id.clone())
            .collect()
    }

    /// Folder holding a build's files.
    pub fn build_dir(&self, ctx: &Ctx, id: &str) -> Result<PathBuf> {
        match &self.record(id)?.location {
            Location::Managed { root_id, dir } => {
                safepath::join_within(ctx.roots.usable_path(root_id)?, dir)
            }
            Location::ExternalInstall { path } => Ok(path.clone()),
        }
    }

    fn manifest_file(&self, ctx: &Ctx, rec: &BuildRecord) -> Result<PathBuf> {
        match &rec.location {
            Location::Managed { root_id, dir } => {
                let build = safepath::join_within(ctx.roots.usable_path(root_id)?, dir)?;
                let parent = build
                    .parent()
                    .ok_or_else(|| Error::Invalid("build folder has no parent".into()))?;
                Ok(manifest_path_for(parent, &rec.id))
            }
            Location::ExternalInstall { .. } => Ok(self
                .external_manifest_dir()
                .join(format!("{}.manifest.json", rec.id))),
        }
    }

    fn external_manifest_dir(&self) -> PathBuf {
        self.file
            .parent()
            .unwrap_or(Path::new("."))
            .join("external-manifests")
    }

    pub fn manifest(&self, ctx: &Ctx, id: &str) -> Result<BuildManifest> {
        let rec = self.record(id)?;
        let path = self.manifest_file(ctx, rec)?;
        Ok(BuildManifest::load_file(ctx.protected, &path)?.0)
    }

    /// Change the manifest of a managed build and save it atomically (spec 1.3: patches, profile references and
    /// the file inventory move together when a mod profile is deployed or purged).
    ///
    /// `change` gets a copy; nothing is saved unless it returns `Ok` and the result is still a valid manifest for
    /// the same game. An install that stays where it is cannot be changed here: SUL never writes to it.
    pub fn update_manifest(
        &mut self,
        ctx: &Ctx,
        id: &str,
        change: impl FnOnce(&mut BuildManifest) -> Result<()>,
    ) -> Result<()> {
        let rec = self.record(id)?.clone();
        if !matches!(rec.location, Location::Managed { .. }) {
            return Err(Error::Invalid(format!(
                "build {id:?} is an install that stays where it is; its record is not changed here"
            )));
        }
        let path = self.manifest_file(ctx, &rec)?;
        let (before, _) = BuildManifest::load_file(ctx.protected, &path)?;
        let mut after = before.clone();
        change(&mut after)?;
        if after.game_id != before.game_id || after.variant != before.variant {
            return Err(Error::Invalid(
                "a manifest change may not change the game or the variant".into(),
            ));
        }
        after.validate()?;
        after.touch();
        after.save_file(ctx.protected, &path)
    }

    fn next_id(&self, game_id: &str, slot: &str, build_parent: Option<&Path>) -> Result<String> {
        for n in 1u32..10_000 {
            let id = format!("{game_id}-{slot}-{n}");
            validate_game_id(&id)?;
            let taken = self.data.builds.iter().any(|b| b.id == id)
                || self.data.retired.iter().any(|r| r.build_id == id)
                || build_parent.is_some_and(|p| {
                    p.join(&id).exists() || p.join(format!("{id}.partial")).exists()
                });
            if !taken {
                return Ok(id);
            }
        }
        Err(Error::Invalid(
            "too many builds of this game and slot".into(),
        ))
    }

    /// Copy an existing install into `req.root_id` as a new managed build and verify the copy.
    ///
    /// `req.src` is only read. The build starts in `Staging`, becomes `Ready` only when the copy matches
    /// a manifest taken from the source before copying, and becomes `Failed` (files left in place for
    /// inspection) when anything differs or the copy fails.
    pub fn import_install(&mut self, ctx: &Ctx, req: &ImportRequest) -> Result<String> {
        self.import_inner(ctx, req, &mut |_| {})
    }

    /// [`Self::import_install`] with a hook that runs on the new copy right after it is made and before
    /// it is verified. Production passes a no-op; tests use it to damage the copy or to stop mid-way.
    fn import_inner(
        &mut self,
        ctx: &Ctx,
        req: &ImportRequest,
        after_copy: &mut dyn FnMut(&Path),
    ) -> Result<String> {
        validate_game_id(&req.game_id)?;
        let slot = slot_of(&req.variant)?;
        let root = ctx
            .roots
            .get(&req.root_id)
            .ok_or_else(|| Error::Invalid(format!("no library root with id {:?}", req.root_id)))?;
        if root.kind != RootKind::Builds {
            return Err(Error::Invalid(format!(
                "library root {:?} is not a builds root",
                req.root_id
            )));
        }
        let root_path = ctx.roots.usable_path(&req.root_id)?;
        if !req.src.is_dir() {
            return Err(Error::Invalid(format!(
                "{} is not a folder",
                req.src.display()
            )));
        }

        let mut manifest = BuildManifest::scan(
            &req.src,
            &req.game_id,
            &req.display_name,
            req.variant.clone(),
            &req.base_version,
            &req.source_description,
        )?;
        if manifest.files.is_empty() {
            return Err(Error::Invalid("the folder has no files to import".into()));
        }
        manifest.source_manifest_id = req.source_manifest_id.clone();
        let total: u64 = manifest.files.iter().map(|f| f.size).sum();
        ctx.roots
            .ensure_space(ctx.disk, &req.root_id, total, COPY_MARGIN_BYTES)?;

        let games_dir = root_path.join("builds").join(&req.game_id);
        let id = self.next_id(&req.game_id, &slot, Some(&games_dir))?;
        let rel_dir = format!("builds/{}/{}", req.game_id, id);
        let dst = safepath::join_within(root_path, &rel_dir)?;

        manifest.state = State::Staging;
        self.data.builds.push(BuildRecord {
            id: id.clone(),
            game_id: req.game_id.clone(),
            slot,
            variant: req.variant.clone(),
            location: Location::Managed {
                root_id: req.root_id.clone(),
                dir: rel_dir,
            },
            state: State::Staging,
        });
        self.save(ctx.protected)?;

        let op = fresh_id("import");
        let copied = ctx.journal.run(
            &op,
            "import_install",
            &format!("{id}: {} -> {}", req.src.display(), dst.display()),
            || {
                fsops::isolated_copy(
                    ctx.protected,
                    ctx.journal,
                    &fresh_id("copy"),
                    &req.src,
                    &dst,
                )?;
                after_copy(&dst);
                manifest.verify(&dst)
            },
        );
        match copied {
            Ok(report) if report.is_clean() => {
                manifest.state = State::Ready;
                manifest.verified_unix = Some(now_unix());
                manifest.touch();
                let mpath = manifest_path_for(dst.parent().unwrap_or(root_path), &id);
                manifest.save_file(ctx.protected, &mpath)?;
                self.record_mut(&id)?.state = State::Ready;
                self.save(ctx.protected)?;
                Ok(id)
            }
            Ok(report) => {
                self.record_mut(&id)?.state = State::Failed;
                self.save(ctx.protected)?;
                Err(Error::Invalid(format!(
                    "the copy does not match the source ({} missing, {} changed, {} extra); it was left at {} for inspection",
                    report.missing.len(),
                    report.modified.len(),
                    report.extra.len(),
                    dst.display()
                )))
            }
            Err(e) => {
                self.record_mut(&id)?.state = State::Failed;
                self.save(ctx.protected)?;
                Err(e)
            }
        }
    }

    /// Record an install that stays where it is, as a read-only baseline (the Original slot, usually).
    ///
    /// The folder must already be a protected location. If it is not, this refuses: SUL could otherwise
    /// write into the user's own install later.
    pub fn register_in_place(&mut self, ctx: &Ctx, req: &RegisterRequest) -> Result<String> {
        validate_game_id(&req.game_id)?;
        let slot = slot_of(&req.variant)?;
        if !req.path.is_dir() {
            return Err(Error::Invalid(format!(
                "{} is not a folder",
                req.path.display()
            )));
        }
        if ctx
            .protected
            .check_writable(&req.path.join(".sul-write-probe"))
            .is_ok()
        {
            return Err(Error::Invalid(format!(
                "{} is not a protected location; protect the install first so SUL can only read it",
                req.path.display()
            )));
        }
        let mut manifest = BuildManifest::scan(
            &req.path,
            &req.game_id,
            &req.display_name,
            req.variant.clone(),
            &req.base_version,
            &req.source_description,
        )?;
        if manifest.files.is_empty() {
            return Err(Error::Invalid("the folder has no files to record".into()));
        }
        let id = self.next_id(&req.game_id, &slot, None)?;
        manifest.state = State::Ready;
        manifest.verified_unix = Some(now_unix());
        let mpath = self
            .external_manifest_dir()
            .join(format!("{id}.manifest.json"));
        manifest.save_file(ctx.protected, &mpath)?;
        self.data.builds.push(BuildRecord {
            id: id.clone(),
            game_id: req.game_id.clone(),
            slot,
            variant: req.variant.clone(),
            location: Location::ExternalInstall {
                path: req.path.clone(),
            },
            state: State::Ready,
        });
        self.save(ctx.protected)?;
        Ok(id)
    }

    /// Compare a build with its manifest and record the result. Reads the build; writes only SUL's own records.
    pub fn verify(&mut self, ctx: &Ctx, id: &str) -> Result<BuildCheck> {
        let rec = self.record(id)?.clone();
        if rec.state == State::Staging {
            return Err(Error::Invalid(format!(
                "build {id:?} is an unfinished import and cannot be verified"
            )));
        }
        let dir = self.build_dir(ctx, id)?;
        let mpath = self.manifest_file(ctx, &rec)?;
        let (mut manifest, _) = BuildManifest::load_file(ctx.protected, &mpath)?;
        let report = manifest.verify(&dir)?;
        let state = if sound(&report) {
            State::Ready
        } else {
            State::Failed
        };
        manifest.state = state.clone();
        manifest.verified_unix = Some(now_unix());
        manifest.touch();
        manifest.save_file(ctx.protected, &mpath)?;
        self.record_mut(id)?.state = state.clone();
        self.save(ctx.protected)?;
        Ok(BuildCheck { state, report })
    }

    /// Make `id` the active build of its slot. It is verified first and must be sound. The previous
    /// active build goes onto the slot's history. One atomic write of the registry is the switch.
    pub fn activate(&mut self, ctx: &Ctx, id: &str) -> Result<()> {
        let check = self.verify(ctx, id)?;
        if check.state != State::Ready {
            return Err(Error::Invalid(format!(
                "build {id:?} does not match its manifest ({} missing, {} changed); repair or re-import it before using it",
                check.report.missing.len(),
                check.report.modified.len()
            )));
        }
        let rec = self.record(id)?.clone();
        let op = fresh_id("activate");
        ctx.journal.run(&op, "activate_build", id, || {
            match self
                .data
                .active
                .iter_mut()
                .find(|a| a.game_id == rec.game_id && a.slot == rec.slot)
            {
                Some(a) if a.build_id == id => return Ok(()),
                Some(a) => {
                    a.history
                        .push(std::mem::replace(&mut a.build_id, id.to_owned()));
                    if a.history.len() > HISTORY_LIMIT {
                        let excess = a.history.len() - HISTORY_LIMIT;
                        a.history.drain(..excess);
                    }
                }
                None => self.data.active.push(ActiveSlot {
                    game_id: rec.game_id.clone(),
                    slot: rec.slot.clone(),
                    build_id: id.to_owned(),
                    history: vec![],
                }),
            }
            self.save(ctx.protected)
        })
    }

    /// Go back to the most recent earlier build of the slot that still verifies. Earlier builds that no
    /// longer verify are skipped and stay in the history. Returns the id now active.
    pub fn rollback(&mut self, ctx: &Ctx, game_id: &str, slot: &str) -> Result<String> {
        let history = self
            .active(game_id, slot)
            .ok_or_else(|| Error::Invalid(format!("{game_id}/{slot} has no active build")))?
            .history
            .clone();
        for (i, candidate) in history.iter().enumerate().rev() {
            if self.get(candidate).is_none() {
                continue;
            }
            let Ok(check) = self.verify(ctx, candidate) else {
                continue;
            };
            if check.state != State::Ready {
                continue;
            }
            let op = fresh_id("rollback");
            let candidate = candidate.clone();
            ctx.journal.run(&op, "rollback_build", &candidate, || {
                let a = self
                    .data
                    .active
                    .iter_mut()
                    .find(|a| a.game_id == game_id && a.slot == slot)
                    .ok_or_else(|| Error::Invalid("slot vanished".into()))?;
                a.build_id = candidate.clone();
                a.history.truncate(i);
                self.save(ctx.protected)
            })?;
            return Ok(candidate);
        }
        Err(Error::Invalid(format!(
            "{game_id}/{slot} has no earlier build that still matches its manifest"
        )))
    }

    /// Restore missing or changed files of a managed build from `source`.
    ///
    /// A file is restored only if the source file's hash and size equal the build's own manifest entry.
    /// A changed file is moved into `<build>.recovery/<op>/` first and nothing is overwritten. Extra
    /// files are listed and left alone. A registered install is never repaired.
    pub fn repair(&mut self, ctx: &Ctx, id: &str, source: &RepairSource) -> Result<RepairReport> {
        let rec = self.record(id)?.clone();
        let Location::Managed { .. } = rec.location else {
            return Err(Error::Invalid(format!(
                "build {id:?} is an install that stays where it is; SUL never writes to it"
            )));
        };
        let target = self.build_dir(ctx, id)?;
        let source_dir = match source {
            RepairSource::Build(other) => {
                if other == id {
                    return Err(Error::Invalid("a build cannot repair itself".into()));
                }
                self.build_dir(ctx, other)?
            }
            RepairSource::Folder(p) => p.clone(),
        };
        let (src_c, dst_c) = (
            source_dir.canonicalize().map_err(io(&source_dir))?,
            target.canonicalize().map_err(io(&target))?,
        );
        if src_c.starts_with(&dst_c) || dst_c.starts_with(&src_c) {
            return Err(Error::Invalid(
                "the repair source overlaps the build".into(),
            ));
        }

        let mpath = self.manifest_file(ctx, &rec)?;
        let (manifest, _) = BuildManifest::load_file(ctx.protected, &mpath)?;
        let before = manifest.verify(&target)?;
        let op = fresh_id("repair");
        let recovery = target
            .parent()
            .ok_or_else(|| Error::Invalid("build folder has no parent".into()))?
            .join(format!("{id}.recovery"))
            .join(&op);

        let mut restored = Vec::new();
        let mut moved_aside = Vec::new();
        let mut unrepairable = Vec::new();
        ctx.journal.run(&op, "repair_build", id, || {
            for (rel, was_changed) in before
                .missing
                .iter()
                .map(|r| (r, false))
                .chain(before.modified.iter().map(|r| (r, true)))
            {
                let entry = manifest
                    .files
                    .iter()
                    .find(|f| &f.path == rel)
                    .ok_or_else(|| Error::Invalid(format!("{rel} is not in the manifest")))?;
                let src_file = match safepath::join_within(&source_dir, rel) {
                    Ok(p) => p,
                    Err(e) => {
                        unrepairable.push((rel.clone(), e.to_string()));
                        continue;
                    }
                };
                let is_plain_file = std::fs::symlink_metadata(&src_file)
                    .map(|m| m.is_file())
                    .unwrap_or(false);
                if !is_plain_file {
                    unrepairable.push((rel.clone(), "the source has no such file".into()));
                    continue;
                }
                match hash_file(&src_file) {
                    Ok((h, size)) if h == entry.sha256 && size == entry.size => {}
                    Ok(_) => {
                        unrepairable.push((
                            rel.clone(),
                            "the source file differs from the manifest".into(),
                        ));
                        continue;
                    }
                    Err(e) => {
                        unrepairable.push((rel.clone(), e.to_string()));
                        continue;
                    }
                }
                let dst_file = safepath::join_within(&target, rel)?;
                if was_changed {
                    let kept = fsops::move_aside(
                        ctx.protected,
                        &dst_file,
                        &recovery,
                        &safepath::validate_relative(rel)?,
                    )?;
                    moved_aside.push((rel.clone(), kept));
                }
                fsops::copy_file_atomic(ctx.protected, &src_file, &dst_file, false)?;
                restored.push(rel.clone());
            }
            Ok(())
        })?;

        let check = self.verify(ctx, id)?;
        Ok(RepairReport {
            restored,
            moved_aside,
            unrepairable,
            extra: check.report.extra,
            state_after: check.state,
        })
    }

    /// Copy build `id` to another builds root, verify the copy, then switch the registry to it.
    ///
    /// The old folder stays where it is and is listed under [`Self::retired`]; nothing is deleted. If the
    /// copy or its verification fails, the registry is unchanged and the build keeps working from where it was.
    pub fn migrate_build(&mut self, ctx: &Ctx, id: &str, to_root_id: &str) -> Result<()> {
        self.migrate_inner(ctx, id, to_root_id, &mut |_| {})
    }

    /// [`Self::migrate_build`] with the same kind of hook as [`Self::import_inner`].
    fn migrate_inner(
        &mut self,
        ctx: &Ctx,
        id: &str,
        to_root_id: &str,
        after_copy: &mut dyn FnMut(&Path),
    ) -> Result<()> {
        let rec = self.record(id)?.clone();
        let Location::Managed {
            root_id: from_root,
            dir: _,
        } = &rec.location
        else {
            return Err(Error::Invalid(format!(
                "build {id:?} is an install that stays where it is and cannot be migrated"
            )));
        };
        if from_root == to_root_id {
            return Err(Error::Invalid("the build is already in that root".into()));
        }
        let to = ctx
            .roots
            .get(to_root_id)
            .ok_or_else(|| Error::Invalid(format!("no library root with id {to_root_id:?}")))?;
        if to.kind != RootKind::Builds {
            return Err(Error::Invalid(format!(
                "library root {to_root_id:?} is not a builds root"
            )));
        }
        let to_path = ctx.roots.usable_path(to_root_id)?;
        let check = self.verify(ctx, id)?;
        if check.state != State::Ready {
            return Err(Error::Invalid(format!(
                "build {id:?} does not match its manifest; repair it before moving it"
            )));
        }
        let src = self.build_dir(ctx, id)?;
        let mpath = self.manifest_file(ctx, &rec)?;
        let (manifest, _) = BuildManifest::load_file(ctx.protected, &mpath)?;
        let total: u64 = manifest.files.iter().map(|f| f.size).sum();
        ctx.roots
            .ensure_space(ctx.disk, to_root_id, total, COPY_MARGIN_BYTES)?;

        let rel_dir = format!("builds/{}/{}", rec.game_id, id);
        let dst = safepath::join_within(to_path, &rel_dir)?;
        let op = fresh_id("migrate");
        ctx.journal.run(
            &op,
            "migrate_build",
            &format!("{id}: {} -> {}", src.display(), dst.display()),
            || {
                fsops::isolated_copy(ctx.protected, ctx.journal, &fresh_id("copy"), &src, &dst)?;
                after_copy(&dst);
                let report = manifest.verify(&dst)?;
                if !sound(&report) {
                    return Err(Error::Invalid(format!(
                        "the copy in {} does not match the manifest ({} missing, {} changed); the build was not moved",
                        dst.display(),
                        report.missing.len(),
                        report.modified.len()
                    )));
                }
                let new_manifest = manifest_path_for(dst.parent().unwrap_or(to_path), id);
                manifest.save_file(ctx.protected, &new_manifest)?;
                let old_location = rec.location.clone();
                self.record_mut(id)?.location = Location::Managed {
                    root_id: to_root_id.to_owned(),
                    dir: rel_dir.clone(),
                };
                self.data.retired.push(RetiredCopy {
                    build_id: id.to_owned(),
                    game_id: rec.game_id.clone(),
                    location: old_location,
                    retired_unix: now_unix(),
                    reason: format!("migrated to library root {to_root_id:?}; the old copy was left in place"),
                });
                self.save(ctx.protected)
            },
        )
    }

    /// Stop managing a build that is not active anywhere. Its files stay on disk and are listed under
    /// [`Self::retired`] with the reason. Nothing is deleted.
    pub fn retire(&mut self, ctx: &Ctx, id: &str, reason: &str) -> Result<()> {
        let rec = self.record(id)?.clone();
        if self.is_active_anywhere(id) {
            return Err(Error::Invalid(format!(
                "build {id:?} is active; switch the slot first"
            )));
        }
        if self
            .data
            .active
            .iter()
            .any(|a| a.history.iter().any(|h| h == id))
        {
            return Err(Error::Invalid(format!(
                "build {id:?} is still in a slot's rollback history"
            )));
        }
        if reason.trim().is_empty() {
            return Err(Error::Invalid("say why the build is being retired".into()));
        }
        self.data.builds.retain(|b| b.id != id);
        self.data.retired.push(RetiredCopy {
            build_id: id.to_owned(),
            game_id: rec.game_id,
            location: rec.location,
            retired_unix: now_unix(),
            reason: reason.to_owned(),
        });
        self.save(ctx.protected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roots::fake::FakeDisk;
    use std::collections::BTreeMap;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    struct World {
        _dir: tempfile::TempDir,
        base: PathBuf,
        protected: ProtectedRoots,
        journal: Journal,
        roots: LibraryRoots,
        disk: FakeDisk,
        reg: PathBuf,
        install: PathBuf,
    }

    impl World {
        fn ctx(&self) -> Ctx<'_> {
            Ctx {
                roots: &self.roots,
                protected: &self.protected,
                journal: &self.journal,
                disk: &self.disk,
            }
        }

        fn req(&self, variant: Variant, root: &str) -> ImportRequest {
            ImportRequest {
                game_id: "terraria".into(),
                display_name: "Terraria".into(),
                variant,
                base_version: "1.4.4".into(),
                source_description: "my own install".into(),
                source_manifest_id: Some("depot-1".into()),
                src: self.install.clone(),
                root_id: root.into(),
            }
        }

        fn registry(&self) -> BuildRegistry {
            BuildRegistry::open(&self.reg).unwrap()
        }
    }

    fn put(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn world() -> World {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        for d in ["ssd", "hdd", "plain"] {
            std::fs::create_dir_all(base.join(d)).unwrap();
        }
        let install = base.join("steam_install");
        put(&install.join("game.exe"), "exe");
        put(&install.join("data/a.bin"), "AAAA");
        put(&install.join("data/b.bin"), "BBBB");
        put(&install.join("config.ini"), "x=1");
        put(&base.join("plain").join("f.txt"), "f");

        let mut protected = ProtectedRoots::new();
        protected.add(&install).unwrap();
        let mut roots = LibraryRoots::load(&base.join("state/roots.json")).unwrap();
        for id in ["ssd", "hdd"] {
            roots
                .add(&protected, id, id, &base.join(id), RootKind::Builds)
                .unwrap();
        }
        World {
            journal: Journal::open(&base.join("state/journal.jsonl")).unwrap(),
            reg: base.join("state/builds.json"),
            disk: FakeDisk::with(u64::MAX / 2),
            _dir: dir,
            base,
            protected,
            roots,
            install,
        }
    }

    /// Path -> (hash, size) of every file under `dir`, to prove a folder did not change.
    fn snapshot(dir: &Path) -> BTreeMap<String, (String, u64)> {
        BuildManifest::scan(dir, "g", "G", Variant::Original, "1", "t")
            .unwrap()
            .files
            .into_iter()
            .map(|f| (f.path, (f.sha256, f.size)))
            .collect()
    }

    fn import(w: &World, reg: &mut BuildRegistry, variant: Variant) -> String {
        reg.import_install(&w.ctx(), &w.req(variant, "ssd"))
            .unwrap()
    }

    fn dir_of(w: &World, reg: &BuildRegistry, id: &str) -> PathBuf {
        reg.build_dir(&w.ctx(), id).unwrap()
    }

    // ---- slots ----

    #[test]
    fn slot_names_come_from_the_variant_and_named_ones_cannot_imitate_built_in_slots() {
        assert_eq!(slot_of(&Variant::Original).unwrap(), "original");
        assert_eq!(slot_of(&Variant::Experimental).unwrap(), "experimental");
        assert_eq!(
            slot_of(&Variant::Custom("My-Mod".into())).unwrap(),
            "my-mod"
        );
        for bad in ["Legacy", "ORIGINAL", "optimized", "", "a/b", "..", "nul"] {
            assert!(slot_of(&Variant::Custom(bad.into())).is_err(), "{bad:?}");
        }
    }

    // ---- import ----

    #[test]
    fn an_import_copies_verifies_and_leaves_the_source_alone() {
        let w = world();
        let before = snapshot(&w.install);
        let mut reg = w.registry();
        let id = import(&w, &mut reg, Variant::Legacy);

        assert_eq!(id, "terraria-legacy-1");
        let rec = reg.get(&id).unwrap();
        assert_eq!(rec.state, State::Ready);
        assert_eq!(rec.slot, "legacy");
        assert_eq!(
            rec.location,
            Location::Managed {
                root_id: "ssd".into(),
                dir: "builds/terraria/terraria-legacy-1".into()
            }
        );
        assert_eq!(
            snapshot(&dir_of(&w, &reg, &id)),
            before,
            "the copy equals the source"
        );
        assert_eq!(snapshot(&w.install), before, "the source did not change");

        let m = reg.manifest(&w.ctx(), &id).unwrap();
        assert_eq!(m.state, State::Ready);
        assert!(m.verified_unix.is_some());
        assert_eq!(m.files.len(), 4);
        assert_eq!(m.source_manifest_id.as_deref(), Some("depot-1"));
        assert_eq!(m.variant, Variant::Legacy);

        assert!(w.journal.pending().unwrap().is_empty());
        assert_eq!(w.registry().builds(), reg.builds(), "saved to disk");
    }

    #[test]
    fn builds_of_the_same_slot_get_new_ids_and_never_reuse_a_folder() {
        let w = world();
        let mut reg = w.registry();
        let a = import(&w, &mut reg, Variant::Legacy);
        let b = import(&w, &mut reg, Variant::Legacy);
        let c = import(&w, &mut reg, Variant::Custom("Hotfix".into()));
        assert_eq!(
            (a.as_str(), b.as_str(), c.as_str()),
            (
                "terraria-legacy-1",
                "terraria-legacy-2",
                "terraria-hotfix-1"
            )
        );
    }

    #[test]
    fn an_import_that_does_not_fit_changes_nothing() {
        let w = world();
        w.disk.0.set(10);
        let mut reg = w.registry();
        let err = reg
            .import_install(&w.ctx(), &w.req(Variant::Legacy, "ssd"))
            .unwrap_err();
        assert!(matches!(err, Error::InsufficientSpace { .. }), "{err}");
        assert!(reg.builds().is_empty());
        assert!(!w.reg.exists(), "no registry written");
        assert!(
            !w.base.join("ssd/builds").exists(),
            "nothing created in the root"
        );
    }

    #[test]
    fn an_import_refuses_bad_requests() {
        let w = world();
        let mut reg = w.registry();
        let ctx = w.ctx();
        let mut r = w.req(Variant::Legacy, "ssd");
        r.root_id = "nope".into();
        assert!(reg.import_install(&ctx, &r).is_err(), "unknown root");
        let mut r = w.req(Variant::Legacy, "ssd");
        r.game_id = "../x".into();
        assert!(reg.import_install(&ctx, &r).is_err(), "bad game id");
        let mut r = w.req(Variant::Custom("legacy".into()), "ssd");
        r.variant = Variant::Custom("legacy".into());
        assert!(
            reg.import_install(&ctx, &r).is_err(),
            "imitates a built-in slot"
        );
        let mut r = w.req(Variant::Legacy, "ssd");
        r.src = w.base.join("missing");
        assert!(reg.import_install(&ctx, &r).is_err(), "not a folder");
        let empty = w.base.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        let mut r = w.req(Variant::Legacy, "ssd");
        r.src = empty;
        assert!(reg.import_install(&ctx, &r).is_err(), "no files");
        assert!(reg.builds().is_empty());
    }

    #[test]
    fn a_copy_that_does_not_match_is_marked_failed_and_left_for_inspection() {
        let w = world();
        let before = snapshot(&w.install);
        let mut reg = w.registry();
        let req = w.req(Variant::Legacy, "ssd");
        let err = reg
            .import_inner(&w.ctx(), &req, &mut |dst| {
                put(&dst.join("data/a.bin"), "tampered")
            })
            .unwrap_err();
        assert!(err.to_string().contains("does not match"), "{err}");

        let rec = &reg.builds()[0];
        assert_eq!(rec.state, State::Failed);
        let dir = dir_of(&w, &reg, &rec.id.clone());
        assert!(
            dir.join("data/a.bin").is_file(),
            "left in place for inspection"
        );
        assert_eq!(snapshot(&w.install), before);
        assert!(w.journal.pending().unwrap().is_empty());
        // A failed build cannot become active.
        let id = reg.builds()[0].id.clone();
        assert!(reg.activate(&w.ctx(), &id).is_err());
        assert!(reg.active("terraria", "legacy").is_none());
    }

    #[test]
    fn an_import_interrupted_midway_is_visible_and_blocks_nothing_else() {
        let w = world();
        let mut reg = w.registry();
        let req = w.req(Variant::Legacy, "ssd");
        let crashed = catch_unwind(AssertUnwindSafe(|| {
            let _ = reg.import_inner(&w.ctx(), &req, &mut |_| panic!("simulated power cut"));
        }));
        assert!(crashed.is_err());
        drop(reg);

        let mut reg = w.registry();
        assert_eq!(reg.builds().len(), 1);
        assert_eq!(
            reg.builds()[0].state,
            State::Staging,
            "recorded as unfinished"
        );
        let pending = w.journal.pending().unwrap();
        assert!(
            pending.iter().any(|p| p.op == "import_install"),
            "{pending:?}"
        );
        let stuck = reg.builds()[0].id.clone();
        assert!(
            reg.verify(&w.ctx(), &stuck).is_err(),
            "an unfinished import cannot be verified"
        );
        assert!(reg.activate(&w.ctx(), &stuck).is_err());

        // A new import does not collide with the leftover folder.
        let next = import(&w, &mut reg, Variant::Legacy);
        assert_eq!(next, "terraria-legacy-2");
        assert_eq!(reg.get(&next).unwrap().state, State::Ready);
    }

    // ---- register in place ----

    #[test]
    fn an_install_that_stays_put_must_already_be_protected_and_is_never_written() {
        let w = world();
        let before = snapshot(&w.install);
        let mut reg = w.registry();
        let ctx = w.ctx();
        let req = RegisterRequest {
            game_id: "terraria".into(),
            display_name: "Terraria".into(),
            variant: Variant::Original,
            base_version: "1.4.4".into(),
            source_description: "steam".into(),
            path: w.base.join("plain"),
        };
        let err = reg.register_in_place(&ctx, &req).unwrap_err();
        assert!(
            err.to_string().contains("not a protected location"),
            "{err}"
        );
        assert!(reg.builds().is_empty());

        let id = reg
            .register_in_place(
                &ctx,
                &RegisterRequest {
                    path: w.install.clone(),
                    ..req
                },
            )
            .unwrap();
        assert_eq!(id, "terraria-original-1");
        assert!(matches!(
            reg.get(&id).unwrap().location,
            Location::ExternalInstall { .. }
        ));
        assert_eq!(reg.get(&id).unwrap().state, State::Ready);
        assert_eq!(
            snapshot(&w.install),
            before,
            "nothing was added to the install"
        );
        assert!(
            w.base
                .join("state/external-manifests/terraria-original-1.manifest.json")
                .is_file()
        );

        // It can be verified and activated, but never repaired or moved.
        reg.activate(&ctx, &id).unwrap();
        assert!(
            reg.repair(&ctx, &id, &RepairSource::Folder(w.base.join("plain")))
                .is_err()
        );
        assert!(reg.migrate_build(&ctx, &id, "hdd").is_err());
        assert_eq!(snapshot(&w.install), before);

        // A store update changes the install: it is reported, not repaired.
        put(&w.install.join("data/a.bin"), "updated by the store");
        let check = reg.verify(&ctx, &id).unwrap();
        assert_eq!(check.state, State::Failed);
        assert_eq!(check.report.modified, vec!["data/a.bin"]);
        assert_eq!(
            std::fs::read_to_string(w.install.join("data/a.bin")).unwrap(),
            "updated by the store"
        );
    }

    // ---- activate and rollback ----

    #[test]
    fn switching_a_slot_keeps_a_history_and_slots_are_independent() {
        let w = world();
        let mut reg = w.registry();
        let ctx = w.ctx();
        let (a, b, c) = (
            import(&w, &mut reg, Variant::Legacy),
            import(&w, &mut reg, Variant::Legacy),
            import(&w, &mut reg, Variant::Legacy),
        );
        let e = import(&w, &mut reg, Variant::Experimental);

        reg.activate(&ctx, &a).unwrap();
        assert_eq!(
            reg.active("terraria", "legacy").unwrap().history,
            Vec::<String>::new()
        );
        reg.activate(&ctx, &b).unwrap();
        reg.activate(&ctx, &b).unwrap(); // again: nothing changes
        reg.activate(&ctx, &c).unwrap();
        reg.activate(&ctx, &e).unwrap();

        let legacy = reg.active("terraria", "legacy").unwrap();
        assert_eq!(legacy.build_id, c);
        assert_eq!(legacy.history, vec![a.clone(), b.clone()]);
        assert_eq!(reg.active("terraria", "experimental").unwrap().build_id, e);
        assert_eq!(
            w.registry().active("terraria", "legacy"),
            Some(legacy),
            "saved"
        );
        assert!(w.journal.pending().unwrap().is_empty());
    }

    #[test]
    fn history_is_capped_and_keeps_the_most_recent() {
        let w = world();
        let mut reg = w.registry();
        let ctx = w.ctx();
        let ids: Vec<String> = (0..HISTORY_LIMIT + 3)
            .map(|_| import(&w, &mut reg, Variant::Legacy))
            .collect();
        for id in &ids {
            reg.activate(&ctx, id).unwrap();
        }
        let a = reg.active("terraria", "legacy").unwrap();
        assert_eq!(a.history.len(), HISTORY_LIMIT);
        assert_eq!(a.build_id, ids[ids.len() - 1]);
        assert_eq!(a.history[HISTORY_LIMIT - 1], ids[ids.len() - 2]);
        assert_eq!(a.history[0], ids[2], "the oldest ones were dropped");
    }

    #[test]
    fn a_build_that_no_longer_matches_cannot_be_switched_to() {
        let w = world();
        let mut reg = w.registry();
        let ctx = w.ctx();
        let (a, b) = (
            import(&w, &mut reg, Variant::Legacy),
            import(&w, &mut reg, Variant::Legacy),
        );
        reg.activate(&ctx, &a).unwrap();
        put(&dir_of(&w, &reg, &b).join("game.exe"), "patched elsewhere");

        let err = reg.activate(&ctx, &b).unwrap_err();
        assert!(
            err.to_string().contains("does not match its manifest"),
            "{err}"
        );
        assert_eq!(reg.active("terraria", "legacy").unwrap().build_id, a);
        assert_eq!(reg.get(&b).unwrap().state, State::Failed);
        assert_eq!(
            reg.manifest(&ctx, &b).unwrap().state,
            State::Failed,
            "record and manifest agree"
        );
    }

    #[test]
    fn rollback_returns_to_the_latest_earlier_build_that_still_verifies() {
        let w = world();
        let mut reg = w.registry();
        let ctx = w.ctx();
        let ids: Vec<String> = (0..3)
            .map(|_| import(&w, &mut reg, Variant::Legacy))
            .collect();
        for id in &ids {
            reg.activate(&ctx, id).unwrap();
        }
        // The middle build was damaged: rollback skips it and lands on the first.
        put(&dir_of(&w, &reg, &ids[1]).join("config.ini"), "x=2");
        assert_eq!(reg.rollback(&ctx, "terraria", "legacy").unwrap(), ids[0]);
        let a = reg.active("terraria", "legacy").unwrap();
        assert_eq!((a.build_id.as_str(), a.history.len()), (ids[0].as_str(), 0));
        assert!(
            reg.get(&ids[1]).is_some(),
            "the skipped build is still registered"
        );

        // Nothing earlier is left.
        assert!(reg.rollback(&ctx, "terraria", "legacy").is_err());
        assert!(
            reg.rollback(&ctx, "terraria", "optimized").is_err(),
            "no such slot"
        );
        assert_eq!(reg.active("terraria", "legacy").unwrap().build_id, ids[0]);
    }

    #[test]
    fn a_rollback_with_no_good_candidate_changes_nothing() {
        let w = world();
        let mut reg = w.registry();
        let ctx = w.ctx();
        let (a, b) = (
            import(&w, &mut reg, Variant::Legacy),
            import(&w, &mut reg, Variant::Legacy),
        );
        reg.activate(&ctx, &a).unwrap();
        reg.activate(&ctx, &b).unwrap();
        std::fs::remove_file(dir_of(&w, &reg, &a).join("game.exe")).unwrap();
        let before = reg.active("terraria", "legacy").unwrap().clone();
        assert!(reg.rollback(&ctx, "terraria", "legacy").is_err());
        assert_eq!(reg.active("terraria", "legacy").unwrap(), &before);

        // After repairing it from the other build, rollback works.
        reg.repair(&ctx, &a, &RepairSource::Build(b.clone()))
            .unwrap();
        assert_eq!(reg.rollback(&ctx, "terraria", "legacy").unwrap(), a);
    }

    // ---- repair ----

    #[test]
    fn repair_restores_from_a_source_that_matches_and_keeps_what_it_replaces() {
        let w = world();
        let mut reg = w.registry();
        let ctx = w.ctx();
        let (good, bad) = (
            import(&w, &mut reg, Variant::Legacy),
            import(&w, &mut reg, Variant::Legacy),
        );
        let dir = dir_of(&w, &reg, &bad);
        put(&dir.join("data/a.bin"), "XX");
        std::fs::remove_file(dir.join("game.exe")).unwrap();
        put(&dir.join("cache.tmp"), "game cache");

        let r = reg
            .repair(&ctx, &bad, &RepairSource::Build(good.clone()))
            .unwrap();
        let mut restored = r.restored.clone();
        restored.sort();
        assert_eq!(restored, vec!["data/a.bin", "game.exe"]);
        assert_eq!(r.state_after, State::Ready);
        assert_eq!(r.extra, vec!["cache.tmp"], "extra files are reported");
        assert!(dir.join("cache.tmp").is_file(), "and left alone");
        assert_eq!(
            std::fs::read_to_string(dir.join("data/a.bin")).unwrap(),
            "AAAA"
        );
        assert!(r.unrepairable.is_empty());

        // The changed file was moved aside, not overwritten or deleted.
        assert_eq!(r.moved_aside.len(), 1);
        let (rel, kept) = &r.moved_aside[0];
        assert_eq!(rel, "data/a.bin");
        assert_eq!(std::fs::read_to_string(kept).unwrap(), "XX");
        assert!(kept.starts_with(dir.parent().unwrap().join(format!("{bad}.recovery"))));

        // Nothing left to do the second time.
        let again = reg.repair(&ctx, &bad, &RepairSource::Build(good)).unwrap();
        assert!(again.restored.is_empty() && again.moved_aside.is_empty());
        assert!(w.journal.pending().unwrap().is_empty());
    }

    #[test]
    fn repair_never_installs_content_that_differs_from_the_manifest() {
        let w = world();
        let mut reg = w.registry();
        let ctx = w.ctx();
        let (src, dst) = (
            import(&w, &mut reg, Variant::Legacy),
            import(&w, &mut reg, Variant::Legacy),
        );
        put(&dir_of(&w, &reg, &src).join("game.exe"), "a different exe");
        std::fs::remove_file(dir_of(&w, &reg, &dst).join("game.exe")).unwrap();
        std::fs::remove_file(dir_of(&w, &reg, &dst).join("config.ini")).unwrap();

        let r = reg.repair(&ctx, &dst, &RepairSource::Build(src)).unwrap();
        assert_eq!(r.restored, vec!["config.ini"]);
        assert_eq!(r.unrepairable.len(), 1);
        assert_eq!(r.unrepairable[0].0, "game.exe");
        assert!(!dir_of(&w, &reg, &dst).join("game.exe").exists());
        assert_eq!(r.state_after, State::Failed);
    }

    #[test]
    fn repair_can_use_the_original_install_as_a_read_only_source() {
        let w = world();
        let before = snapshot(&w.install);
        let mut reg = w.registry();
        let ctx = w.ctx();
        let b = import(&w, &mut reg, Variant::Legacy);
        std::fs::remove_file(dir_of(&w, &reg, &b).join("data/b.bin")).unwrap();
        let r = reg
            .repair(&ctx, &b, &RepairSource::Folder(w.install.clone()))
            .unwrap();
        assert_eq!(r.restored, vec!["data/b.bin"]);
        assert_eq!(snapshot(&w.install), before);
    }

    #[test]
    fn repair_refuses_itself_an_overlapping_source_and_unknown_builds() {
        let w = world();
        let mut reg = w.registry();
        let ctx = w.ctx();
        let b = import(&w, &mut reg, Variant::Legacy);
        assert!(
            reg.repair(&ctx, &b, &RepairSource::Build(b.clone()))
                .is_err()
        );
        let own_parent = dir_of(&w, &reg, &b).parent().unwrap().to_path_buf();
        assert!(
            reg.repair(&ctx, &b, &RepairSource::Folder(own_parent))
                .is_err()
        );
        assert!(
            reg.repair(&ctx, &b, &RepairSource::Build("nope".into()))
                .is_err()
        );
        assert!(
            reg.repair(&ctx, "nope", &RepairSource::Folder(w.install.clone()))
                .is_err()
        );
    }

    #[test]
    fn a_forged_manifest_cannot_make_repair_write_outside_the_build() {
        let w = world();
        let mut reg = w.registry();
        let ctx = w.ctx();
        let b = import(&w, &mut reg, Variant::Legacy);
        let mpath = reg.manifest_file(&ctx, reg.get(&b).unwrap()).unwrap();
        let forged = std::fs::read_to_string(&mpath)
            .unwrap()
            .replace("data/a.bin", "../../../escape.bin");
        std::fs::write(&mpath, forged).unwrap();

        assert!(matches!(reg.verify(&ctx, &b), Err(Error::UnsafePath(_))));
        assert!(
            reg.repair(&ctx, &b, &RepairSource::Folder(w.install.clone()))
                .is_err()
        );
        assert!(!w.base.join("ssd").join("escape.bin").exists());
        assert!(!w.base.join("escape.bin").exists());
    }

    // ---- migrate ----

    #[test]
    fn migrating_copies_verifies_switches_and_keeps_the_old_copy() {
        let w = world();
        let mut reg = w.registry();
        let ctx = w.ctx();
        let b = import(&w, &mut reg, Variant::Legacy);
        reg.activate(&ctx, &b).unwrap();
        let old_dir = dir_of(&w, &reg, &b);
        let before = snapshot(&old_dir);

        reg.migrate_build(&ctx, &b, "hdd").unwrap();

        let new_dir = dir_of(&w, &reg, &b);
        assert!(new_dir.starts_with(w.base.join("hdd")));
        assert_eq!(snapshot(&new_dir), before);
        assert_eq!(
            snapshot(&old_dir),
            before,
            "the old copy is untouched and still there"
        );
        assert_eq!(reg.retired().len(), 1);
        assert_eq!(reg.retired()[0].build_id, b);
        assert!(
            matches!(&reg.retired()[0].location, Location::Managed { root_id, .. } if root_id == "ssd")
        );
        assert_eq!(
            reg.active("terraria", "legacy").unwrap().build_id,
            b,
            "slot pointer unchanged"
        );
        assert_eq!(reg.verify(&ctx, &b).unwrap().state, State::Ready);
        assert_eq!(
            w.registry().get(&b).unwrap().location,
            reg.get(&b).unwrap().location,
            "saved"
        );
        assert!(w.journal.pending().unwrap().is_empty());
        // The root that no longer holds it can be forgotten; the one that does cannot.
        assert!(reg.builds_in_root("ssd").is_empty());
        assert_eq!(reg.builds_in_root("hdd"), vec![b]);
    }

    #[test]
    fn a_migration_that_cannot_finish_leaves_the_build_where_it_was() {
        let w = world();
        let mut reg = w.registry();
        let ctx = w.ctx();
        let b = import(&w, &mut reg, Variant::Legacy);
        let location = reg.get(&b).unwrap().location.clone();

        // Not enough room on the target.
        w.disk.0.set(10);
        assert!(matches!(
            reg.migrate_build(&ctx, &b, "hdd"),
            Err(Error::InsufficientSpace { .. })
        ));
        w.disk.0.set(u64::MAX / 2);
        assert!(!w.base.join("hdd/builds").exists());

        // The copy comes out wrong.
        let err = reg
            .migrate_inner(&ctx, &b, "hdd", &mut |dst| {
                put(&dst.join("game.exe"), "bad")
            })
            .unwrap_err();
        assert!(err.to_string().contains("does not match"), "{err}");
        assert_eq!(reg.get(&b).unwrap().location, location);
        assert!(reg.retired().is_empty());
        assert_eq!(reg.verify(&ctx, &b).unwrap().state, State::Ready);
        assert!(w.journal.pending().unwrap().is_empty());
    }

    #[test]
    fn migration_refuses_the_same_root_unknown_roots_and_damaged_builds() {
        let w = world();
        let mut reg = w.registry();
        let ctx = w.ctx();
        let b = import(&w, &mut reg, Variant::Legacy);
        assert!(reg.migrate_build(&ctx, &b, "ssd").is_err());
        assert!(reg.migrate_build(&ctx, &b, "nope").is_err());
        assert!(reg.migrate_build(&ctx, "nope", "hdd").is_err());
        std::fs::remove_file(dir_of(&w, &reg, &b).join("game.exe")).unwrap();
        assert!(reg.migrate_build(&ctx, &b, "hdd").is_err());
        assert!(!w.base.join("hdd/builds").exists());
    }

    // ---- retire ----

    #[test]
    fn retiring_only_edits_the_registry_and_refuses_builds_still_in_use() {
        let w = world();
        let mut reg = w.registry();
        let ctx = w.ctx();
        let (a, b, c) = (
            import(&w, &mut reg, Variant::Legacy),
            import(&w, &mut reg, Variant::Legacy),
            import(&w, &mut reg, Variant::Legacy),
        );
        reg.activate(&ctx, &a).unwrap();
        reg.activate(&ctx, &b).unwrap();

        assert!(reg.retire(&ctx, &b, "old").is_err(), "active");
        assert!(
            reg.retire(&ctx, &a, "old").is_err(),
            "in the rollback history"
        );
        assert!(reg.retire(&ctx, &c, "  ").is_err(), "needs a reason");
        let dir = dir_of(&w, &reg, &c);
        reg.retire(&ctx, &c, "superseded").unwrap();
        assert!(reg.get(&c).is_none());
        assert!(
            dir.join("game.exe").is_file(),
            "the files are still on disk"
        );
        assert_eq!(reg.retired()[0].reason, "superseded");
        // The id is not handed out again while its folder exists.
        assert_eq!(import(&w, &mut reg, Variant::Legacy), "terraria-legacy-4");
    }

    // ---- the registry file ----

    #[test]
    fn a_corrupt_or_newer_registry_is_an_error_not_an_empty_registry() {
        let w = world();
        std::fs::create_dir_all(w.reg.parent().unwrap()).unwrap();
        std::fs::write(&w.reg, "{ nope").unwrap();
        assert!(BuildRegistry::open(&w.reg).is_err());
        std::fs::write(
            &w.reg,
            r#"{"schema":9,"builds":[],"active":[],"retired":[]}"#,
        )
        .unwrap();
        assert!(BuildRegistry::open(&w.reg).is_err());
        assert!(
            std::fs::read_to_string(&w.reg)
                .unwrap()
                .contains("\"schema\":9")
        );
    }
}
