//! The service behind the app's commands for Module 1 (spec 1.2, 1.3 and 1.5): library folders and
//! managed builds.
//!
//! The rules are in [`crate::roots`] and [`crate::builds`]. This module only owns what every operation
//! needs (the library roots, the protected locations, the journal, the disk-space reader and the build
//! registry) behind one lock, checks the arguments, and returns plain views the window can show. It adds
//! no new way to touch a file: every change is made by a function those modules already tested.
//!
//! * **Original installs are protected before anything is read near them.** The caller (the app) tells the
//!   service which folders the library lists as installs ([`ManagerService::protect_installs`]), and an
//!   import or a register-in-place also protects the folder it was asked to use. The source of an import is
//!   always a library game's install folder, supplied by the app from the library, never typed by a person.
//! * **One operation at a time.** A long copy holds the lock, so a second request waits. Nothing reports
//!   progress while a copy runs.
//! * Nothing here deletes anything. Forgetting a folder edits SUL's list only.
//!
//! Everything is tested on made-up folders. It has not been used on a real game.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

use crate::adapters::AdapterRegistry;
use crate::builder::book::RecipeBook;
use crate::builder::tools::ToolRegistry;
use crate::builds::{
    ActiveSlot, BuildCheck, BuildRecord, BuildRegistry, Ctx, ImportRequest, RegisterRequest,
    RetiredCopy,
};
use crate::fsops::ProtectedRoots;
use crate::journal::Journal;
use crate::manifest::Variant;
use crate::mods::profile::Profiles;
use crate::mods::store::ModStore;
use crate::process::{ProcessLauncher, StdLauncher};
use crate::roots::{DiskSpace, LibraryRoots, Root, RootKind, SystemDiskSpace};
use crate::{Error, Result};

mod building;
mod modding;
mod tooling;
pub use building::*;
pub use modding::*;
pub use tooling::*;

/// The folder (beside the library file) that holds this service's state.
pub const STATE_FOLDER: &str = "managed";

/// A library root, with what can be seen of it right now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RootView {
    #[serde(flatten)]
    pub root: Root,
    /// The folder exists.
    pub present: bool,
    /// The folder holds this root's marker file. `false` while `present` means another folder or drive is there.
    pub marker_ok: bool,
    pub free_bytes: Option<u64>,
}

/// A build, and whether it is the active one of its slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuildView {
    #[serde(flatten)]
    pub record: BuildRecord,
    pub active: bool,
}

/// An operation that began and never finished (a crash or a power cut). Shown so the owner knows;
/// SUL does not repair anything by itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InterruptedView {
    pub id: String,
    pub op: String,
    pub detail: String,
}

/// Everything Module 1 shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ManagerView {
    pub roots: Vec<RootView>,
    pub builds: Vec<BuildView>,
    pub slots: Vec<ActiveSlot>,
    /// Old copies SUL no longer manages. They are only listed; SUL never deletes them.
    pub retired: Vec<RetiredCopy>,
    pub interrupted: Vec<InterruptedView>,
}

/// What a check of one build found, with the view afterwards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerifiedView {
    pub check: BuildCheck,
    pub view: ManagerView,
}

/// A new build, and the view afterwards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuiltView {
    pub build_id: String,
    pub view: ManagerView,
}

/// What an import or a register-in-place is asked to do. The folder comes from the library, not from a person.
#[derive(Debug, Clone, Deserialize)]
pub struct BuildSource {
    pub game_id: String,
    pub display_name: String,
    pub install_dir: PathBuf,
    pub variant: Variant,
    pub base_version: String,
    pub source_description: String,
}

struct Inner {
    roots: LibraryRoots,
    protected: ProtectedRoots,
    protected_dirs: BTreeSet<PathBuf>,
    journal: Journal,
    builds: BuildRegistry,
    disk: Box<dyn DiskSpace + Send>,
    store: ModStore,
    profiles: Profiles,
    recipes: RecipeBook,
    adapters: AdapterRegistry,
    tools: ToolRegistry,
    /// What starts programs. Only the app's own constructor gives one that does.
    launcher: Box<dyn ProcessLauncher + Send>,
    /// Tools SUL started and is waiting for, by the number it gave each.
    running: BTreeMap<u64, tooling::Following>,
    next_run: u64,
}

