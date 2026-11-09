//! The mod half of the service (spec 2.1): the mod store, profiles, and the preview, deploy, purge and undo of a
//! profile in a managed build.
//!
//! As in [`super`], the rules live elsewhere ([`crate::mods`]); this file owns nothing new that can touch a
//! file. It finds the pieces an operation needs, refuses what the modules cannot see (a build of another game,
//! a profile moved to another build while it is deployed, a game with no adapter) and returns plain views.
//!
//! * **A game with no adapter takes no mods.** The folders a mod may change come from the game's adapter
//!   ([`crate::adapters::ModLayout`]). With no adapter there is no layout, so a preview or a deploy is refused
//!   with that reason. No adapter for a real game exists yet, so in the app every deploy is refused today.
//!   A purge and an undo need no layout: they follow the ownership ledger.
//! * **A preview is never trusted.** The core makes its plan again when it deploys.
//! * **Editing a profile touches no game file.** Moving a profile to another build is refused while it is
//!   deployed in the current one, because the purge would no longer be reachable from the profile.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::{Inner, ManagerService, RootView, root_views};
use crate::adapters::ModLayout;
use crate::builds::{BuildRecord, Ctx, Location};
use crate::manifest::VerifyReport;
use crate::mods::deploy::{
    self, DeployPlan, DeployReport, DeployStatus, HistoryItem, PurgeAndDeployReport, PurgePlan,
    PurgeReport, UndoReport,
};
use crate::mods::profile::{Problem, ProblemKind, Profile, check_profile};
use crate::mods::store::{ModImport, ModRecord, Requirement};
use crate::roots::RootKind;
use crate::{Error, Result};

/// An imported mod, without its file list (a mod can have tens of thousands of files).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModView {
    pub game_id: String,
    pub mod_id: String,
    pub name: String,
    pub version: String,
    pub requires: Vec<Requirement>,
    /// Game versions the mod is said to work with. Empty means nobody said; it is never read as "any".
    pub compatible_versions: Vec<String>,
    pub file_count: usize,
    pub total_bytes: u64,
    pub source: String,
    pub license_note: String,
    pub imported_unix: u64,
    pub root_id: String,
}

impl ModView {
    pub(super) fn of(rec: &ModRecord) -> Self {
        let m = &rec.manifest;
        Self {
            game_id: m.game_id.clone(),
            mod_id: m.mod_id.clone(),
            name: m.name.clone(),
            version: m.version.clone(),
            requires: m.requires.clone(),
            compatible_versions: m.compatible_versions.clone(),
            file_count: m.files.len(),
            total_bytes: m.files.iter().map(|f| f.size).sum(),
            source: m.source.clone(),
            license_note: m.license_note.clone(),
            imported_unix: m.imported_unix,
            root_id: rec.root_id.clone(),
        }
    }
}

/// A profile with everything wrong or doubtful about it right now. The problems are worked out from the profile
/// and the store only; a preview adds what the build and the disk say.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProfileView {
    #[serde(flatten)]
    pub profile: Profile,
    pub problems: Vec<Problem>,
}

/// What is deployed in a build (or was started there and not finished).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeploymentView {
    pub id: String,
    pub profile_id: String,
    pub profile_name: String,
    pub status: DeployStatus,
    pub started_unix: u64,
    pub finished_unix: Option<u64>,
    pub files: usize,
}

/// The latest purge of a build, which can still be undone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PurgeSummary {
    pub id: String,
    pub profile_name: String,
    pub unix: u64,
    pub files: usize,
}

/// The mod state of one managed build. `error` is set when it could not be read (a drive that is unplugged, a
/// damaged state file); the rest is then empty rather than guessed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuildModsView {
    pub build_id: String,
    pub game_id: String,
    pub slot: String,
    pub deployment: Option<DeploymentView>,
    pub undoable_purge: Option<PurgeSummary>,
    pub history: Vec<HistoryItem>,
    pub error: Option<String>,
}

/// Everything the Mods tab shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModsView {
    pub mods: Vec<ModView>,
    pub profiles: Vec<ProfileView>,
    /// One entry for every build SUL manages as a copy (an install that stays where it is takes no mods).
    pub builds: Vec<BuildModsView>,
    /// The library folders that hold mods.
    pub mod_roots: Vec<RootView>,
    /// Games with an adapter, which are the only games mods are deployed for.
    pub deployable_games: Vec<String>,
}

/// A mod folder to import. The folder is only read.
#[derive(Debug, Clone, Deserialize)]
pub struct ModSource {
    pub game_id: String,
    pub mod_id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub requires: Vec<Requirement>,
    #[serde(default)]
    pub compatible_versions: Vec<String>,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub license_note: String,
    pub path: PathBuf,
}

