//! Reproducible recipes and the builder workspace (spec 2.2: patching, merging, validation, provenance and
//! reproducible recipes in one place).
//!
//! # A recipe
//!
//! A recipe is a named, ordered list of steps for one game. The steps that can run are **asset-level**:
//!
//! * `layer_mod`: put every file of an imported mod into the workspace, under a folder if one is given. A later
//!   layer wins a path (the same rule a deploy uses, and a path that differs only by letter case is the same
//!   path, as on Windows).
//! * `patch_file`: apply a [`BytePatch`] to a file the earlier steps produced.
//!
//! A `runtime_code` step can be written down, because the owner may want a recipe to say that some change is
//! code that runs inside the game. It is a different class of operation ([`OpClass::RuntimeCode`]) and SUL has
//! nothing that can run, check or undo it, so a recipe that contains one is **refused**, not run partly.
//!
//! The recipe's identity is the SHA-256 of its canonical JSON. Saving a recipe in another order of steps, or with
//! another patch, is a different recipe.
//!
//! # A run
//!
//! [`plan_recipe`] is a dry run: it reads the mods and works out, in memory, the exact files the workspace
//! would hold, with each file's SHA-256 and where it came from, plus warnings and problems. It writes nothing.
//! [`run_recipe`] does the same and then writes the files into
//! `<mod library root>/.workspaces/<recipe id>/<run id>/files/`, checks what it wrote against the plan, and only
//! then renames the finished folder into place. A `run.json` beside `files/` is the **provenance record**: the
//! recipe, the mods used (with the licence text the owner wrote down, which SUL does not check), every patch
//! with its before and after hashes, and for every file which step produced it and which earlier ones it
//! replaced.
//!
//! **Reproducible** here means: the same recipe over the same stored mods gives the same set of file hashes, and
//! so the same `output_sha256`; the dry run, the run and a later run all agree. It does not mean the files work
//! in a game.
//!
//! # What this does not do
//!
//! It never writes into a game build and never touches the mod store (mods are only read, and only after they
//! verify against their own inventory). It does not delete anything: an interrupted run leaves a `.staging-*`
//! folder that nothing refers to, and finished runs stay until the owner removes them. It does not make a build
//! (spec 2.7), does not know what a file means to a game, and has not been used on a real mod.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::adapters::ModLayout;
use crate::builds::{COPY_MARGIN_BYTES, Ctx};
use crate::error::io;
use crate::fsops;
use crate::journal::fresh_id;
use crate::manifest::{VerifyReport, inventory, now_unix};
use crate::mods::store::ModStore;
use crate::mods::{validate_mod_path, validate_name, validate_note, validate_text};
use crate::roots::RootKind;
use crate::safepath;
use crate::{Error, Result};

use super::OpClass;
use super::patch::{BytePatch, MAX_PATCH_FILE_BYTES, PatchOutcome, read_plain};

pub const RECIPE_SCHEMA: u32 = 1;
pub const RUN_SCHEMA: u32 = 1;
/// Most steps one recipe may have. Chosen here to keep a run bounded, not a measured figure.
pub const MAX_STEPS: usize = 500;
/// Most bytes of patched files a plan keeps in memory at once (counted generously: every patch counts its
/// whole output). Chosen here, not measured.
pub const MAX_PATCHED_BYTES: u64 = 256 * 1024 * 1024;
/// The hidden folder in a mod library root that holds workspaces. A game id may not start with a dot, so it
/// cannot collide with one.
pub const WORKSPACE_DIR: &str = ".workspaces";
pub const FILES_DIR: &str = "files";
pub const RUN_FILE: &str = "run.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Step {
    /// Every file of an imported mod, under `into` (empty: the top of the workspace).
    LayerMod {
        mod_id: String,
        version: String,
        into: String,
    },
    /// A byte patch applied to a file the earlier steps produced.
    PatchFile { path: String, patch: BytePatch },
    /// A change to code that runs inside the game. Can be written down; cannot be run.
    RuntimeCode { description: String },
}