pub struct ManagerService {
    inner: Mutex<Inner>,
}

impl ManagerService {
    /// Open the service with its state in `dir` (`roots.json`, `builds.json`, `mods.json`, `profiles.json` and
    /// `journal.jsonl`), reading the free space of the real disks. A missing file is an empty list; a file that
    /// cannot be read is an error and is never replaced. No game adapter is registered: no adapter for a real
    /// game exists yet, so nothing can be deployed (see [`with_parts`](Self::with_parts)).
    pub fn open(dir: &Path) -> Result<Self> {
        Self::with_parts_and_launcher(
            dir,
            Box::new(SystemDiskSpace),
            AdapterRegistry::new(),
            Box::new(StdLauncher),
        )
    }

    /// As [`open`](Self::open) with the given disk-space reader (tests).
    pub fn with_disk(dir: &Path, disk: Box<dyn DiskSpace + Send>) -> Result<Self> {
        Self::with_parts(dir, disk, AdapterRegistry::new())
    }

    /// As [`open`](Self::open) with a disk-space reader and the game adapters this build of SUL knows. A game with
    /// no adapter has no mod layout, and a mod is never deployed into a game without one.
    ///
    /// The service made here starts no programs (a tool cannot be run): only [`open`](Self::open) and
    /// [`with_parts_and_launcher`](Self::with_parts_and_launcher) give it a launcher that does.
    pub fn with_parts(
        dir: &Path,
        disk: Box<dyn DiskSpace + Send>,
        adapters: AdapterRegistry,
    ) -> Result<Self> {
        Self::with_parts_and_launcher(dir, disk, adapters, Box::new(tooling::NoLauncher))
    }