/// One change to a profile.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ProfileEdit {
    Rename {
        name: String,
    },
    Notes {
        notes: String,
    },
    SetBuild {
        build_id: String,
    },
    AddMod {
        mod_id: String,
        version: String,
        position: Option<usize>,
    },
    RemoveMod {
        mod_id: String,
    },
    SetEnabled {
        mod_id: String,
        enabled: bool,
    },
    SetVersion {
        mod_id: String,
        version: String,
    },
    Move {
        mod_id: String,
        to_index: usize,
    },
    SetConfig {
        key: String,
        value: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModVerified {
    pub report: VerifyReport,
    pub view: ModsView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Deployed {
    pub report: DeployReport,
    pub view: ModsView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Purged {
    pub report: PurgeReport,
    pub view: ModsView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Undone {
    pub report: UndoReport,
    pub view: ModsView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PurgedAndDeployed {
    pub report: PurgeAndDeployReport,
    pub view: ModsView,
}

impl ManagerService {
    /// Everything the Mods tab shows.
    pub fn mods_view(&self) -> Result<ModsView> {
        let g = self.lock()?;
        mods_view_of(&g)
    }

    /// Copy a mod folder into the mod store as a new, verified version. The folder is only read; a version is
    /// never imported over another.
    pub fn import_mod(&self, src: &ModSource, root_id: &str) -> Result<ModsView> {
        let mut g = self.lock()?;
        let Inner {
            roots,
            protected,
            journal,
            disk,
            store,
            ..
        } = &mut *g;
        let ctx = Ctx {
            roots,
            protected,
            journal,
            disk: &**disk,
        };
        store.import_folder(
            &ctx,
            &ModImport {
                game_id: src.game_id.clone(),
                mod_id: src.mod_id.clone(),
                name: src.name.clone(),
                version: src.version.clone(),
                requires: src.requires.clone(),
                compatible_versions: src.compatible_versions.clone(),
                source: src.source.clone(),
                license_note: src.license_note.clone(),
                src: src.path.clone(),
                root_id: root_id.to_owned(),
            },
        )?;
        mods_view_of(&g)
    }

    /// Compare a stored mod with its own inventory. Reads only.
    pub fn verify_mod(&self, game_id: &str, mod_id: &str, version: &str) -> Result<ModVerified> {
        let g = self.lock()?;
        let ctx = ctx_of(&g);
        let rec = g
            .store
            .get(game_id, mod_id, version)
            .ok_or_else(|| Error::Invalid(format!("{mod_id}@{version} is not in the mod store")))?;
        let report = g.store.verify(&ctx, rec)?;
        Ok(ModVerified {
            report,
            view: mods_view_of(&g)?,
        })
    }

    /// Make a profile for one game and one of its builds. Editing profiles changes only the profile file.
    pub fn create_profile(
        &self,
        id: &str,
        name: &str,
        game_id: &str,
        build_id: &str,
    ) -> Result<ModsView> {
        let mut g = self.lock()?;
        let build = g
            .builds
            .get(build_id)
            .ok_or_else(|| Error::Invalid(format!("no build with id {build_id:?}")))?;
        if build.game_id != game_id {
            return Err(Error::Invalid(
                "that build belongs to another game than the profile".into(),
            ));
        }
        let Inner {
            protected,
            profiles,
            ..
        } = &mut *g;
        profiles.create(protected, id, name, game_id, build_id)?;
        mods_view_of(&g)
    }

    /// Change one profile. Nothing is deployed or purged, and no game file is touched.
    pub fn edit_profile(&self, id: &str, edit: &ProfileEdit) -> Result<ModsView> {
        let mut g = self.lock()?;
        if let ProfileEdit::SetBuild { build_id } = edit {
            check_new_build(&g, id, build_id)?;
        }
        let Inner {
            protected,
            profiles,
            ..
        } = &mut *g;
        match edit {
            ProfileEdit::Rename { name } => profiles.rename(protected, id, name)?,
            ProfileEdit::Notes { notes } => profiles.set_notes(protected, id, notes)?,
            ProfileEdit::SetBuild { build_id } => profiles.set_build(protected, id, build_id)?,
            ProfileEdit::AddMod {
                mod_id,
                version,
                position,
            } => profiles.add_mod(protected, id, mod_id, version, *position)?,
            ProfileEdit::RemoveMod { mod_id } => profiles.remove_mod(protected, id, mod_id)?,
            ProfileEdit::SetEnabled { mod_id, enabled } => {
                profiles.set_enabled(protected, id, mod_id, *enabled)?
            }
            ProfileEdit::SetVersion { mod_id, version } => {
                profiles.set_version(protected, id, mod_id, version)?
            }
            ProfileEdit::Move { mod_id, to_index } => {
                profiles.move_mod(protected, id, mod_id, *to_index)?
            }
            ProfileEdit::SetConfig { key, value } => {
                profiles.set_config(protected, id, key, value.as_deref())?
            }
        }
        mods_view_of(&g)
    }

    /// The dry run of a deploy: every file it would add or replace, and everything that blocks it. Changes nothing.
    pub fn plan_deploy(&self, profile_id: &str) -> Result<DeployPlan> {
        let g = self.lock()?;
        let ctx = ctx_of(&g);
        let profile = profile_of(&g, profile_id)?;
        let layout = layout_for(&g, &profile.game_id)?;
        deploy::plan_deploy(&g.builds, &ctx, &g.store, profile, &layout)
    }

    /// Put the enabled mods of a profile into its build. The core plans again first and refuses a blocked plan.
    pub fn deploy(&self, profile_id: &str) -> Result<Deployed> {
        let mut g = self.lock()?;
        let profile = profile_of(&g, profile_id)?.clone();
        let layout = layout_for(&g, &profile.game_id)?;
        let Inner {
            roots,
            protected,
            journal,
            builds,
            disk,
            store,
            ..
        } = &mut *g;
        let ctx = Ctx {
            roots,
            protected,
            journal,
            disk: &**disk,
        };
        let report = deploy::deploy(builds, &ctx, store, &profile, &layout)?;
        Ok(Deployed {
            report,
            view: mods_view_of(&g)?,
        })
    }

    /// The dry run of a purge: for every file of what is deployed, what would be done and why. Changes nothing.
    pub fn plan_purge(&self, build_id: &str) -> Result<PurgePlan> {
        let g = self.lock()?;
        let ctx = ctx_of(&g);
        deploy::plan_purge(&g.builds, &ctx, build_id)
    }

    /// Take what is deployed out of a build, touching only what the ledger proves SUL put there. What it removes
    /// is moved to a recovery folder, and this purge can be undone until something else changes in the build.
    pub fn purge(&self, build_id: &str) -> Result<Purged> {
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
        let report = deploy::purge(builds, &ctx, build_id)?;
        Ok(Purged {
            report,
            view: mods_view_of(&g)?,
        })
    }

    /// Put back what the latest purge took out. Only possible right after that purge.
    pub fn undo_purge(&self, build_id: &str, purge_id: &str) -> Result<Undone> {
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
        let report = deploy::undo_purge(builds, &ctx, build_id, purge_id)?;
        Ok(Undone {
            report,
            view: mods_view_of(&g)?,
        })
    }

    /// Purge what is deployed in the profile's build, then deploy the profile. If the purge leaves a file in
    /// place the deploy is not attempted.
    pub fn purge_and_deploy(&self, profile_id: &str) -> Result<PurgedAndDeployed> {
        let mut g = self.lock()?;
        let profile = profile_of(&g, profile_id)?.clone();
        let layout = layout_for(&g, &profile.game_id)?;
        let Inner {
            roots,
            protected,
            journal,
            builds,
            disk,
            store,
            ..
        } = &mut *g;
        let ctx = Ctx {
            roots,
            protected,
            journal,
            disk: &**disk,
        };
        let report = deploy::purge_and_deploy(builds, &ctx, store, &profile, &layout)?;
        Ok(PurgedAndDeployed {
            report,
            view: mods_view_of(&g)?,
        })
    }
}

pub(super) fn ctx_of(g: &Inner) -> Ctx<'_> {
    Ctx {
        roots: &g.roots,
        protected: &g.protected,
        journal: &g.journal,
        disk: &*g.disk,
    }
}

fn profile_of<'a>(g: &'a Inner, id: &str) -> Result<&'a Profile> {
    g.profiles
        .get(id)
        .ok_or_else(|| Error::Invalid(format!("no profile with id {id:?}")))
}

/// The folders a game lets mods change. A game with no adapter has none, and no mod is deployed for it.
pub(super) fn layout_for(g: &Inner, game_id: &str) -> Result<ModLayout> {
    let adapter = g.adapters.get(game_id).ok_or_else(|| {
        Error::Invalid(format!(
            "SUL has no adapter for {game_id:?}, so it does not know which folders mods may change in that game, and it deploys nothing without that. Nothing was changed. (No adapter for a real game exists yet.)"
        ))
    })?;
    let layout = adapter.mod_layout();
    layout.validate()?;
    Ok(layout)
}

/// A profile may move to another build of the same game, but not while it is deployed where it is: the purge
/// that takes it out is found through the profile's build.
fn check_new_build(g: &Inner, profile_id: &str, new_build: &str) -> Result<()> {
    let profile = profile_of(g, profile_id)?;
    let new = g
        .builds
        .get(new_build)
        .ok_or_else(|| Error::Invalid(format!("no build with id {new_build:?}")))?;
    if new.game_id != profile.game_id {
        return Err(Error::Invalid(
            "that build belongs to another game than the profile".into(),
        ));
    }
    if new_build == profile.build_id {
        return Ok(());
    }
    let Some(old) = g.builds.get(&profile.build_id) else {
        return Ok(());
    };
    if !matches!(old.location, Location::Managed { .. }) {
        return Ok(());
    }
    match deploy::deployment_of(&g.builds, &ctx_of(g), &old.id) {
        Ok(Some(d)) if d.profile_id == profile_id => Err(Error::Invalid(format!(
            "this profile is deployed in build {:?}; purge it there before moving the profile to another build",
            old.id
        ))),
        Ok(_) => Ok(()),
        Err(e) => Err(Error::Invalid(format!(
            "could not tell whether this profile is deployed in build {:?} ({e}); nothing was changed",
            old.id
        ))),
    }
}

fn problems_of(g: &Inner, ctx: &Ctx, p: &Profile) -> Vec<Problem> {
    match g.builds.manifest(ctx, &p.build_id) {
        Ok(m) => check_profile(p, &g.store, &m.base_version),
        Err(e) => vec![Problem::error(
            ProblemKind::BuildNotFound,
            None,
            format!("the build {:?} could not be read: {e}", p.build_id),
        )],
    }
}

fn build_mods_view(g: &Inner, ctx: &Ctx, b: &BuildRecord) -> BuildModsView {
    let mut v = BuildModsView {
        build_id: b.id.clone(),
        game_id: b.game_id.clone(),
        slot: b.slot.clone(),
        deployment: None,
        undoable_purge: None,
        history: vec![],
        error: None,
    };
    let read = || -> Result<_> {
        Ok((
            deploy::deployment_of(&g.builds, ctx, &b.id)?,
            deploy::last_purge_of(&g.builds, ctx, &b.id)?,
            deploy::history_of(&g.builds, ctx, &b.id)?,
        ))
    };
    match read() {
        Ok((deployment, purge, history)) => {
            v.deployment = deployment.map(|d| DeploymentView {
                files: d.entries.len(),
                id: d.id,
                profile_id: d.profile_id,
                profile_name: d.profile_name,
                status: d.status,
                started_unix: d.started_unix,
                finished_unix: d.finished_unix,
            });
            v.undoable_purge = purge.map(|p| PurgeSummary {
                files: p.reverted.len(),
                id: p.id,
                profile_name: p.deployment.profile_name,
                unix: p.unix,
            });
            v.history = history;
        }
        Err(e) => v.error = Some(e.to_string()),
    }
    v
}

fn mods_view_of(g: &Inner) -> Result<ModsView> {
    let ctx = ctx_of(g);
    Ok(ModsView {
        mods: g.store.mods().iter().map(ModView::of).collect(),
        profiles: g
            .profiles
            .list()
            .iter()
            .map(|p| ProfileView {
                problems: problems_of(g, &ctx, p),
                profile: p.clone(),
            })
            .collect(),
        builds: g
            .builds
            .builds()
            .iter()
            .filter(|b| matches!(b.location, Location::Managed { .. }))
            .map(|b| build_mods_view(g, &ctx, b))
            .collect(),
        mod_roots: root_views(g)
            .into_iter()
            .filter(|r| r.root.kind == RootKind::Mods)
            .collect(),
        deployable_games: g.adapters.ids().into_iter().map(str::to_owned).collect(),
    })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use crate::adapters::fixtures::MadeUpGame;
    use crate::adapters::{AdapterRegistry, GameAdapter, LaunchPlan, ModLayout, Recognition};
    use crate::capability::CapabilityEntry;
    use crate::manifest::Variant;
    use crate::mods::deploy::PurgeAction;
    use crate::mods::profile::Severity;
    use crate::roots::DiskSpace;

    const GAME: &str = "madeupgame";

    struct Disk(Cell<u64>);
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
        build: String,
    }

    fn put(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn open(base: &Path, with_adapter: bool) -> ManagerService {
        let mut adapters = AdapterRegistry::new();
        if with_adapter {
            adapters.register(Box::new(MadeUpGame::new())).unwrap();
        }
        ManagerService::with_parts(
            &base.join("state"),
            Box::new(Disk(Cell::new(u64::MAX / 2))),
            adapters,
        )
        .unwrap()
    }

    /// A service with one build folder, one mod folder, and one build (imported from a made-up install) of
    /// the given variant.
    fn fx_with(variant: Variant, with_adapter: bool) -> Fx {
        fx_using(variant, |base| open(base, with_adapter))
    }

    /// The same, with the service opened by `make` (for a test that needs an adapter of its own).
    fn fx_using(variant: Variant, make: impl FnOnce(&Path) -> ManagerService) -> Fx {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        for d in ["ssd", "modlib", "mods_src"] {
            std::fs::create_dir_all(base.join(d)).unwrap();
        }
        let install = base.join("original_install");
        put(&install.join("game.exe"), "exe");
        put(&install.join("data/a.bin"), "AAAA");
        let svc = make(&base);
        svc.add_root("ssd", "Fast drive", &base.join("ssd"), RootKind::Builds)
            .unwrap();
        svc.add_root(
            "modlib",
            "Mod library",
            &base.join("modlib"),
            RootKind::Mods,
        )
        .unwrap();
        svc.protect_installs(std::slice::from_ref(&install))
            .unwrap();
        let built = svc
            .import_build(
                &super::super::BuildSource {
                    game_id: GAME.into(),
                    display_name: "Made-up Game".into(),
                    install_dir: install.clone(),
                    variant,
                    base_version: "1.0.0".into(),
                    source_description: "my own install".into(),
                },
                "ssd",
            )
            .unwrap();
        Fx {
            _dir: dir,
            base,
            install,
            svc,
            build: built.build_id,
        }
    }

    fn fx() -> Fx {
        fx_with(Variant::Experimental, true)
    }

    fn source(f: &Fx, id: &str, version: &str, files: &[(&str, &str)]) -> ModSource {
        let folder = f.base.join("mods_src").join(format!("{id}-{version}"));
        for (p, t) in files {
            put(&folder.join(p), t);
        }
        ModSource {
            game_id: GAME.into(),
            mod_id: id.into(),
            name: format!("Mod {id}"),
            version: version.into(),
            requires: vec![],
            compatible_versions: vec![],
            source: "made by hand".into(),
            license_note: "mine".into(),
            path: folder,
        }
    }

    fn import(f: &Fx, id: &str, version: &str, files: &[(&str, &str)]) -> ModsView {
        f.svc
            .import_mod(&source(f, id, version, files), "modlib")
            .unwrap()
    }

    fn two_file_mod(f: &Fx) {
        import(
            f,
            "ui",
            "1.0",
            &[("data/a.bin", "NEW-A"), ("mods/ui.txt", "ui")],
        );
        f.svc
            .create_profile("main", "Main", GAME, &f.build)
            .unwrap();
        f.svc
            .edit_profile(
                "main",
                &ProfileEdit::AddMod {
                    mod_id: "ui".into(),
                    version: "1.0".into(),
                    position: None,
                },
            )
            .unwrap();
    }

    fn build_dir(f: &Fx) -> PathBuf {
        let g = f.svc.lock().unwrap();
        g.builds.build_dir(&ctx_of(&g), &f.build).unwrap()
    }

    fn read(f: &Fx, rel: &str) -> String {
        std::fs::read_to_string(build_dir(f).join(rel)).unwrap()
    }

    fn mods_of(f: &Fx) -> BuildModsView {
        f.svc
            .mods_view()
            .unwrap()
            .builds
            .into_iter()
            .find(|b| b.build_id == f.build)
            .unwrap()
    }

    #[test]
    fn a_new_service_has_no_mods_and_lists_which_games_can_take_them() {
        let f = fx();
        let v = f.svc.mods_view().unwrap();
        assert!(v.mods.is_empty() && v.profiles.is_empty());
        assert_eq!(v.deployable_games, [GAME]);
        assert_eq!(
            v.mod_roots.len(),
            1,
            "only the mod folder, not the build folder"
        );
        assert_eq!(v.mod_roots[0].root.id, "modlib");
        assert_eq!(v.builds.len(), 1);
        assert!(v.builds[0].deployment.is_none() && v.builds[0].error.is_none());

        let none = fx_with(Variant::Experimental, false);
        assert!(none.svc.mods_view().unwrap().deployable_games.is_empty());
    }

    #[test]
    fn an_import_stores_a_verified_copy_and_only_reads_the_source() {
        let f = fx();
        let src = source(
            &f,
            "ui",
            "1.0",
            &[("data/a.bin", "NEW-A"), ("mods/ui.txt", "ui")],
        );
        let v = f.svc.import_mod(&src, "modlib").unwrap();
        assert_eq!(v.mods.len(), 1);
        let m = &v.mods[0];
        assert_eq!((m.mod_id.as_str(), m.version.as_str()), ("ui", "1.0"));
        assert_eq!((m.file_count, m.total_bytes), (2, 7));
        assert_eq!(m.root_id, "modlib");
        assert_eq!(
            std::fs::read_to_string(src.path.join("data/a.bin")).unwrap(),
            "NEW-A"
        );
        let stored = f.base.join("modlib").join(GAME).join("ui").join("1.0");
        assert_eq!(
            std::fs::read_to_string(stored.join("mods/ui.txt")).unwrap(),
            "ui"
        );
        let ok = f.svc.verify_mod(GAME, "ui", "1.0").unwrap();
        assert_eq!(ok.report, VerifyReport::default());
    }

    #[test]
    fn a_version_is_never_imported_over_another_and_a_build_folder_is_no_mod_library() {
        let f = fx();
        import(&f, "ui", "1.0", &[("data/a.bin", "ONE")]);
        let again = source(&f, "ui", "1.0", &[("data/a.bin", "TWO")]);
        let e = f.svc.import_mod(&again, "modlib").unwrap_err().to_string();
        assert!(e.contains("already in the store"), "{e}");
        let wrong_root = source(&f, "other", "1.0", &[("data/a.bin", "X")]);
        let e = f
            .svc
            .import_mod(&wrong_root, "ssd")
            .unwrap_err()
            .to_string();
        assert!(e.contains("not a mod library"), "{e}");
        let e = f
            .svc
            .import_mod(&wrong_root, "nowhere")
            .unwrap_err()
            .to_string();
        assert!(e.contains("no library root"), "{e}");
        assert_eq!(f.svc.mods_view().unwrap().mods.len(), 1);
    }

    #[test]
    fn a_changed_stored_mod_is_reported_by_a_check() {
        let f = fx();
        import(
            &f,
            "ui",
            "1.0",
            &[("data/a.bin", "ONE"), ("mods/b.txt", "b")],
        );
        let stored = f.base.join("modlib").join(GAME).join("ui").join("1.0");
        std::fs::write(stored.join("data/a.bin"), "tampered").unwrap();
        let r = f.svc.verify_mod(GAME, "ui", "1.0").unwrap().report;
        assert_eq!(r.modified, ["data/a.bin"]);
        assert!(r.missing.is_empty());
        let e = f.svc.verify_mod(GAME, "ui", "9.9").unwrap_err().to_string();
        assert!(e.contains("not in the mod store"), "{e}");
    }

    #[test]
    fn a_profile_needs_a_real_build_of_its_own_game() {
        let f = fx();
        let e = f
            .svc
            .create_profile("p", "P", GAME, "no-such-build")
            .unwrap_err()
            .to_string();
        assert!(e.contains("no build"), "{e}");
        let e = f
            .svc
            .create_profile("p", "P", "othergame", &f.build)
            .unwrap_err()
            .to_string();
        assert!(e.contains("another game"), "{e}");
        f.svc.create_profile("p", "P", GAME, &f.build).unwrap();
        let e = f
            .svc
            .create_profile("p", "Again", GAME, &f.build)
            .unwrap_err()
            .to_string();
        assert!(e.contains("already exists"), "{e}");
        assert_eq!(f.svc.mods_view().unwrap().profiles.len(), 1);
    }

    #[test]
    fn every_kind_of_profile_edit_is_applied_and_problems_are_listed() {
        let f = fx();
        import(&f, "base", "1.0", &[("data/base.bin", "b")]);
        let mut needs = source(&f, "addon", "1.0", &[("data/addon.bin", "a")]);
        needs.requires = vec![Requirement {
            mod_id: "base".into(),
            min_version: None,
        }];
        f.svc.import_mod(&needs, "modlib").unwrap();
        f.svc.create_profile("p", "P", GAME, &f.build).unwrap();
        let edit = |e: ProfileEdit| f.svc.edit_profile("p", &e).unwrap();
        let add = |id: &str, pos| ProfileEdit::AddMod {
            mod_id: id.into(),
            version: "1.0".into(),
            position: pos,
        };

        // The add-on first, with its dependency missing from the profile.
        let v = edit(add("addon", None));
        let p = &v.profiles[0];
        assert!(
            p.problems
                .iter()
                .any(|x| x.kind == ProblemKind::MissingDependency && x.severity == Severity::Error),
            "{:?}",
            p.problems
        );
        // The dependency after it loads too late.
        let v = edit(add("base", None));
        assert!(
            v.profiles[0]
                .problems
                .iter()
                .any(|x| x.kind == ProblemKind::DependencyLoadsLater)
        );
        // Move it to the front: the problem goes away.
        let v = edit(ProfileEdit::Move {
            mod_id: "base".into(),
            to_index: 0,
        });
        let ids: Vec<_> = v.profiles[0]
            .profile
            .mods
            .iter()
            .map(|m| m.mod_id.as_str())
            .collect();
        assert_eq!(ids, ["base", "addon"]);
        assert!(
            !v.profiles[0]
                .problems
                .iter()
                .any(|x| x.severity == Severity::Error),
            "{:?}",
            v.profiles[0].problems
        );
        // Disable, re-version, configure, rename, note.
        let v = edit(ProfileEdit::SetEnabled {
            mod_id: "addon".into(),
            enabled: false,
        });
        assert!(!v.profiles[0].profile.mods[1].enabled);
        let v = edit(ProfileEdit::SetVersion {
            mod_id: "addon".into(),
            version: "2.0".into(),
        });
        assert_eq!(v.profiles[0].profile.mods[1].version, "2.0");
        let v = edit(ProfileEdit::SetConfig {
            key: "volume".into(),
            value: Some("3".into()),
        });
        assert_eq!(v.profiles[0].profile.config["volume"], "3");
        let v = edit(ProfileEdit::SetConfig {
            key: "volume".into(),
            value: None,
        });
        assert!(v.profiles[0].profile.config.is_empty());
        let v = edit(ProfileEdit::Rename {
            name: "Renamed".into(),
        });
        assert_eq!(v.profiles[0].profile.name, "Renamed");
        let v = edit(ProfileEdit::Notes {
            notes: "line\nline".into(),
        });
        assert_eq!(v.profiles[0].profile.notes, "line\nline");
        let v = edit(ProfileEdit::RemoveMod {
            mod_id: "addon".into(),
        });
        assert_eq!(v.profiles[0].profile.mods.len(), 1);
        // A refused edit leaves the profile as it was.
        let e = f
            .svc
            .edit_profile(
                "p",
                &ProfileEdit::RemoveMod {
                    mod_id: "nope".into(),
                },
            )
            .unwrap_err()
            .to_string();
        assert!(e.contains("not in this profile"), "{e}");
        let e = f
            .svc
            .edit_profile("nope", &ProfileEdit::Rename { name: "x".into() })
            .unwrap_err()
            .to_string();
        assert!(e.contains("no profile"), "{e}");
    }

    #[test]
    fn a_preview_changes_nothing_and_a_deploy_then_purge_follows_the_ledger() {
        let f = fx();
        two_file_mod(&f);
        let before = (
            read(&f, "data/a.bin"),
            build_dir(&f).join("mods/ui.txt").exists(),
        );

        let plan = f.svc.plan_deploy("main").unwrap();
        assert!(!plan.blocked(), "{:?}", plan.problems);
        assert_eq!(plan.files.len(), 2);
        assert_eq!(
            (
                read(&f, "data/a.bin"),
                build_dir(&f).join("mods/ui.txt").exists()
            ),
            before
        );
        assert!(mods_of(&f).deployment.is_none());

        let d = f.svc.deploy("main").unwrap();
        assert_eq!(d.report.added, ["mods/ui.txt"]);
        assert_eq!(d.report.replaced, ["data/a.bin"]);
        assert_eq!(read(&f, "data/a.bin"), "NEW-A");
        let b = d
            .view
            .builds
            .iter()
            .find(|b| b.build_id == f.build)
            .unwrap();
        let dep = b.deployment.as_ref().unwrap();
        assert_eq!((dep.profile_id.as_str(), dep.files), ("main", 2));
        assert_eq!(dep.status, DeployStatus::Deployed);
        // The original install is exactly as it was.
        assert_eq!(
            std::fs::read_to_string(f.install.join("data/a.bin")).unwrap(),
            "AAAA"
        );

        // A second deploy is refused until the first is purged.
        let e = f.svc.deploy("main").unwrap_err().to_string();
        assert!(
            e.contains("purge it first") || e.contains("cannot be deployed"),
            "{e}"
        );

        let pp = f.svc.plan_purge(&f.build).unwrap();
        assert_eq!(pp.items.len(), 2);
        assert!(
            pp.items
                .iter()
                .any(|i| i.action == PurgeAction::RemoveModFileAndRestoreBase)
        );
        assert_eq!(
            read(&f, "data/a.bin"),
            "NEW-A",
            "a purge preview changes nothing"
        );

        let purged = f.svc.purge(&f.build).unwrap();
        assert_eq!(purged.report.removed.len(), 2);
        assert!(purged.report.left.is_empty());
        assert_eq!(read(&f, "data/a.bin"), "AAAA");
        assert!(!build_dir(&f).join("mods/ui.txt").exists());
        let b = mods_of(&f);
        assert!(b.deployment.is_none());
        assert_eq!(b.undoable_purge.as_ref().unwrap().files, 2);
        assert!(b.history.len() >= 2);
    }

    #[test]
    fn the_latest_purge_can_be_undone_once() {
        let f = fx();
        two_file_mod(&f);
        f.svc.deploy("main").unwrap();
        let purged = f.svc.purge(&f.build).unwrap();
        let id = purged.report.purge_id.clone().unwrap();
        let e = f
            .svc
            .undo_purge(&f.build, "not-the-purge")
            .unwrap_err()
            .to_string();
        assert!(e.contains("not the latest purge"), "{e}");
        let undone = f.svc.undo_purge(&f.build, &id).unwrap();
        assert_eq!(undone.report.put_back.len(), 2);
        assert_eq!(read(&f, "data/a.bin"), "NEW-A");
        assert!(undone.view.builds[0].deployment.is_some());
        assert!(undone.view.builds[0].undoable_purge.is_none());
        assert!(
            f.svc.undo_purge(&f.build, &id).is_err(),
            "an undone purge cannot be undone again"
        );
    }

    #[test]
    fn purge_and_deploy_replaces_what_is_deployed() {
        let f = fx();
        two_file_mod(&f);
        f.svc.deploy("main").unwrap();
        import(
            &f,
            "ui",
            "2.0",
            &[("data/a.bin", "NEWER"), ("mods/ui.txt", "ui2")],
        );
        f.svc
            .edit_profile(
                "main",
                &ProfileEdit::SetVersion {
                    mod_id: "ui".into(),
                    version: "2.0".into(),
                },
            )
            .unwrap();
        let r = f.svc.purge_and_deploy("main").unwrap();
        assert!(r.report.purge.left.is_empty());
        assert!(r.report.deploy.is_some());
        assert_eq!(read(&f, "data/a.bin"), "NEWER");
        assert_eq!(read(&f, "mods/ui.txt"), "ui2");
    }

    #[test]
    fn a_file_changed_after_it_was_deployed_is_left_alone_and_stops_purge_and_deploy() {
        let f = fx();
        two_file_mod(&f);
        f.svc.deploy("main").unwrap();
        std::fs::write(build_dir(&f).join("mods/ui.txt"), "edited by hand").unwrap();
        let pp = f.svc.plan_purge(&f.build).unwrap();
        assert!(
            pp.items
                .iter()
                .any(|i| i.action == PurgeAction::LeaveChangedSinceDeploy)
        );
        let r = f.svc.purge_and_deploy("main").unwrap();
        assert!(r.report.deploy.is_none(), "the deploy is not attempted");
        assert_eq!(r.report.purge.left.len(), 1);
        assert_eq!(read(&f, "mods/ui.txt"), "edited by hand");
    }

    #[test]
    fn a_mod_file_in_a_forbidden_folder_blocks_the_deploy_and_nothing_is_written() {
        let f = fx();
        import(
            &f,
            "bad",
            "1.0",
            &[("data/anticheat/x.bin", "x"), ("data/ok.bin", "ok")],
        );
        f.svc.create_profile("p", "P", GAME, &f.build).unwrap();
        f.svc
            .edit_profile(
                "p",
                &ProfileEdit::AddMod {
                    mod_id: "bad".into(),
                    version: "1.0".into(),
                    position: None,
                },
            )
            .unwrap();
        let plan = f.svc.plan_deploy("p").unwrap();
        assert!(plan.blocked());
        assert!(
            plan.problems
                .iter()
                .any(|p| p.kind == ProblemKind::OutsideAllowedFolders)
        );
        assert!(f.svc.deploy("p").is_err());
        assert!(!build_dir(&f).join("data/ok.bin").exists());
        assert!(!build_dir(&f).join("data/anticheat").exists());
        assert!(mods_of(&f).deployment.is_none());
    }

    #[test]
    fn a_game_with_no_adapter_gets_no_preview_and_no_deploy_but_can_still_have_profiles() {
        let f = fx_with(Variant::Experimental, false);
        two_file_mod(&f);
        for e in [
            f.svc.plan_deploy("main").unwrap_err().to_string(),
            f.svc.deploy("main").map(|_| ()).unwrap_err().to_string(),
            f.svc
                .purge_and_deploy("main")
                .map(|_| ())
                .unwrap_err()
                .to_string(),
        ] {
            assert!(e.contains("no adapter"), "{e}");
        }
        assert_eq!(read(&f, "data/a.bin"), "AAAA");
        assert!(mods_of(&f).deployment.is_none());
        assert_eq!(f.svc.mods_view().unwrap().profiles.len(), 1);
    }

    #[test]
    fn the_original_build_and_an_install_that_stays_put_take_no_mods() {
        let f = fx_with(Variant::Original, true);
        two_file_mod(&f);
        let plan = f.svc.plan_deploy("main").unwrap();
        assert!(plan.blocked());
        assert!(
            plan.problems
                .iter()
                .any(|p| p.kind == ProblemKind::BuildIsOriginal)
        );
        assert!(f.svc.deploy("main").is_err());
        assert_eq!(read(&f, "data/a.bin"), "AAAA");

        // An install recorded where it is has no mod state at all, and is not listed.
        let g = fx();
        let reg = g
            .svc
            .register_in_place(&super::super::BuildSource {
                game_id: GAME.into(),
                display_name: "Made-up Game".into(),
                install_dir: g.install.clone(),
                variant: Variant::Legacy,
                base_version: "1.0.0".into(),
                source_description: "in place".into(),
            })
            .unwrap();
        assert!(
            g.svc
                .mods_view()
                .unwrap()
                .builds
                .iter()
                .all(|b| b.build_id != reg.build_id)
        );
        g.svc
            .create_profile("inplace", "In place", GAME, &reg.build_id)
            .unwrap();
        let plan = g.svc.plan_deploy("inplace").unwrap();
        assert!(
            plan.problems
                .iter()
                .any(|p| p.kind == ProblemKind::BuildNotManaged)
        );
        assert!(g.svc.deploy("inplace").is_err());
        assert_eq!(
            std::fs::read_to_string(g.install.join("data/a.bin")).unwrap(),
            "AAAA"
        );
    }

    #[test]
    fn a_profile_cannot_move_builds_while_it_is_deployed_and_can_after_a_purge() {
        let f = fx();
        two_file_mod(&f);
        let second = f
            .svc
            .import_build(
                &super::super::BuildSource {
                    game_id: GAME.into(),
                    display_name: "Made-up Game".into(),
                    install_dir: f.install.clone(),
                    variant: Variant::Legacy,
                    base_version: "1.0.0".into(),
                    source_description: "second copy".into(),
                },
                "ssd",
            )
            .unwrap()
            .build_id;
        f.svc.deploy("main").unwrap();
        let to_second = ProfileEdit::SetBuild {
            build_id: second.clone(),
        };
        let e = f
            .svc
            .edit_profile("main", &to_second)
            .unwrap_err()
            .to_string();
        assert!(e.contains("purge it there"), "{e}");
        assert_eq!(
            f.svc.mods_view().unwrap().profiles[0].profile.build_id,
            f.build
        );
        let e = f
            .svc
            .edit_profile(
                "main",
                &ProfileEdit::SetBuild {
                    build_id: "nope".into(),
                },
            )
            .unwrap_err()
            .to_string();
        assert!(e.contains("no build"), "{e}");
        f.svc.purge(&f.build).unwrap();
        let v = f.svc.edit_profile("main", &to_second).unwrap();
        assert_eq!(v.profiles[0].profile.build_id, second);
    }

    #[test]
    fn a_profile_cannot_move_to_a_build_of_another_game() {
        let f = fx();
        two_file_mod(&f);
        let other = f
            .svc
            .import_build(
                &super::super::BuildSource {
                    game_id: "othergame".into(),
                    display_name: "Another game".into(),
                    install_dir: f.install.clone(),
                    variant: Variant::Legacy,
                    base_version: "1.0.0".into(),
                    source_description: "a copy of another game".into(),
                },
                "ssd",
            )
            .unwrap()
            .build_id;
        let e = f
            .svc
            .edit_profile("main", &ProfileEdit::SetBuild { build_id: other })
            .unwrap_err()
            .to_string();
        assert!(e.contains("another game"), "{e}");
        assert_eq!(
            f.svc.mods_view().unwrap().profiles[0].profile.build_id,
            f.build,
            "the profile stayed where it was"
        );
    }

    /// An adapter that is fine when it is registered and later reports a layout that is not (a path that
    /// climbs out of its folder), the way an adapter that reads its layout from a file could.
    struct Moody {
        inner: MadeUpGame,
        broken: Arc<AtomicBool>,
    }

    impl GameAdapter for Moody {
        fn id(&self) -> &str {
            self.inner.id()
        }
        fn display_name(&self) -> &str {
            self.inner.display_name()
        }
        fn version(&self) -> &str {
            self.inner.version()
        }
        fn recognise(&self, folder: &Path) -> Result<Recognition> {
            self.inner.recognise(folder)
        }
        fn launch_plan(&self, build_dir: &Path, base_version: &str) -> Result<LaunchPlan> {
            self.inner.launch_plan(build_dir, base_version)
        }
        fn mod_layout(&self) -> ModLayout {
            if self.broken.load(Ordering::SeqCst) {
                ModLayout {
                    allowed_prefixes: vec!["../outside".into()],
                    forbidden_prefixes: vec![],
                    case_insensitive: true,
                }
            } else {
                self.inner.mod_layout()
            }
        }
        fn capabilities(&self) -> Vec<CapabilityEntry> {
            self.inner.capabilities()
        }
    }

    #[test]
    fn a_layout_that_is_invalid_when_it_is_used_stops_the_preview_and_the_deploy_and_nothing_changes()
     {
        let broken = Arc::new(AtomicBool::new(false));
        let f = {
            let broken = broken.clone();
            fx_using(Variant::Experimental, move |base| {
                let mut adapters = AdapterRegistry::new();
                adapters
                    .register(Box::new(Moody {
                        inner: MadeUpGame::new(),
                        broken,
                    }))
                    .unwrap();
                ManagerService::with_parts(
                    &base.join("state"),
                    Box::new(Disk(Cell::new(u64::MAX / 2))),
                    adapters,
                )
                .unwrap()
            })
        };
        two_file_mod(&f);
        f.svc.plan_deploy("main").unwrap();

        broken.store(true, Ordering::SeqCst);
        for e in [
            f.svc.plan_deploy("main").map(|_| ()).unwrap_err(),
            f.svc.deploy("main").map(|_| ()).unwrap_err(),
            f.svc.purge_and_deploy("main").map(|_| ()).unwrap_err(),
        ] {
            assert!(matches!(e, Error::UnsafePath(_)), "{e}");
        }
        assert_eq!(read(&f, "data/a.bin"), "AAAA");
        assert!(mods_of(&f).deployment.is_none());

        broken.store(false, Ordering::SeqCst);
        f.svc.deploy("main").unwrap();
    }

    #[test]
    fn state_survives_opening_the_service_again() {
        let f = fx();
        two_file_mod(&f);
        f.svc.deploy("main").unwrap();
        let before = f.svc.mods_view().unwrap();
        let again = open(&f.base, true);
        assert_eq!(again.mods_view().unwrap(), before);
    }

    #[test]
    fn a_build_on_an_unplugged_drive_is_reported_and_the_rest_still_shows() {
        let f = fx();
        two_file_mod(&f);
        std::fs::rename(f.base.join("ssd"), f.base.join("ssd-unplugged")).unwrap();
        let v = f.svc.mods_view().unwrap();
        assert_eq!(v.mods.len(), 1);
        let b = &v.builds[0];
        assert!(b.error.is_some(), "the build's mod state cannot be read");
        assert!(b.deployment.is_none());
        assert!(
            v.profiles[0]
                .problems
                .iter()
                .any(|p| p.kind == ProblemKind::BuildNotFound),
            "{:?}",
            v.profiles[0].problems
        );
        assert!(f.svc.deploy("main").is_err());
    }

    #[test]
    fn a_damaged_mod_index_or_profile_file_stops_opening_and_is_not_reset() {
        for file in ["mods.json", "profiles.json"] {
            let f = fx();
            two_file_mod(&f);
            drop(f.svc);
            let path = f.base.join("state").join(file);
            std::fs::write(&path, "{ not json").unwrap();
            let err = ManagerService::with_parts(
                &f.base.join("state"),
                Box::new(Disk(Cell::new(u64::MAX / 2))),
                AdapterRegistry::new(),
            )
            .err()
            .expect("a damaged file is an error");
            assert!(!err.to_string().is_empty());
            assert_eq!(
                std::fs::read_to_string(&path).unwrap(),
                "{ not json",
                "{file} must not be replaced"
            );
        }
    }
}
