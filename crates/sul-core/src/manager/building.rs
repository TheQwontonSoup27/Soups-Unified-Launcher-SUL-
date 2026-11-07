//! The builder half of the service (spec 2.2): saved recipes, a preview of what a recipe would build, a run that
//! builds it into a workspace folder, and the record every run keeps.
//!
//! As in [`super::modding`], the rules live elsewhere ([`crate::builder`]); this file finds the pieces an operation
//! needs and returns plain views.
//!
//! * **A game with no adapter gets no preview and no run.** The core needs the folders the game lets mods change
//!   ([`crate::adapters::ModLayout`]) to judge a recipe's output. With no adapter there is no layout, so the
//!   service refuses with that reason. No adapter for a real game exists yet, so in the app every preview and every
//!   run is refused today. Writing and saving a recipe needs no adapter: a saved recipe is only a description.
//! * **A run writes only into the workspace folder** of a mod library (`.workspaces/<recipe>/<run>`), never into a
//!   game or a build, and it deletes nothing. A preview writes nothing.
//! * **A preview is never trusted.** A run plans again; a recipe whose mods or patches no longer fit is refused.
//! * **A recipe that changes code that runs inside the game cannot be previewed or run**, as the core decides. It can
//!   still be written down and saved.

use serde::Serialize;

use super::modding::{ModView, ctx_of, layout_for};
use super::{Inner, ManagerService, RootView, root_views};
use crate::builder::OpClass;
use crate::builder::recipe::{self, Recipe, RunPlan, RunRecord};
use crate::manifest::VerifyReport;
use crate::roots::RootKind;
use crate::{Error, Result};

/// Most files a lookup of a mod's files returns. Chosen here to keep an answer small, not a measured figure.
pub const MAX_LISTED_MOD_FILES: usize = 200;

/// A finished run of a recipe, found in a mod library's workspace folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunRef {
    pub root_id: String,
    pub run_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecipeView {
    pub recipe: Recipe,
    /// The recipe's identity. A change to a saved recipe must name this, so a stale window cannot overwrite a newer edit.
    pub sha256: String,
    /// The recipe has a step that changes code inside the game, which SUL cannot run, check or undo.
    pub has_runtime_code: bool,
    pub runs: Vec<RunRef>,
}

