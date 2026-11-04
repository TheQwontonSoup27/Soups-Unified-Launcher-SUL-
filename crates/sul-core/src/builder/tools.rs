//! External tools (spec 2.3: "tool settings/version records"; spec 2.4: "treat the tool as an external
//! integration unless redistribution permission is established").
//!
//! SUL **never copies, bundles or downloads a tool.** The owner points it at a program they installed
//! themselves. SUL then records, and keeps checking:
//!
//! * the tool's name, the version the owner typed (not verified), its size and SHA-256, and what the owner
//!   knows about its licence and whether it may be redistributed (words the owner wrote; SUL does not check
//!   them). Claiming "redistribution permitted" needs the evidence written down.
//! * whether the file is still the one that was recorded. A tool that changed is **not run** until the owner
//!   accepts the change, and the old version and hash are kept in the tool's history.
//!
//! # Running a tool
//!
//! [`start_tool_run`] shows exactly what it will start ([`plan_tool_run`] is the preview), refuses a tool whose
//! file changed, starts it through the same launcher seam as everything else (no shell, bounded arguments,
//! the working folder is the workspace folder the caller names), and returns a handle. Polling the handle
//! returns a [`ToolRunRecord`] once the program has exited: the tool's version and hash, the arguments (as
//! given: do not put secrets in them) and the names of the environment variables, the exit code, and which files
//! in the workspace folder were **added, changed or removed**, with hashes.
//!
//! What it cannot do: SUL has no way to stop a program it started, so there is no timeout; it cannot confine a
//! tool to the workspace folder, so a file the tool writes elsewhere is invisible to the record; and it has never
//! been run against a real tool, only against a stand-in that starts nothing. [`known_tools`] lists what was
//! found out about the two tools spec 2.4 names, and where that is thin.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::capability::Status;
use crate::error::io;
use crate::fsops::{self, ProtectedRoots};
use crate::manifest::{FileEntry, hash_file, inventory, now_unix};
use crate::mods::{validate_name, validate_note, validate_text};
use crate::process::{Child, ExitInfo, ProcessLauncher, ProcessSpec, check_args_and_env};
use crate::{Error, Result};

pub const TOOLS_SCHEMA: u32 = 1;
pub const MAX_SETTINGS: usize = 32;
pub const MAX_HISTORY: usize = 50;
/// Files and folders in a workspace whose names start with this are SUL's own and are left out of the
/// before-and-after comparison.
pub const OWN_PREFIX: &str = ".sul-";
pub const RUN_RECORD_DIR: &str = ".sul-tool-runs";

/// What the owner says about redistributing a tool. SUL bundles nothing whatever this says: shipping a tool
/// inside SUL would be a separate decision that has not been made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Redistribution {
    Unknown,
    NotPermitted,
    Permitted,
}

/// An earlier state of a tool's file, kept when the owner accepts a change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolHistory {
    pub version: String,
    pub exe_sha256: String,
    pub exe_size: u64,
    pub recorded_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolRecord {
    pub id: String,
    pub display_name: String,
    /// As the owner typed it. SUL does not read it from the program.
    pub version: String,
    pub exe_path: PathBuf,
    pub exe_sha256: String,
    pub exe_size: u64,
    pub registered_unix: u64,
    pub redistribution: Redistribution,
    /// The licence or permission, in the owner's words. Not checked.
    pub license_note: String,
    /// Where the owner got the tool. No secrets.
    pub source_note: String,
    /// Earlier states, newest last, at most [`MAX_HISTORY`].
    pub history: Vec<ToolHistory>,
}

