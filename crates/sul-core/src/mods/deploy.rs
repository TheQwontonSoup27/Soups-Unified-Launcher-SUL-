//! Deploying a profile into a managed build, and purging it again (spec 2.1: "Purge & Deploy" and a standalone
//! "Purge", with a dry-run preview, per-file ownership, exact affected paths and recovery logs).
//!
//! # The ownership ledger
//!
//! Every deploy first writes a ledger (`<build id>.mods/state.json`, next to the build folder, not inside it)
//! that lists each file it is about to add or replace, with the SHA-256 of the file that will be put there and,
//! for a file that is replaced, the SHA-256 of the file that was there. Only then are files copied. A purge uses
//! the ledger and nothing else: it will only touch a path the ledger names, and only while the file there still
//! has exactly the hash the deploy put there. That is the whole definition of "provably managed".
//!
//! # What a purge does with each ledger entry
//!
//! | The file now                                  | What the purge does                                      |
//! |-----------------------------------------------|----------------------------------------------------------|
//! | still the deployed file                       | moves it into the purge's recovery folder; if it replaced a base file, moves the kept base file back |
//! | gone, and it replaced a base file with a kept copy | moves the kept base file back                       |
//! | gone, and it was an added file                | forgets the entry (nothing to do)                        |
//! | already the base file again                   | forgets the entry                                        |
//! | anything else (changed since the deploy)      | **leaves it alone** and reports it                       |
//! | would need a kept base file that is missing or changed | **leaves it alone** and reports it            |
//!
//! Files that are not in the ledger are never looked at, however much they look like mod files. Nothing is
//! deleted: removed files go to `<build id>.mods/recovery/<purge id>/removed/`, and the latest purge can be
//! undone while nothing else has changed since.
//!
//! A deploy or purge that stops part way (a crash, a full disk) leaves the ledger as written first, and the
//! rules above work out from the hashes what is done and what is not, so running a purge finishes the job.
//!
//! # What this does not do
//!
//! It does not check that a mod works, does not scan it for malware, and does not know a game's real file
//! layout: where mods may go comes from the game adapter's [`ModLayout`]. It is tested on made-up files only.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::adapters::ModLayout;
use crate::builds::{BuildRegistry, COPY_MARGIN_BYTES, Ctx, Location};
use crate::error::io;
use crate::fsops;
use crate::journal::fresh_id;
use crate::manifest::{BuildManifest, FileEntry, PatchRef, Variant, hash_file, now_unix};
use crate::safepath;
use crate::{Error, Result};

use super::profile::{Problem, ProblemKind, Profile, Severity, check_profile};
use super::store::ModStore;

pub const STATE_SCHEMA: u32 = 1;
/// How many deploys and purges the history keeps.
pub const HISTORY_LIMIT: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryAction {
    /// There was no file at that path.
    Add,
    /// There was a file at that path; it was moved aside and kept.
    Replace,
}