impl Step {
    pub fn class(&self) -> OpClass {
        match self {
            Step::LayerMod { .. } | Step::PatchFile { .. } => OpClass::Asset,
            Step::RuntimeCode { .. } => OpClass::RuntimeCode,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recipe {
    pub schema: u32,
    pub id: String,
    pub name: String,
    pub game_id: String,
    pub description: String,
    pub steps: Vec<Step>,
}

impl Recipe {
    /// Check the recipe on its own (no mods, no files). A recipe with a runtime-code step is valid to write down.
    pub fn validate(&self) -> Result<()> {
        if self.schema != RECIPE_SCHEMA {
            return Err(Error::Invalid(format!(
                "recipe schema {} is not supported (this version reads {RECIPE_SCHEMA})",
                self.schema
            )));
        }
        validate_name("recipe id", &self.id)?;
        validate_text("recipe name", &self.name, 80)?;
        validate_name("game id", &self.game_id)?;
        validate_note("recipe description", &self.description, 1000)?;
        if self.steps.is_empty() || self.steps.len() > MAX_STEPS {
            return Err(Error::Invalid(format!(
                "a recipe has 1 to {MAX_STEPS} steps; this one has {}",
                self.steps.len()
            )));
        }
        for (i, step) in self.steps.iter().enumerate() {
            let n = i + 1;
            let wrap = |e: Error| Error::Invalid(format!("step {n}: {e}"));
            match step {
                Step::LayerMod {
                    mod_id,
                    version,
                    into,
                } => {
                    validate_name("mod id", mod_id).map_err(wrap)?;
                    validate_text("version", version, 64).map_err(wrap)?;
                    if !into.is_empty() {
                        validate_mod_path(into).map_err(wrap)?;
                    }
                }
                Step::PatchFile { path, patch } => {
                    validate_mod_path(path).map_err(wrap)?;
                    patch.validate().map_err(wrap)?;
                }
                Step::RuntimeCode { description } => {
                    validate_text("description", description, 200).map_err(wrap)?;
                }
            }
        }
        Ok(())
    }

    /// The bytes the recipe's identity is the hash of.
    pub fn canonical_json(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }

    /// The recipe's identity: SHA-256 of its canonical JSON.
    pub fn sha256(&self) -> Result<String> {
        Ok(hex::encode(Sha256::digest(self.canonical_json()?)))
    }

    /// The class of every step, in order.
    pub fn classes(&self) -> Vec<OpClass> {
        self.steps.iter().map(Step::class).collect()
    }

    /// Refuse a recipe that has a step SUL cannot run.
    pub fn check_runnable(&self) -> Result<()> {
        let bad: Vec<String> = self
            .steps
            .iter()
            .enumerate()
            .filter(|(_, s)| s.class() == OpClass::RuntimeCode)
            .map(|(i, _)| (i + 1).to_string())
            .collect();
        if bad.is_empty() {
            Ok(())
        } else {
            Err(Error::Invalid(format!(
                "step {} changes code that runs inside the game; SUL cannot run, check or undo that, so the recipe is refused as a whole",
                bad.join(", ")
            )))
        }
    }
}

/// A mod that a recipe used, as it was when the plan was made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunInput {
    pub step: usize,
    pub mod_id: String,
    pub version: String,
    pub source: String,
    /// What the owner wrote down when importing the mod. SUL does not check it.
    pub license_note: String,
    /// SHA-256 over the mod's own inventory (each path and hash, in order).
    pub inventory_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedPatch {
    pub step: usize,
    pub path: String,
    pub outcome: PatchOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedFile {
    pub path: String,
    pub sha256: String,
    pub size: u64,
    /// Which step produced the file as it is now.
    pub origin: String,
    /// The earlier providers of this path that it replaced, oldest first.
    pub replaced: Vec<String>,
}

/// What a run would write, worked out without writing anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunPlan {
    pub recipe_sha256: String,
    pub files: Vec<PlannedFile>,
    pub inputs: Vec<RunInput>,
    pub patches: Vec<AppliedPatch>,
    pub total_bytes: u64,
    /// SHA-256 over every output path and file hash, in path order. Two plans (or runs) of the same recipe over
    /// the same stored mods have the same value.
    pub output_sha256: String,
    /// Things worth knowing that do not stop a run.
    pub warnings: Vec<String>,
    /// Anything here stops a run.
    pub problems: Vec<String>,
}

impl RunPlan {
    pub fn blocked(&self) -> bool {
        !self.problems.is_empty()
    }
}

/// The provenance record of a finished run, saved as `run.json` beside its `files/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRecord {
    pub schema: u32,
    pub run_id: String,
    pub started_unix: u64,
    pub recipe: Recipe,
    pub recipe_sha256: String,
    pub inputs: Vec<RunInput>,
    pub patches: Vec<AppliedPatch>,
    pub files: Vec<PlannedFile>,
    pub output_sha256: String,
    pub warnings: Vec<String>,
}

impl RunRecord {
    /// True when `plan` (made now) would give the same files as this run did.
    pub fn matches_plan(&self, plan: &RunPlan) -> bool {
        !plan.blocked()
            && plan.recipe_sha256 == self.recipe_sha256
            && plan.output_sha256 == self.output_sha256
    }
}

enum Content {
    File(PathBuf),
    Mem(Vec<u8>),
}

struct Entry {
    rel: String,
    content: Content,
    sha256: String,
    size: u64,
    origin: String,
    replaced: Vec<String>,
}

struct Built {
    plan: RunPlan,
    entries: BTreeMap<String, Entry>,
}

fn output_hash(entries: &BTreeMap<String, Entry>) -> String {
    let mut h = Sha256::new();
    for e in entries.values() {
        h.update(e.rel.as_bytes());
        h.update([0]);
        h.update(e.sha256.as_bytes());
        h.update(b"\n");
    }
    hex::encode(h.finalize())
}

fn inventory_hash(files: &[crate::manifest::FileEntry]) -> String {
    let mut h = Sha256::new();
    for f in files {
        h.update(f.path.as_bytes());
        h.update([0]);
        h.update(f.sha256.as_bytes());
        h.update(b"\n");
    }
    hex::encode(h.finalize())
}

fn build(ctx: &Ctx, store: &ModStore, recipe: &Recipe, layout: &ModLayout) -> Result<Built> {
    recipe.validate()?;
    recipe.check_runnable()?;
    let mut entries: BTreeMap<String, Entry> = BTreeMap::new();
    let mut inputs = Vec::new();
    let mut patches = Vec::new();
    let mut warnings = Vec::new();
    let mut problems = Vec::new();
    let mut patched_total = 0u64;

    for (i, step) in recipe.steps.iter().enumerate() {
        let n = i + 1;
        match step {
            Step::LayerMod {
                mod_id,
                version,
                into,
            } => {
                let Some(rec) = store.get(&recipe.game_id, mod_id, version) else {
                    problems.push(format!(
                        "step {n}: mod {mod_id} {version} for {} is not in the mod store",
                        recipe.game_id
                    ));
                    continue;
                };
                match store.verify(ctx, rec) {
                    Ok(r) if r.is_clean() => {}
                    Ok(_) => {
                        problems.push(format!(
                            "step {n}: the stored files of mod {mod_id} {version} no longer match its own inventory (changed, missing or extra files)"
                        ));
                        continue;
                    }
                    Err(e) => {
                        problems.push(format!(
                            "step {n}: mod {mod_id} {version} could not be checked: {e}"
                        ));
                        continue;
                    }
                }
                let dir = match store.mod_dir(ctx, rec) {
                    Ok(d) => d,
                    Err(e) => {
                        problems.push(format!("step {n}: mod {mod_id} {version}: {e}"));
                        continue;
                    }
                };
                if rec.manifest.license_note.trim().is_empty() {
                    warnings.push(format!(
                        "mod {mod_id} {version} has no licence or permission note"
                    ));
                }
                inputs.push(RunInput {
                    step: n,
                    mod_id: mod_id.clone(),
                    version: version.clone(),
                    source: rec.manifest.source.clone(),
                    license_note: rec.manifest.license_note.clone(),
                    inventory_sha256: inventory_hash(&rec.manifest.files),
                });
                for f in &rec.manifest.files {
                    let rel = if into.is_empty() {
                        f.path.clone()
                    } else {
                        format!("{into}/{}", f.path)
                    };
                    let (rel, source) = match validate_mod_path(&rel)
                        .and_then(|r| safepath::join_within(&dir, &f.path).map(|s| (r, s)))
                    {
                        Ok(v) => v,
                        Err(e) => {
                            problems.push(format!("step {n}: {}: {e}", f.path));
                            continue;
                        }
                    };
                    let key = rel.to_ascii_lowercase();
                    let mut replaced = Vec::new();
                    if let Some(old) = entries.get(&key) {
                        replaced = old.replaced.clone();
                        replaced.push(old.origin.clone());
                    }
                    entries.insert(
                        key,
                        Entry {
                            rel,
                            content: Content::File(source),
                            sha256: f.sha256.clone(),
                            size: f.size,
                            origin: format!("step {n}: mod {mod_id} {version}"),
                            replaced,
                        },
                    );
                }
            }
            Step::PatchFile { path, patch } => {
                let key = match validate_mod_path(path) {
                    Ok(p) => p.to_ascii_lowercase(),
                    Err(e) => {
                        problems.push(format!("step {n}: {e}"));
                        continue;
                    }
                };
                let Some(entry) = entries.get_mut(&key) else {
                    problems.push(format!(
                        "step {n}: there is no file {path} yet for the patch to change"
                    ));
                    continue;
                };
                let bytes = match &entry.content {
                    Content::File(p) => read_plain(p, MAX_PATCH_FILE_BYTES),
                    Content::Mem(b) => Ok(b.clone()),
                };
                let bytes = match bytes {
                    Ok(b) => b,
                    Err(e) => {
                        problems.push(format!("step {n}: {path}: {e}"));
                        continue;
                    }
                };
                match patch.apply_bytes(&bytes) {
                    Ok(out) => {
                        patched_total = patched_total.saturating_add(out.len() as u64);
                        if patched_total > MAX_PATCHED_BYTES {
                            problems.push(format!(
                                "step {n}: the patched files would need more than {MAX_PATCHED_BYTES} bytes of memory"
                            ));
                            continue;
                        }
                        let outcome = patch.outcome(&bytes, &out);
                        entry.sha256 = outcome.after_sha256.clone();
                        entry.size = out.len() as u64;
                        entry.origin =
                            format!("step {n}: patch {} applied to [{}]", patch.id, entry.origin);
                        entry.content = Content::Mem(out);
                        patches.push(AppliedPatch {
                            step: n,
                            path: entry.rel.clone(),
                            outcome,
                        });
                    }
                    Err(e) => problems.push(format!("step {n}: {path}: {e}")),
                }
            }
            Step::RuntimeCode { .. } => unreachable!("check_runnable refused it"),
        }
    }

    if entries.is_empty() && problems.is_empty() {
        problems.push("the recipe produces no files".into());
    }
    for e in entries.values() {
        if let Some(why) = layout.refusal_for(&e.rel) {
            warnings.push(format!(
                "{}: a deploy would refuse this path ({why})",
                e.rel
            ));
        }
    }
    let files: Vec<PlannedFile> = entries
        .values()
        .map(|e| PlannedFile {
            path: e.rel.clone(),
            sha256: e.sha256.clone(),
            size: e.size,
            origin: e.origin.clone(),
            replaced: e.replaced.clone(),
        })
        .collect();
    let total_bytes = files.iter().map(|f| f.size).sum();
    Ok(Built {
        plan: RunPlan {
            recipe_sha256: recipe.sha256()?,
            files,
            inputs,
            patches,
            total_bytes,
            output_sha256: output_hash(&entries),
            warnings,
            problems,
        },
        entries,
    })
}

/// A dry run. Reads the stored mods; writes nothing.
pub fn plan_recipe(
    ctx: &Ctx,
    store: &ModStore,
    recipe: &Recipe,
    layout: &ModLayout,
) -> Result<RunPlan> {
    Ok(build(ctx, store, recipe, layout)?.plan)
}

fn workspace_dir(ctx: &Ctx, root_id: &str, recipe_id: &str) -> Result<PathBuf> {
    let root = ctx
        .roots
        .get(root_id)
        .ok_or_else(|| Error::Invalid(format!("no library root {root_id:?}")))?;
    if root.kind != RootKind::Mods {
        return Err(Error::Invalid(format!(
            "library root {root_id:?} is not a mod library; workspaces live in a mod library"
        )));
    }
    validate_name("recipe id", recipe_id)?;
    Ok(ctx
        .roots
        .usable_path(root_id)?
        .join(WORKSPACE_DIR)
        .join(recipe_id))
}

/// Build the recipe's files into a new workspace folder and write its provenance record.
pub fn run_recipe(
    ctx: &Ctx,
    store: &ModStore,
    recipe: &Recipe,
    layout: &ModLayout,
    root_id: &str,
) -> Result<RunRecord> {
    let ws = workspace_dir(ctx, root_id, &recipe.id)?;
    let built = build(ctx, store, recipe, layout)?;
    if built.plan.blocked() {
        return Err(Error::Invalid(format!(
            "the recipe cannot run: {}",
            built.plan.problems.join("; ")
        )));
    }
    ctx.roots
        .ensure_space(ctx.disk, root_id, built.plan.total_bytes, COPY_MARGIN_BYTES)?;

    let run_id = fresh_id("run");
    let final_dir = ws.join(&run_id);
    let staging = ws.join(format!(".staging-{run_id}"));
    let record = RunRecord {
        schema: RUN_SCHEMA,
        run_id: run_id.clone(),
        started_unix: now_unix(),
        recipe: recipe.clone(),
        recipe_sha256: built.plan.recipe_sha256.clone(),
        inputs: built.plan.inputs.clone(),
        patches: built.plan.patches.clone(),
        files: built.plan.files.clone(),
        output_sha256: built.plan.output_sha256.clone(),
        warnings: built.plan.warnings.clone(),
    };

    ctx.journal.run(
        &run_id,
        "recipe_run",
        &format!("recipe {} into {}", recipe.id, final_dir.display()),
        || {
            let files_dir = staging.join(FILES_DIR);
            fsops::create_dir(ctx.protected, &files_dir)?;
            for e in built.entries.values() {
                let dst = safepath::join_within(&files_dir, &e.rel)?;
                match &e.content {
                    Content::File(src) => fsops::copy_file_atomic(ctx.protected, src, &dst, false)?,
                    Content::Mem(bytes) => fsops::atomic_write(ctx.protected, &dst, bytes)?,
                }
            }
            let written = inventory(&files_dir)?;
            let wrote: BTreeMap<String, &str> = written
                .iter()
                .map(|f| (f.path.to_ascii_lowercase(), f.sha256.as_str()))
                .collect();
            let planned = built.entries.iter().all(|(k, e)| wrote.get(k) == Some(&e.sha256.as_str()));
            if !planned || wrote.len() != built.entries.len() {
                return Err(Error::Invalid(
                    "what was written does not match the plan; the run was left in its staging folder".into(),
                ));
            }
            let body = serde_json::to_string_pretty(&record)?;
            fsops::atomic_write(ctx.protected, &staging.join(RUN_FILE), body.as_bytes())?;
            fsops::rename_new(ctx.protected, &staging, &final_dir)
        },
    )?;
    Ok(record)
}

fn run_dir(ctx: &Ctx, root_id: &str, recipe_id: &str, run_id: &str) -> Result<PathBuf> {
    validate_name("run id", run_id)?;
    Ok(workspace_dir(ctx, root_id, recipe_id)?.join(run_id))
}

/// The two folders of a finished run: the run folder (it holds `run.json`) and the `files` folder inside it that the
/// recipe built. Both must be plain folders, so a link cannot send a tool somewhere else. A run that does not exist,
/// or one still being staged, is refused.
pub fn run_folders(
    ctx: &Ctx,
    root_id: &str,
    recipe_id: &str,
    run_id: &str,
) -> Result<(PathBuf, PathBuf)> {
    let dir = run_dir(ctx, root_id, recipe_id, run_id)?;
    let files = dir.join(FILES_DIR);
    let plain = |p: &std::path::Path| std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir());
    if !plain(&dir) || !dir.join(RUN_FILE).is_file() || !plain(&files) {
        return Err(Error::Invalid(format!(
            "there is no finished run {run_id:?} of recipe {recipe_id:?} in library folder {root_id:?}"
        )));
    }
    Ok((dir, files))
}

/// Read a finished run's provenance record.
pub fn read_run(ctx: &Ctx, root_id: &str, recipe_id: &str, run_id: &str) -> Result<RunRecord> {
    let file = run_dir(ctx, root_id, recipe_id, run_id)?.join(RUN_FILE);
    let text = std::fs::read_to_string(&file).map_err(io(&file))?;
    let rec: RunRecord = serde_json::from_str(&text).map_err(|e| Error::json(&file, e))?;
    if rec.schema > RUN_SCHEMA {
        return Err(Error::Invalid(format!(
            "run record schema {} is newer than supported {RUN_SCHEMA}",
            rec.schema
        )));
    }
    Ok(rec)
}

/// The finished runs of a recipe, oldest first by name. A staging folder is not listed.
pub fn list_runs(ctx: &Ctx, root_id: &str, recipe_id: &str) -> Result<Vec<String>> {
    let ws = workspace_dir(ctx, root_id, recipe_id)?;
    let mut out = Vec::new();
    match std::fs::read_dir(&ws) {
        Ok(rd) => {
            for e in rd {
                let e = e.map_err(io(&ws))?;
                if let Some(name) = e.file_name().to_str()
                    && !name.starts_with('.')
                    && e.path().join(RUN_FILE).is_file()
                {
                    out.push(name.to_owned());
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(io(&ws)(e)),
    }
    out.sort();
    Ok(out)
}

/// Compare a run's files with its record. Read only.
pub fn verify_run(ctx: &Ctx, root_id: &str, recipe_id: &str, run_id: &str) -> Result<VerifyReport> {
    let dir = run_dir(ctx, root_id, recipe_id, run_id)?;
    let rec = read_run(ctx, root_id, recipe_id, run_id)?;
    let now = inventory(&dir.join(FILES_DIR))?;
    let now: BTreeMap<&str, &str> = now
        .iter()
        .map(|f| (f.path.as_str(), f.sha256.as_str()))
        .collect();
    let mut report = VerifyReport::default();
    for f in &rec.files {
        match now.get(f.path.as_str()) {
            None => report.missing.push(f.path.clone()),
            Some(h) if *h != f.sha256 => report.modified.push(f.path.clone()),
            Some(_) => {}
        }
    }
    let known: std::collections::HashSet<&str> =
        rec.files.iter().map(|f| f.path.as_str()).collect();
    for p in now.keys() {
        if !known.contains(p) {
            report.extra.push((*p).to_owned());
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::super::patch::tests::patch_for;
    use super::*;
    use crate::mods::store::tests::{mod_world, req};
    use crate::testkit::{GAME, World};

    fn layout() -> ModLayout {
        ModLayout {
            allowed_prefixes: vec!["mods".into(), "data".into()],
            forbidden_prefixes: vec!["data/anticheat".into()],
            case_insensitive: true,
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

    fn layer(mod_id: &str, version: &str, into: &str) -> Step {
        Step::LayerMod {
            mod_id: mod_id.into(),
            version: version.into(),
            into: into.into(),
        }
    }

    struct Fx {
        w: World,
        store: ModStore,
    }

    fn fx() -> Fx {
        let w = mod_world();
        let mut store = ModStore::open(&w.base.join("state/mods.json")).unwrap();
        store
            .import_folder(
                &w.ctx(),
                &req(
                    &w,
                    "a",
                    "1.0",
                    &[("data/x.bin", "from-a"), ("mods/only-a.txt", "A")],
                ),
            )
            .unwrap();
        store
            .import_folder(
                &w.ctx(),
                &req(
                    &w,
                    "b",
                    "2.0",
                    &[("data/X.bin", "from-b"), ("mods/only-b.txt", "B")],
                ),
            )
            .unwrap();
        Fx { w, store }
    }

    fn files_of(f: &Fx, rec: &RunRecord) -> BTreeMap<String, String> {
        let dir = run_dir(&f.w.ctx(), "mods", &rec.recipe.id, &rec.run_id)
            .unwrap()
            .join(FILES_DIR);
        crate::manifest::inventory(&dir)
            .unwrap()
            .into_iter()
            .map(|e| {
                let text = std::fs::read_to_string(dir.join(&e.path)).unwrap();
                (e.path, text)
            })
            .collect()
    }

    #[test]
    fn a_later_layer_wins_a_path_and_a_case_variant_is_the_same_path() {
        let f = fx();
        let r = recipe("r1", vec![layer("a", "1.0", ""), layer("b", "2.0", "")]);
        let plan = plan_recipe(&f.w.ctx(), &f.store, &r, &layout()).unwrap();
        assert!(plan.problems.is_empty(), "{:?}", plan.problems);
        let paths: Vec<&str> = plan.files.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(paths, ["data/X.bin", "mods/only-a.txt", "mods/only-b.txt"]);
        let x = &plan.files[0];
        assert_eq!(x.origin, "step 2: mod b 2.0");
        assert_eq!(x.replaced, ["step 1: mod a 1.0"]);
    }

    #[test]
    fn a_dry_run_writes_nothing_and_a_run_matches_it() {
        let f = fx();
        let r = recipe("r1", vec![layer("a", "1.0", ""), layer("b", "2.0", "")]);
        let plan = plan_recipe(&f.w.ctx(), &f.store, &r, &layout()).unwrap();
        assert!(
            !f.w.base.join("mods").join(WORKSPACE_DIR).exists(),
            "a dry run creates nothing"
        );
        let rec = run_recipe(&f.w.ctx(), &f.store, &r, &layout(), "mods").unwrap();
        assert!(rec.matches_plan(&plan));
        assert_eq!(rec.output_sha256, plan.output_sha256);
        let files = files_of(&f, &rec);
        assert_eq!(files["data/X.bin"], "from-b");
        assert_eq!(files["mods/only-a.txt"], "A");
        assert!(f.w.journal.pending().unwrap().is_empty());
    }

    #[test]
    fn the_same_recipe_over_the_same_mods_gives_the_same_files_every_time() {
        let f = fx();
        let r = recipe("r1", vec![layer("a", "1.0", "mods"), layer("b", "2.0", "")]);
        let one = run_recipe(&f.w.ctx(), &f.store, &r, &layout(), "mods").unwrap();
        let two = run_recipe(&f.w.ctx(), &f.store, &r, &layout(), "mods").unwrap();
        assert_ne!(one.run_id, two.run_id);
        assert_eq!(one.output_sha256, two.output_sha256);
        assert_eq!(one.files, two.files);
        assert_eq!(files_of(&f, &one), files_of(&f, &two));
        assert_eq!(
            list_runs(&f.w.ctx(), "mods", "r1").unwrap().len(),
            2,
            "both runs are kept"
        );
    }

    #[test]
    fn another_order_or_another_patch_is_another_recipe_with_another_output() {
        let f = fx();
        let ab = recipe("r1", vec![layer("a", "1.0", ""), layer("b", "2.0", "")]);
        let ba = recipe("r1", vec![layer("b", "2.0", ""), layer("a", "1.0", "")]);
        assert_ne!(ab.sha256().unwrap(), ba.sha256().unwrap());
        let p1 = plan_recipe(&f.w.ctx(), &f.store, &ab, &layout()).unwrap();
        let p2 = plan_recipe(&f.w.ctx(), &f.store, &ba, &layout()).unwrap();
        assert_ne!(
            p1.output_sha256, p2.output_sha256,
            "the winner of data/x.bin differs"
        );
    }

    #[test]
    fn a_patch_changes_the_planned_hash_and_the_written_bytes_and_is_recorded() {
        let f = fx();
        let patch = patch_for("p1", b"from-a", &[(0, b"FROM")]);
        let r = recipe(
            "r1",
            vec![
                layer("a", "1.0", ""),
                Step::PatchFile {
                    path: "data/x.bin".into(),
                    patch: patch.clone(),
                },
            ],
        );
        let plain = recipe("r0", vec![layer("a", "1.0", "")]);
        let before = plan_recipe(&f.w.ctx(), &f.store, &plain, &layout()).unwrap();
        let plan = plan_recipe(&f.w.ctx(), &f.store, &r, &layout()).unwrap();
        assert!(plan.problems.is_empty(), "{:?}", plan.problems);
        assert_ne!(plan.output_sha256, before.output_sha256);
        assert_eq!(plan.patches.len(), 1);
        assert_eq!(plan.patches[0].step, 2);
        assert_eq!(plan.patches[0].outcome.bytes_changed, 4);

        let rec = run_recipe(&f.w.ctx(), &f.store, &r, &layout(), "mods").unwrap();
        assert_eq!(files_of(&f, &rec)["data/x.bin"], "FROM-a");
        assert!(rec.matches_plan(&plan));
        let x = rec.files.iter().find(|p| p.path == "data/x.bin").unwrap();
        assert!(x.origin.contains("patch p1"), "{}", x.origin);
        assert!(x.origin.contains("mod a 1.0"), "{}", x.origin);

        // The stored mod is untouched.
        let rec_mod = f.store.get(GAME, "a", "1.0").unwrap();
        assert!(f.store.verify(&f.w.ctx(), rec_mod).unwrap().is_clean());
    }

    #[test]
    fn a_patch_for_another_file_or_a_missing_file_is_a_problem_that_blocks_the_run() {
        let f = fx();
        let wrong = patch_for("p1", b"not the file", &[(0, b"xx")]);
        let r = recipe(
            "r1",
            vec![
                layer("a", "1.0", ""),
                Step::PatchFile {
                    path: "data/x.bin".into(),
                    patch: wrong.clone(),
                },
                Step::PatchFile {
                    path: "data/none.bin".into(),
                    patch: wrong,
                },
            ],
        );
        let plan = plan_recipe(&f.w.ctx(), &f.store, &r, &layout()).unwrap();
        assert_eq!(plan.problems.len(), 2, "{:?}", plan.problems);
        assert!(plan.blocked());
        assert!(run_recipe(&f.w.ctx(), &f.store, &r, &layout(), "mods").is_err());
        assert!(!f.w.base.join("mods").join(WORKSPACE_DIR).exists());
    }

    #[test]
    fn a_recipe_with_a_runtime_code_step_is_valid_to_keep_but_is_never_run() {
        let f = fx();
        let r = recipe(
            "r1",
            vec![
                layer("a", "1.0", ""),
                Step::RuntimeCode {
                    description: "hook the damage function".into(),
                },
            ],
        );
        assert!(r.validate().is_ok());
        assert_eq!(r.classes(), [OpClass::Asset, OpClass::RuntimeCode]);
        let err = r.check_runnable().unwrap_err().to_string();
        assert!(err.contains("step 2"), "{err}");
        assert!(plan_recipe(&f.w.ctx(), &f.store, &r, &layout()).is_err());
        assert!(run_recipe(&f.w.ctx(), &f.store, &r, &layout(), "mods").is_err());
        assert!(!f.w.base.join("mods").join(WORKSPACE_DIR).exists());
    }

    #[test]
    fn a_missing_mod_and_a_mod_changed_in_the_store_are_problems() {
        let f = fx();
        let r = recipe("r1", vec![layer("ghost", "1.0", ""), layer("a", "1.0", "")]);
        let plan = plan_recipe(&f.w.ctx(), &f.store, &r, &layout()).unwrap();
        assert!(plan.problems[0].contains("not in the mod store"));

        let rec = f.store.get(GAME, "a", "1.0").unwrap().clone();
        let dir = f.store.mod_dir(&f.w.ctx(), &rec).unwrap();
        std::fs::write(dir.join("data/x.bin"), "TAMPERED").unwrap();
        let r2 = recipe("r2", vec![layer("a", "1.0", "")]);
        let plan = plan_recipe(&f.w.ctx(), &f.store, &r2, &layout()).unwrap();
        assert!(plan.blocked());
        assert!(
            plan.problems[0].contains("no longer match"),
            "{:?}",
            plan.problems
        );
    }

    #[test]
    fn provenance_names_every_input_with_its_licence_note_and_warns_about_a_missing_one() {
        let w = mod_world();
        let mut store = ModStore::open(&w.base.join("state/mods.json")).unwrap();
        let mut import = req(&w, "nolic", "1.0", &[("mods/n.txt", "n")]);
        import.license_note = String::new();
        store.import_folder(&w.ctx(), &import).unwrap();
        store
            .import_folder(&w.ctx(), &req(&w, "lic", "1.0", &[("mods/l.txt", "l")]))
            .unwrap();
        let r = recipe(
            "r1",
            vec![layer("nolic", "1.0", ""), layer("lic", "1.0", "")],
        );
        let rec = run_recipe(&w.ctx(), &store, &r, &layout(), "mods").unwrap();
        assert_eq!(rec.inputs.len(), 2);
        assert_eq!(rec.inputs[1].license_note, "mine");
        assert_eq!(rec.inputs[1].source, "made up for a test");
        assert_eq!(rec.inputs[0].inventory_sha256.len(), 64);
        assert_eq!(
            rec.warnings,
            ["mod nolic 1.0 has no licence or permission note"]
        );
        // The record on disk is the record returned.
        assert_eq!(read_run(&w.ctx(), "mods", "r1", &rec.run_id).unwrap(), rec);
    }

    #[test]
    fn files_a_deploy_would_refuse_are_warned_about_not_blocked() {
        let w = mod_world();
        let mut store = ModStore::open(&w.base.join("state/mods.json")).unwrap();
        store
            .import_folder(
                &w.ctx(),
                &req(
                    &w,
                    "m",
                    "1.0",
                    &[
                        ("data/anticheat/g.dll", "x"),
                        ("elsewhere/y.txt", "y"),
                        ("mods/ok.txt", "ok"),
                    ],
                ),
            )
            .unwrap();
        let r = recipe("r1", vec![layer("m", "1.0", "")]);
        let plan = plan_recipe(&w.ctx(), &store, &r, &layout()).unwrap();
        assert!(plan.problems.is_empty());
        assert_eq!(plan.warnings.len(), 2, "{:?}", plan.warnings);
        assert!(
            plan.warnings
                .iter()
                .any(|m| m.starts_with("data/anticheat/g.dll"))
        );
        assert!(
            plan.warnings
                .iter()
                .any(|m| m.starts_with("elsewhere/y.txt"))
        );
    }

    #[test]
    fn verifying_a_run_notices_a_changed_missing_or_extra_file() {
        let f = fx();
        let r = recipe("r1", vec![layer("a", "1.0", ""), layer("b", "2.0", "")]);
        let rec = run_recipe(&f.w.ctx(), &f.store, &r, &layout(), "mods").unwrap();
        assert!(
            verify_run(&f.w.ctx(), "mods", "r1", &rec.run_id)
                .unwrap()
                .is_clean()
        );
        let dir = run_dir(&f.w.ctx(), "mods", "r1", &rec.run_id)
            .unwrap()
            .join(FILES_DIR);
        std::fs::write(dir.join("data/X.bin"), "changed").unwrap();
        std::fs::remove_file(dir.join("mods/only-a.txt")).unwrap();
        std::fs::write(dir.join("mods/new.txt"), "new").unwrap();
        let v = verify_run(&f.w.ctx(), "mods", "r1", &rec.run_id).unwrap();
        assert_eq!(v.modified, ["data/X.bin"]);
        assert_eq!(v.missing, ["mods/only-a.txt"]);
        assert_eq!(v.extra, ["mods/new.txt"]);
    }

    #[test]
    fn a_run_folder_without_its_files_folder_is_not_a_finished_run() {
        let f = fx();
        let r = recipe("r1", vec![layer("a", "1.0", "")]);
        let rec = run_recipe(&f.w.ctx(), &f.store, &r, &layout(), "mods").unwrap();
        let (dir, files) = run_folders(&f.w.ctx(), "mods", "r1", &rec.run_id).unwrap();
        std::fs::remove_dir_all(&files).unwrap();
        assert!(dir.join(RUN_FILE).is_file(), "the record is still there");
        let e = run_folders(&f.w.ctx(), "mods", "r1", &rec.run_id)
            .unwrap_err()
            .to_string();
        assert!(e.contains("no finished run"), "{e}");
    }

    #[test]
    fn the_folders_of_a_run_are_found_only_for_a_finished_run_in_a_mod_library() {
        let f = fx();
        let r = recipe("r1", vec![layer("a", "1.0", "")]);
        let rec = run_recipe(&f.w.ctx(), &f.store, &r, &layout(), "mods").unwrap();
        let (dir, files) = run_folders(&f.w.ctx(), "mods", "r1", &rec.run_id).unwrap();
        assert!(dir.join(RUN_FILE).is_file());
        assert_eq!(files, dir.join(FILES_DIR));
        assert!(files.is_dir());

        // A run that does not exist, a staging folder, another recipe, a builds root and a name that is not a run id.
        assert!(run_folders(&f.w.ctx(), "mods", "r1", "run-nope").is_err());
        assert!(run_folders(&f.w.ctx(), "mods", "r2", &rec.run_id).is_err());
        assert!(run_folders(&f.w.ctx(), "ssd", "r1", &rec.run_id).is_err());
        assert!(run_folders(&f.w.ctx(), "mods", "r1", "../r1").is_err());
        assert!(run_folders(&f.w.ctx(), "mods", "r1", "").is_err());

        // A run folder without its record is not a finished run.
        std::fs::remove_file(dir.join(RUN_FILE)).unwrap();
        let e = run_folders(&f.w.ctx(), "mods", "r1", &rec.run_id)
            .unwrap_err()
            .to_string();
        assert!(e.contains("no finished run"), "{e}");
    }

    #[test]
    fn a_run_goes_only_into_a_mod_library_and_needs_the_space() {
        let mut f = fx();
        let r = recipe("r1", vec![layer("a", "1.0", "")]);
        assert!(
            run_recipe(&f.w.ctx(), &f.store, &r, &layout(), "ssd").is_err(),
            "a builds root"
        );
        assert!(run_recipe(&f.w.ctx(), &f.store, &r, &layout(), "nowhere").is_err());
        f.w.disk = crate::roots::fake::FakeDisk::with(10);
        assert!(matches!(
            run_recipe(&f.w.ctx(), &f.store, &r, &layout(), "mods"),
            Err(Error::InsufficientSpace { .. })
        ));
        assert!(!f.w.base.join("mods").join(WORKSPACE_DIR).exists());
    }

    #[test]
    fn recipes_with_bad_fields_are_refused() {
        let ok = recipe("r1", vec![layer("a", "1.0", "mods")]);
        assert!(ok.validate().is_ok());
        let mut r = ok.clone();
        r.schema = 2;
        assert!(r.validate().is_err());
        let mut r = ok.clone();
        r.id = "../x".into();
        assert!(r.validate().is_err());
        let mut r = ok.clone();
        r.steps.clear();
        assert!(r.validate().is_err());
        let mut r = ok.clone();
        r.steps = vec![layer("a", "1.0", "../out")];
        assert!(r.validate().is_err());
        let mut r = ok.clone();
        r.steps = vec![layer("a", "", "")];
        assert!(r.validate().is_err());
        let mut r = ok.clone();
        r.steps = vec![Step::PatchFile {
            path: "../x".into(),
            patch: patch_for("p", b"abcd", &[(0, b"x")]),
        }];
        assert!(r.validate().is_err());
        let mut r = ok;
        r.steps = (0..=MAX_STEPS).map(|_| layer("a", "1.0", "")).collect();
        assert!(r.validate().is_err());
        r.steps.pop();
        assert!(r.validate().is_ok(), "exactly MAX_STEPS");
    }

    #[test]
    fn the_recipe_identity_is_pinned_so_a_change_to_the_format_is_noticed() {
        let r = recipe("r1", vec![layer("a", "1.0", "mods")]);
        assert_eq!(
            String::from_utf8(r.canonical_json().unwrap()).unwrap(),
            r#"{"schema":1,"id":"r1","name":"Recipe r1","game_id":"madeupgame","description":"","steps":[{"op":"layer_mod","mod_id":"a","version":"1.0","into":"mods"}]}"#
        );
    }
}