#[derive(Debug, Clone)]
pub struct ToolRegistration {
    pub id: String,
    pub display_name: String,
    pub version: String,
    pub exe_path: PathBuf,
    pub redistribution: Redistribution,
    pub license_note: String,
    pub source_note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ToolStatus {
    /// The file is the one that was recorded.
    Matches,
    /// The file exists but is not the recorded one.
    Changed { exe_sha256: String, exe_size: u64 },
    /// There is no file at the recorded path.
    Missing,
}

#[derive(Debug, Serialize, Deserialize)]
struct ToolsData {
    schema: u32,
    tools: Vec<ToolRecord>,
}

pub struct ToolRegistry {
    file: PathBuf,
    data: ToolsData,
}

/// A path that names one plain file: not a link, not a folder.
fn plain_file(path: &Path) -> Result<()> {
    let meta = std::fs::symlink_metadata(path).map_err(io(path))?;
    if !meta.is_file() {
        return Err(Error::Invalid(format!(
            "{} is not a plain file (a link or a folder cannot be a tool)",
            path.display()
        )));
    }
    Ok(())
}

impl ToolRegistry {
    pub fn open(file: &Path) -> Result<Self> {
        let data = match std::fs::read_to_string(file) {
            Ok(text) => {
                let d: ToolsData = serde_json::from_str(&text).map_err(|e| Error::json(file, e))?;
                if d.schema > TOOLS_SCHEMA {
                    return Err(Error::Invalid(format!(
                        "tool registry schema {} is newer than supported {TOOLS_SCHEMA}",
                        d.schema
                    )));
                }
                d
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => ToolsData {
                schema: TOOLS_SCHEMA,
                tools: vec![],
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

    pub fn tools(&self) -> &[ToolRecord] {
        &self.data.tools
    }

    pub fn get(&self, id: &str) -> Option<&ToolRecord> {
        self.data.tools.iter().find(|t| t.id == id)
    }

    fn check_exe(protected: &ProtectedRoots, exe: &Path) -> Result<()> {
        if !exe.is_absolute() {
            return Err(Error::Invalid("a tool is named by its full path".into()));
        }
        plain_file(exe)?;
        protected.check_writable(exe).map_err(|_| {
            Error::Invalid(
                "a tool may not live inside a protected game install; put it in a folder of its own".into(),
            )
        })
    }

    /// Record a tool the owner installed. Hashes the file now; changes nothing else.
    pub fn register(
        &mut self,
        protected: &ProtectedRoots,
        req: &ToolRegistration,
    ) -> Result<ToolRecord> {
        validate_name("tool id", &req.id)?;
        validate_text("tool name", &req.display_name, 80)?;
        validate_text("tool version", &req.version, 64)?;
        validate_note("licence note", &req.license_note, 500)?;
        validate_note("source note", &req.source_note, 500)?;
        if req.redistribution == Redistribution::Permitted && req.license_note.trim().is_empty() {
            return Err(Error::Invalid(
                "to record that redistribution is permitted, write down the licence or permission in the licence note".into(),
            ));
        }
        if self.get(&req.id).is_some() {
            return Err(Error::Invalid(format!(
                "a tool {:?} is already recorded",
                req.id
            )));
        }
        Self::check_exe(protected, &req.exe_path)?;
        let (exe_sha256, exe_size) = hash_file(&req.exe_path)?;
        let rec = ToolRecord {
            id: req.id.clone(),
            display_name: req.display_name.clone(),
            version: req.version.clone(),
            exe_path: req.exe_path.clone(),
            exe_sha256,
            exe_size,
            registered_unix: now_unix(),
            redistribution: req.redistribution,
            license_note: req.license_note.clone(),
            source_note: req.source_note.clone(),
            history: vec![],
        };
        self.data.tools.push(rec.clone());
        if let Err(e) = self.save(protected) {
            self.data.tools.pop();
            return Err(e);
        }
        Ok(rec)
    }

    /// Whether the tool's file is still the recorded one. Read only.
    pub fn check(&self, id: &str) -> Result<ToolStatus> {
        let rec = self
            .get(id)
            .ok_or_else(|| Error::Invalid(format!("no tool {id:?} is recorded")))?;
        match std::fs::symlink_metadata(&rec.exe_path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(ToolStatus::Missing),
            Err(e) => return Err(io(&rec.exe_path)(e)),
            Ok(m) if !m.is_file() => return Ok(ToolStatus::Missing),
            Ok(_) => {}
        }
        let (sha, size) = hash_file(&rec.exe_path)?;
        Ok(if sha == rec.exe_sha256 && size == rec.exe_size {
            ToolStatus::Matches
        } else {
            ToolStatus::Changed {
                exe_sha256: sha,
                exe_size: size,
            }
        })
    }

    /// The owner confirms that the file now at the recorded path is the tool they mean, at `new_version`. The
    /// old version and hash go into the history.
    pub fn accept_change(
        &mut self,
        protected: &ProtectedRoots,
        id: &str,
        new_version: &str,
    ) -> Result<()> {
        validate_text("tool version", new_version, 64)?;
        let i = self
            .data
            .tools
            .iter()
            .position(|t| t.id == id)
            .ok_or_else(|| Error::Invalid(format!("no tool {id:?} is recorded")))?;
        let before = self.data.tools[i].clone();
        Self::check_exe(protected, &before.exe_path)?;
        let (sha, size) = hash_file(&before.exe_path)?;
        let t = &mut self.data.tools[i];
        t.history.push(ToolHistory {
            version: before.version.clone(),
            exe_sha256: before.exe_sha256.clone(),
            exe_size: before.exe_size,
            recorded_unix: now_unix(),
        });
        if t.history.len() > MAX_HISTORY {
            t.history.remove(0);
        }
        t.version = new_version.to_owned();
        t.exe_sha256 = sha;
        t.exe_size = size;
        if let Err(e) = self.save(protected) {
            self.data.tools[i] = before;
            return Err(e);
        }
        Ok(())
    }
}

/// What to run, and in which workspace folder.
#[derive(Debug, Clone)]
pub struct ToolRunSpec {
    pub tool_id: String,
    pub args: Vec<String>,
    /// Names and values. Only the names are kept in the record.
    pub env: Vec<(String, String)>,
    /// The tool's settings as the owner wants them recorded (for example "bank = music").
    pub settings: BTreeMap<String, String>,
    /// The program's working folder and the folder whose files are compared before and after.
    pub workspace: PathBuf,
}

/// Exactly what a run would start. Nothing is started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolRunPlan {
    pub exe: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub tool_version: String,
    pub tool_sha256: String,
    /// Anything here stops a run.
    pub problems: Vec<String>,
}

impl ToolRunPlan {
    pub fn blocked(&self) -> bool {
        !self.problems.is_empty()
    }
}

pub fn plan_tool_run(
    reg: &ToolRegistry,
    protected: &ProtectedRoots,
    spec: &ToolRunSpec,
) -> Result<ToolRunPlan> {
    let rec = reg
        .get(&spec.tool_id)
        .ok_or_else(|| Error::Invalid(format!("no tool {:?} is recorded", spec.tool_id)))?;
    let mut problems = Vec::new();
    match reg.check(&spec.tool_id)? {
        ToolStatus::Matches => {}
        ToolStatus::Changed { .. } => problems.push(
            "the tool's file is not the one that was recorded; check it, then accept the change before running it".into(),
        ),
        ToolStatus::Missing => problems.push("the tool's file is missing".into()),
    }
    if let Err(e) = check_args_and_env(&spec.args, &spec.env) {
        problems.push(e.to_string());
    }
    if spec.settings.len() > MAX_SETTINGS {
        problems.push(format!("at most {MAX_SETTINGS} settings can be recorded"));
    }
    for (k, v) in &spec.settings {
        if validate_text("setting name", k, 100).is_err()
            || validate_text("setting value", v, 200).is_err()
        {
            problems.push(format!("setting {k:?} has a name or value that is empty, too long or has control characters"));
        }
    }
    match std::fs::symlink_metadata(&spec.workspace) {
        Ok(m) if m.is_dir() => {
            if protected.check_writable(&spec.workspace).is_err() {
                problems.push("the workspace folder is inside a protected game install".into());
            }
        }
        _ => problems.push("the workspace folder does not exist or is not a plain folder".into()),
    }
    Ok(ToolRunPlan {
        exe: rec.exe_path.clone(),
        args: spec.args.clone(),
        cwd: spec.workspace.clone(),
        tool_version: rec.version.clone(),
        tool_sha256: rec.exe_sha256.clone(),
        problems,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangedFile {
    pub path: String,
    pub before_sha256: String,
    pub after_sha256: String,
}

/// What a finished run did to the workspace folder, and what it was asked to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolRunRecord {
    pub tool_id: String,
    pub tool_version: String,
    pub tool_sha256: String,
    pub args: Vec<String>,
    pub env_names: Vec<String>,
    pub settings: BTreeMap<String, String>,
    pub started_unix: u64,
    pub finished_unix: u64,
    pub exit_code: Option<i32>,
    pub success: bool,
    pub added: Vec<FileEntry>,
    pub changed: Vec<ChangedFile>,
    pub removed: Vec<String>,
    /// Set when the folder could not be compared (a link inside it, for example). Then the three lists above
    /// say nothing.
    pub comparison_error: Option<String>,
    /// Always the same sentence: what this record cannot see.
    pub limits: String,
}

const LIMITS: &str = "Only files inside the workspace folder are compared. SUL cannot confine a program, so anything it wrote elsewhere is not in this record.";

fn snapshot(dir: &Path) -> Result<BTreeMap<String, FileEntry>> {
    Ok(inventory(dir)?
        .into_iter()
        .filter(|f| !f.path.split('/').any(|p| p.starts_with(OWN_PREFIX)))
        .map(|f| (f.path.clone(), f))
        .collect())
}

/// A started tool. Poll it until it returns a record.
pub struct ToolRunHandle {
    child: Box<dyn Child>,
    workspace: PathBuf,
    before: Result<BTreeMap<String, FileEntry>>,
    base: ToolRunRecord,
}

impl ToolRunHandle {
    pub fn pid(&self) -> u32 {
        self.child.pid()
    }

    /// `Ok(None)` while the tool is still running. When it has exited, the record (once: poll again and the
    /// child's exit is reported again with a fresh comparison, so keep the first record).
    pub fn poll(&mut self) -> Result<Option<ToolRunRecord>> {
        let Some(ExitInfo { code, success }) = self.child.poll()? else {
            return Ok(None);
        };
        let mut rec = self.base.clone();
        rec.finished_unix = now_unix();
        rec.exit_code = code;
        rec.success = success;
        let after = snapshot(&self.workspace);
        match (&self.before, &after) {
            (Ok(before), Ok(after)) => {
                for (p, a) in after {
                    match before.get(p) {
                        None => rec.added.push(a.clone()),
                        Some(b) if b.sha256 != a.sha256 => rec.changed.push(ChangedFile {
                            path: p.clone(),
                            before_sha256: b.sha256.clone(),
                            after_sha256: a.sha256.clone(),
                        }),
                        Some(_) => {}
                    }
                }
                rec.removed = before
                    .keys()
                    .filter(|p| !after.contains_key(*p))
                    .cloned()
                    .collect();
            }
            (Err(e), _) | (_, Err(e)) => rec.comparison_error = Some(e.to_string()),
        }
        Ok(Some(rec))
    }
}

/// Start a recorded tool in `spec.workspace`. Refuses anything [`plan_tool_run`] reports as a problem.
pub fn start_tool_run(
    reg: &ToolRegistry,
    protected: &ProtectedRoots,
    launcher: &dyn ProcessLauncher,
    spec: &ToolRunSpec,
) -> Result<ToolRunHandle> {
    let plan = plan_tool_run(reg, protected, spec)?;
    if plan.blocked() {
        return Err(Error::Invalid(format!(
            "the tool cannot be run: {}",
            plan.problems.join("; ")
        )));
    }
    let before = snapshot(&spec.workspace);
    let started_unix = now_unix();
    let child = launcher.start(&ProcessSpec {
        exe: plan.exe.clone(),
        args: plan.args.clone(),
        cwd: plan.cwd.clone(),
        env: spec.env.clone(),
        capture_bytes: None,
    })?;
    Ok(ToolRunHandle {
        child,
        workspace: spec.workspace.clone(),
        before,
        base: ToolRunRecord {
            tool_id: spec.tool_id.clone(),
            tool_version: plan.tool_version,
            tool_sha256: plan.tool_sha256,
            args: spec.args.clone(),
            env_names: spec.env.iter().map(|(k, _)| k.clone()).collect(),
            settings: spec.settings.clone(),
            started_unix,
            finished_unix: started_unix,
            exit_code: None,
            success: false,
            added: vec![],
            changed: vec![],
            removed: vec![],
            comparison_error: None,
            limits: LIMITS.into(),
        },
    })
}

/// Keep a finished run's record in the workspace folder, in `.sul-tool-runs/` (which later comparisons skip).
/// Never overwrites an earlier record.
pub fn save_run_record(
    protected: &ProtectedRoots,
    workspace: &Path,
    rec: &ToolRunRecord,
) -> Result<PathBuf> {
    let dir = workspace.join(RUN_RECORD_DIR);
    let body = serde_json::to_string_pretty(rec)?;
    for n in 0u32..10_000 {
        let path = dir.join(format!("{}-{}-{n}.json", rec.finished_unix, rec.tool_id));
        if std::fs::symlink_metadata(&path).is_err() {
            fsops::atomic_write(protected, &path, body.as_bytes())?;
            return Ok(path);
        }
    }
    Err(Error::Invalid(
        "too many run records with the same name".into(),
    ))
}

/// What is known about a tool spec 2.4 names. A finding is not a promise: it says what was read, on what date.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KnownTool {
    pub id: &'static str,
    pub name: &'static str,
    /// The label for using it through SUL today.
    pub status: Status,
    pub finding: &'static str,
    pub not_established: &'static str,
}

/// Research done 2026-11-01 by reading public pages (not by running anything). See
/// `docs/investigations/audio-modding-2026-11-01.md`.
pub fn known_tools() -> &'static [KnownTool] {
    &[
        KnownTool {
            id: "hd2-audio-modder",
            name: "Helldivers 2 Audio Modder (GitHub: RaidingForPants/hd2-audio-modder; \"Pants' HD2 Audio Modder\" is taken to be this)",
            status: Status::NotInvestigated,
            finding: "The repository page lists a LICENSE file and links to it as the MIT license. Its README gives set-up steps for Windows, Linux and macOS and says prebuilt programs (made with PyInstaller) are in its releases. Its 1.10 release notes state no licence or redistribution terms. It needs Python 3.10 or later to run from source and vgmstream for audio playback.",
            not_established: "That the repository is the tool spec 2.4 means; the text of the licence (not read); whether the MIT licence covers everything in a release (including the vgmstream and other parts that come with it); whether it works with the current game; whether its output is accepted by the game. SUL has never run it.",
        },
        KnownTool {
            id: "wwise",
            name: "Audiokinetic Wwise (authoring tool)",
            status: Status::NotInvestigated,
            finding: "A community modding guide lists Wwise (version 2023.1.7.8574, installed through the Audiokinetic Launcher) as the program that converts audio to the .wem format the game reads. That guide does not say whether Wwise is needed to repack.",
            not_established: "Audiokinetic's licence terms (not read, so no redistribution is assumed); whether another way to make .wem files exists; what repacking needs.",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::fake::{FakeLauncher, Script};

    struct Fx {
        _d: tempfile::TempDir,
        base: PathBuf,
        protected: ProtectedRoots,
        reg: ToolRegistry,
        exe: PathBuf,
        ws: PathBuf,
    }

    fn fx() -> Fx {
        let d = tempfile::tempdir().unwrap();
        let base = d.path().canonicalize().unwrap();
        let exe = base.join("tools/audio.exe");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"pretend program").unwrap();
        let ws = base.join("work");
        std::fs::create_dir_all(&ws).unwrap();
        let mut protected = ProtectedRoots::new();
        let install = base.join("install");
        std::fs::create_dir_all(&install).unwrap();
        protected.add(&install).unwrap();
        let reg = ToolRegistry::open(&base.join("state/tools.json")).unwrap();
        Fx {
            _d: d,
            base,
            protected,
            reg,
            exe,
            ws,
        }
    }

    fn registration(f: &Fx) -> ToolRegistration {
        ToolRegistration {
            id: "audio".into(),
            display_name: "Audio tool".into(),
            version: "1.0".into(),
            exe_path: f.exe.clone(),
            redistribution: Redistribution::Unknown,
            license_note: String::new(),
            source_note: "downloaded by me".into(),
        }
    }

    fn spec(f: &Fx) -> ToolRunSpec {
        ToolRunSpec {
            tool_id: "audio".into(),
            args: vec!["--in".into(), "a.wav".into()],
            env: vec![("TOKEN_FOR_TOOL".into(), "s3cret".into())],
            settings: BTreeMap::from([("bank".into(), "music".into())]),
            workspace: f.ws.clone(),
        }
    }

    #[test]
    fn registering_records_the_hash_and_size_and_leaves_the_tool_alone() {
        let mut f = fx();
        let rec = f.reg.register(&f.protected, &registration(&f)).unwrap();
        assert_eq!(rec.exe_size, 15);
        assert_eq!(rec.exe_sha256.len(), 64);
        assert!(rec.history.is_empty());
        assert_eq!(std::fs::read(&f.exe).unwrap(), b"pretend program");
        assert_eq!(f.reg.check("audio").unwrap(), ToolStatus::Matches);
        // Survives a restart.
        let again = ToolRegistry::open(&f.base.join("state/tools.json")).unwrap();
        assert_eq!(again.tools(), f.reg.tools());
    }

    #[test]
    fn a_registration_is_refused_when_it_is_wrong() {
        let mut f = fx();
        let ok = registration(&f);
        f.reg.register(&f.protected, &ok).unwrap();
        assert!(f.reg.register(&f.protected, &ok).is_err(), "twice");

        let mut r = registration(&f);
        r.id = "other".into();
        r.exe_path = PathBuf::from("relative/tool.exe");
        assert!(f.reg.register(&f.protected, &r).is_err(), "relative path");
        r.exe_path = f.base.join("tools");
        assert!(f.reg.register(&f.protected, &r).is_err(), "a folder");
        r.exe_path = f.base.join("tools/missing.exe");
        assert!(f.reg.register(&f.protected, &r).is_err(), "missing");

        let inside = f.base.join("install/tool.exe");
        std::fs::write(&inside, b"x").unwrap();
        r.exe_path = inside;
        let err = f.reg.register(&f.protected, &r).unwrap_err().to_string();
        assert!(err.contains("protected game install"), "{err}");

        let mut r = registration(&f);
        r.id = "perm".into();
        r.redistribution = Redistribution::Permitted;
        let err = f.reg.register(&f.protected, &r).unwrap_err().to_string();
        assert!(err.contains("licence note"), "{err}");
        r.license_note = "MIT, see LICENSE in the repo".into();
        assert!(f.reg.register(&f.protected, &r).is_ok());

        let mut r = registration(&f);
        r.id = "con".into();
        assert!(f.reg.register(&f.protected, &r).is_err(), "device name");
        assert_eq!(
            f.reg.tools().len(),
            2,
            "failed registrations leave nothing behind"
        );
    }

    #[test]
    fn a_tool_that_changed_or_vanished_is_noticed_and_not_run() {
        let mut f = fx();
        f.reg.register(&f.protected, &registration(&f)).unwrap();
        std::fs::write(&f.exe, b"a different program!").unwrap();
        assert!(matches!(
            f.reg.check("audio").unwrap(),
            ToolStatus::Changed { exe_size: 20, .. }
        ));
        let plan = plan_tool_run(&f.reg, &f.protected, &spec(&f)).unwrap();
        assert!(plan.blocked());
        let launcher = FakeLauncher::new();
        assert!(start_tool_run(&f.reg, &f.protected, &launcher, &spec(&f)).is_err());
        assert!(
            launcher.started.lock().unwrap().is_empty(),
            "nothing was started"
        );

        std::fs::remove_file(&f.exe).unwrap();
        assert_eq!(f.reg.check("audio").unwrap(), ToolStatus::Missing);
        assert!(start_tool_run(&f.reg, &f.protected, &launcher, &spec(&f)).is_err());
    }

    #[test]
    fn accepting_a_change_keeps_the_old_version_in_the_history_and_allows_a_run() {
        let mut f = fx();
        let first = f.reg.register(&f.protected, &registration(&f)).unwrap();
        std::fs::write(&f.exe, b"version two").unwrap();
        f.reg.accept_change(&f.protected, "audio", "2.0").unwrap();
        let now = f.reg.get("audio").unwrap();
        assert_eq!(now.version, "2.0");
        assert_ne!(now.exe_sha256, first.exe_sha256);
        assert_eq!(now.history.len(), 1);
        assert_eq!(now.history[0].version, "1.0");
        assert_eq!(now.history[0].exe_sha256, first.exe_sha256);
        assert_eq!(f.reg.check("audio").unwrap(), ToolStatus::Matches);
        assert!(
            !plan_tool_run(&f.reg, &f.protected, &spec(&f))
                .unwrap()
                .blocked()
        );
        assert!(f.reg.accept_change(&f.protected, "nope", "1").is_err());
    }

    #[test]
    fn a_run_is_started_exactly_as_planned_and_records_what_changed_in_the_workspace() {
        let mut f = fx();
        f.reg.register(&f.protected, &registration(&f)).unwrap();
        std::fs::write(f.ws.join("keep.txt"), "same").unwrap();
        std::fs::write(f.ws.join("edit.txt"), "before").unwrap();
        std::fs::write(f.ws.join("gone.txt"), "bye").unwrap();

        let launcher = FakeLauncher::new();
        launcher.scripts.lock().unwrap().push_back(Script {
            polls_before_exit: 2,
            exit: ExitInfo {
                code: Some(0),
                success: true,
            },
            output: None,
        });
        let mut h = start_tool_run(&f.reg, &f.protected, &launcher, &spec(&f)).unwrap();
        {
            let started = launcher.started.lock().unwrap();
            assert_eq!(started.len(), 1);
            assert_eq!(started[0].exe, f.exe);
            assert_eq!(started[0].args, ["--in", "a.wav"]);
            assert_eq!(started[0].cwd, f.ws);
            assert_eq!(started[0].capture_bytes, None);
        }
        // The tool "works": changes files while it runs.
        std::fs::write(f.ws.join("edit.txt"), "after").unwrap();
        std::fs::remove_file(f.ws.join("gone.txt")).unwrap();
        std::fs::write(f.ws.join("new.wem"), "audio").unwrap();

        assert!(h.poll().unwrap().is_none());
        assert!(h.poll().unwrap().is_none());
        let rec = h.poll().unwrap().expect("exited");
        assert!(rec.success);
        assert_eq!(rec.exit_code, Some(0));
        assert_eq!(rec.tool_version, "1.0");
        assert_eq!(rec.args, ["--in", "a.wav"]);
        assert_eq!(rec.env_names, ["TOKEN_FOR_TOOL"]);
        assert!(
            !serde_json::to_string(&rec).unwrap().contains("s3cret"),
            "an environment value is never recorded"
        );
        assert_eq!(rec.settings["bank"], "music");
        assert_eq!(
            rec.added
                .iter()
                .map(|a| a.path.as_str())
                .collect::<Vec<_>>(),
            ["new.wem"]
        );
        assert_eq!(
            rec.changed
                .iter()
                .map(|c| c.path.as_str())
                .collect::<Vec<_>>(),
            ["edit.txt"]
        );
        assert_eq!(rec.removed, ["gone.txt"]);
        assert!(rec.comparison_error.is_none());
        assert!(rec.limits.contains("elsewhere"));
    }

    #[test]
    fn saved_records_are_kept_beside_the_work_and_never_counted_as_changes() {
        let mut f = fx();
        f.reg.register(&f.protected, &registration(&f)).unwrap();
        let launcher = FakeLauncher::new();
        let mut h = start_tool_run(&f.reg, &f.protected, &launcher, &spec(&f)).unwrap();
        let rec = h.poll().unwrap().unwrap();
        let one = save_run_record(&f.protected, &f.ws, &rec).unwrap();
        let two = save_run_record(&f.protected, &f.ws, &rec).unwrap();
        assert_ne!(one, two, "an earlier record is never overwritten");
        assert!(one.starts_with(f.ws.join(RUN_RECORD_DIR)));
        let back: ToolRunRecord =
            serde_json::from_str(&std::fs::read_to_string(&one).unwrap()).unwrap();
        assert_eq!(back, rec);

        // A second run now sees no added file for the saved records.
        let mut h2 = start_tool_run(&f.reg, &f.protected, &launcher, &spec(&f)).unwrap();
        let rec2 = h2.poll().unwrap().unwrap();
        assert!(rec2.added.is_empty() && rec2.changed.is_empty() && rec2.removed.is_empty());
    }

    #[test]
    fn a_failing_tool_is_recorded_as_failed_and_a_start_failure_is_an_error() {
        let mut f = fx();
        f.reg.register(&f.protected, &registration(&f)).unwrap();
        let launcher = FakeLauncher::new();
        launcher.scripts.lock().unwrap().push_back(Script {
            polls_before_exit: 0,
            exit: ExitInfo {
                code: Some(3),
                success: false,
            },
            output: None,
        });
        let mut h = start_tool_run(&f.reg, &f.protected, &launcher, &spec(&f)).unwrap();
        let rec = h.poll().unwrap().unwrap();
        assert!(!rec.success);
        assert_eq!(rec.exit_code, Some(3));

        *launcher.fail_with.lock().unwrap() = Some("cannot start".into());
        assert!(start_tool_run(&f.reg, &f.protected, &launcher, &spec(&f)).is_err());
    }

    #[test]
    fn a_plan_names_every_reason_a_run_is_refused() {
        let mut f = fx();
        f.reg.register(&f.protected, &registration(&f)).unwrap();
        let mut s = spec(&f);
        s.workspace = f.base.join("install");
        let p = plan_tool_run(&f.reg, &f.protected, &s).unwrap();
        assert!(
            p.problems
                .iter()
                .any(|m| m.contains("protected game install")),
            "{:?}",
            p.problems
        );

        let mut s = spec(&f);
        s.workspace = f.base.join("nope");
        assert!(plan_tool_run(&f.reg, &f.protected, &s).unwrap().blocked());

        let mut s = spec(&f);
        s.args = vec!["a\0b".into()];
        assert!(plan_tool_run(&f.reg, &f.protected, &s).unwrap().blocked());

        let mut s = spec(&f);
        s.env = vec![("1BAD".into(), "x".into())];
        assert!(plan_tool_run(&f.reg, &f.protected, &s).unwrap().blocked());

        let mut s = spec(&f);
        s.settings = (0..=MAX_SETTINGS)
            .map(|i| (format!("k{i}"), "v".into()))
            .collect();
        assert!(plan_tool_run(&f.reg, &f.protected, &s).unwrap().blocked());

        let mut s = spec(&f);
        s.settings = BTreeMap::from([("k".into(), "line\nbreak".into())]);
        assert!(plan_tool_run(&f.reg, &f.protected, &s).unwrap().blocked());

        let mut s = spec(&f);
        s.tool_id = "unknown".into();
        assert!(plan_tool_run(&f.reg, &f.protected, &s).is_err());

        assert!(
            !plan_tool_run(&f.reg, &f.protected, &spec(&f))
                .unwrap()
                .blocked()
        );
    }

    #[test]
    fn a_comparison_that_could_not_be_made_is_reported_not_hidden() {
        // The comparison refuses links, so the record says it could not compare; it does not claim "nothing changed".
        let mut f = fx();
        f.reg.register(&f.protected, &registration(&f)).unwrap();
        let launcher = FakeLauncher::new();
        let mut h = start_tool_run(&f.reg, &f.protected, &launcher, &spec(&f)).unwrap();
        h.before = Err(Error::Invalid("a link was found".into()));
        let rec = h.poll().unwrap().unwrap();
        assert!(
            rec.comparison_error
                .as_deref()
                .is_some_and(|m| m.contains("a link was found"))
        );
        assert!(rec.added.is_empty());
    }

    #[test]
    fn what_is_known_about_the_two_named_tools_does_not_claim_more_than_was_read() {
        let k = known_tools();
        assert_eq!(
            k.iter().map(|t| t.id).collect::<Vec<_>>(),
            ["hd2-audio-modder", "wwise"]
        );
        for t in k {
            assert_eq!(
                t.status,
                Status::NotInvestigated,
                "{}: nothing was run",
                t.id
            );
            assert!(!t.not_established.is_empty());
        }
    }
}