/// Everything the Builder tab shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuilderView {
    pub recipes: Vec<RecipeView>,
    /// The mods a recipe can layer.
    pub mods: Vec<ModView>,
    /// The library folders that hold mods: a run's workspace goes in one of them.
    pub mod_roots: Vec<RootView>,
    /// Games with an adapter, which are the only games a recipe can be previewed or run for.
    pub deployable_games: Vec<String>,
    /// Folders whose runs could not be listed, with the reason (an unplugged drive, say).
    pub run_errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecipeRan {
    pub record: RunRecord,
    pub view: BuilderView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModFile {
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

/// Some of a mod's files, to help write a patch step (which names a file by path and by SHA-256).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModFileList {
    pub files: Vec<ModFile>,
    /// How many files matched, of which at most [`MAX_LISTED_MOD_FILES`] are listed.
    pub matching: usize,
    pub total: usize,
}

impl ManagerService {
    /// Everything the Builder tab shows.
    pub fn builder_view(&self) -> Result<BuilderView> {
        let g = self.lock()?;
        builder_view_of(&g)
    }

    /// Keep a recipe. A new one (`replaces` empty) needs an unused id; a changed one names the SHA-256 of the recipe
    /// it started from. Reads no mod and touches no game file.
    pub fn recipe_save(&self, recipe: &Recipe, replaces: Option<&str>) -> Result<BuilderView> {
        let mut g = self.lock()?;
        let Inner {
            recipes, protected, ..
        } = &mut *g;
        recipes.save(protected, recipe, replaces)?;
        builder_view_of(&g)
    }

    /// What a saved recipe would build, worked out without writing anything.
    pub fn recipe_plan(&self, recipe_id: &str) -> Result<RunPlan> {
        let g = self.lock()?;
        let recipe = recipe_of(&g, recipe_id)?;
        let layout = layout_for(&g, &recipe.game_id)?;
        recipe::plan_recipe(&ctx_of(&g), &g.store, recipe, &layout)
    }

    /// Build a saved recipe into a new run folder in the workspace of mod library `root_id`.
    pub fn recipe_run(&self, recipe_id: &str, root_id: &str) -> Result<RecipeRan> {
        let g = self.lock()?;
        let recipe = recipe_of(&g, recipe_id)?;
        let layout = layout_for(&g, &recipe.game_id)?;
        let record = recipe::run_recipe(&ctx_of(&g), &g.store, recipe, &layout, root_id)?;
        Ok(RecipeRan {
            record,
            view: builder_view_of(&g)?,
        })
    }

    /// The provenance record of a finished run.
    pub fn recipe_read_run(
        &self,
        recipe_id: &str,
        root_id: &str,
        run_id: &str,
    ) -> Result<RunRecord> {
        let g = self.lock()?;
        recipe::read_run(&ctx_of(&g), root_id, recipe_id, run_id)
    }

    /// Compare a finished run's files with its record. Read only.
    pub fn recipe_verify_run(
        &self,
        recipe_id: &str,
        root_id: &str,
        run_id: &str,
    ) -> Result<VerifyReport> {
        let g = self.lock()?;
        recipe::verify_run(&ctx_of(&g), root_id, recipe_id, run_id)
    }

    /// Some of an imported mod's files (those whose path contains `filter`, ignoring case), with their hashes.
    pub fn mod_files(
        &self,
        game_id: &str,
        mod_id: &str,
        version: &str,
        filter: &str,
    ) -> Result<ModFileList> {
        let g = self.lock()?;
        let rec = g.store.get(game_id, mod_id, version).ok_or_else(|| {
            Error::Invalid(format!(
                "mod {mod_id} {version} for {game_id} is not in the mod store"
            ))
        })?;
        let needle = filter.trim().to_lowercase();
        let all = &rec.manifest.files;
        let matching: Vec<&crate::manifest::FileEntry> = all
            .iter()
            .filter(|f| needle.is_empty() || f.path.to_lowercase().contains(&needle))
            .collect();
        Ok(ModFileList {
            matching: matching.len(),
            total: all.len(),
            files: matching
                .into_iter()
                .take(MAX_LISTED_MOD_FILES)
                .map(|f| ModFile {
                    path: f.path.clone(),
                    sha256: f.sha256.clone(),
                    size: f.size,
                })
                .collect(),
        })
    }
}

fn recipe_of<'a>(g: &'a Inner, id: &str) -> Result<&'a Recipe> {
    g.recipes
        .get(id)
        .ok_or_else(|| Error::Invalid(format!("no saved recipe with id {id:?}")))
}