/// One file a deploy put (or was about to put) in a build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerEntry {
    /// Forward-slash path in the build, spelled as the build's manifest spells it when it lists the file.
    pub path: String,
    pub mod_id: String,
    pub mod_version: String,
    pub action: EntryAction,
    pub deployed_sha256: String,
    pub deployed_size: u64,
    /// What was there before, when `action` is `Replace`.
    pub base_sha256: Option<String>,
    pub base_size: Option<u64>,
    /// The build manifest's own entry for this path before the deploy (it can differ from `base_*` when the
    /// base file had been changed), so a purge puts the manifest back exactly as it was.
    pub manifest_before: Option<FileEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeployStatus {
    /// Written first; the files may be only partly in place.
    InProgress,
    Deployed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deployment {
    pub id: String,
    pub profile_id: String,
    pub profile_name: String,
    pub status: DeployStatus,
    pub started_unix: u64,
    pub finished_unix: Option<u64>,
    pub entries: Vec<LedgerEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevertedEntry {
    pub entry: LedgerEntry,
    /// The deployed file was moved to the purge's recovery folder.
    pub removed_mod_file: bool,
    /// The kept base file was moved back into place.
    pub restored_base: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentHeader {
    pub id: String,
    pub profile_id: String,
    pub profile_name: String,
    pub started_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurgeRecord {
    pub id: String,
    pub deployment: DeploymentHeader,
    pub reverted: Vec<RevertedEntry>,
    pub unix: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryKind {
    Deploy,
    Purge,
    UndoPurge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryItem {
    pub id: String,
    pub kind: HistoryKind,
    pub profile_id: String,
    pub unix: u64,
    pub files: usize,
    pub note: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ModsState {
    schema: u32,
    deployment: Option<Deployment>,
    last_purge: Option<PurgeRecord>,
    history: Vec<HistoryItem>,
}

impl ModsState {
    fn empty() -> Self {
        Self {
            schema: STATE_SCHEMA,
            deployment: None,
            last_purge: None,
            history: vec![],
        }
    }

    fn push_history(&mut self, item: HistoryItem) {
        self.history.push(item);
        let extra = self.history.len().saturating_sub(HISTORY_LIMIT);
        self.history.drain(..extra);
    }
}

// ---- where things live ----

/// A managed build: its record's folder and its mods folder (`<build id>.mods`, beside the build).
struct Target {
    dir: PathBuf,
    state_dir: PathBuf,
}

fn target(reg: &BuildRegistry, ctx: &Ctx, build_id: &str) -> Result<Target> {
    let rec = reg
        .get(build_id)
        .ok_or_else(|| Error::Invalid(format!("no build with id {build_id:?}")))?;
    if !matches!(rec.location, Location::Managed { .. }) {
        return Err(Error::Invalid(format!(
            "build {build_id:?} is an install that stays where it is; SUL never writes to it"
        )));
    }
    let dir = reg.build_dir(ctx, build_id)?;
    let parent = dir
        .parent()
        .ok_or_else(|| Error::Invalid("build folder has no parent".into()))?;
    Ok(Target {
        state_dir: parent.join(format!("{build_id}.mods")),
        dir,
    })
}

fn state_file(t: &Target) -> PathBuf {
    t.state_dir.join("state.json")
}

fn load_state(t: &Target) -> Result<ModsState> {
    let f = state_file(t);
    match std::fs::read_to_string(&f) {
        Ok(text) => {
            let s: ModsState = serde_json::from_str(&text).map_err(|e| Error::json(&f, e))?;
            if s.schema > STATE_SCHEMA {
                return Err(Error::Invalid(format!(
                    "mod deployment state schema {} is newer than supported {STATE_SCHEMA}",
                    s.schema
                )));
            }
            Ok(s)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ModsState::empty()),
        Err(e) => Err(io(&f)(e)),
    }
}

fn save_state(ctx: &Ctx, t: &Target, s: &ModsState) -> Result<()> {
    let body = serde_json::to_string_pretty(s)?;
    fsops::atomic_write(ctx.protected, &state_file(t), body.as_bytes())
}

/// What is deployed in a build now (or was started and not finished), if anything. Read only.
pub fn deployment_of(reg: &BuildRegistry, ctx: &Ctx, build_id: &str) -> Result<Option<Deployment>> {
    Ok(load_state(&target(reg, ctx, build_id)?)?.deployment)
}

/// The latest purge of a build that can still be undone, if there is one. Read only. It is the record that
/// [`undo_purge`] accepts: a later deploy, purge or undo makes the earlier one disappear from here.
pub fn last_purge_of(
    reg: &BuildRegistry,
    ctx: &Ctx,
    build_id: &str,
) -> Result<Option<PurgeRecord>> {
    Ok(load_state(&target(reg, ctx, build_id)?)?.last_purge)
}

/// The deploys and purges of a build, oldest first. Read only.
pub fn history_of(reg: &BuildRegistry, ctx: &Ctx, build_id: &str) -> Result<Vec<HistoryItem>> {
    Ok(load_state(&target(reg, ctx, build_id)?)?.history)
}

fn base_dir(t: &Target, deployment_id: &str) -> PathBuf {
    t.state_dir
        .join("recovery")
        .join(deployment_id)
        .join("base")
}

fn removed_dir(t: &Target, purge_id: &str) -> PathBuf {
    t.state_dir.join("recovery").join(purge_id).join("removed")
}

/// The folder rules applied to where a write would really land, not only to how its path is spelled.
///
/// `refusal_for` looks at the text of a path. A path can spell a protected folder in another way (`./`, a
/// doubled slash, or on Windows a short name such as `ANTICH~1`), and the write goes to the place the path
/// resolves to. So the resolved place is checked as well, relative to the build folder. Anything that does
/// not resolve to somewhere inside the build folder is refused.
fn landing_refusal(layout: &ModLayout, build_dir: &Path, target_path: &Path) -> Option<String> {
    let outside = || {
        Some(format!(
            "{} lands outside the build folder",
            target_path.display()
        ))
    };
    let Ok(root) = build_dir.canonicalize() else {
        return outside();
    };
    let landed = safepath::resolve_lenient(target_path);
    let Ok(rel) = landed.strip_prefix(&root) else {
        return outside();
    };
    let rel: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    layout.refusal_for(&rel.join("/"))
}

fn key(layout: &ModLayout, path: &str) -> String {
    if layout.case_insensitive {
        path.to_ascii_lowercase()
    } else {
        path.to_owned()
    }
}

// ---- the preview and the plan ----

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReplacedFile {
    pub sha256: String,
    pub size: u64,
    /// False when the file is in the build but the build's manifest does not list it.
    pub in_build_manifest: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlannedFile {
    pub path: String,
    pub mod_id: String,
    pub mod_version: String,
    pub action: EntryAction,
    pub sha256: String,
    pub size: u64,
    pub replaces: Option<ReplacedFile>,
    /// Other enabled mods that provide the same file and lose to this one (earlier in the load order).
    pub overrides: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeployPlan {
    pub profile_id: String,
    pub build_id: String,
    pub files: Vec<PlannedFile>,
    pub problems: Vec<Problem>,
    pub total_bytes: u64,
}

impl DeployPlan {
    /// True when something stops the deploy. A plan with only warnings is not blocked.
    pub fn blocked(&self) -> bool {
        self.problems.iter().any(|p| p.severity == Severity::Error)
    }
}

struct Prepared {
    plan: DeployPlan,
    sources: Vec<PathBuf>,
    manifest_before: Vec<Option<FileEntry>>,
}

fn blocked_plan(profile: &Profile, problems: Vec<Problem>) -> Prepared {
    Prepared {
        plan: DeployPlan {
            profile_id: profile.id.clone(),
            build_id: profile.build_id.clone(),
            files: vec![],
            problems,
            total_bytes: 0,
        },
        sources: vec![],
        manifest_before: vec![],
    }
}

fn prepare(
    reg: &BuildRegistry,
    ctx: &Ctx,
    store: &ModStore,
    profile: &Profile,
    layout: &ModLayout,
) -> Result<Prepared> {
    let Some(rec) = reg.get(&profile.build_id) else {
        return Ok(blocked_plan(
            profile,
            vec![Problem::error(
                ProblemKind::BuildNotFound,
                None,
                format!("there is no build {:?}", profile.build_id),
            )],
        ));
    };
    if rec.game_id != profile.game_id {
        return Ok(blocked_plan(
            profile,
            vec![Problem::error(
                ProblemKind::WrongGame,
                None,
                format!(
                    "build {:?} belongs to another game than profile {:?}",
                    rec.id, profile.id
                ),
            )],
        ));
    }
    if !matches!(rec.location, Location::Managed { .. }) {
        return Ok(blocked_plan(
            profile,
            vec![Problem::error(
                ProblemKind::BuildNotManaged,
                None,
                "this build is an install that stays where it is; SUL never writes to it".into(),
            )],
        ));
    }
    if rec.variant == Variant::Original {
        return Ok(blocked_plan(
            profile,
            vec![Problem::error(
                ProblemKind::BuildIsOriginal,
                None,
                "the Original build stays as it was; import a copy as an optimized, legacy, experimental or named build and deploy there".into(),
            )],
        ));
    }
    let t = target(reg, ctx, &profile.build_id)?;
    if let Some(d) = load_state(&t)?.deployment {
        return Ok(blocked_plan(
            profile,
            vec![Problem::error(
                ProblemKind::AlreadyDeployed,
                None,
                format!(
                    "profile {:?} is {} in this build; purge it first",
                    d.profile_name,
                    if d.status == DeployStatus::InProgress {
                        "only partly deployed"
                    } else {
                        "deployed"
                    }
                ),
            )],
        ));
    }
    let manifest = reg.manifest(ctx, &profile.build_id)?;
    let mut problems = Vec::new();
    let report = manifest.verify(&t.dir)?;
    if !report.missing.is_empty() || !report.modified.is_empty() {
        problems.push(Problem::error(
            ProblemKind::BuildNotSound,
            None,
            format!(
                "the build no longer matches its manifest ({} missing, {} changed); verify or repair it first",
                report.missing.len(),
                report.modified.len()
            ),
        ));
    }
    problems.extend(check_profile(profile, store, &manifest.base_version));

    // Which enabled mod provides each file: the last one in load order wins.
    struct Provider<'a> {
        mod_id: &'a str,
        version: &'a str,
        path: &'a str,
        sha256: &'a str,
        size: u64,
        source: PathBuf,
    }
    let mut by_key: BTreeMap<String, Vec<Provider>> = BTreeMap::new();
    for entry in profile.mods.iter().filter(|m| m.enabled) {
        let Some(rec) = store.get(&profile.game_id, &entry.mod_id, &entry.version) else {
            continue; // already reported by check_profile
        };
        let mod_dir = store.mod_dir(ctx, rec)?;
        for f in &rec.manifest.files {
            let source = safepath::join_within(&mod_dir, &f.path)?;
            by_key
                .entry(key(layout, &f.path))
                .or_default()
                .push(Provider {
                    mod_id: &entry.mod_id,
                    version: &entry.version,
                    path: &f.path,
                    sha256: &f.sha256,
                    size: f.size,
                    source,
                });
        }
    }
    if by_key.is_empty() && !problems.iter().any(|p| p.severity == Severity::Error) {
        problems.push(Problem::error(
            ProblemKind::NothingToDeploy,
            None,
            "no enabled mod in this profile has any file".into(),
        ));
    }

    let manifest_by_key: HashMap<String, &FileEntry> = manifest
        .files
        .iter()
        .map(|f| (key(layout, &f.path), f))
        .collect();
    let mut files = Vec::new();
    let mut sources = Vec::new();
    let mut before = Vec::new();
    let mut total = 0u64;
    for (k, providers) in &by_key {
        let (winner, losers) = providers.split_last().expect("at least one provider");
        let in_manifest = manifest_by_key.get(k).copied();
        let path = in_manifest
            .map_or(winner.path, |m| m.path.as_str())
            .to_owned();
        let mut refused = false;
        for p in [winner.path, path.as_str()] {
            if let Some(why) = layout.refusal_for(p) {
                let mut pr =
                    Problem::error(ProblemKind::OutsideAllowedFolders, Some(winner.mod_id), why);
                pr.path = Some(p.to_owned());
                if !problems.contains(&pr) {
                    problems.push(pr);
                }
                refused = true;
            }
        }
        if refused {
            continue;
        }
        let target_path = safepath::join_within(&t.dir, &path)?;
        if let Some(why) = landing_refusal(layout, &t.dir, &target_path) {
            let mut pr =
                Problem::error(ProblemKind::OutsideAllowedFolders, Some(winner.mod_id), why);
            pr.path = Some(path.clone());
            if !problems.contains(&pr) {
                problems.push(pr);
            }
            continue;
        }
        let mut store_problem = |msg: String| {
            let mut pr = Problem::error(ProblemKind::StoreFileProblem, Some(winner.mod_id), msg);
            pr.path = Some(path.clone());
            problems.push(pr);
        };
        match std::fs::metadata(&winner.source) {
            Ok(m) if m.is_file() && m.len() == winner.size => {}
            _ => {
                store_problem(format!(
                    "{} is missing or changed in the mod store",
                    winner.path
                ));
                continue;
            }
        }
        // A file where a folder is needed, or a folder where the file goes, cannot be deployed over.
        let mut ancestor = target_path.parent();
        let mut blocked_by: Option<String> = None;
        while let Some(a) = ancestor {
            if a == t.dir {
                break;
            }
            if let Ok(m) = std::fs::symlink_metadata(a) {
                if !m.is_dir() {
                    blocked_by = Some(format!(
                        "{} is a file in the build, not a folder",
                        a.display()
                    ));
                }
                break;
            }
            ancestor = a.parent();
        }
        let existing = std::fs::symlink_metadata(&target_path).ok();
        if let Some(m) = &existing {
            if !m.is_file() {
                blocked_by = Some(format!(
                    "{path} is a folder or a link in the build, not a file"
                ));
            }
        }
        if let Some(msg) = blocked_by {
            store_problem(msg);
            continue;
        }
        let (action, replaces) = if existing.is_some() {
            let (sha256, size) = hash_file(&target_path)?;
            (
                EntryAction::Replace,
                Some(ReplacedFile {
                    sha256,
                    size,
                    in_build_manifest: in_manifest.is_some(),
                }),
            )
        } else {
            (EntryAction::Add, None)
        };
        total = total.saturating_add(winner.size);
        files.push(PlannedFile {
            path,
            mod_id: winner.mod_id.to_owned(),
            mod_version: winner.version.to_owned(),
            action,
            sha256: winner.sha256.to_owned(),
            size: winner.size,
            replaces,
            overrides: losers.iter().map(|l| l.mod_id.to_owned()).collect(),
        });
        sources.push(winner.source.clone());
        before.push(in_manifest.cloned());
    }
    if let Location::Managed { root_id, .. } = &rec.location {
        if let Err(Error::InsufficientSpace { needed, free, .. }) =
            ctx.roots
                .ensure_space(ctx.disk, root_id, total, COPY_MARGIN_BYTES)
        {
            problems.push(Problem::error(
                ProblemKind::NotEnoughSpace,
                None,
                format!("not enough free space: {needed} bytes needed including a safety margin, {free} free"),
            ));
        }
    }
    // Sort by path so the preview and the ledger are in a fixed order.
    let mut order: Vec<usize> = (0..files.len()).collect();
    order.sort_by(|&a, &b| files[a].path.cmp(&files[b].path));
    let files_sorted = order.iter().map(|&i| files[i].clone()).collect();
    let sources_sorted = order.iter().map(|&i| sources[i].clone()).collect();
    let before_sorted = order.iter().map(|&i| before[i].clone()).collect();
    Ok(Prepared {
        plan: DeployPlan {
            profile_id: profile.id.clone(),
            build_id: profile.build_id.clone(),
            files: files_sorted,
            problems,
            total_bytes: total,
        },
        sources: sources_sorted,
        manifest_before: before_sorted,
    })
}

/// The dry run of a deploy: exactly which paths would be added or replaced, which mod wins each overlap, and
/// what would stop it. Reads files (it hashes the ones it would replace) and changes nothing.
pub fn plan_deploy(
    reg: &BuildRegistry,
    ctx: &Ctx,
    store: &ModStore,
    profile: &Profile,
    layout: &ModLayout,
) -> Result<DeployPlan> {
    Ok(prepare(reg, ctx, store, profile, layout)?.plan)
}

// ---- manifest bookkeeping ----

fn patch_id(deployment_id: &str) -> String {
    format!("deploy:{deployment_id}")
}

fn apply_to_manifest(m: &mut BuildManifest, entries: &[LedgerEntry], header: &DeploymentHeader) {
    for e in entries {
        let fe = FileEntry {
            path: e.path.clone(),
            sha256: e.deployed_sha256.clone(),
            size: e.deployed_size,
        };
        // The ledger path is spelled as the manifest spells the file when it lists it, so an exact match is the
        // right one; matching ignoring case would merge two files that differ only in case on a layout that
        // keeps them apart.
        match m.files.iter_mut().find(|f| f.path == e.path) {
            Some(slot) => *slot = fe,
            None => m.files.push(fe),
        }
    }
    m.files.sort_by(|a, b| a.path.cmp(&b.path));
    let id = patch_id(&header.id);
    if !m.patches.iter().any(|p| p.id == id) {
        m.patches.push(PatchRef {
            id,
            description: format!("mod profile {}", header.profile_name),
            applied_unix: now_unix(),
        });
    }
    if !m.profiles.contains(&header.profile_id) {
        m.profiles.push(header.profile_id.clone());
    }
}

fn revert_in_manifest(
    m: &mut BuildManifest,
    entries: &[&LedgerEntry],
    header: &DeploymentHeader,
    nothing_left: bool,
) {
    for e in entries {
        let at = m.files.iter().position(|f| f.path == e.path);
        match (e.manifest_before.as_ref(), at) {
            (Some(before), Some(i)) => m.files[i] = before.clone(),
            (Some(before), None) => m.files.push(before.clone()),
            (None, Some(i)) => {
                m.files.remove(i);
            }
            (None, None) => {}
        }
    }
    m.files.sort_by(|a, b| a.path.cmp(&b.path));
    if nothing_left {
        let id = patch_id(&header.id);
        m.patches.retain(|p| p.id != id);
        m.profiles.retain(|p| p != &header.profile_id);
    }
}

fn header_of(d: &Deployment) -> DeploymentHeader {
    DeploymentHeader {
        id: d.id.clone(),
        profile_id: d.profile_id.clone(),
        profile_name: d.profile_name.clone(),
        started_unix: d.started_unix,
    }
}

// ---- deploy ----

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeployReport {
    pub deployment_id: String,
    pub added: Vec<String>,
    pub replaced: Vec<String>,
    /// Replaced files that were kept: (path in the build, where the kept copy is).
    pub kept: Vec<(String, PathBuf)>,
    /// Warnings from the plan the owner was shown or could have been shown.
    pub warnings: Vec<Problem>,
}

/// Put the enabled mods of `profile` into its build.
///
/// The plan is made again here, so a plan shown earlier is never trusted; a blocked plan changes nothing. The
/// ledger is written before the first file moves. A failure part way leaves the deployment recorded as
/// unfinished, and a purge undoes whatever was done.
pub fn deploy(
    reg: &mut BuildRegistry,
    ctx: &Ctx,
    store: &ModStore,
    profile: &Profile,
    layout: &ModLayout,
) -> Result<DeployReport> {
    let prepared = prepare(reg, ctx, store, profile, layout)?;
    if prepared.plan.blocked() {
        let why: Vec<String> = prepared
            .plan
            .problems
            .iter()
            .filter(|p| p.severity == Severity::Error)
            .map(|p| p.message.clone())
            .collect();
        return Err(Error::Invalid(format!(
            "the profile cannot be deployed: {}",
            why.join("; ")
        )));
    }
    let t = target(reg, ctx, &profile.build_id)?;
    let op = fresh_id("deploy");
    let entries: Vec<LedgerEntry> = prepared
        .plan
        .files
        .iter()
        .zip(&prepared.manifest_before)
        .map(|(f, before)| LedgerEntry {
            path: f.path.clone(),
            mod_id: f.mod_id.clone(),
            mod_version: f.mod_version.clone(),
            action: f.action,
            deployed_sha256: f.sha256.clone(),
            deployed_size: f.size,
            base_sha256: f.replaces.as_ref().map(|r| r.sha256.clone()),
            base_size: f.replaces.as_ref().map(|r| r.size),
            manifest_before: before.clone(),
        })
        .collect();

    let mut state = load_state(&t)?;
    let started = now_unix();
    state.deployment = Some(Deployment {
        id: op.clone(),
        profile_id: profile.id.clone(),
        profile_name: profile.name.clone(),
        status: DeployStatus::InProgress,
        started_unix: started,
        finished_unix: None,
        entries: entries.clone(),
    });
    save_state(ctx, &t, &state)?;

    let base = base_dir(&t, &op);
    let kept = std::cell::RefCell::new(Vec::new());
    let run = ctx.journal.run(
        &op,
        "deploy_profile",
        &format!("{} into {}", profile.id, profile.build_id),
        || {
            for (e, src) in entries.iter().zip(&prepared.sources) {
                let to = safepath::join_within(&t.dir, &e.path)?;
                let (h, size) = hash_file(src)?;
                if h != e.deployed_sha256 || size != e.deployed_size {
                    return Err(Error::Invalid(format!(
                        "{} in the mod store changed after it was imported; nothing further was written",
                        e.path
                    )));
                }
                if e.action == EntryAction::Replace {
                    let rel = safepath::validate_relative(&e.path)?;
                    let at = fsops::move_aside(ctx.protected, &to, &base, &rel)?;
                    kept.borrow_mut().push((e.path.clone(), at));
                }
                fsops::copy_file_atomic(ctx.protected, src, &to, false)?;
                let (after, _) = hash_file(&to)?;
                if after != e.deployed_sha256 {
                    return Err(Error::Invalid(format!(
                        "{} was written but does not match what was deployed",
                        e.path
                    )));
                }
            }
            Ok(())
        },
    );
    if let Err(e) = run {
        return Err(Error::Invalid(format!(
            "the deploy stopped part way ({e}); it is recorded as unfinished, and a purge will undo what was done"
        )));
    }

    let header = header_of(state.deployment.as_ref().expect("just written"));
    reg.update_manifest(ctx, &profile.build_id, |m| {
        apply_to_manifest(m, &entries, &header);
        Ok(())
    })?;
    if let Some(d) = state.deployment.as_mut() {
        d.status = DeployStatus::Deployed;
        d.finished_unix = Some(now_unix());
    }
    state.last_purge = None;
    state.push_history(HistoryItem {
        id: op.clone(),
        kind: HistoryKind::Deploy,
        profile_id: profile.id.clone(),
        unix: now_unix(),
        files: entries.len(),
        note: format!(
            "{} added, {} replaced",
            entries
                .iter()
                .filter(|e| e.action == EntryAction::Add)
                .count(),
            entries
                .iter()
                .filter(|e| e.action == EntryAction::Replace)
                .count()
        ),
    });
    save_state(ctx, &t, &state)?;

    Ok(DeployReport {
        deployment_id: op,
        added: entries
            .iter()
            .filter(|e| e.action == EntryAction::Add)
            .map(|e| e.path.clone())
            .collect(),
        replaced: entries
            .iter()
            .filter(|e| e.action == EntryAction::Replace)
            .map(|e| e.path.clone())
            .collect(),
        kept: kept.into_inner(),
        warnings: prepared.plan.problems,
    })
}

// ---- purge ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PurgeAction {
    /// Move the deployed file to the recovery folder.
    RemoveModFile,
    /// Move the deployed file to the recovery folder and put the kept base file back.
    RemoveModFileAndRestoreBase,
    /// The deployed file is already gone; put the kept base file back.
    RestoreBaseOnly,
    /// Nothing to do; drop the entry from the ledger.
    Forget,
    /// The file is no longer what was deployed. Left alone.
    LeaveChangedSinceDeploy,
    /// Restoring would need the kept base file and it is missing or different. Left alone.
    LeaveBaseCopyProblem,
}

impl PurgeAction {
    fn changes_files(self) -> bool {
        matches!(
            self,
            Self::RemoveModFile | Self::RemoveModFileAndRestoreBase | Self::RestoreBaseOnly
        )
    }

    fn is_left(self) -> bool {
        matches!(
            self,
            Self::LeaveChangedSinceDeploy | Self::LeaveBaseCopyProblem
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PurgeItem {
    pub path: String,
    pub mod_id: String,
    pub action: PurgeAction,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PurgePlan {
    pub build_id: String,
    pub deployment: Option<DeploymentHeader>,
    pub items: Vec<PurgeItem>,
}

enum Now {
    Missing,
    File(String),
    Other,
}

fn now_of(path: &Path) -> Result<Now> {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.is_file() => Ok(Now::File(hash_file(path)?.0)),
        Ok(_) => Ok(Now::Other),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Now::Missing),
        Err(e) => Err(io(path)(e)),
    }
}

fn classify(e: &LedgerEntry, cur: &Now, backup: &Now) -> (PurgeAction, String) {
    let is_deployed = matches!(cur, Now::File(h) if *h == e.deployed_sha256);
    match e.action {
        EntryAction::Add => match cur {
            Now::Missing => (PurgeAction::Forget, "already gone".into()),
            _ if is_deployed => (PurgeAction::RemoveModFile, String::new()),
            Now::File(_) => (
                PurgeAction::LeaveChangedSinceDeploy,
                "the file changed after it was deployed".into(),
            ),
            Now::Other => (
                PurgeAction::LeaveChangedSinceDeploy,
                "this is no longer a plain file".into(),
            ),
        },
        EntryAction::Replace => {
            let base = e.base_sha256.as_deref();
            let backup_ok = matches!(backup, Now::File(h) if Some(h.as_str()) == base);
            match cur {
                _ if is_deployed && backup_ok => {
                    (PurgeAction::RemoveModFileAndRestoreBase, String::new())
                }
                Now::Missing if backup_ok => (PurgeAction::RestoreBaseOnly, String::new()),
                Now::File(h) if Some(h.as_str()) == base => (
                    PurgeAction::Forget,
                    "already the original file again".into(),
                ),
                _ if is_deployed || matches!(cur, Now::Missing) => (
                    PurgeAction::LeaveBaseCopyProblem,
                    "the kept copy of the original file is missing or has changed".into(),
                ),
                Now::File(_) => (
                    PurgeAction::LeaveChangedSinceDeploy,
                    "the file changed after it was deployed".into(),
                ),
                _ => (
                    PurgeAction::LeaveChangedSinceDeploy,
                    "this is no longer a plain file".into(),
                ),
            }
        }
    }
}

fn plan_for(t: &Target, d: &Deployment) -> Result<Vec<PurgeItem>> {
    let base = base_dir(t, &d.id);
    let mut items = Vec::new();
    for e in &d.entries {
        let to = safepath::join_within(&t.dir, &e.path)?;
        let rel = safepath::validate_relative(&e.path)?;
        let cur = now_of(&to)?;
        let backup = if e.action == EntryAction::Replace {
            now_of(&base.join(&rel))?
        } else {
            Now::Missing
        };
        let (action, detail) = classify(e, &cur, &backup);
        items.push(PurgeItem {
            path: e.path.clone(),
            mod_id: e.mod_id.clone(),
            action,
            detail,
        });
    }
    Ok(items)
}

/// The dry run of a purge: for every path in the ledger, what would be done and why. Changes nothing.
pub fn plan_purge(reg: &BuildRegistry, ctx: &Ctx, build_id: &str) -> Result<PurgePlan> {
    let t = target(reg, ctx, build_id)?;
    let state = load_state(&t)?;
    match &state.deployment {
        None => Ok(PurgePlan {
            build_id: build_id.into(),
            deployment: None,
            items: vec![],
        }),
        Some(d) => Ok(PurgePlan {
            build_id: build_id.into(),
            deployment: Some(header_of(d)),
            items: plan_for(&t, d)?,
        }),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PurgeReport {
    pub purge_id: Option<String>,
    /// Deployed files moved to the recovery folder.
    pub removed: Vec<String>,
    /// Original files put back.
    pub restored: Vec<String>,
    /// Entries dropped because there was nothing left to do.
    pub forgotten: Vec<String>,
    /// Files left in place, with the reason. Nonempty means the build is still partly modded.
    pub left: Vec<PurgeItem>,
    /// Where the removed files went.
    pub recovery_dir: Option<PathBuf>,
}

/// Take a deployed profile out of its build, touching only what the ledger proves is SUL's.
pub fn purge(reg: &mut BuildRegistry, ctx: &Ctx, build_id: &str) -> Result<PurgeReport> {
    purge_inner(reg, ctx, build_id, &mut |_| {})
}

/// [`purge`] with a hook that runs just before a deployed file is moved out (tests use it to change the file at
/// that moment).
fn purge_inner(
    reg: &mut BuildRegistry,
    ctx: &Ctx,
    build_id: &str,
    before_removal: &mut dyn FnMut(&Path),
) -> Result<PurgeReport> {
    let t = target(reg, ctx, build_id)?;
    let mut state = load_state(&t)?;
    let Some(dep) = state.deployment.clone() else {
        return Ok(PurgeReport {
            purge_id: None,
            removed: vec![],
            restored: vec![],
            forgotten: vec![],
            left: vec![],
            recovery_dir: None,
        });
    };
    let items = plan_for(&t, &dep)?;
    let op = fresh_id("purge");
    let base = base_dir(&t, &dep.id);
    let removed_to = removed_dir(&t, &op);

    ctx.journal.run(
        &op,
        "purge_profile",
        &format!("{} from {build_id}", dep.profile_id),
        || {
            for (e, item) in dep
                .entries
                .iter()
                .zip(&items)
                .filter(|(_, i)| i.action.changes_files())
            {
                let to = safepath::join_within(&t.dir, &item.path)?;
                let rel = safepath::validate_relative(&item.path)?;
                let removes = matches!(
                    item.action,
                    PurgeAction::RemoveModFile | PurgeAction::RemoveModFileAndRestoreBase
                );
                if removes {
                    before_removal(&to);
                    // The plan hashed this file earlier. It is hashed again right before it moves, so a change
                    // made in between (by the user or the game) is not carried off as if it were SUL's file.
                    // Stopping here is safe: the ledger is unchanged, and running the purge again finishes the job.
                    if !matches!(now_of(&to)?, Now::File(h) if h == e.deployed_sha256) {
                        return Err(Error::Invalid(format!(
                            "{} changed while the purge was running; it was left where it is. Run the purge again to finish",
                            e.path
                        )));
                    }
                    fsops::move_aside(ctx.protected, &to, &removed_to, &rel)?;
                }
                if matches!(
                    item.action,
                    PurgeAction::RemoveModFileAndRestoreBase | PurgeAction::RestoreBaseOnly
                ) {
                    fsops::rename_new(ctx.protected, &base.join(&rel), &to)?;
                }
            }
            Ok(())
        },
    )?;

    let header = header_of(&dep);
    let mut reverted = Vec::new();
    let mut done: Vec<&LedgerEntry> = Vec::new();
    let mut left_paths = Vec::new();
    let mut report = PurgeReport {
        purge_id: Some(op.clone()),
        removed: vec![],
        restored: vec![],
        forgotten: vec![],
        left: vec![],
        recovery_dir: None,
    };
    for (e, item) in dep.entries.iter().zip(&items) {
        match item.action {
            PurgeAction::RemoveModFile | PurgeAction::RemoveModFileAndRestoreBase => {
                report.removed.push(e.path.clone());
                let restored = item.action == PurgeAction::RemoveModFileAndRestoreBase;
                if restored {
                    report.restored.push(e.path.clone());
                }
                reverted.push(RevertedEntry {
                    entry: e.clone(),
                    removed_mod_file: true,
                    restored_base: restored,
                });
                done.push(e);
            }
            PurgeAction::RestoreBaseOnly => {
                report.restored.push(e.path.clone());
                reverted.push(RevertedEntry {
                    entry: e.clone(),
                    removed_mod_file: false,
                    restored_base: true,
                });
                done.push(e);
            }
            PurgeAction::Forget => {
                report.forgotten.push(e.path.clone());
                done.push(e);
            }
            _ => {
                report.left.push(item.clone());
                left_paths.push(e.path.clone());
            }
        }
    }
    debug_assert!(items.iter().all(|i| i.action.changes_files()
        || i.action == PurgeAction::Forget
        || i.action.is_left()));
    if !report.removed.is_empty() {
        report.recovery_dir = Some(removed_to);
    }

    let nothing_left = left_paths.is_empty();
    // A purge that put nothing back and forgot nothing changes nothing, and so does not rewrite the manifest
    // (that would only move its "updated" time).
    if !done.is_empty() || nothing_left {
        reg.update_manifest(ctx, build_id, |m| {
            revert_in_manifest(m, &done, &header, nothing_left);
            Ok(())
        })?;
    }
    if nothing_left {
        state.deployment = None;
    } else if let Some(d) = state.deployment.as_mut() {
        d.entries.retain(|e| left_paths.contains(&e.path));
    }
    state.last_purge = if reverted.is_empty() {
        None
    } else {
        Some(PurgeRecord {
            id: op.clone(),
            deployment: header.clone(),
            reverted,
            unix: now_unix(),
        })
    };
    state.push_history(HistoryItem {
        id: op,
        kind: HistoryKind::Purge,
        profile_id: dep.profile_id.clone(),
        unix: now_unix(),
        files: report.removed.len() + report.restored.len(),
        note: format!(
            "{} removed, {} originals put back, {} left in place",
            report.removed.len(),
            report.restored.len(),
            report.left.len()
        ),
    });
    save_state(ctx, &t, &state)?;
    Ok(report)
}

// ---- undo the latest purge ----

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UndoReport {
    pub put_back: Vec<String>,
    pub originals_moved_aside_again: Vec<String>,
}

/// Put back what the latest purge took out. Only possible right after that purge, and only when every file is
/// where the purge left it: nothing is partly undone, and nothing is overwritten.
pub fn undo_purge(
    reg: &mut BuildRegistry,
    ctx: &Ctx,
    build_id: &str,
    purge_id: &str,
) -> Result<UndoReport> {
    let t = target(reg, ctx, build_id)?;
    let mut state = load_state(&t)?;
    let Some(rec) = state.last_purge.clone().filter(|p| p.id == purge_id) else {
        return Err(Error::Invalid(format!(
            "{purge_id:?} is not the latest purge of this build"
        )));
    };
    let latest = state.history.last();
    if !matches!(latest, Some(h) if h.id == purge_id && h.kind == HistoryKind::Purge) {
        return Err(Error::Invalid(
            "something changed in this build after that purge; it can no longer be undone".into(),
        ));
    }
    if let Some(d) = &state.deployment {
        if d.id != rec.deployment.id {
            return Err(Error::Invalid(
                "another profile is deployed now; purge it before undoing an older purge".into(),
            ));
        }
    }
    let base = base_dir(&t, &rec.deployment.id);
    let removed_from = removed_dir(&t, &rec.id);

    // Check everything first, so an undo is all or nothing.
    for r in &rec.reverted {
        let e = &r.entry;
        let to = safepath::join_within(&t.dir, &e.path)?;
        let rel = safepath::validate_relative(&e.path)?;
        let kept_spot = base.join(&rel);
        let removed_file = removed_from.join(&rel);
        let target_now = now_of(&to)?;
        if r.restored_base {
            let is_base =
                matches!((&target_now, &e.base_sha256), (Now::File(h), Some(b)) if h == b);
            if !is_base || !matches!(now_of(&kept_spot)?, Now::Missing) {
                return Err(Error::Invalid(format!(
                    "{} is not where the purge left it; nothing was undone",
                    e.path
                )));
            }
        } else if !matches!(target_now, Now::Missing) {
            return Err(Error::Invalid(format!(
                "{} exists again; nothing was undone",
                e.path
            )));
        }
        if r.removed_mod_file {
            let ok = matches!(now_of(&removed_file)?, Now::File(h) if h == e.deployed_sha256);
            if !ok {
                return Err(Error::Invalid(format!(
                    "the removed copy of {} is missing or changed; nothing was undone",
                    e.path
                )));
            }
        }
    }

    // The ledger is written first, as a deploy does it. If the moves below stop part way, the files that went
    // back are already listed as SUL's, so a purge finishes the job. The purge record is dropped at the same
    // time: an undo that stopped cannot be repeated, and a purge is the way forward.
    let entries: Vec<LedgerEntry> = rec.reverted.iter().map(|r| r.entry.clone()).collect();
    match state.deployment.as_mut() {
        Some(d) => {
            d.entries.extend(entries.clone());
            d.entries.sort_by(|a, b| a.path.cmp(&b.path));
        }
        None => {
            state.deployment = Some(Deployment {
                id: rec.deployment.id.clone(),
                profile_id: rec.deployment.profile_id.clone(),
                profile_name: rec.deployment.profile_name.clone(),
                status: DeployStatus::InProgress,
                started_unix: rec.deployment.started_unix,
                finished_unix: None,
                entries: entries.clone(),
            });
        }
    }
    state.last_purge = None;
    save_state(ctx, &t, &state)?;

    let op = fresh_id("undo-purge");
    ctx.journal.run(
        &op,
        "undo_purge",
        &format!("{} in {build_id}", rec.id),
        || {
            for r in &rec.reverted {
                let to = safepath::join_within(&t.dir, &r.entry.path)?;
                let rel = safepath::validate_relative(&r.entry.path)?;
                if r.restored_base {
                    fsops::rename_new(ctx.protected, &to, &base.join(&rel))?;
                }
                if r.removed_mod_file {
                    fsops::rename_new(ctx.protected, &removed_from.join(&rel), &to)?;
                }
            }
            Ok(())
        },
    )?;

    reg.update_manifest(ctx, build_id, |m| {
        apply_to_manifest(m, &entries, &rec.deployment);
        Ok(())
    })?;
    if let Some(d) = state.deployment.as_mut() {
        d.status = DeployStatus::Deployed;
        d.finished_unix.get_or_insert_with(now_unix);
    }
    state.push_history(HistoryItem {
        id: op,
        kind: HistoryKind::UndoPurge,
        profile_id: rec.deployment.profile_id.clone(),
        unix: now_unix(),
        files: entries.len(),
        note: format!("undid purge {}", rec.id),
    });
    save_state(ctx, &t, &state)?;
    Ok(UndoReport {
        put_back: rec
            .reverted
            .iter()
            .filter(|r| r.removed_mod_file)
            .map(|r| r.entry.path.clone())
            .collect(),
        originals_moved_aside_again: rec
            .reverted
            .iter()
            .filter(|r| r.restored_base)
            .map(|r| r.entry.path.clone())
            .collect(),
    })
}

// ---- Purge & Deploy ----

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PurgeAndDeployReport {
    pub purge: PurgeReport,
    /// `None` when the purge left files in place and the deploy was not attempted.
    pub deploy: Option<DeployReport>,
}

/// Purge what is deployed in the profile's build, then deploy the profile.
///
/// Checks that do not depend on the disk run first, so a profile with a missing dependency does not purge
/// anything. If the purge leaves any file in place (a file that changed since it was deployed) the deploy is not
/// attempted. The purge stays undoable until the deploy starts.
pub fn purge_and_deploy(
    reg: &mut BuildRegistry,
    ctx: &Ctx,
    store: &ModStore,
    profile: &Profile,
    layout: &ModLayout,
) -> Result<PurgeAndDeployReport> {
    let t = target(reg, ctx, &profile.build_id)?;
    let manifest = reg.manifest(ctx, &profile.build_id)?;
    let early: Vec<String> = check_profile(profile, store, &manifest.base_version)
        .into_iter()
        .filter(|p| p.severity == Severity::Error)
        .map(|p| p.message)
        .collect();
    if !early.is_empty() {
        return Err(Error::Invalid(format!(
            "the profile cannot be deployed, so nothing was purged: {}",
            early.join("; ")
        )));
    }
    if reg
        .get(&profile.build_id)
        .is_some_and(|r| r.variant == Variant::Original)
    {
        return Err(Error::Invalid(
            "the Original build stays as it was; nothing was purged".into(),
        ));
    }
    let _ = t;
    let purged = purge(reg, ctx, &profile.build_id)?;
    if !purged.left.is_empty() {
        return Ok(PurgeAndDeployReport {
            purge: purged,
            deploy: None,
        });
    }
    let deployed = deploy(reg, ctx, store, profile, layout)?;
    Ok(PurgeAndDeployReport {
        purge: purged,
        deploy: Some(deployed),
    })
}

#[cfg(test)]
mod tests {
    use super::super::profile::Profiles;
    use super::super::store::tests::{mod_world, req};
    use super::super::store::{ModImport, Requirement};
    use super::*;
    use crate::manifest::{State, inventory};
    use crate::testkit::{GAME, World, put};

    struct Fx {
        w: World,
        reg: BuildRegistry,
        store: ModStore,
        profiles: Profiles,
        build: String,
        layout: ModLayout,
    }

    fn layout() -> ModLayout {
        ModLayout {
            allowed_prefixes: vec!["data".into(), "mods".into()],
            forbidden_prefixes: vec!["data/anticheat".into()],
            case_insensitive: true,
        }
    }

    fn fx() -> Fx {
        let w = mod_world();
        let mut reg = w.registry();
        let build = w.import(&mut reg, Variant::Experimental);
        let store = ModStore::open(&w.base.join("state/mods.json")).unwrap();
        let mut profiles = Profiles::open(&w.base.join("state/profiles.json")).unwrap();
        profiles
            .create(&w.protected, "main", "Main", GAME, &build)
            .unwrap();
        Fx {
            w,
            reg,
            store,
            profiles,
            build,
            layout: layout(),
        }
    }

    impl Fx {
        fn add_mod(&mut self, id: &str, version: &str, files: &[(&str, &str)]) {
            let r = req(&self.w, id, version, files);
            self.store.import_folder(&self.w.ctx(), &r).unwrap();
        }
        fn add_mod_with(&mut self, r: ModImport) {
            self.store.import_folder(&self.w.ctx(), &r).unwrap();
        }
        fn use_mod(&mut self, id: &str, version: &str) {
            self.profiles
                .add_mod(&self.w.protected, "main", id, version, None)
                .unwrap();
        }
        fn profile(&self) -> Profile {
            self.profiles.get("main").unwrap().clone()
        }
        fn dir(&self) -> PathBuf {
            self.reg.build_dir(&self.w.ctx(), &self.build).unwrap()
        }
        fn plan(&self) -> DeployPlan {
            plan_deploy(
                &self.reg,
                &self.w.ctx(),
                &self.store,
                &self.profile(),
                &self.layout,
            )
            .unwrap()
        }
        fn deploy(&mut self) -> Result<DeployReport> {
            let p = self.profile();
            deploy(&mut self.reg, &self.w.ctx(), &self.store, &p, &self.layout)
        }
        fn purge(&mut self) -> PurgeReport {
            purge(&mut self.reg, &self.w.ctx(), &self.build).unwrap()
        }
        fn snap(&self) -> BTreeMap<String, String> {
            inventory(&self.dir())
                .unwrap()
                .into_iter()
                .map(|f| (f.path, f.sha256))
                .collect()
        }
        fn manifest(&self) -> BuildManifest {
            self.reg.manifest(&self.w.ctx(), &self.build).unwrap()
        }
        fn read(&self, rel: &str) -> String {
            std::fs::read_to_string(self.dir().join(rel)).unwrap()
        }
        fn sound(&mut self) -> bool {
            let c = self.reg.verify(&self.w.ctx(), &self.build).unwrap();
            c.state == State::Ready
        }
        fn state_json(&self) -> PathBuf {
            self.dir()
                .parent()
                .unwrap()
                .join(format!("{}.mods", self.build))
                .join("state.json")
        }
    }

    fn two_file_mod(f: &mut Fx) {
        f.add_mod(
            "ui",
            "1.0",
            &[("data/a.bin", "NEW-A"), ("mods/ui.txt", "ui")],
        );
        f.use_mod("ui", "1.0");
    }

    #[test]
    fn a_preview_changes_nothing_and_a_deploy_adds_replaces_and_records_every_file() {
        let mut f = fx();
        two_file_mod(&mut f);
        let before = f.snap();
        let install_before = inventory(&f.w.install).unwrap();

        let plan = f.plan();
        assert!(!plan.blocked(), "{:?}", plan.problems);
        let by_path: BTreeMap<_, _> = plan.files.iter().map(|p| (p.path.as_str(), p)).collect();
        assert_eq!(by_path["data/a.bin"].action, EntryAction::Replace);
        assert_eq!(by_path["data/a.bin"].replaces.as_ref().unwrap().size, 4);
        assert!(
            by_path["data/a.bin"]
                .replaces
                .as_ref()
                .unwrap()
                .in_build_manifest
        );
        assert_eq!(by_path["mods/ui.txt"].action, EntryAction::Add);
        assert_eq!(plan.total_bytes, 7);
        assert_eq!(f.snap(), before, "a preview must not change the build");
        assert!(
            deployment_of(&f.reg, &f.w.ctx(), &f.build)
                .unwrap()
                .is_none()
        );

        let report = f.deploy().unwrap();
        assert_eq!(report.added, ["mods/ui.txt"]);
        assert_eq!(report.replaced, ["data/a.bin"]);
        assert_eq!(f.read("data/a.bin"), "NEW-A");
        assert_eq!(f.read("mods/ui.txt"), "ui");
        // The replaced file is kept, not deleted.
        assert_eq!(report.kept.len(), 1);
        assert_eq!(std::fs::read_to_string(&report.kept[0].1).unwrap(), "AAAA");
        // The original install is untouched.
        assert_eq!(inventory(&f.w.install).unwrap(), install_before);

        // The ledger names both files with the hashes that were deployed.
        let d = deployment_of(&f.reg, &f.w.ctx(), &f.build)
            .unwrap()
            .unwrap();
        assert_eq!(d.status, DeployStatus::Deployed);
        assert_eq!(d.entries.len(), 2);
        let after = f.snap();
        for e in &d.entries {
            assert_eq!(after[&e.path], e.deployed_sha256);
        }
        // The build still verifies against its own manifest: that is the point of updating it.
        let m = f.manifest();
        assert_eq!(m.patches.len(), 1);
        assert_eq!(m.profiles, ["main"]);
        assert!(f.sound());
        f.reg.activate(&f.w.ctx(), &f.build.clone()).unwrap();
        assert_eq!(history_of(&f.reg, &f.w.ctx(), &f.build).unwrap().len(), 1);
        assert!(f.w.journal.pending().unwrap().is_empty());
    }

    #[test]
    fn a_purge_removes_only_what_the_ledger_owns_and_puts_the_original_back() {
        let mut f = fx();
        let manifest_before = f.manifest().files;
        let before = f.snap();
        two_file_mod(&mut f);
        f.deploy().unwrap();
        // Files the owner (or the game) added are never looked at, however mod-like they are.
        put(&f.dir().join("mods/mine.txt"), "mine");
        put(&f.dir().join("data/c.bin"), "C");

        let preview = plan_purge(&f.reg, &f.w.ctx(), &f.build).unwrap();
        let acts: BTreeMap<_, _> = preview
            .items
            .iter()
            .map(|i| (i.path.as_str(), i.action))
            .collect();
        assert_eq!(acts["data/a.bin"], PurgeAction::RemoveModFileAndRestoreBase);
        assert_eq!(acts["mods/ui.txt"], PurgeAction::RemoveModFile);
        assert_eq!(preview.items.len(), 2);

        let r = f.purge();
        assert_eq!(r.removed, ["data/a.bin", "mods/ui.txt"]);
        assert_eq!(r.restored, ["data/a.bin"]);
        assert!(r.left.is_empty());
        let mut expected = before.clone();
        expected.insert(
            "mods/mine.txt".into(),
            crate::manifest::hash_file(&f.dir().join("mods/mine.txt"))
                .unwrap()
                .0,
        );
        expected.insert(
            "data/c.bin".into(),
            crate::manifest::hash_file(&f.dir().join("data/c.bin"))
                .unwrap()
                .0,
        );
        assert_eq!(f.snap(), expected);
        assert_eq!(f.read("data/a.bin"), "AAAA");
        assert_eq!(f.manifest().files, manifest_before);
        assert!(f.manifest().patches.is_empty() && f.manifest().profiles.is_empty());
        assert!(f.sound());
        assert!(
            deployment_of(&f.reg, &f.w.ctx(), &f.build)
                .unwrap()
                .is_none()
        );

        // Nothing was deleted: the removed files are in the recovery folder.
        let rec = r.recovery_dir.unwrap();
        assert_eq!(
            std::fs::read_to_string(rec.join("data/a.bin")).unwrap(),
            "NEW-A"
        );
        assert_eq!(
            std::fs::read_to_string(rec.join("mods/ui.txt")).unwrap(),
            "ui"
        );
        // A purge with nothing deployed does nothing.
        let again = f.purge();
        assert!(again.purge_id.is_none() && again.removed.is_empty());
    }

    #[test]
    fn a_file_changed_since_the_deploy_is_left_alone_and_reported() {
        let mut f = fx();
        two_file_mod(&mut f);
        f.deploy().unwrap();
        std::fs::write(f.dir().join("mods/ui.txt"), "the owner edited this").unwrap();
        std::fs::write(f.dir().join("data/a.bin"), "and this").unwrap();
        let before = f.snap();
        let manifest = f.manifest();
        // The manifest records when it was last updated in whole seconds. Wait for the second to change, so a
        // purge that rewrote the manifest for no reason could not hide behind an equal timestamp.
        std::thread::sleep(std::time::Duration::from_millis(1100));

        let plan = plan_purge(&f.reg, &f.w.ctx(), &f.build).unwrap();
        assert!(
            plan.items
                .iter()
                .all(|i| i.action == PurgeAction::LeaveChangedSinceDeploy)
        );
        let r = f.purge();
        assert_eq!(r.left.len(), 2);
        assert!(r.removed.is_empty() && r.restored.is_empty());
        assert_eq!(f.snap(), before, "a changed file is not touched");
        assert_eq!(
            f.manifest(),
            manifest,
            "and the manifest still says what was deployed"
        );
        let d = deployment_of(&f.reg, &f.w.ctx(), &f.build)
            .unwrap()
            .unwrap();
        assert_eq!(
            d.entries.len(),
            2,
            "the entries stay so a later purge can finish"
        );

        // Put one file back exactly as deployed: only that one is purged.
        std::fs::write(f.dir().join("mods/ui.txt"), "ui").unwrap();
        let r = f.purge();
        assert_eq!(r.removed, ["mods/ui.txt"]);
        assert_eq!(r.left.len(), 1);
        assert_eq!(f.read("data/a.bin"), "and this");
        let d = deployment_of(&f.reg, &f.w.ctx(), &f.build)
            .unwrap()
            .unwrap();
        assert_eq!(d.entries.len(), 1);
        assert_eq!(d.entries[0].path, "data/a.bin");
    }

    #[test]
    fn a_missing_kept_copy_stops_a_purge_from_taking_the_deployed_file_out() {
        let mut f = fx();
        two_file_mod(&mut f);
        let report = f.deploy().unwrap();
        let kept = report.kept[0].1.clone();
        std::fs::write(&kept, "someone changed the kept copy").unwrap();
        let before = f.snap();
        let r = f.purge();
        assert_eq!(
            r.removed,
            ["mods/ui.txt"],
            "the file with a good kept copy is not held up"
        );
        assert_eq!(r.left.len(), 1);
        assert_eq!(r.left[0].action, PurgeAction::LeaveBaseCopyProblem);
        assert_eq!(
            f.read("data/a.bin"),
            "NEW-A",
            "the deployed file stays rather than leaving a hole"
        );
        let mut expected = before;
        expected.remove("mods/ui.txt");
        assert_eq!(f.snap(), expected);
    }

    #[test]
    fn the_latest_purge_can_be_undone_and_only_the_latest() {
        let mut f = fx();
        two_file_mod(&mut f);
        f.deploy().unwrap();
        let deployed = f.snap();
        let manifest = f.manifest();
        let r = f.purge();
        let id = r.purge_id.clone().unwrap();

        assert!(undo_purge(&mut f.reg, &f.w.ctx(), &f.build, "purge-nope").is_err());
        let u = undo_purge(&mut f.reg, &f.w.ctx(), &f.build, &id).unwrap();
        assert_eq!(u.put_back, ["data/a.bin", "mods/ui.txt"]);
        assert_eq!(u.originals_moved_aside_again, ["data/a.bin"]);
        assert_eq!(f.snap(), deployed);
        assert_eq!(f.manifest().files, manifest.files);
        assert_eq!(f.manifest().profiles, manifest.profiles);
        assert!(f.sound());
        let d = deployment_of(&f.reg, &f.w.ctx(), &f.build)
            .unwrap()
            .unwrap();
        assert_eq!(d.entries.len(), 2);
        // The undo itself is not undoable twice, and a purge works again afterwards.
        assert!(undo_purge(&mut f.reg, &f.w.ctx(), &f.build, &id).is_err());
        let r2 = f.purge();
        assert_eq!(r2.removed.len(), 2);

        // After the next purge, an older purge id is refused.
        assert!(undo_purge(&mut f.reg, &f.w.ctx(), &f.build, &id).is_err());
        // An undo is refused when a file is not where the purge left it, and then nothing moves.
        let id2 = r2.purge_id.unwrap();
        put(&f.dir().join("mods/ui.txt"), "someone put a file here");
        let snap = f.snap();
        assert!(undo_purge(&mut f.reg, &f.w.ctx(), &f.build, &id2).is_err());
        assert_eq!(f.snap(), snap);
        assert_eq!(f.read("data/a.bin"), "AAAA");
    }

    #[test]
    fn the_latest_purge_is_readable_until_it_is_undone_or_replaced() {
        let mut f = fx();
        two_file_mod(&mut f);
        let latest = |f: &Fx| last_purge_of(&f.reg, &f.w.ctx(), &f.build).unwrap();
        assert!(latest(&f).is_none(), "nothing was purged yet");
        f.deploy().unwrap();
        assert!(latest(&f).is_none(), "a deploy is not a purge");
        let id = f.purge().purge_id.unwrap();
        let rec = latest(&f).expect("the purge can be undone");
        assert_eq!(rec.id, id);
        assert_eq!(rec.reverted.len(), 2);
        assert_eq!(rec.deployment.profile_id, "main");
        undo_purge(&mut f.reg, &f.w.ctx(), &f.build, &id).unwrap();
        assert!(
            latest(&f).is_none(),
            "an undone purge cannot be undone again"
        );
        let second = f.purge().purge_id.unwrap();
        assert_eq!(latest(&f).unwrap().id, second);
        // A purge of an install that stays where it is has nothing to read.
        assert!(last_purge_of(&f.reg, &f.w.ctx(), "no-such-build").is_err());
    }

    #[test]
    fn the_last_enabled_mod_in_the_load_order_wins_an_overlap() {
        let mut f = fx();
        f.add_mod(
            "one",
            "1.0",
            &[("data/a.bin", "FROM-ONE"), ("mods/one.txt", "1")],
        );
        f.add_mod("two", "1.0", &[("data/a.bin", "FROM-TWO")]);
        f.add_mod("three", "1.0", &[("DATA/A.BIN", "FROM-THREE")]);
        f.use_mod("one", "1.0");
        f.use_mod("two", "1.0");
        f.use_mod("three", "1.0");
        f.profiles
            .set_enabled(&f.w.protected, "main", "three", false)
            .unwrap();

        let plan = f.plan();
        let a = plan.files.iter().find(|p| p.path == "data/a.bin").unwrap();
        assert_eq!(
            (a.mod_id.as_str(), a.overrides.clone()),
            ("two", vec!["one".to_string()])
        );
        assert_eq!(plan.files.len(), 2);

        // Enabling the third (its path differs only by case) makes it win and still uses the build's spelling.
        f.profiles
            .set_enabled(&f.w.protected, "main", "three", true)
            .unwrap();
        let plan = f.plan();
        let a = plan.files.iter().find(|p| p.path == "data/a.bin").unwrap();
        assert_eq!(a.mod_id, "three");
        assert_eq!(a.overrides, ["one", "two"]);
        assert!(plan.files.iter().all(|p| p.path != "DATA/A.BIN"));

        // Moving one to the end flips the winner.
        f.profiles
            .move_mod(&f.w.protected, "main", "one", 99)
            .unwrap();
        let plan = f.plan();
        assert_eq!(
            plan.files
                .iter()
                .find(|p| p.path == "data/a.bin")
                .unwrap()
                .mod_id,
            "one"
        );
        f.deploy().unwrap();
        assert_eq!(f.read("data/a.bin"), "FROM-ONE");
        assert!(f.sound());
    }

    #[test]
    fn mods_cannot_reach_forbidden_or_unlisted_folders() {
        let mut f = fx();
        f.add_mod(
            "evil",
            "1.0",
            &[("data/anticheat/guard.dll", "X"), ("mods/ok.txt", "ok")],
        );
        f.use_mod("evil", "1.0");
        let before = f.snap();
        let plan = f.plan();
        assert!(plan.blocked());
        assert!(
            plan.problems
                .iter()
                .any(|p| p.kind == ProblemKind::OutsideAllowedFolders
                    && p.path.as_deref() == Some("data/anticheat/guard.dll"))
        );
        let err = f.deploy().unwrap_err().to_string();
        assert!(
            err.contains("anticheat") || err.contains("never change"),
            "{err}"
        );
        assert_eq!(f.snap(), before);
        assert!(
            deployment_of(&f.reg, &f.w.ctx(), &f.build)
                .unwrap()
                .is_none()
        );

        // Not inside an allowed folder at all, in any letter case.
        let mut g = fx();
        g.add_mod("exe", "1.0", &[("Game.EXE", "X")]);
        g.use_mod("exe", "1.0");
        assert!(g.plan().blocked());
        let mut h = fx();
        h.add_mod("sneaky", "1.0", &[("DATA/AntiCheat/new.dll", "X")]);
        h.use_mod("sneaky", "1.0");
        assert!(h.plan().blocked());
        // A game whose adapter takes no mods takes none.
        let mut k = fx();
        k.layout = ModLayout::none();
        k.add_mod("ui", "1.0", &[("mods/ui.txt", "ui")]);
        k.use_mod("ui", "1.0");
        assert!(k.plan().blocked());
    }

    #[test]
    fn builds_that_must_not_be_written_are_refused() {
        // The Original build.
        let mut f = fx();
        let orig = f.w.import(&mut f.reg, Variant::Original);
        f.profiles.set_build(&f.w.protected, "main", &orig).unwrap();
        f.add_mod("ui", "1.0", &[("mods/ui.txt", "ui")]);
        f.use_mod("ui", "1.0");
        assert!(
            f.plan()
                .problems
                .iter()
                .any(|p| p.kind == ProblemKind::BuildIsOriginal)
        );
        assert!(f.deploy().is_err());
        assert_eq!(f.read_in(&orig, "game.exe"), "exe");

        // An install that stays where it is.
        let mut g = fx();
        let ext = g
            .reg
            .register_in_place(
                &g.w.ctx(),
                &crate::builds::RegisterRequest {
                    game_id: GAME.into(),
                    display_name: "Made-up Game".into(),
                    variant: Variant::Experimental,
                    base_version: "1.0.0".into(),
                    source_description: "mine".into(),
                    path: g.w.install.clone(),
                },
            )
            .unwrap();
        g.profiles.set_build(&g.w.protected, "main", &ext).unwrap();
        g.add_mod("ui", "1.0", &[("mods/ui.txt", "ui")]);
        g.use_mod("ui", "1.0");
        let before = inventory(&g.w.install).unwrap();
        assert!(
            g.plan()
                .problems
                .iter()
                .any(|p| p.kind == ProblemKind::BuildNotManaged)
        );
        assert!(g.deploy().is_err());
        assert!(purge(&mut g.reg, &g.w.ctx(), &ext).is_err());
        assert_eq!(inventory(&g.w.install).unwrap(), before);

        // An unknown build, and a build of another game.
        let mut h = fx();
        h.profiles
            .set_build(&h.w.protected, "main", "ghost-1")
            .unwrap();
        h.add_mod("ui", "1.0", &[("mods/ui.txt", "ui")]);
        h.use_mod("ui", "1.0");
        assert!(
            h.plan()
                .problems
                .iter()
                .any(|p| p.kind == ProblemKind::BuildNotFound)
        );
    }

    impl Fx {
        fn read_in(&self, build: &str, rel: &str) -> String {
            let d = self.reg.build_dir(&self.w.ctx(), build).unwrap();
            std::fs::read_to_string(d.join(rel)).unwrap()
        }
    }

    #[test]
    fn a_build_that_no_longer_matches_its_manifest_is_not_deployed_into() {
        let mut f = fx();
        two_file_mod(&mut f);
        std::fs::write(f.dir().join("data/b.bin"), "damaged").unwrap();
        let plan = f.plan();
        assert!(
            plan.problems
                .iter()
                .any(|p| p.kind == ProblemKind::BuildNotSound)
        );
        assert!(f.deploy().is_err());
        assert_eq!(f.read("data/a.bin"), "AAAA");
    }

    #[test]
    fn a_second_deploy_needs_a_purge_first() {
        let mut f = fx();
        two_file_mod(&mut f);
        f.deploy().unwrap();
        assert!(
            f.plan()
                .problems
                .iter()
                .any(|p| p.kind == ProblemKind::AlreadyDeployed)
        );
        let snap = f.snap();
        assert!(f.deploy().is_err());
        assert_eq!(f.snap(), snap);
    }

    #[test]
    fn a_case_variant_of_a_base_file_replaces_it_and_never_makes_a_second_copy() {
        let mut f = fx();
        f.add_mod("ui", "1.0", &[("Data/A.BIN", "NEW")]);
        f.use_mod("ui", "1.0");
        let plan = f.plan();
        assert_eq!(plan.files.len(), 1);
        assert_eq!(plan.files[0].path, "data/a.bin");
        assert_eq!(plan.files[0].action, EntryAction::Replace);
        f.deploy().unwrap();
        let names: Vec<_> = f
            .snap()
            .into_keys()
            .filter(|k| k.eq_ignore_ascii_case("data/a.bin"))
            .collect();
        assert_eq!(names, ["data/a.bin"]);
        assert!(f.sound());
        f.purge();
        assert_eq!(f.read("data/a.bin"), "AAAA");
    }

    #[test]
    fn a_file_where_a_folder_is_needed_blocks_the_deploy() {
        let mut f = fx();
        put(&f.dir().join("mods/blocker"), "i am a file");
        // Keep the build sound by recording the file in its manifest through the same door a deploy uses.
        let rec = f.reg.manifest(&f.w.ctx(), &f.build).unwrap();
        let (sha, size) = crate::manifest::hash_file(&f.dir().join("mods/blocker")).unwrap();
        let mut files = rec.files.clone();
        files.push(FileEntry {
            path: "mods/blocker".into(),
            sha256: sha,
            size,
        });
        f.reg
            .update_manifest(&f.w.ctx(), &f.build.clone(), |m| {
                m.files = files;
                Ok(())
            })
            .unwrap();
        f.add_mod(
            "ui",
            "1.0",
            &[("mods/blocker/inner.txt", "x"), ("mods/ok.txt", "ok")],
        );
        f.use_mod("ui", "1.0");
        let plan = f.plan();
        assert!(
            plan.problems
                .iter()
                .any(|p| p.kind == ProblemKind::StoreFileProblem),
            "{:?}",
            plan.problems
        );
        assert!(f.deploy().is_err());
        assert!(!f.dir().join("mods/ok.txt").exists());
    }

    #[test]
    fn a_deploy_that_fails_part_way_is_recorded_and_a_purge_undoes_it() {
        let mut f = fx();
        let manifest = f.manifest();
        let before = f.snap();
        f.add_mod(
            "ui",
            "1.0",
            &[("data/a.bin", "NEW-A"), ("data/z.bin", "ZZZZZ")],
        );
        f.use_mod("ui", "1.0");
        // Tamper with the stored copy after the import, keeping its size so only the hash can notice.
        let rec = f.store.get(GAME, "ui", "1.0").unwrap().clone();
        let sdir = f.store.mod_dir(&f.w.ctx(), &rec).unwrap();
        std::fs::write(sdir.join("data/z.bin"), "YYYYY").unwrap();

        let err = f.deploy().unwrap_err().to_string();
        assert!(
            err.contains("stopped part way") && err.contains("changed after it was imported"),
            "{err}"
        );
        let d = deployment_of(&f.reg, &f.w.ctx(), &f.build)
            .unwrap()
            .unwrap();
        assert_eq!(d.status, DeployStatus::InProgress);
        assert_eq!(f.read("data/a.bin"), "NEW-A", "the first file went in");
        assert!(
            !f.dir().join("data/z.bin").exists(),
            "the tampered one did not"
        );
        assert_eq!(
            f.manifest(),
            manifest,
            "the manifest is only updated when the whole deploy finished"
        );
        assert!(
            f.w.journal.pending().unwrap().is_empty(),
            "the failed operation is recorded as aborted"
        );
        assert!(
            f.plan()
                .problems
                .iter()
                .any(|p| p.kind == ProblemKind::AlreadyDeployed)
        );

        let preview = plan_purge(&f.reg, &f.w.ctx(), &f.build).unwrap();
        let acts: BTreeMap<_, _> = preview
            .items
            .iter()
            .map(|i| (i.path.as_str(), i.action))
            .collect();
        assert_eq!(acts["data/a.bin"], PurgeAction::RemoveModFileAndRestoreBase);
        assert_eq!(acts["data/z.bin"], PurgeAction::Forget);
        let r = f.purge();
        assert!(r.left.is_empty());
        assert_eq!(f.snap(), before);
        assert_eq!(f.manifest().files, manifest.files);
        assert!(f.sound());
        assert!(
            deployment_of(&f.reg, &f.w.ctx(), &f.build)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn a_purge_that_was_interrupted_is_finished_by_running_it_again() {
        let mut f = fx();
        two_file_mod(&mut f);
        let report = f.deploy().unwrap();
        // Do by hand the first half of what a purge does, as if the program had stopped there.
        let moved = f.w.base.join("elsewhere.txt");
        std::fs::rename(f.dir().join("mods/ui.txt"), &moved).unwrap();
        std::fs::remove_file(f.dir().join("data/a.bin")).unwrap();
        std::fs::rename(&report.kept[0].1, f.dir().join("data/a.bin")).unwrap();

        let plan = plan_purge(&f.reg, &f.w.ctx(), &f.build).unwrap();
        assert!(
            plan.items.iter().all(|i| i.action == PurgeAction::Forget),
            "{:?}",
            plan.items
        );
        let r = f.purge();
        assert_eq!(r.forgotten.len(), 2);
        assert!(
            deployment_of(&f.reg, &f.w.ctx(), &f.build)
                .unwrap()
                .is_none()
        );
        assert!(f.sound());
        assert_eq!(f.read("data/a.bin"), "AAAA");
        assert!(f.manifest().patches.is_empty());
    }

    #[test]
    fn a_forged_ledger_cannot_make_a_purge_reach_outside_the_build() {
        let mut f = fx();
        two_file_mod(&mut f);
        f.deploy().unwrap();
        put(&f.w.base.join("ssd/builds/precious.txt"), "do not touch");
        let text = std::fs::read_to_string(f.state_json()).unwrap();
        let forged = text.replace(
            "\"path\": \"mods/ui.txt\"",
            "\"path\": \"../../precious.txt\"",
        );
        assert_ne!(forged, text);
        std::fs::write(f.state_json(), forged).unwrap();
        assert!(plan_purge(&f.reg, &f.w.ctx(), &f.build).is_err());
        assert!(purge(&mut f.reg, &f.w.ctx(), &f.build).is_err());
        assert_eq!(
            std::fs::read_to_string(f.w.base.join("ssd/builds/precious.txt")).unwrap(),
            "do not touch"
        );
        assert_eq!(f.read("mods/ui.txt"), "ui");
        // A state file that cannot be read is an error, not an empty ledger.
        std::fs::write(f.state_json(), "{ nope").unwrap();
        assert!(plan_purge(&f.reg, &f.w.ctx(), &f.build).is_err());
        assert!(f.deploy().is_err());
    }

    #[test]
    fn purge_and_deploy_switches_profiles_and_checks_the_new_one_before_purging() {
        let mut f = fx();
        two_file_mod(&mut f);
        f.deploy().unwrap();
        let deployed = f.snap();

        // A profile that needs a mod it does not have is refused before anything is purged.
        let mut bad = req(&f.w, "needy", "1.0", &[("mods/needy.txt", "n")]);
        bad.requires = vec![Requirement {
            mod_id: "ghost".into(),
            min_version: None,
        }];
        f.add_mod_with(bad);
        f.profiles
            .create(&f.w.protected, "other", "Other", GAME, &f.build.clone())
            .unwrap();
        f.profiles
            .add_mod(&f.w.protected, "other", "needy", "1.0", None)
            .unwrap();
        let other = f.profiles.get("other").unwrap().clone();
        let err =
            purge_and_deploy(&mut f.reg, &f.w.ctx(), &f.store, &other, &f.layout).unwrap_err();
        assert!(err.to_string().contains("nothing was purged"), "{err}");
        assert_eq!(f.snap(), deployed);

        // A good one replaces it.
        f.add_mod(
            "sound",
            "1.0",
            &[("mods/sound.txt", "s"), ("data/b.bin", "NEW-B")],
        );
        f.profiles
            .create(&f.w.protected, "third", "Third", GAME, &f.build.clone())
            .unwrap();
        f.profiles
            .add_mod(&f.w.protected, "third", "sound", "1.0", None)
            .unwrap();
        let third = f.profiles.get("third").unwrap().clone();
        let r = purge_and_deploy(&mut f.reg, &f.w.ctx(), &f.store, &third, &f.layout).unwrap();
        assert_eq!(r.purge.removed.len(), 2);
        let dep = r.deploy.unwrap();
        assert_eq!(dep.added, ["mods/sound.txt"]);
        assert_eq!(f.read("data/a.bin"), "AAAA");
        assert_eq!(f.read("data/b.bin"), "NEW-B");
        assert!(!f.dir().join("mods/ui.txt").exists());
        let m = f.manifest();
        assert_eq!(m.profiles, ["third"]);
        assert_eq!(m.patches.len(), 1);
        assert!(f.sound());

        // If the old profile's file was changed, the purge leaves it and the new deploy is not attempted.
        let mut g = fx();
        two_file_mod(&mut g);
        g.deploy().unwrap();
        std::fs::write(g.dir().join("mods/ui.txt"), "edited").unwrap();
        g.add_mod("sound", "1.0", &[("mods/sound.txt", "s")]);
        g.profiles
            .create(&g.w.protected, "third", "Third", GAME, &g.build.clone())
            .unwrap();
        g.profiles
            .add_mod(&g.w.protected, "third", "sound", "1.0", None)
            .unwrap();
        let third = g.profiles.get("third").unwrap().clone();
        let r = purge_and_deploy(&mut g.reg, &g.w.ctx(), &g.store, &third, &g.layout).unwrap();
        assert!(r.deploy.is_none());
        assert_eq!(r.purge.left.len(), 1);
        assert!(!g.dir().join("mods/sound.txt").exists());
        assert_eq!(g.read("mods/ui.txt"), "edited");
    }

    #[test]
    fn nothing_to_deploy_and_space_problems_block() {
        let mut f = fx();
        f.add_mod("ui", "1.0", &[("mods/ui.txt", "ui")]);
        f.use_mod("ui", "1.0");
        f.profiles
            .set_enabled(&f.w.protected, "main", "ui", false)
            .unwrap();
        assert!(
            f.plan()
                .problems
                .iter()
                .any(|p| p.kind == ProblemKind::NothingToDeploy)
        );
        assert!(f.deploy().is_err());

        let mut g = fx();
        g.add_mod("ui", "1.0", &[("mods/ui.txt", "ui")]);
        g.use_mod("ui", "1.0");
        g.w.disk = crate::roots::fake::FakeDisk::with(10);
        let plan = plan_deploy(&g.reg, &g.w.ctx(), &g.store, &g.profile(), &g.layout).unwrap();
        assert!(
            plan.problems
                .iter()
                .any(|p| p.kind == ProblemKind::NotEnoughSpace)
        );
    }

    #[test]
    fn manifest_changes_cannot_change_the_game_and_failures_change_nothing() {
        let mut f = fx();
        let before = f.manifest();
        assert!(
            f.reg
                .update_manifest(&f.w.ctx(), &f.build.clone(), |m| {
                    m.game_id = "other".into();
                    Ok(())
                })
                .is_err()
        );
        assert!(
            f.reg
                .update_manifest(&f.w.ctx(), &f.build.clone(), |m| {
                    m.notes = "half done".into();
                    Err(Error::Invalid("stop".into()))
                })
                .is_err()
        );
        assert!(
            f.reg
                .update_manifest(&f.w.ctx(), &f.build.clone(), |m| {
                    m.files.push(FileEntry {
                        path: "../x".into(),
                        sha256: "0".repeat(64),
                        size: 1,
                    });
                    Ok(())
                })
                .is_err()
        );
        assert_eq!(f.manifest(), before);
    }
    // ---- found by an independent review of the purge and deploy code ----

    #[test]
    fn a_build_with_a_profile_deployed_is_not_moved_until_it_is_purged() {
        let mut f = fx();
        two_file_mod(&mut f);
        let original = f.snap();
        f.deploy().unwrap();
        let build = f.build.clone();

        let err = f
            .reg
            .migrate_build(&f.w.ctx(), &build, "hdd")
            .unwrap_err()
            .to_string();
        assert!(err.contains("deployed"), "{err}");
        // Nothing moved: the records are still found, and the build still matches its manifest.
        assert!(deployment_of(&f.reg, &f.w.ctx(), &build).unwrap().is_some());
        assert!(f.sound());

        f.purge();
        assert_eq!(f.snap(), original);
        f.reg.migrate_build(&f.w.ctx(), &build, "hdd").unwrap();
        assert!(f.sound());
    }

    #[test]
    fn a_build_whose_mod_records_cannot_be_read_is_not_moved() {
        let mut f = fx();
        two_file_mod(&mut f);
        f.deploy().unwrap();
        let build = f.build.clone();
        f.purge();
        std::fs::write(f.state_json(), b"{ not json").unwrap();
        assert!(f.reg.migrate_build(&f.w.ctx(), &build, "hdd").is_err());
        // Valid JSON that is not the records is just as unreadable.
        std::fs::write(f.state_json(), b"{}").unwrap();
        assert!(f.reg.migrate_build(&f.w.ctx(), &build, "hdd").is_err());
        assert!(f.sound());
    }

    #[test]
    fn an_undo_that_stops_part_way_is_finished_by_a_purge() {
        let mut f = fx();
        let original = f.snap();
        two_file_mod(&mut f);
        f.deploy().unwrap();
        let pid = f.purge().purge_id.unwrap();
        assert_eq!(f.snap(), original);

        // Make the second file (mods/ui.txt) fail inside the undo, after its checks passed: the build's mods
        // folder is protected for this one call.
        let mut stricter = f.w.protected.clone();
        stricter.add(&f.dir().join("mods")).unwrap();
        let ctx = Ctx {
            roots: &f.w.roots,
            protected: &stricter,
            journal: &f.w.journal,
            disk: &f.w.disk,
        };
        let build = f.build.clone();
        assert!(undo_purge(&mut f.reg, &ctx, &build, &pid).is_err());
        assert_eq!(f.read("data/a.bin"), "NEW-A", "the first file went back");

        // The ledger already lists both files again, so nothing is left without an owner.
        let d = deployment_of(&f.reg, &f.w.ctx(), &build)
            .unwrap()
            .expect("the ledger is written before the files move");
        assert_eq!(d.entries.len(), 2);
        assert_eq!(d.status, DeployStatus::InProgress);

        // A purge finishes the job: the build is as it was before the profile was ever deployed.
        let r = f.purge();
        assert!(r.left.is_empty(), "{:?}", r.left);
        assert_eq!(f.snap(), original);
        assert!(f.sound());
        assert!(deployment_of(&f.reg, &f.w.ctx(), &build).unwrap().is_none());
    }

    #[test]
    fn a_finished_undo_marks_the_deployment_as_complete() {
        let mut f = fx();
        two_file_mod(&mut f);
        f.deploy().unwrap();
        let pid = f.purge().purge_id.unwrap();
        let build = f.build.clone();
        undo_purge(&mut f.reg, &f.w.ctx(), &build, &pid).unwrap();
        let d = deployment_of(&f.reg, &f.w.ctx(), &build).unwrap().unwrap();
        assert_eq!(d.status, DeployStatus::Deployed);
        assert!(d.finished_unix.is_some());
        assert_eq!(d.entries.len(), 2);
        assert!(f.sound());
    }

    #[test]
    fn a_protected_folder_is_refused_however_the_path_to_it_is_spelled() {
        let l = layout();
        for p in [
            "data/anticheat/guard.dll",
            "data/./anticheat/guard.dll",
            "data//anticheat/guard.dll",
            "data\\anticheat\\guard.dll",
            "data/anticheat/./guard.dll",
            "./data/anticheat/guard.dll",
            "data/ANTICHEAT/guard.dll",
            "data/../data/anticheat/guard.dll",
            "data/../anticheat/guard.dll",
        ] {
            assert!(l.refusal_for(p).is_some(), "{p} was not refused");
        }
        for p in ["data/other/./file.bin", "./data/file.bin", "mods//ui.txt"] {
            assert_eq!(l.refusal_for(p), None, "{p}");
        }

        // The same by where the write would really land.
        let f = fx();
        let dir = f.dir();
        std::fs::create_dir_all(dir.join("data/anticheat")).unwrap();
        for p in ["data/./anticheat/guard.dll", "data//anticheat/guard.dll"] {
            let target = safepath::join_within(&dir, p).unwrap();
            assert!(landing_refusal(&l, &dir, &target).is_some(), "{p}");
        }
        let fine = safepath::join_within(&dir, "data/other/x.bin").unwrap();
        assert_eq!(landing_refusal(&l, &dir, &fine), None);
        let outside = dir.parent().unwrap().join("elsewhere.bin");
        assert!(landing_refusal(&l, &dir, &outside).is_some());
    }

    #[test]
    fn a_file_changed_after_the_plan_but_before_its_move_is_not_removed() {
        let mut f = fx();
        two_file_mod(&mut f);
        f.deploy().unwrap();
        let build = f.build.clone();

        // The user edits mods/ui.txt after the purge planned it and before it was moved.
        let err = purge_inner(&mut f.reg, &f.w.ctx(), &build, &mut |p: &Path| {
            if p.ends_with("mods/ui.txt") {
                std::fs::write(p, "USER EDIT").unwrap();
            }
        })
        .unwrap_err()
        .to_string();
        assert!(err.contains("changed while the purge was running"), "{err}");
        assert_eq!(f.read("mods/ui.txt"), "USER EDIT");

        // Running the purge again finishes the rest and leaves the edited file, reporting it.
        let r = f.purge();
        assert_eq!(f.read("mods/ui.txt"), "USER EDIT");
        assert!(
            r.left.iter().any(
                |i| i.path == "mods/ui.txt" && i.action == PurgeAction::LeaveChangedSinceDeploy
            ),
            "{:?}",
            r.left
        );
        assert_eq!(f.read("data/a.bin"), "AAAA");
        assert!(r.recovery_dir.is_none() || !r.removed.contains(&"mods/ui.txt".to_owned()));
    }

    #[test]
    fn files_that_differ_only_in_case_are_kept_apart_in_the_manifest_when_the_ledger_says_so() {
        let f = fx();
        let mut m = f.manifest();
        let before: Vec<String> = m.files.iter().map(|e| e.path.clone()).collect();
        assert!(before.contains(&"data/a.bin".to_owned()));
        let entry = LedgerEntry {
            path: "data/A.bin".into(),
            mod_id: "ui".into(),
            mod_version: "1.0".into(),
            action: EntryAction::Add,
            deployed_sha256: "ff".repeat(32),
            deployed_size: 2,
            base_sha256: None,
            base_size: None,
            manifest_before: None,
        };
        let header = DeploymentHeader {
            id: "dep-1".into(),
            profile_id: "p".into(),
            profile_name: "P".into(),
            started_unix: 1,
        };
        apply_to_manifest(&mut m, std::slice::from_ref(&entry), &header);
        let paths: Vec<String> = m.files.iter().map(|e| e.path.clone()).collect();
        assert!(paths.contains(&"data/a.bin".to_owned()), "{paths:?}");
        assert!(paths.contains(&"data/A.bin".to_owned()), "{paths:?}");

        revert_in_manifest(&mut m, &[&entry], &header, true);
        let after: Vec<String> = m.files.iter().map(|e| e.path.clone()).collect();
        assert_eq!(after, before);
    }
    #[test]
    fn a_folder_where_a_deployed_file_was_is_left_alone_by_a_purge() {
        let mut f = fx();
        two_file_mod(&mut f);
        f.deploy().unwrap();
        std::fs::remove_file(f.dir().join("mods/ui.txt")).unwrap();
        put(&f.dir().join("mods/ui.txt/keep.txt"), "mine");

        let r = f.purge();
        assert!(
            r.left.iter().any(
                |i| i.path == "mods/ui.txt" && i.action == PurgeAction::LeaveChangedSinceDeploy
            ),
            "{:?}",
            r.left
        );
        assert_eq!(f.read("mods/ui.txt/keep.txt"), "mine");
    }

    #[test]
    fn an_undo_is_refused_when_the_removed_copy_or_the_put_back_original_was_changed() {
        // The kept copy of the removed mod file was edited.
        let mut f = fx();
        two_file_mod(&mut f);
        f.deploy().unwrap();
        let r = f.purge();
        let pid = r.purge_id.clone().unwrap();
        let kept = r.recovery_dir.clone().unwrap().join("mods/ui.txt");
        std::fs::write(&kept, "EDITED").unwrap();
        let snap = f.snap();
        let err = undo_purge(&mut f.reg, &f.w.ctx(), &f.build, &pid)
            .unwrap_err()
            .to_string();
        assert!(err.contains("missing or changed"), "{err}");
        assert_eq!(f.snap(), snap);

        // The original that the purge put back was edited by the user.
        let mut g = fx();
        two_file_mod(&mut g);
        g.deploy().unwrap();
        let pid = g.purge().purge_id.unwrap();
        std::fs::write(g.dir().join("data/a.bin"), "MINE").unwrap();
        let err = undo_purge(&mut g.reg, &g.w.ctx(), &g.build, &pid)
            .unwrap_err()
            .to_string();
        assert!(err.contains("is not where the purge left it"), "{err}");
        assert_eq!(g.read("data/a.bin"), "MINE");
    }
}