    /// As [`with_parts`](Self::with_parts) with what starts programs for the Tools tab (the app gives the real one;
    /// tests give one that starts nothing).
    pub fn with_parts_and_launcher(
        dir: &Path,
        disk: Box<dyn DiskSpace + Send>,
        adapters: AdapterRegistry,
        launcher: Box<dyn ProcessLauncher + Send>,
    ) -> Result<Self> {
        Ok(Self {
            inner: Mutex::new(Inner {
                roots: LibraryRoots::load(&dir.join("roots.json"))?,
                protected: ProtectedRoots::new(),
                protected_dirs: BTreeSet::new(),
                journal: Journal::open(&dir.join("journal.jsonl"))?,
                builds: BuildRegistry::open(&dir.join("builds.json"))?,
                disk,
                store: ModStore::open(&dir.join("mods.json"))?,
                profiles: Profiles::open(&dir.join("profiles.json"))?,
                recipes: RecipeBook::open(&dir.join("recipes.json"))?,
                adapters,
                tools: ToolRegistry::open(&dir.join("tools.json"))?,
                launcher,
                running: BTreeMap::new(),
                next_run: 1,
            }),
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, Inner>> {
        self.inner.lock().map_err(|_| {
            Error::Invalid("the build manager stopped working; restart the app".into())
        })
    }

    /// Tell the service which folders are original installs. Folders that do not exist are skipped. Returns
    /// how many folders are protected now.
    pub fn protect_installs(&self, dirs: &[PathBuf]) -> Result<usize> {
        let mut g = self.lock()?;
        for d in dirs {
            protect(&mut g, d).ok();
        }
        Ok(g.protected_dirs.len())
    }

    /// Everything Module 1 shows.
    pub fn view(&self) -> Result<ManagerView> {
        let g = self.lock()?;
        view_of(&g)
    }

    /// Register a folder as a library root. The folder must exist, may not overlap another root or a protected
    /// location, and gets a marker file (never over an existing file).
    pub fn add_root(
        &self,
        id: &str,
        label: &str,
        path: &Path,
        kind: RootKind,
    ) -> Result<ManagerView> {
        let mut g = self.lock()?;
        let Inner {
            roots, protected, ..
        } = &mut *g;
        roots.add(protected, id, label, path, kind)?;
        view_of(&g)
    }

    /// Point a root at its folder again after the folder or drive moved. Only a folder that carries the right
    /// marker is accepted.
    pub fn relink_root(&self, id: &str, path: &Path) -> Result<ManagerView> {
        let mut g = self.lock()?;
        let Inner {
            roots, protected, ..
        } = &mut *g;
        roots.relink(protected, id, path)?;
        view_of(&g)
    }

    /// Remove a root from SUL's list. The folder and everything in it stay where they are. Refused while a build
    /// is stored there.
    pub fn forget_root(&self, id: &str) -> Result<ManagerView> {
        let mut g = self.lock()?;
        let Inner {
            roots,
            protected,
            builds,
            ..
        } = &mut *g;
        let in_use = builds.builds_in_root(id);
        roots.forget(protected, id, &in_use)?;
        view_of(&g)
    }

    /// Copy a library game's install into the library root `root_id` as a new managed build. The original is
    /// only read, and the copy must match it before the build can be used.
    pub fn import_build(&self, src: &BuildSource, root_id: &str) -> Result<BuiltView> {
        let mut g = self.lock()?;
        protect(&mut g, &src.install_dir)?;
        let Inner {
            roots,
            protected,
            journal,
            builds,
            disk,
            ..
        } = &mut *g;
        let ctx = Ctx {
            roots,
            protected,
            journal,
            disk: &**disk,
        };
        let build_id = builds.import_install(
            &ctx,
            &ImportRequest {
                game_id: src.game_id.clone(),
                display_name: src.display_name.clone(),
                variant: src.variant.clone(),
                base_version: src.base_version.clone(),
                source_description: src.source_description.clone(),
                source_manifest_id: None,
                src: src.install_dir.clone(),
                root_id: root_id.to_owned(),
            },
        )?;
        Ok(BuiltView {
            build_id,
            view: view_of(&g)?,
        })
    }

    /// Record an install that stays where it is, as a read-only baseline. SUL never writes there.
    pub fn register_in_place(&self, src: &BuildSource) -> Result<BuiltView> {
        let mut g = self.lock()?;
        protect(&mut g, &src.install_dir)?;
        let Inner {
            roots,
            protected,
            journal,
            builds,
            disk,
            ..
        } = &mut *g;
        let ctx = Ctx {
            roots,
            protected,
            journal,
            disk: &**disk,
        };
        let build_id = builds.register_in_place(
            &ctx,
            &RegisterRequest {
                game_id: src.game_id.clone(),
                display_name: src.display_name.clone(),
                variant: src.variant.clone(),
                base_version: src.base_version.clone(),
                source_description: src.source_description.clone(),
                path: src.install_dir.clone(),
            },
        )?;
        Ok(BuiltView {
            build_id,
            view: view_of(&g)?,
        })
    }

    /// Compare a build with its manifest. Reads only; it records the result in the registry.
    pub fn verify_build(&self, id: &str) -> Result<VerifiedView> {
        let mut g = self.lock()?;
        let Inner {
            roots,
            protected,
            journal,
            builds,
            disk,
            ..
        } = &mut *g;
        let ctx = Ctx {
            roots,
            protected,
            journal,
            disk: &**disk,
        };
        let check = builds.verify(&ctx, id)?;
        Ok(VerifiedView {
            check,
            view: view_of(&g)?,
        })
    }

    /// Make a build the active one of its slot. It is verified first and must match its manifest.
    pub fn activate_build(&self, id: &str) -> Result<ManagerView> {
        let mut g = self.lock()?;
        let Inner {
            roots,
            protected,
            journal,
            builds,
            disk,
            ..
        } = &mut *g;
        let ctx = Ctx {
            roots,
            protected,
            journal,
            disk: &**disk,
        };
        builds.activate(&ctx, id)?;
        view_of(&g)
    }

    /// Return a slot to the most recent earlier build that still verifies.
    pub fn rollback_slot(&self, game_id: &str, slot: &str) -> Result<ManagerView> {
        let mut g = self.lock()?;
        let Inner {
            roots,
            protected,
            journal,
            builds,
            disk,
            ..
        } = &mut *g;
        let ctx = Ctx {
            roots,
            protected,
            journal,
            disk: &**disk,
        };
        builds.rollback(&ctx, game_id, slot)?;
        view_of(&g)
    }
}

/// Add `dir` to the protected locations once.
fn protect(g: &mut Inner, dir: &Path) -> Result<()> {
    let canonical = dir.canonicalize().map_err(crate::error::io(dir))?;
    if g.protected_dirs.insert(canonical) {
        g.protected.add(dir)?;
    }
    Ok(())
}

/// Every library root with what can be seen of it right now.
fn root_views(g: &Inner) -> Vec<RootView> {
    let status = g.roots.status(&*g.disk);
    g.roots
        .roots()
        .iter()
        .map(|r| {
            let s = status.iter().find(|s| s.id == r.id);
            RootView {
                root: r.clone(),
                present: s.is_some_and(|s| s.present),
                marker_ok: s.is_some_and(|s| s.marker_ok),
                free_bytes: s.and_then(|s| s.free_bytes),
            }
        })
        .collect()
}

fn view_of(g: &Inner) -> Result<ManagerView> {
    let roots = root_views(g);
    let slots = g_slots(g);
    let builds = g
        .builds
        .builds()
        .iter()
        .map(|b| BuildView {
            active: slots
                .iter()
                .any(|s| s.game_id == b.game_id && s.slot == b.slot && s.build_id == b.id),
            record: b.clone(),
        })
        .collect();
    let interrupted = g
        .journal
        .pending()?
        .into_iter()
        .map(|p| InterruptedView {
            id: p.id,
            op: p.op,
            detail: p.detail,
        })
        .collect();
    Ok(ManagerView {
        roots,
        builds,
        slots,
        retired: g.builds.retired().to_vec(),
        interrupted,
    })
}

/// The active slots, found through the registry's public lookups (one per game and slot that has a build).
fn g_slots(g: &Inner) -> Vec<ActiveSlot> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for b in g.builds.builds() {
        if seen.insert((b.game_id.clone(), b.slot.clone())) {
            if let Some(a) = g.builds.active(&b.game_id, &b.slot) {
                out.push(a.clone());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::manifest::State;

    struct Disk(Cell<u64>);
    // `Cell` is not `Sync`, but the service only needs `Send`, and a test disk is used by one thread.
    impl DiskSpace for Disk {
        fn free_bytes(&self, _p: &Path) -> Result<u64> {
            Ok(self.0.get())
        }
    }

    struct Fx {
        _dir: tempfile::TempDir,
        base: PathBuf,
        install: PathBuf,
        svc: ManagerService,
    }

    fn put(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn fx() -> Fx {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        for d in ["ssd", "hdd"] {
            std::fs::create_dir_all(base.join(d)).unwrap();
        }
        let install = base.join("original_install");
        put(&install.join("game.exe"), "exe");
        put(&install.join("data/a.bin"), "AAAA");
        let svc =
            ManagerService::with_disk(&base.join("state"), Box::new(Disk(Cell::new(u64::MAX / 2))))
                .unwrap();
        Fx {
            _dir: dir,
            base,
            install,
            svc,
        }
    }

    fn source(f: &Fx, variant: Variant) -> BuildSource {
        BuildSource {
            game_id: "madeupgame".into(),
            display_name: "Made-up Game".into(),
            install_dir: f.install.clone(),
            variant,
            base_version: "1.0.0".into(),
            source_description: "my own install".into(),
        }
    }

    fn with_root(f: &Fx) {
        f.svc
            .add_root("ssd", "Fast drive", &f.base.join("ssd"), RootKind::Builds)
            .unwrap();
    }

    #[test]
    fn a_new_service_is_empty() {
        let f = fx();
        let v = f.svc.view().unwrap();
        assert!(v.roots.is_empty() && v.builds.is_empty() && v.slots.is_empty());
        assert!(v.retired.is_empty() && v.interrupted.is_empty());
    }

    #[test]
    fn a_root_is_listed_with_what_can_be_seen_of_it() {
        let f = fx();
        let v = f
            .svc
            .add_root("ssd", "Fast drive", &f.base.join("ssd"), RootKind::Builds)
            .unwrap();
        assert_eq!(v.roots.len(), 1);
        let r = &v.roots[0];
        assert_eq!(
            (r.root.id.as_str(), r.root.label.as_str()),
            ("ssd", "Fast drive")
        );
        assert!(r.present && r.marker_ok);
        assert_eq!(r.free_bytes, Some(u64::MAX / 2));
        // The folder gets its marker file; nothing else is written there.
        assert!(f.base.join("ssd/.sul-root.json").is_file());
    }

    #[test]
    fn a_root_inside_a_protected_install_is_refused() {
        let f = fx();
        f.svc
            .protect_installs(std::slice::from_ref(&f.install))
            .unwrap();
        let inside = f.install.join("data");
        let err = f
            .svc
            .add_root("bad", "Bad", &inside, RootKind::Builds)
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("protect"), "{err}");
        assert!(f.svc.view().unwrap().roots.is_empty());
        assert!(!inside.join(".sul-root.json").exists());
    }

    #[test]
    fn a_missing_folder_is_not_made_a_root() {
        let f = fx();
        assert!(
            f.svc
                .add_root(
                    "nope",
                    "Nope",
                    &f.base.join("does-not-exist"),
                    RootKind::Builds
                )
                .is_err()
        );
        assert!(f.svc.view().unwrap().roots.is_empty());
    }

    #[test]
    fn an_import_makes_a_ready_build_and_leaves_the_original_alone() {
        let f = fx();
        with_root(&f);
        let before = std::fs::read_to_string(f.install.join("data/a.bin")).unwrap();
        let out = f
            .svc
            .import_build(&source(&f, Variant::Original), "ssd")
            .unwrap();
        let b = out
            .view
            .builds
            .iter()
            .find(|b| b.record.id == out.build_id)
            .unwrap();
        assert_eq!(b.record.state, State::Ready);
        assert!(!b.active, "importing does not switch anything on");
        assert_eq!(
            std::fs::read_to_string(f.install.join("data/a.bin")).unwrap(),
            before
        );
        // The original is protected from now on.
        assert!(
            f.svc
                .add_root(
                    "inside",
                    "Inside",
                    &f.install.join("data"),
                    RootKind::Builds
                )
                .is_err()
        );
    }

    #[test]
    fn activating_and_rolling_back_move_the_active_mark() {
        let f = fx();
        with_root(&f);
        let a = f
            .svc
            .import_build(&source(&f, Variant::Optimized), "ssd")
            .unwrap()
            .build_id;
        let v = f.svc.activate_build(&a).unwrap();
        assert!(v.builds.iter().any(|b| b.record.id == a && b.active));
        assert_eq!(v.slots.len(), 1);

        let b = f
            .svc
            .import_build(&source(&f, Variant::Optimized), "ssd")
            .unwrap()
            .build_id;
        let v = f.svc.activate_build(&b).unwrap();
        assert!(v.builds.iter().any(|x| x.record.id == b && x.active));
        assert!(v.builds.iter().any(|x| x.record.id == a && !x.active));
        // Two builds share the slot, but the slot is listed once, and it points at the build now active.
        assert_eq!(v.builds.len(), 2);
        assert_eq!(v.slots.len(), 1);
        assert_eq!(v.slots[0].build_id, b);
        assert_eq!(v.slots[0].history, vec![a.clone()]);

        let v = f.svc.rollback_slot("madeupgame", "optimized").unwrap();
        assert!(v.builds.iter().any(|x| x.record.id == a && x.active));
    }

    #[test]
    fn a_changed_build_does_not_verify_and_cannot_be_activated() {
        let f = fx();
        with_root(&f);
        let out = f
            .svc
            .import_build(&source(&f, Variant::Original), "ssd")
            .unwrap();
        let id = out.build_id;
        let dir = out
            .view
            .roots
            .iter()
            .find(|r| r.root.id == "ssd")
            .unwrap()
            .root
            .path
            .clone();
        // Change a file inside the managed copy (never the original).
        let changed = walk_find(&dir, "a.bin");
        std::fs::write(&changed, "TAMPERED").unwrap();
        let v = f.svc.verify_build(&id).unwrap();
        assert_eq!(v.check.report.modified.len(), 1);
        assert!(f.svc.activate_build(&id).is_err());
        assert_eq!(
            std::fs::read_to_string(f.install.join("data/a.bin")).unwrap(),
            "AAAA"
        );
    }

    fn walk_find(dir: &Path, name: &str) -> PathBuf {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                let found = walk_find(&p, name);
                if found != PathBuf::new() {
                    return found;
                }
            } else if p.file_name().is_some_and(|n| n == name) {
                return p;
            }
        }
        PathBuf::new()
    }

    #[test]
    fn registering_in_place_records_a_read_only_baseline() {
        let f = fx();
        let out = f
            .svc
            .register_in_place(&source(&f, Variant::Original))
            .unwrap();
        let b = out
            .view
            .builds
            .iter()
            .find(|b| b.record.id == out.build_id)
            .unwrap();
        assert!(matches!(
            b.record.location,
            crate::builds::Location::ExternalInstall { .. }
        ));
        // Nothing was written into the install.
        assert!(!f.install.join(".sul-root.json").exists());
        let names: Vec<_> = std::fs::read_dir(&f.install)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 2, "{names:?}");
    }

    #[test]
    fn an_install_that_is_not_there_is_not_imported_or_registered() {
        let f = fx();
        with_root(&f);
        let mut s = source(&f, Variant::Original);
        s.install_dir = f.base.join("gone");
        assert!(f.svc.import_build(&s, "ssd").is_err());
        assert!(f.svc.register_in_place(&s).is_err());
        assert!(f.svc.view().unwrap().builds.is_empty());
    }

    #[test]
    fn a_root_that_holds_a_build_cannot_be_forgotten_but_an_empty_one_can() {
        let f = fx();
        with_root(&f);
        f.svc
            .add_root("hdd", "Big drive", &f.base.join("hdd"), RootKind::Builds)
            .unwrap();
        f.svc
            .import_build(&source(&f, Variant::Original), "ssd")
            .unwrap();
        let err = f.svc.forget_root("ssd").unwrap_err().to_string();
        assert!(err.contains("still holds"), "{err}");
        let v = f.svc.forget_root("hdd").unwrap();
        assert_eq!(v.roots.len(), 1);
        // Forgetting leaves the folder and its marker.
        assert!(f.base.join("hdd/.sul-root.json").is_file());
    }

    #[test]
    fn state_survives_opening_the_service_again() {
        let f = fx();
        with_root(&f);
        let id = f
            .svc
            .import_build(&source(&f, Variant::Legacy), "ssd")
            .unwrap()
            .build_id;
        f.svc.activate_build(&id).unwrap();
        let again =
            ManagerService::with_disk(&f.base.join("state"), Box::new(Disk(Cell::new(1)))).unwrap();
        let v = again.view().unwrap();
        assert_eq!(v.roots.len(), 1);
        assert!(v.builds.iter().any(|b| b.record.id == id && b.active));
    }

    #[test]
    fn an_unreadable_state_file_is_an_error_not_a_reset() {
        let f = fx();
        with_root(&f);
        std::fs::write(f.base.join("state/builds.json"), b"{ not json").unwrap();
        assert!(
            ManagerService::with_disk(&f.base.join("state"), Box::new(Disk(Cell::new(1)))).is_err()
        );
        assert_eq!(
            std::fs::read(f.base.join("state/builds.json")).unwrap(),
            b"{ not json"
        );
    }

    #[test]
    fn an_operation_that_never_finished_is_shown() {
        let f = fx();
        let j = Journal::open(&f.base.join("state/journal.jsonl")).unwrap();
        j.begin("op-1", "purge_profile", "x from y").unwrap();
        let v = f.svc.view().unwrap();
        assert_eq!(v.interrupted.len(), 1);
        assert_eq!(v.interrupted[0].op, "purge_profile");
    }

    #[test]
    fn a_folder_that_went_missing_is_shown_as_missing_and_a_stranger_in_its_place_as_not_ours() {
        let f = fx();
        with_root(&f);
        let ssd = f.base.join("ssd");
        let moved = f.base.join("ssd-moved");
        std::fs::rename(&ssd, &moved).unwrap();
        let v = f.svc.view().unwrap();
        assert!(!v.roots[0].present, "a folder that is gone is not present");
        assert!(!v.roots[0].marker_ok);

        // Another folder at the old path: it is there, but it is not this root.
        std::fs::create_dir(&ssd).unwrap();
        let v = f.svc.view().unwrap();
        assert!(v.roots[0].present);
        assert!(
            !v.roots[0].marker_ok,
            "a folder without the marker must not pass for the root"
        );
    }

    #[test]
    fn a_moved_folder_is_found_again_by_pointing_the_root_at_it() {
        let f = fx();
        with_root(&f);
        let moved = f.base.join("ssd-moved");
        std::fs::rename(f.base.join("ssd"), &moved).unwrap();
        assert!(!f.svc.view().unwrap().roots[0].present);

        let v = f.svc.relink_root("ssd", &moved).unwrap();
        assert_eq!(v.roots[0].root.path, moved);
        assert!(v.roots[0].present && v.roots[0].marker_ok);
        // The new place is what is saved.
        let again =
            ManagerService::with_disk(&f.base.join("state"), Box::new(Disk(Cell::new(1)))).unwrap();
        assert_eq!(again.view().unwrap().roots[0].root.path, moved);
    }

    #[test]
    fn a_root_is_not_pointed_at_a_folder_that_is_not_it_and_nothing_is_written_there() {
        let f = fx();
        with_root(&f);
        // `hdd` exists but carries no marker, so it may not stand in for `ssd`.
        let err = f
            .svc
            .relink_root("ssd", &f.base.join("hdd"))
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("no .sul-root.json") || err.contains("marker"),
            "{err}"
        );
        assert!(!f.base.join("hdd/.sul-root.json").exists());
        assert_eq!(f.svc.view().unwrap().roots[0].root.path, f.base.join("ssd"));

        // A folder that is another listed root is refused (a file may belong to only one root).
        f.svc
            .add_root("hdd", "Big drive", &f.base.join("hdd"), RootKind::Builds)
            .unwrap();
        let err = f
            .svc
            .relink_root("ssd", &f.base.join("hdd"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("overlaps"), "{err}");
        // So is a folder that carries another root's marker once that root is no longer listed.
        f.svc.forget_root("hdd").unwrap();
        let err = f
            .svc
            .relink_root("ssd", &f.base.join("hdd"))
            .unwrap_err()
            .to_string();
        assert!(
            err.contains(r#"is the library root "hdd", not "ssd""#),
            "{err}"
        );
        assert_eq!(f.svc.view().unwrap().roots[0].root.path, f.base.join("ssd"));
        assert!(f.svc.relink_root("nothing", &f.base.join("ssd")).is_err());
    }

    #[test]
    fn copies_that_are_no_longer_managed_stay_listed() {
        let f = fx();
        with_root(&f);
        let a = f
            .svc
            .import_build(&source(&f, Variant::Legacy), "ssd")
            .unwrap()
            .build_id;
        {
            let mut g = f.svc.lock().unwrap();
            let Inner {
                roots,
                protected,
                journal,
                builds,
                disk,
                ..
            } = &mut *g;
            let ctx = Ctx {
                roots,
                protected,
                journal,
                disk: &**disk,
            };
            builds.retire(&ctx, &a, "superseded").unwrap();
        }
        let v = f.svc.view().unwrap();
        assert!(v.builds.is_empty());
        assert_eq!(v.retired.len(), 1);
        assert_eq!(
            (v.retired[0].build_id.as_str(), v.retired[0].reason.as_str()),
            (a.as_str(), "superseded")
        );
    }

    #[test]
    fn protecting_the_same_folder_twice_counts_once_and_missing_folders_are_skipped() {
        let f = fx();
        let n = f
            .svc
            .protect_installs(&[f.install.clone(), f.install.clone(), f.base.join("gone")])
            .unwrap();
        assert_eq!(n, 1);
    }
}