fn builder_view_of(g: &Inner) -> Result<BuilderView> {
    let ctx = ctx_of(g);
    let mod_roots: Vec<RootView> = root_views(g)
        .into_iter()
        .filter(|r| r.root.kind == RootKind::Mods)
        .collect();
    let mut run_errors = Vec::new();
    let mut recipes = Vec::new();
    for r in g.recipes.list() {
        let mut runs = Vec::new();
        for root in mod_roots.iter().filter(|x| x.present && x.marker_ok) {
            match recipe::list_runs(&ctx, &root.root.id, &r.id) {
                Ok(ids) => runs.extend(ids.into_iter().map(|run_id| RunRef {
                    root_id: root.root.id.clone(),
                    run_id,
                })),
                Err(e) => run_errors.push(format!(
                    "could not list the runs of {:?} in {:?}: {e}",
                    r.id, root.root.id
                )),
            }
        }
        recipes.push(RecipeView {
            sha256: r.sha256()?,
            has_runtime_code: r.classes().contains(&OpClass::RuntimeCode),
            recipe: r.clone(),
            runs,
        });
    }
    Ok(BuilderView {
        recipes,
        mods: g.store.mods().iter().map(ModView::of).collect(),
        mod_roots,
        deployable_games: g.adapters.ids().into_iter().map(str::to_owned).collect(),
        run_errors,
    })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::adapters::AdapterRegistry;
    use crate::adapters::fixtures::MadeUpGame;
    use crate::builder::patch::{BytePatch, Hunk};
    use crate::builder::recipe::{RECIPE_SCHEMA, Step};
    use crate::manager::ModSource;
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
        svc: ManagerService,
    }

    fn put(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn sha(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(bytes))
    }

    fn fx_with(with_adapter: bool) -> Fx {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(base.join("modlib")).unwrap();
        std::fs::create_dir_all(base.join("ssd")).unwrap();
        let mut adapters = AdapterRegistry::new();
        if with_adapter {
            adapters.register(Box::new(MadeUpGame::new())).unwrap();
        }
        let svc = ManagerService::with_parts(
            &base.join("state"),
            Box::new(Disk(Cell::new(u64::MAX / 2))),
            adapters,
        )
        .unwrap();
        svc.add_root(
            "modlib",
            "Mod library",
            &base.join("modlib"),
            RootKind::Mods,
        )
        .unwrap();
        svc.add_root("ssd", "Fast drive", &base.join("ssd"), RootKind::Builds)
            .unwrap();
        Fx {
            _dir: dir,
            base,
            svc,
        }
    }

    fn fx() -> Fx {
        fx_with(true)
    }

    fn import(f: &Fx, id: &str, version: &str, files: &[(&str, &[u8])]) {
        let folder = f.base.join("mods_src").join(format!("{id}-{version}"));
        for (p, b) in files {
            put(&folder.join(p), b);
        }
        f.svc
            .import_mod(
                &ModSource {
                    game_id: GAME.into(),
                    mod_id: id.into(),
                    name: format!("Mod {id}"),
                    version: version.into(),
                    requires: vec![],
                    compatible_versions: vec![],
                    source: "made by hand".into(),
                    license_note: "mine".into(),
                    path: folder,
                },
                "modlib",
            )
            .unwrap();
    }

    fn layer(mod_id: &str, version: &str, into: &str) -> Step {
        Step::LayerMod {
            mod_id: mod_id.into(),
            version: version.into(),
            into: into.into(),
        }
    }

    fn recipe(id: &str, steps: Vec<Step>) -> Recipe {
        Recipe {
            schema: RECIPE_SCHEMA,
            id: id.into(),
            name: format!("Recipe {id}"),
            game_id: GAME.into(),
            description: String::new(),
            steps,
        }
    }

    /// A patch that changes the byte at `offset` from `old` to `new` in a file whose content is `file`.
    fn patch(file: &[u8], offset: u64, old: &str, new: &str) -> BytePatch {
        BytePatch {
            id: "p1".into(),
            description: "one byte".into(),
            expect_sha256: sha(file),
            hunks: vec![Hunk {
                offset,
                old_hex: old.into(),
                new_hex: new.into(),
            }],
        }
    }

    fn the_view(f: &Fx) -> BuilderView {
        f.svc.builder_view().unwrap()
    }

    #[test]
    fn a_new_service_has_no_recipes_and_lists_the_mods_the_mod_libraries_and_the_games_that_can_be_planned()
     {
        let f = fx();
        import(&f, "louder", "1.0", &[("data/a.bin", b"AAAA")]);
        let v = the_view(&f);
        assert!(v.recipes.is_empty() && v.run_errors.is_empty());
        assert_eq!(v.mods.len(), 1);
        assert_eq!(v.mod_roots.len(), 1, "only a mod library takes a workspace");
        assert_eq!(v.mod_roots[0].root.id, "modlib");
        assert_eq!(v.deployable_games, vec![GAME.to_string()]);
        assert!(
            fx_with(false)
                .svc
                .builder_view()
                .unwrap()
                .deployable_games
                .is_empty()
        );
    }

    #[test]
    fn a_saved_recipe_is_listed_with_its_identity_and_survives_reopening_the_service() {
        let f = fx();
        let r = recipe("hello", vec![layer("louder", "1.0", "")]);
        let v = f.svc.recipe_save(&r, None).unwrap();
        assert_eq!(v.recipes.len(), 1);
        assert_eq!(v.recipes[0].recipe, r);
        assert_eq!(v.recipes[0].sha256, r.sha256().unwrap());
        assert!(!v.recipes[0].has_runtime_code && v.recipes[0].runs.is_empty());

        let again = ManagerService::with_parts(
            &f.base.join("state"),
            Box::new(Disk(Cell::new(u64::MAX / 2))),
            AdapterRegistry::new(),
        )
        .unwrap();
        assert_eq!(again.builder_view().unwrap().recipes[0].recipe, r);
    }

    #[test]
    fn saving_is_checked_and_a_stale_or_duplicate_save_is_refused() {
        let f = fx();
        let r = recipe("one", vec![layer("louder", "1.0", "")]);
        f.svc.recipe_save(&r, None).unwrap();
        let e = f.svc.recipe_save(&r, None).unwrap_err().to_string();
        assert!(e.contains("already exists"), "{e}");
        let mut changed = r.clone();
        changed.name = "Changed".into();
        let e = f
            .svc
            .recipe_save(&changed, Some("0".repeat(64).as_str()))
            .unwrap_err()
            .to_string();
        assert!(e.contains("changed since it was opened"), "{e}");
        f.svc
            .recipe_save(&changed, Some(&r.sha256().unwrap()))
            .unwrap();
        assert_eq!(the_view(&f).recipes[0].recipe.name, "Changed");
        let mut empty = r.clone();
        empty.id = "empty".into();
        empty.steps.clear();
        assert!(
            f.svc.recipe_save(&empty, None).is_err(),
            "a recipe with no steps is not kept"
        );
        assert_eq!(the_view(&f).recipes.len(), 1);
    }

    #[test]
    fn a_runtime_code_step_can_be_saved_but_the_recipe_cannot_be_planned_or_run() {
        let f = fx();
        let r = recipe(
            "code",
            vec![
                layer("louder", "1.0", ""),
                Step::RuntimeCode {
                    description: "hook the audio engine".into(),
                },
            ],
        );
        let v = f.svc.recipe_save(&r, None).unwrap();
        assert!(v.recipes[0].has_runtime_code, "the page can say so");
        for e in [
            f.svc.recipe_plan("code").map(|_| ()).unwrap_err(),
            f.svc.recipe_run("code", "modlib").map(|_| ()).unwrap_err(),
        ] {
            assert!(
                e.to_string()
                    .contains("changes code that runs inside the game"),
                "{e}"
            );
        }
    }

    #[test]
    fn a_plan_lists_what_would_be_built_and_writes_nothing() {
        let f = fx();
        import(
            &f,
            "louder",
            "1.0",
            &[("data/a.bin", b"AAAA"), ("data/b.bin", b"BB")],
        );
        f.svc
            .recipe_save(&recipe("hello", vec![layer("louder", "1.0", "out")]), None)
            .unwrap();
        let plan = f.svc.recipe_plan("hello").unwrap();
        assert!(!plan.blocked(), "{:?}", plan.problems);
        assert_eq!(plan.files.len(), 2);
        assert_eq!(plan.files[0].path, "out/data/a.bin");
        assert_eq!(plan.total_bytes, 6);
        assert!(
            !f.base.join("modlib/.workspaces").exists(),
            "a preview writes no workspace"
        );
    }

    #[test]
    fn a_plan_names_a_mod_that_is_not_in_the_store_as_a_problem_and_a_run_is_refused() {
        let f = fx();
        f.svc
            .recipe_save(&recipe("hello", vec![layer("ghost", "1.0", "")]), None)
            .unwrap();
        let plan = f.svc.recipe_plan("hello").unwrap();
        assert!(plan.blocked());
        assert!(plan.problems[0].contains("ghost"), "{:?}", plan.problems);
        let e = f.svc.recipe_run("hello", "modlib").map(|_| ()).unwrap_err();
        assert!(e.to_string().contains("cannot run"), "{e}");
        assert!(!f.base.join("modlib/.workspaces").exists());
    }

    #[test]
    fn a_run_builds_the_files_into_the_workspace_keeps_a_record_and_can_be_listed_read_and_verified()
     {
        let f = fx();
        import(&f, "louder", "1.0", &[("data/a.bin", b"AAAA")]);
        f.svc
            .recipe_save(&recipe("hello", vec![layer("louder", "1.0", "")]), None)
            .unwrap();
        let ran = f.svc.recipe_run("hello", "modlib").unwrap();
        let run_dir = f
            .base
            .join("modlib/.workspaces/hello")
            .join(&ran.record.run_id);
        assert_eq!(
            std::fs::read(run_dir.join("files/data/a.bin")).unwrap(),
            b"AAAA"
        );
        assert!(run_dir.join("run.json").is_file());

        let v = ran.view;
        assert_eq!(
            v.recipes[0].runs,
            vec![RunRef {
                root_id: "modlib".into(),
                run_id: ran.record.run_id.clone()
            }]
        );
        let read = f
            .svc
            .recipe_read_run("hello", "modlib", &ran.record.run_id)
            .unwrap();
        assert_eq!(read, ran.record);
        assert_eq!(
            read.inputs[0].license_note, "mine",
            "the owner's words are kept as written"
        );

        let report = f
            .svc
            .recipe_verify_run("hello", "modlib", &ran.record.run_id)
            .unwrap();
        assert!(report.missing.is_empty() && report.modified.is_empty() && report.extra.is_empty());
        std::fs::write(run_dir.join("files/data/a.bin"), b"changed").unwrap();
        std::fs::write(run_dir.join("files/data/extra.bin"), b"x").unwrap();
        let report = f
            .svc
            .recipe_verify_run("hello", "modlib", &ran.record.run_id)
            .unwrap();
        assert_eq!(report.modified, vec!["data/a.bin".to_string()]);
        assert_eq!(report.extra, vec!["data/extra.bin".to_string()]);
    }

    #[test]
    fn a_run_goes_into_a_mod_library_only_and_never_into_a_build_folder_or_a_missing_root() {
        let f = fx();
        import(&f, "louder", "1.0", &[("data/a.bin", b"AAAA")]);
        f.svc
            .recipe_save(&recipe("hello", vec![layer("louder", "1.0", "")]), None)
            .unwrap();
        let e = f.svc.recipe_run("hello", "ssd").map(|_| ()).unwrap_err();
        assert!(e.to_string().contains("not a mod library"), "{e}");
        let e = f.svc.recipe_run("hello", "nope").map(|_| ()).unwrap_err();
        assert!(e.to_string().contains("no library root"), "{e}");
        assert!(!f.base.join("ssd/.workspaces").exists());
        assert!(!f.base.join("modlib/.workspaces").exists());
    }

    #[test]
    fn a_game_with_no_adapter_gets_no_plan_and_no_run_but_its_recipes_can_still_be_saved() {
        let f = fx_with(false);
        import(&f, "louder", "1.0", &[("data/a.bin", b"AAAA")]);
        f.svc
            .recipe_save(&recipe("hello", vec![layer("louder", "1.0", "")]), None)
            .unwrap();
        for e in [
            f.svc.recipe_plan("hello").map(|_| ()).unwrap_err(),
            f.svc.recipe_run("hello", "modlib").map(|_| ()).unwrap_err(),
        ] {
            assert!(e.to_string().contains("SUL has no adapter"), "{e}");
        }
        assert!(!f.base.join("modlib/.workspaces").exists());
    }

    #[test]
    fn an_unknown_recipe_is_refused() {
        let f = fx();
        for e in [
            f.svc.recipe_plan("nope").map(|_| ()).unwrap_err(),
            f.svc.recipe_run("nope", "modlib").map(|_| ()).unwrap_err(),
        ] {
            assert!(e.to_string().contains("no saved recipe"), "{e}");
        }
    }

    #[test]
    fn a_patch_step_changes_a_copy_in_the_workspace_and_never_the_stored_mod() {
        let f = fx();
        let original = b"ABCD".to_vec();
        import(&f, "louder", "1.0", &[("data/a.bin", &original)]);
        let steps = vec![
            layer("louder", "1.0", ""),
            Step::PatchFile {
                path: "data/a.bin".into(),
                patch: patch(&original, 1, "42", "5a"), // 'B' (0x42) becomes 'Z' (0x5a)
            },
        ];
        f.svc.recipe_save(&recipe("patched", steps), None).unwrap();
        let plan = f.svc.recipe_plan("patched").unwrap();
        assert!(!plan.blocked(), "{:?}", plan.problems);
        assert_eq!(plan.patches.len(), 1);
        assert_eq!(plan.patches[0].outcome.bytes_changed, 1);

        let ran = f.svc.recipe_run("patched", "modlib").unwrap();
        let out = f
            .base
            .join("modlib/.workspaces/patched")
            .join(&ran.record.run_id)
            .join("files/data/a.bin");
        assert_eq!(std::fs::read(out).unwrap(), b"AZCD");
        let stored = f
            .base
            .join("modlib")
            .read_dir()
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .is_some_and(|n| !n.to_string_lossy().starts_with('.'))
            })
            .flat_map(|d| walk(&d))
            .find(|p| p.ends_with("data/a.bin"))
            .expect("the stored mod's file");
        assert_eq!(
            std::fs::read(stored).unwrap(),
            b"ABCD",
            "the imported mod is untouched"
        );
    }

    fn walk(dir: &Path) -> Vec<PathBuf> {
        let mut out = vec![];
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    out.extend(walk(&p));
                } else {
                    out.push(p);
                }
            }
        }
        out
    }

    #[test]
    fn a_patch_made_for_another_file_or_other_bytes_blocks_the_plan_and_the_run() {
        let f = fx();
        import(&f, "louder", "1.0", &[("data/a.bin", b"ABCD")]);
        let wrong_file = patch(b"some other file", 1, "42", "5a");
        let wrong_bytes = patch(b"ABCD", 1, "41", "5a"); // the byte at 1 is 0x42, not 0x41
        for (id, p) in [("wrongfile", wrong_file), ("wrongbytes", wrong_bytes)] {
            let steps = vec![
                layer("louder", "1.0", ""),
                Step::PatchFile {
                    path: "data/a.bin".into(),
                    patch: p,
                },
            ];
            f.svc.recipe_save(&recipe(id, steps), None).unwrap();
            let plan = f.svc.recipe_plan(id).unwrap();
            assert!(plan.blocked(), "{id}: {plan:?}");
            assert!(f.svc.recipe_run(id, "modlib").is_err(), "{id}");
        }
        assert!(!f.base.join("modlib/.workspaces").exists());
    }

    #[test]
    fn a_run_is_repeatable_the_same_recipe_over_the_same_mods_gives_the_same_output_hash() {
        let f = fx();
        import(&f, "louder", "1.0", &[("data/a.bin", b"AAAA")]);
        f.svc
            .recipe_save(&recipe("hello", vec![layer("louder", "1.0", "")]), None)
            .unwrap();
        let plan = f.svc.recipe_plan("hello").unwrap();
        let a = f.svc.recipe_run("hello", "modlib").unwrap().record;
        let b = f.svc.recipe_run("hello", "modlib").unwrap().record;
        assert_ne!(a.run_id, b.run_id, "every run is its own folder");
        assert_eq!(a.output_sha256, plan.output_sha256);
        assert_eq!(a.output_sha256, b.output_sha256);
        assert!(a.matches_plan(&plan));
        assert_eq!(the_view(&f).recipes[0].runs.len(), 2);
    }

    #[test]
    fn a_run_that_does_not_fit_the_disk_is_refused_before_anything_is_written() {
        let f = fx();
        import(&f, "louder", "1.0", &[("data/a.bin", b"AAAA")]);
        f.svc
            .recipe_save(&recipe("hello", vec![layer("louder", "1.0", "")]), None)
            .unwrap();
        // Another service over the same state, told the disk is full.
        let full =
            ManagerService::with_parts(&f.base.join("state"), Box::new(Disk(Cell::new(0))), {
                let mut a = AdapterRegistry::new();
                a.register(Box::new(MadeUpGame::new())).unwrap();
                a
            })
            .unwrap();
        assert!(full.recipe_run("hello", "modlib").is_err());
        assert!(!f.base.join("modlib/.workspaces").exists());
    }

    #[test]
    fn the_mod_file_lookup_filters_by_path_ignoring_case_and_says_how_many_matched() {
        let f = fx();
        import(
            &f,
            "louder",
            "1.0",
            &[
                ("data/A.bin", b"AAAA"),
                ("data/b.txt", b"B"),
                ("DATA/c.bin", b"CC"),
                // The part the filter names is upper case in the path: the match must ignore case on both sides.
                ("data/D.BIN", b"D"),
            ],
        );
        let all = f.svc.mod_files(GAME, "louder", "1.0", "").unwrap();
        assert_eq!((all.matching, all.total, all.files.len()), (4, 4, 4));
        let bins = f.svc.mod_files(GAME, "louder", "1.0", " .BIN ").unwrap();
        assert_eq!((bins.matching, bins.total), (3, 4));
        assert!(bins.files.iter().any(|x| x.path == "data/D.BIN"));
        assert!(
            bins.files
                .iter()
                .all(|x| x.path.to_lowercase().ends_with(".bin"))
        );
        let a = bins.files.iter().find(|x| x.path == "data/A.bin").unwrap();
        assert_eq!((a.size, a.sha256.as_str()), (4, sha(b"AAAA").as_str()));
        let e = f
            .svc
            .mod_files(GAME, "ghost", "1.0", "")
            .unwrap_err()
            .to_string();
        assert!(e.contains("not in the mod store"), "{e}");
    }

    #[test]
    fn the_mod_file_lookup_lists_at_most_the_limit_and_still_counts_the_rest() {
        let f = fx();
        let many: Vec<(String, Vec<u8>)> = (0..MAX_LISTED_MOD_FILES + 5)
            .map(|i| (format!("data/f{i:03}.bin"), vec![b'x']))
            .collect();
        let refs: Vec<(&str, &[u8])> = many
            .iter()
            .map(|(p, b)| (p.as_str(), b.as_slice()))
            .collect();
        import(&f, "many", "1.0", &refs);
        let list = f.svc.mod_files(GAME, "many", "1.0", "").unwrap();
        assert_eq!(list.files.len(), MAX_LISTED_MOD_FILES);
        assert_eq!(
            (list.matching, list.total),
            (MAX_LISTED_MOD_FILES + 5, MAX_LISTED_MOD_FILES + 5)
        );
    }

    #[test]
    fn a_damaged_run_folder_listing_is_reported_not_hidden() {
        let f = fx();
        f.svc
            .recipe_save(&recipe("hello", vec![layer("louder", "1.0", "")]), None)
            .unwrap();
        // The recipe's workspace name is taken by a plain file, so its runs cannot be listed.
        put(&f.base.join("modlib/.workspaces/hello"), b"not a folder");
        let v = the_view(&f);
        assert_eq!(v.run_errors.len(), 1, "{:?}", v.run_errors);
        assert!(v.run_errors[0].contains("hello"), "{:?}", v.run_errors);
        assert!(v.recipes[0].runs.is_empty());
    }
}
