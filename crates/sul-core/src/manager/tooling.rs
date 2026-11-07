//! The service behind the Tools tab (spec 2.3 and 2.4): record the external programs the owner installed, notice
//! when one changes, and run one on the files of a finished recipe run with a record of what it did.
//!
//! The rules are in [`crate::builder::tools`]; this module wires them to the service's state and adds the few rules
//! that belong to the app and not to the library:
//!
//! * **SUL never copies, bundles or downloads a tool.** The owner types the full path of a program they installed.
//! * **A tool works on one place only: the `files` folder of a finished recipe run** in a mod library, found by the
//!   recipe, the library folder and the run, never by a path the page sends. A tool that changes those files makes
//!   "check the run's files" report them as changed, which is the check working.
//! * **A changed tool is not run.** Accepting a change names the hash the owner was shown, and is refused if the file
//!   is no longer that one, or if it never changed.
//! * **Nothing starts without the page asking twice** (a preview, then the start); the core plans again at the start
//!   and refuses a blocked plan, whatever the page sends.
//! * **No environment variables are passed** (the page has no way to send them), so no secret can be put in one.
//!   Arguments are recorded as given: do not put secrets in them.
//! * **At most [`MAX_RUNNING_TOOLS`] tools at once, and one per workspace.** A run is followed only while the app
//!   stays open: SUL cannot stop a program it started, and a program still running when the app closes is no longer
//!   followed, so its record is never written.
//! * A finished run's record is kept in the run's folder beside `run.json` (outside `files`, so it never looks like
//!   a change to the files) and is never overwritten.
//!
//! Everything is tested with a launcher that starts nothing. It has never run a real tool.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::building::builder_view_of;
use super::modding::ctx_of;
use super::{Inner, ManagerService, RootView, root_views};
use crate::builder::recipe;
use crate::builder::tools::{
    self, KnownTool, Redistribution, StoredRunRecords, ToolRecord, ToolRegistration, ToolRunHandle,
    ToolRunPlan, ToolRunRecord, ToolRunSpec, ToolStatus,
};
use crate::journal::fresh_id;
use crate::manifest::now_unix;
use crate::process::{ProcessLauncher, ProcessSpec};
use crate::roots::RootKind;
use crate::{Error, Result};

/// Most tools followed at once. Chosen here to keep the page and the program count bounded, not a measured figure.
pub const MAX_RUNNING_TOOLS: usize = 4;
/// Most finished runs listed as places a tool can work. Chosen here, not measured.
pub const MAX_LISTED_WORKSPACES: usize = 200;

/// A finished recipe run: where a tool can work (the `files` folder the recipe built).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceRef {
    pub recipe_id: String,
    pub root_id: String,
    pub run_id: String,
}

/// A finished recipe run, as listed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkspaceView {
    pub recipe_id: String,
    pub recipe_name: String,
    pub root_id: String,
    pub run_id: String,
}

/// A tool SUL is following.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunningTool {
    /// What to poll. Only meaningful until the app closes.
    pub run: u64,
    pub tool_id: String,
    pub pid: u32,
    pub started_unix: u64,
    pub args: Vec<String>,
    pub workspace: WorkspaceRef,
}

/// Everything the Tools tab shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolsView {
    pub tools: Vec<ToolRecord>,
    /// What was found out about the tools spec 2.4 names, and what was not.
    pub known: Vec<KnownTool>,
    /// The finished recipe runs a tool can work in.
    pub workspaces: Vec<WorkspaceView>,
    /// Mod library folders, for their names.
    pub mod_roots: Vec<RootView>,
    pub running: Vec<RunningTool>,
    /// Anything that could not be listed, with the reason.
    pub run_errors: Vec<String>,
}

/// A tool the owner installed, as typed in.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolRegisterRequest {
    pub id: String,
    pub display_name: String,
    /// As the owner typed it: SUL does not read it from the program.
    pub version: String,
    pub exe_path: PathBuf,
    pub redistribution: Redistribution,
    pub license_note: String,
    pub source_note: String,
}

/// What to run and where. There is no environment: the page cannot send one.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolRunRequest {
    pub tool_id: String,
    pub args: Vec<String>,
    pub settings: BTreeMap<String, String>,
    pub workspace: WorkspaceRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolChecked {
    pub id: String,
    pub status: ToolStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolStarted {
    pub run: RunningTool,
    pub view: ToolsView,
}

/// The state of a followed run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ToolPolled {
    Running {
        pid: u32,
    },
    Finished {
        record: Box<ToolRunRecord>,
        /// The name of the file the record was kept in, in the run's folder.
        saved_as: Option<String>,
        /// Why the record could not be kept, if it could not. The record above is still the true one.
        save_error: Option<String>,
        view: ToolsView,
    },
}

/// A tool SUL started and is waiting for.
pub(super) struct Following {
    handle: ToolRunHandle,
    shown: RunningTool,
    /// The run's folder (it holds `run.json`); the record is kept here.
    run_dir: PathBuf,
}

/// The launcher of a service that was not given one: it starts nothing. Only the app's own constructor (`open`) and
/// [`ManagerService::with_parts_and_launcher`] give a service a launcher that starts programs.
pub(super) struct NoLauncher;

impl ProcessLauncher for NoLauncher {
    fn start(&self, _spec: &ProcessSpec) -> Result<Box<dyn crate::process::Child>> {
        Err(Error::Invalid(
            "this copy of SUL was set up without a way to start programs, so nothing was started"
                .into(),
        ))
    }
}

impl ManagerService {
    /// Everything the Tools tab shows. Reads no tool: a tool's file is only read when it is checked or run.
    pub fn tools_view(&self) -> Result<ToolsView> {
        let g = self.lock()?;
        tools_view_of(&g)
    }

    /// Record a tool the owner installed. The file is hashed now and otherwise left alone.
    pub fn tool_register(&self, req: &ToolRegisterRequest) -> Result<ToolsView> {
        let mut g = self.lock()?;
        let Inner {
            tools,
            protected,
            journal,
            ..
        } = &mut *g;
        journal.run(
            &fresh_id("tool"),
            "tool_register",
            &format!("tool {} at {}", req.id, req.exe_path.display()),
            || {
                tools
                    .register(
                        protected,
                        &ToolRegistration {
                            id: req.id.clone(),
                            display_name: req.display_name.clone(),
                            version: req.version.clone(),
                            exe_path: req.exe_path.clone(),
                            redistribution: req.redistribution,
                            license_note: req.license_note.clone(),
                            source_note: req.source_note.clone(),
                        },
                    )
                    .map(|_| ())
            },
        )?;
        tools_view_of(&g)
    }

    /// Whether a tool's file is still the recorded one. Reads the file and changes nothing.
    pub fn tool_check(&self, id: &str) -> Result<ToolChecked> {
        let g = self.lock()?;
        Ok(ToolChecked {
            id: id.to_owned(),
            status: g.tools.check(id)?,
        })
    }

    /// The owner accepts the file now at a tool's path as the tool, at `new_version`. `seen_sha256` is the hash a
    /// check showed them. Refused when the file is the recorded one (there is nothing to accept), when it is missing,
    /// or when it is no longer the file they were shown.
    pub fn tool_accept_change(
        &self,
        id: &str,
        new_version: &str,
        seen_sha256: &str,
    ) -> Result<ToolsView> {
        let mut g = self.lock()?;
        match g.tools.check(id)? {
            ToolStatus::Matches => {
                return Err(Error::Invalid(
                    "the tool's file is the one that was recorded, so there is nothing to accept"
                        .into(),
                ));
            }
            ToolStatus::Missing => {
                return Err(Error::Invalid(
                    "the tool's file is missing, so there is nothing to accept".into(),
                ));
            }
            ToolStatus::Changed { .. } => {}
        }
        let Inner {
            tools,
            protected,
            journal,
            ..
        } = &mut *g;
        journal.run(
            &fresh_id("tool"),
            "tool_accept_change",
            &format!("tool {id} as version {new_version}"),
            || tools.accept_change(protected, id, new_version, Some(seen_sha256)),
        )?;
        tools_view_of(&g)
    }

    /// Exactly what a run would start, with anything that would stop it. Starts nothing.
    pub fn tool_plan_run(&self, req: &ToolRunRequest) -> Result<ToolRunPlan> {
        let g = self.lock()?;
        let (plan, _, _) = plan_of(&g, req)?;
        Ok(plan)
    }

    /// Start a tool on a finished recipe run's files. The plan is made again here and a blocked plan is refused.
    /// Returns at once: poll the run until it has finished.
    pub fn tool_start_run(&self, req: &ToolRunRequest) -> Result<ToolStarted> {
        let mut g = self.lock()?;
        let (plan, spec, run_dir) = plan_of(&g, req)?;
        if plan.blocked() {
            return Err(Error::Invalid(format!(
                "the tool cannot be run: {}",
                plan.problems.join("; ")
            )));
        }
        let Inner {
            tools,
            protected,
            journal,
            launcher,
            running,
            next_run,
            ..
        } = &mut *g;
        let handle = journal.run(
            &fresh_id("tool"),
            "tool_run_start",
            &format!(
                "tool {} in run {} of {}",
                req.tool_id, req.workspace.run_id, req.workspace.recipe_id
            ),
            || tools::start_tool_run(tools, protected, &**launcher, &spec),
        )?;
        let run = *next_run;
        *next_run += 1;
        let shown = RunningTool {
            run,
            tool_id: req.tool_id.clone(),
            pid: handle.pid(),
            started_unix: now_unix(),
            args: req.args.clone(),
            workspace: req.workspace.clone(),
        };
        running.insert(
            run,
            Following {
                handle,
                shown: shown.clone(),
                run_dir,
            },
        );
        Ok(ToolStarted {
            run: shown,
            view: tools_view_of(&g)?,
        })
    }

    /// Has a followed tool finished? Once it has, its record is kept and the run is no longer followed.
    pub fn tool_poll_run(&self, run: u64) -> Result<ToolPolled> {
        let mut g = self.lock()?;
        let Inner {
            protected,
            journal,
            running,
            ..
        } = &mut *g;
        let Some(f) = running.get_mut(&run) else {
            return Err(Error::Invalid(format!(
                "no tool run {run} is being followed (SUL does not follow a program across a restart of the app)"
            )));
        };
        let polled = f.handle.poll();
        let record = match polled {
            Ok(None) => {
                return Ok(ToolPolled::Running { pid: f.shown.pid });
            }
            Ok(Some(record)) => record,
            Err(e) => {
                running.remove(&run);
                return Err(Error::Invalid(format!(
                    "SUL lost track of tool run {run}: {e}. The program may still be running, and nothing was recorded"
                )));
            }
        };
        let done = running.remove(&run).expect("it was just found");
        let kept = journal.run(
            &fresh_id("tool"),
            "tool_run_record",
            &format!(
                "record of tool {} in {}",
                record.tool_id,
                done.run_dir.display()
            ),
            || tools::save_run_record(protected, &done.run_dir, &record),
        );
        let (saved_as, save_error) = match kept {
            Ok(path) => (
                path.file_name().and_then(|n| n.to_str()).map(str::to_owned),
                None,
            ),
            Err(e) => (None, Some(e.to_string())),
        };
        Ok(ToolPolled::Finished {
            record: Box::new(record),
            saved_as,
            save_error,
            view: tools_view_of(&g)?,
        })
    }

    /// The records kept for a finished recipe run, newest first. Reads only.
    pub fn tool_run_records(&self, workspace: &WorkspaceRef) -> Result<StoredRunRecords> {
        let g = self.lock()?;
        let (run_dir, _files) = recipe::run_folders(
            &ctx_of(&g),
            &workspace.root_id,
            &workspace.recipe_id,
            &workspace.run_id,
        )?;
        tools::list_run_records(&run_dir)
    }
}

/// The plan for a request, the spec the core needs, and the run's folder. A workspace that is not a finished
/// recipe run is refused here, before the core is asked anything.
fn plan_of(g: &Inner, req: &ToolRunRequest) -> Result<(ToolRunPlan, ToolRunSpec, PathBuf)> {
    let w = &req.workspace;
    let (run_dir, files) = recipe::run_folders(&ctx_of(g), &w.root_id, &w.recipe_id, &w.run_id)?;
    let spec = ToolRunSpec {
        tool_id: req.tool_id.clone(),
        args: req.args.clone(),
        env: vec![],
        settings: req.settings.clone(),
        workspace: files,
    };
    let mut plan = tools::plan_tool_run(&g.tools, &g.protected, &spec)?;
    if g.running.len() >= MAX_RUNNING_TOOLS {
        plan.problems.push(format!(
            "{MAX_RUNNING_TOOLS} tools are already running; wait for one to finish"
        ));
    }
    if g.running.values().any(|f| f.shown.workspace == *w) {
        plan.problems
            .push("a tool is already running on this run's files; wait for it to finish".into());
    }
    Ok((plan, spec, run_dir))
}

fn tools_view_of(g: &Inner) -> Result<ToolsView> {
    let builder = builder_view_of(g)?;
    let mut run_errors = builder.run_errors;
    let mut workspaces = Vec::new();
    for r in &builder.recipes {
        for run in &r.runs {
            workspaces.push(WorkspaceView {
                recipe_id: r.recipe.id.clone(),
                recipe_name: r.recipe.name.clone(),
                root_id: run.root_id.clone(),
                run_id: run.run_id.clone(),
            });
        }
    }
    if workspaces.len() > MAX_LISTED_WORKSPACES {
        run_errors.push(format!(
            "there are {} finished runs; only the first {MAX_LISTED_WORKSPACES} are listed",
            workspaces.len()
        ));
        workspaces.truncate(MAX_LISTED_WORKSPACES);
    }
    Ok(ToolsView {
        tools: g.tools.tools().to_vec(),
        known: tools::known_tools().to_vec(),
        workspaces,
        mod_roots: root_views(g)
            .into_iter()
            .filter(|r| r.root.kind == RootKind::Mods)
            .collect(),
        running: g.running.values().map(|f| f.shown.clone()).collect(),
        run_errors,
    })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::VecDeque;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::adapters::AdapterRegistry;
    use crate::adapters::fixtures::MadeUpGame;
    use crate::builder::recipe::{RECIPE_SCHEMA, Recipe, Step};
    use crate::manager::ModSource;
    use crate::process::ExitInfo;
    use crate::process::fake::{FakeLauncher, Script};
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
        exe: PathBuf,
        started: Arc<Mutex<Vec<ProcessSpec>>>,
        scripts: Arc<Mutex<VecDeque<Script>>>,
        fail: Arc<Mutex<Option<String>>>,
    }

    fn put(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn service(base: &Path, launcher: Box<dyn ProcessLauncher + Send>) -> ManagerService {
        let mut adapters = AdapterRegistry::new();
        adapters.register(Box::new(MadeUpGame::new())).unwrap();
        ManagerService::with_parts_and_launcher(
            &base.join("state"),
            Box::new(Disk(Cell::new(u64::MAX / 2))),
            adapters,
            launcher,
        )
        .unwrap()
    }

    /// A service with a mod library, one stored mod, one saved recipe and one tool program on disk (not yet recorded).
    fn fx() -> Fx {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(base.join("modlib")).unwrap();
        std::fs::create_dir_all(base.join("ssd")).unwrap();
        let launcher = FakeLauncher::new();
        let (started, scripts, fail) = (
            launcher.started.clone(),
            launcher.scripts.clone(),
            launcher.fail_with.clone(),
        );
        let svc = service(&base, Box::new(launcher));
        svc.add_root(
            "modlib",
            "Mod library",
            &base.join("modlib"),
            RootKind::Mods,
        )
        .unwrap();
        svc.add_root("ssd", "Fast drive", &base.join("ssd"), RootKind::Builds)
            .unwrap();
        let folder = base.join("mods_src/louder-1.0");
        put(&folder.join("data/a.bin"), b"aaaa");
        put(&folder.join("data/b.bin"), b"bbbb");
        svc.import_mod(
            &ModSource {
                game_id: GAME.into(),
                mod_id: "louder".into(),
                name: "Louder".into(),
                version: "1.0".into(),
                requires: vec![],
                compatible_versions: vec![],
                source: "made by hand".into(),
                license_note: "mine".into(),
                path: folder,
            },
            "modlib",
        )
        .unwrap();
        svc.recipe_save(
            &Recipe {
                schema: RECIPE_SCHEMA,
                id: "loud".into(),
                name: "Loud".into(),
                game_id: GAME.into(),
                description: String::new(),
                steps: vec![Step::LayerMod {
                    mod_id: "louder".into(),
                    version: "1.0".into(),
                    into: String::new(),
                }],
            },
            None,
        )
        .unwrap();
        let exe = base.join("tools/audio.exe");
        put(&exe, b"pretend program");
        Fx {
            _dir: dir,
            base,
            svc,
            exe,
            started,
            scripts,
            fail,
        }
    }

    fn registration(f: &Fx) -> ToolRegisterRequest {
        ToolRegisterRequest {
            id: "audio".into(),
            display_name: "Audio tool".into(),
            version: "1.0".into(),
            exe_path: f.exe.clone(),
            redistribution: Redistribution::Unknown,
            license_note: String::new(),
            source_note: "downloaded by me".into(),
        }
    }

    /// Record the tool and build one run of the recipe: the place a tool works.
    fn with_tool_and_run(f: &Fx) -> WorkspaceRef {
        f.svc.tool_register(&registration(f)).unwrap();
        new_run(f)
    }

    fn new_run(f: &Fx) -> WorkspaceRef {
        let ran = f.svc.recipe_run("loud", "modlib").unwrap();
        WorkspaceRef {
            recipe_id: "loud".into(),
            root_id: "modlib".into(),
            run_id: ran.record.run_id,
        }
    }

    fn request(w: &WorkspaceRef) -> ToolRunRequest {
        ToolRunRequest {
            tool_id: "audio".into(),
            args: vec!["--in".into(), "a.bin".into()],
            settings: BTreeMap::from([("bank".into(), "music".into())]),
            workspace: w.clone(),
        }
    }

    fn files_of(f: &Fx, w: &WorkspaceRef) -> PathBuf {
        f.base
            .join("modlib/.workspaces")
            .join(&w.recipe_id)
            .join(&w.run_id)
            .join("files")
    }

    fn exits(code: i32, polls_before_exit: usize) -> Script {
        Script {
            polls_before_exit,
            exit: ExitInfo {
                code: Some(code),
                success: code == 0,
            },
            output: None,
        }
    }

    fn all_files(dir: &Path) -> Vec<String> {
        let mut out = vec![];
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    out.push(
                        p.strip_prefix(dir)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn a_new_service_has_no_tools_lists_the_two_known_tools_honestly_and_has_nowhere_to_work() {
        let f = fx();
        let v = f.svc.tools_view().unwrap();
        assert!(v.tools.is_empty() && v.running.is_empty() && v.workspaces.is_empty());
        assert_eq!(
            v.known.iter().map(|k| k.id).collect::<Vec<_>>(),
            ["hd2-audio-modder", "wwise"]
        );
        assert!(
            v.known
                .iter()
                .all(|k| k.status == crate::capability::Status::NotInvestigated)
        );
        assert_eq!(
            v.mod_roots.len(),
            1,
            "only the mod library, not the builds root"
        );
        assert_eq!(v.mod_roots[0].root.id, "modlib");
    }

    #[test]
    fn registering_records_the_tool_and_leaves_it_alone_and_survives_a_restart() {
        let f = fx();
        let v = f.svc.tool_register(&registration(&f)).unwrap();
        assert_eq!(v.tools.len(), 1);
        let t = &v.tools[0];
        assert_eq!((t.id.as_str(), t.version.as_str()), ("audio", "1.0"));
        assert_eq!(t.exe_size, 15);
        assert_eq!(t.exe_sha256.len(), 64);
        assert_eq!(std::fs::read(&f.exe).unwrap(), b"pretend program");
        assert!(
            f.started.lock().unwrap().is_empty(),
            "registering starts nothing"
        );

        let again = service(&f.base, Box::new(FakeLauncher::new()));
        assert_eq!(again.tools_view().unwrap().tools, v.tools);
    }

    #[test]
    fn registering_keeps_every_word_the_owner_typed() {
        let f = fx();
        let mut r = registration(&f);
        r.display_name = "The audio tool".into();
        r.version = "v3 (my build)".into();
        r.redistribution = Redistribution::NotPermitted;
        r.license_note = "GPL, I think, not checked".into();
        r.source_note = "from a forum post".into();
        let v = f.svc.tool_register(&r).unwrap();
        let t = &v.tools[0];
        assert_eq!(t.display_name, "The audio tool");
        assert_eq!(t.version, "v3 (my build)");
        assert_eq!(t.redistribution, Redistribution::NotPermitted);
        assert_eq!(t.license_note, "GPL, I think, not checked");
        assert_eq!(t.source_note, "from a forum post");
        assert_eq!(t.exe_path, f.exe);
    }

    #[test]
    fn a_registration_that_is_wrong_is_refused_and_the_list_is_left_as_it_was() {
        let f = fx();
        f.svc.tool_register(&registration(&f)).unwrap();
        let before = f.svc.tools_view().unwrap().tools;
        let mut r = registration(&f);
        assert!(f.svc.tool_register(&r).is_err(), "the same id twice");
        r.id = "other".into();
        r.exe_path = PathBuf::from("relative/tool.exe");
        assert!(f.svc.tool_register(&r).is_err(), "a relative path");
        r.exe_path = f.base.join("tools");
        assert!(f.svc.tool_register(&r).is_err(), "a folder");
        r.exe_path = f.base.join("tools/missing.exe");
        assert!(f.svc.tool_register(&r).is_err(), "no such file");
        r.exe_path = f.exe.clone();
        r.redistribution = Redistribution::Permitted;
        let e = f.svc.tool_register(&r).unwrap_err().to_string();
        assert!(
            e.contains("licence"),
            "permitted needs the permission written down: {e}"
        );
        r.redistribution = Redistribution::Unknown;
        r.id = "Not An Id!".into();
        assert!(f.svc.tool_register(&r).is_err(), "a bad id");
        assert_eq!(f.svc.tools_view().unwrap().tools, before);
    }

    #[test]
    fn a_tool_inside_a_protected_game_install_is_refused() {
        let f = fx();
        let install = f.base.join("install");
        put(&install.join("game.exe"), b"game");
        f.svc
            .protect_installs(std::slice::from_ref(&install))
            .unwrap();
        let mut r = registration(&f);
        r.exe_path = install.join("game.exe");
        let e = f.svc.tool_register(&r).unwrap_err().to_string();
        assert!(e.contains("protected game install"), "{e}");
        assert!(f.svc.tools_view().unwrap().tools.is_empty());
    }

    #[test]
    fn a_check_says_matches_changed_or_missing_and_changes_nothing() {
        let f = fx();
        let rec = f
            .svc
            .tool_register(&registration(&f))
            .unwrap()
            .tools
            .remove(0);
        assert_eq!(
            f.svc.tool_check("audio").unwrap().status,
            ToolStatus::Matches
        );
        std::fs::write(&f.exe, b"version two").unwrap();
        let ToolStatus::Changed {
            exe_sha256,
            exe_size,
        } = f.svc.tool_check("audio").unwrap().status
        else {
            panic!("expected changed");
        };
        assert_ne!(exe_sha256, rec.exe_sha256);
        assert_eq!(exe_size, 11);
        assert_eq!(
            f.svc.tools_view().unwrap().tools[0],
            rec,
            "a check records nothing"
        );
        std::fs::remove_file(&f.exe).unwrap();
        assert_eq!(
            f.svc.tool_check("audio").unwrap().status,
            ToolStatus::Missing
        );
        assert!(f.svc.tool_check("nope").is_err());
    }

    #[test]
    fn accepting_needs_a_changed_file_and_the_hash_the_owner_was_shown() {
        let f = fx();
        let first = f
            .svc
            .tool_register(&registration(&f))
            .unwrap()
            .tools
            .remove(0);

        // Nothing changed: nothing to accept, nothing recorded.
        let e = f
            .svc
            .tool_accept_change("audio", "2.0", &first.exe_sha256)
            .unwrap_err()
            .to_string();
        assert!(e.contains("nothing to accept"), "{e}");

        std::fs::write(&f.exe, b"version two").unwrap();
        let ToolStatus::Changed {
            exe_sha256: seen, ..
        } = f.svc.tool_check("audio").unwrap().status
        else {
            panic!("expected changed");
        };

        // A hash that is not the file's: refused, and the record is unchanged.
        let e = f
            .svc
            .tool_accept_change("audio", "2.0", &first.exe_sha256)
            .unwrap_err()
            .to_string();
        assert!(e.contains("not the one you were shown"), "{e}");
        assert_eq!(f.svc.tools_view().unwrap().tools[0], first);

        let v = f.svc.tool_accept_change("audio", "2.0", &seen).unwrap();
        let now = &v.tools[0];
        assert_eq!(
            (now.version.as_str(), now.exe_sha256.as_str()),
            ("2.0", seen.as_str())
        );
        assert_eq!(now.history.len(), 1);
        assert_eq!(now.history[0].exe_sha256, first.exe_sha256);
        assert_eq!(
            f.svc.tool_check("audio").unwrap().status,
            ToolStatus::Matches
        );

        // A missing file cannot be accepted either.
        std::fs::remove_file(&f.exe).unwrap();
        let e = f
            .svc
            .tool_accept_change("audio", "3.0", &seen)
            .unwrap_err()
            .to_string();
        assert!(e.contains("missing"), "{e}");
    }

    #[test]
    fn the_finished_runs_of_the_recipes_are_the_places_a_tool_can_work() {
        let f = fx();
        let w = new_run(&f);
        let v = f.svc.tools_view().unwrap();
        assert_eq!(
            v.workspaces,
            [WorkspaceView {
                recipe_id: "loud".into(),
                recipe_name: "Loud".into(),
                root_id: "modlib".into(),
                run_id: w.run_id.clone(),
            }]
        );
    }

    #[test]
    fn a_plan_shows_what_would_start_in_the_runs_files_folder_and_starts_and_writes_nothing() {
        let f = fx();
        let w = with_tool_and_run(&f);
        let before = all_files(&f.base.join("modlib"));
        let plan = f.svc.tool_plan_run(&request(&w)).unwrap();
        assert!(!plan.blocked(), "{:?}", plan.problems);
        assert_eq!(plan.exe, f.exe);
        assert_eq!(plan.args, ["--in", "a.bin"]);
        assert_eq!(plan.cwd, files_of(&f, &w));
        assert_eq!(plan.tool_version, "1.0");
        assert!(f.started.lock().unwrap().is_empty());
        assert_eq!(all_files(&f.base.join("modlib")), before);
    }

    #[test]
    fn a_place_that_is_not_a_finished_run_is_refused_before_anything_else() {
        let f = fx();
        let w = with_tool_and_run(&f);
        let bad = |change: &dyn Fn(&mut WorkspaceRef)| {
            let mut x = w.clone();
            change(&mut x);
            let r = request(&x);
            assert!(f.svc.tool_plan_run(&r).is_err(), "{x:?}");
            assert!(f.svc.tool_start_run(&r).is_err(), "{x:?}");
            assert!(f.svc.tool_run_records(&x).is_err(), "{x:?}");
        };
        bad(&|x| x.run_id = "run-nope".into());
        bad(&|x| x.run_id = "../loud".into());
        bad(&|x| x.recipe_id = "other".into());
        bad(&|x| x.root_id = "ssd".into());
        bad(&|x| x.root_id = "nowhere".into());
        assert!(f.started.lock().unwrap().is_empty());
    }

    #[test]
    fn an_unknown_tool_is_refused() {
        let f = fx();
        let w = new_run(&f);
        assert!(f.svc.tool_plan_run(&request(&w)).is_err());
        assert!(f.svc.tool_start_run(&request(&w)).is_err());
    }

    #[test]
    fn a_changed_or_missing_tool_is_not_run_and_the_plan_says_why() {
        let f = fx();
        let w = with_tool_and_run(&f);
        std::fs::write(&f.exe, b"version two").unwrap();
        let plan = f.svc.tool_plan_run(&request(&w)).unwrap();
        assert!(plan.blocked());
        assert!(
            plan.problems
                .iter()
                .any(|p| p.contains("not the one that was recorded"))
        );
        let e = f.svc.tool_start_run(&request(&w)).unwrap_err().to_string();
        assert!(e.contains("cannot be run") && e.contains("recorded"), "{e}");
        assert!(f.started.lock().unwrap().is_empty(), "nothing was started");

        std::fs::remove_file(&f.exe).unwrap();
        assert!(f.svc.tool_start_run(&request(&w)).is_err());
        assert!(f.started.lock().unwrap().is_empty());
        assert!(f.svc.tools_view().unwrap().running.is_empty());
    }

    #[test]
    fn a_start_passes_exactly_the_plan_and_no_environment_and_is_followed() {
        let f = fx();
        let w = with_tool_and_run(&f);
        let s = f.svc.tool_start_run(&request(&w)).unwrap();
        assert_eq!(s.run.tool_id, "audio");
        assert_eq!(s.run.pid, 4242);
        assert_eq!(s.run.workspace, w);
        assert_eq!(
            s.run.args,
            ["--in", "a.bin"],
            "what is shown as running is what was started"
        );
        assert_eq!(s.view.running, std::slice::from_ref(&s.run));
        let started = f.started.lock().unwrap();
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].exe, f.exe);
        assert_eq!(started[0].args, ["--in", "a.bin"]);
        assert_eq!(started[0].cwd, files_of(&f, &w));
        assert!(
            started[0].env.is_empty(),
            "the page cannot send an environment"
        );
    }

    #[test]
    fn only_one_tool_runs_on_a_workspace_and_only_a_few_at_a_time() {
        let f = fx();
        let w = with_tool_and_run(&f);
        f.svc.tool_start_run(&request(&w)).unwrap();
        let again = f.svc.tool_plan_run(&request(&w)).unwrap();
        assert!(
            again
                .problems
                .iter()
                .any(|p| p.contains("already running on this run"))
        );
        assert!(f.svc.tool_start_run(&request(&w)).is_err());
        assert_eq!(f.started.lock().unwrap().len(), 1);

        for _ in 1..MAX_RUNNING_TOOLS {
            let other = new_run(&f);
            f.svc.tool_start_run(&request(&other)).unwrap();
        }
        assert_eq!(f.svc.tools_view().unwrap().running.len(), MAX_RUNNING_TOOLS);
        let last = new_run(&f);
        let plan = f.svc.tool_plan_run(&request(&last)).unwrap();
        assert!(
            plan.problems
                .iter()
                .any(|p| p.contains("already running; wait")),
            "{:?}",
            plan.problems
        );
        assert!(f.svc.tool_start_run(&request(&last)).is_err());
        assert_eq!(f.started.lock().unwrap().len(), MAX_RUNNING_TOOLS);
    }

    #[test]
    fn polling_reports_running_then_a_record_of_what_the_tool_changed_which_is_kept_beside_the_run()
    {
        let f = fx();
        let w = with_tool_and_run(&f);
        f.scripts.lock().unwrap().push_back(exits(0, 2));
        let files = files_of(&f, &w);
        std::fs::write(files.join("data/b.bin"), b"to be edited").unwrap();
        let started = f.svc.tool_start_run(&request(&w)).unwrap();
        let run = started.run.run;

        // The "tool" works: one file added, one changed, one removed.
        std::fs::write(files.join("data/new.bin"), b"new").unwrap();
        std::fs::write(files.join("data/b.bin"), b"edited").unwrap();
        std::fs::remove_file(files.join("data/a.bin")).unwrap();

        assert_eq!(
            f.svc.tool_poll_run(run).unwrap(),
            ToolPolled::Running { pid: 4242 }
        );
        assert_eq!(
            f.svc.tool_poll_run(run).unwrap(),
            ToolPolled::Running { pid: 4242 }
        );
        let ToolPolled::Finished {
            record,
            saved_as,
            save_error,
            view,
        } = f.svc.tool_poll_run(run).unwrap()
        else {
            panic!("expected it to have finished");
        };
        assert!(record.success && record.exit_code == Some(0));
        assert_eq!(record.args, ["--in", "a.bin"]);
        assert!(record.env_names.is_empty());
        assert_eq!(record.settings["bank"], "music");
        assert_eq!(record.tool_version, "1.0");
        assert_eq!(
            record
                .added
                .iter()
                .map(|a| a.path.as_str())
                .collect::<Vec<_>>(),
            ["data/new.bin"]
        );
        assert_eq!(
            record
                .changed
                .iter()
                .map(|c| c.path.as_str())
                .collect::<Vec<_>>(),
            ["data/b.bin"]
        );
        assert_eq!(record.removed, ["data/a.bin"]);
        assert!(record.limits.contains("anything it wrote elsewhere"));
        assert!(save_error.is_none());
        let name = saved_as.expect("the record was kept");
        assert!(
            view.running.is_empty(),
            "a finished run is no longer followed"
        );

        // The record is beside run.json, not among the files the recipe built.
        let run_dir = files.parent().unwrap();
        assert!(run_dir.join(".sul-tool-runs").join(&name).is_file());
        assert!(!all_files(&files).iter().any(|p| p.contains(".sul-")));

        // Checking the run's files reports the tool's work (the check working) and not the record.
        let check = f
            .svc
            .recipe_verify_run("loud", "modlib", &w.run_id)
            .unwrap();
        assert_eq!(check.extra, ["data/new.bin"]);
        assert_eq!(check.modified, ["data/b.bin"]);
        assert_eq!(check.missing, ["data/a.bin"]);

        // The record can be listed afterwards, and the run is not followed any more.
        let listed = f.svc.tool_run_records(&w).unwrap();
        assert_eq!(listed.records.len(), 1);
        assert_eq!(listed.records[0].name, name);
        assert_eq!(listed.records[0].record, *record);
        let e = f.svc.tool_poll_run(run).unwrap_err().to_string();
        assert!(e.contains("no tool run"), "{e}");
    }

    #[test]
    fn a_tool_that_fails_is_recorded_as_failed_and_two_runs_keep_two_records() {
        let f = fx();
        let w = with_tool_and_run(&f);
        f.scripts.lock().unwrap().push_back(exits(3, 0));
        let r1 = f.svc.tool_start_run(&request(&w)).unwrap().run.run;
        let ToolPolled::Finished { record, .. } = f.svc.tool_poll_run(r1).unwrap() else {
            panic!("expected it to have finished");
        };
        assert!(!record.success);
        assert_eq!(record.exit_code, Some(3));

        f.scripts.lock().unwrap().push_back(exits(0, 0));
        let r2 = f.svc.tool_start_run(&request(&w)).unwrap().run.run;
        assert_ne!(r1, r2);
        f.svc.tool_poll_run(r2).unwrap();
        let listed = f.svc.tool_run_records(&w).unwrap();
        assert_eq!(
            listed.records.len(),
            2,
            "the earlier record was not overwritten"
        );
        assert_eq!(listed.total, 2);
    }

    #[test]
    fn a_record_that_cannot_be_kept_is_still_returned_and_says_so() {
        let f = fx();
        let w = with_tool_and_run(&f);
        let run_dir = files_of(&f, &w).parent().unwrap().to_path_buf();
        // The record folder's name is taken by a file, so nothing can be written there.
        std::fs::write(run_dir.join(".sul-tool-runs"), "in the way").unwrap();
        f.scripts.lock().unwrap().push_back(exits(0, 0));
        let run = f.svc.tool_start_run(&request(&w)).unwrap().run.run;
        let ToolPolled::Finished {
            record,
            saved_as,
            save_error,
            ..
        } = f.svc.tool_poll_run(run).unwrap()
        else {
            panic!("expected it to have finished");
        };
        assert!(record.success, "the record is still the true one");
        assert!(saved_as.is_none());
        assert!(save_error.is_some_and(|e| !e.is_empty()));
    }

    #[test]
    fn a_start_the_launcher_refuses_is_an_error_and_follows_nothing_and_leaves_no_half_done_operation()
     {
        let f = fx();
        let w = with_tool_and_run(&f);
        *f.fail.lock().unwrap() = Some("cannot start: access denied".into());
        let e = f.svc.tool_start_run(&request(&w)).unwrap_err().to_string();
        assert!(e.contains("access denied"), "{e}");
        assert!(f.svc.tools_view().unwrap().running.is_empty());
        assert!(f.svc.view().unwrap().interrupted.is_empty());
        // The workspace is free to try again.
        *f.fail.lock().unwrap() = None;
        assert!(f.svc.tool_start_run(&request(&w)).is_ok());
    }

    #[test]
    fn a_service_made_without_a_launcher_starts_nothing() {
        let f = fx();
        let w = with_tool_and_run(&f);
        let plain = ManagerService::with_parts(
            &f.base.join("state"),
            Box::new(Disk(Cell::new(u64::MAX / 2))),
            AdapterRegistry::new(),
        )
        .unwrap();
        let e = plain.tool_start_run(&request(&w)).unwrap_err().to_string();
        assert!(e.contains("without a way to start programs"), "{e}");
        assert!(plain.tools_view().unwrap().running.is_empty());
    }

    #[test]
    fn every_change_is_in_the_journal_and_none_is_left_unfinished() {
        let f = fx();
        let w = with_tool_and_run(&f);
        std::fs::write(&f.exe, b"version two").unwrap();
        let ToolStatus::Changed { exe_sha256, .. } = f.svc.tool_check("audio").unwrap().status
        else {
            panic!("expected changed");
        };
        f.svc
            .tool_accept_change("audio", "2.0", &exe_sha256)
            .unwrap();
        let run = f.svc.tool_start_run(&request(&w)).unwrap().run.run;
        f.svc.tool_poll_run(run).unwrap();
        let log = std::fs::read_to_string(f.base.join("state/journal.jsonl")).unwrap();
        for op in [
            "tool_register",
            "tool_accept_change",
            "tool_run_start",
            "tool_run_record",
        ] {
            assert!(log.contains(op), "{op} is not in the journal:\n{log}");
        }
        assert!(f.svc.view().unwrap().interrupted.is_empty());
    }

    /// A launcher whose programs start but can no longer be waited for.
    struct LosingLauncher;
    struct LostChild;
    impl crate::process::Child for LostChild {
        fn pid(&self) -> u32 {
            7
        }
        fn poll(&mut self) -> Result<Option<ExitInfo>> {
            Err(Error::Invalid("the wait failed".into()))
        }
        fn captured(&self) -> Option<crate::process::Captured> {
            None
        }
    }
    impl ProcessLauncher for LosingLauncher {
        fn start(&self, _spec: &ProcessSpec) -> Result<Box<dyn crate::process::Child>> {
            Ok(Box::new(LostChild))
        }
    }

    #[test]
    fn a_run_that_can_no_longer_be_waited_for_is_dropped_with_an_honest_message_and_no_record() {
        let f = fx();
        let w = with_tool_and_run(&f);
        let svc = service(&f.base, Box::new(LosingLauncher));
        let run = svc.tool_start_run(&request(&w)).unwrap().run.run;
        assert_eq!(svc.tools_view().unwrap().running.len(), 1);
        let e = svc.tool_poll_run(run).unwrap_err().to_string();
        assert!(
            e.contains("lost track")
                && e.contains("may still be running")
                && e.contains("wait failed"),
            "{e}"
        );
        assert!(
            svc.tools_view().unwrap().running.is_empty(),
            "it is no longer said to be followed"
        );
        let e = svc.tool_poll_run(run).unwrap_err().to_string();
        assert!(e.contains("is being followed"), "{e}");
        assert!(
            svc.tool_run_records(&w).unwrap().records.is_empty(),
            "nothing was recorded, and the page was told so"
        );
        // The workspace is free again.
        assert!(svc.tool_start_run(&request(&w)).is_ok());
    }

    #[test]
    fn a_damaged_run_listing_is_passed_on_to_the_tools_page() {
        let f = fx();
        f.svc
            .recipe_save(
                &Recipe {
                    schema: RECIPE_SCHEMA,
                    id: "hello".into(),
                    name: "Hello".into(),
                    game_id: GAME.into(),
                    description: String::new(),
                    steps: vec![Step::LayerMod {
                        mod_id: "louder".into(),
                        version: "1.0".into(),
                        into: String::new(),
                    }],
                },
                None,
            )
            .unwrap();
        // The recipe's workspace name is taken by a plain file, so its runs cannot be listed.
        put(&f.base.join("modlib/.workspaces/hello"), b"not a folder");
        let v = f.svc.tools_view().unwrap();
        assert_eq!(v.run_errors.len(), 1, "{:?}", v.run_errors);
        assert!(v.run_errors[0].contains("hello"), "{:?}", v.run_errors);
    }

    #[test]
    fn the_list_of_places_a_tool_can_work_is_bounded_and_says_so() {
        let f = fx();
        for _ in 0..=MAX_LISTED_WORKSPACES {
            new_run(&f);
        }
        let v = f.svc.tools_view().unwrap();
        assert_eq!(v.workspaces.len(), MAX_LISTED_WORKSPACES);
        let n = MAX_LISTED_WORKSPACES + 1;
        assert!(
            v.run_errors
                .iter()
                .any(|e| e.contains(&format!("{n} finished runs"))
                    && e.contains(&format!("first {MAX_LISTED_WORKSPACES}"))),
            "{:?}",
            v.run_errors
        );
    }

    #[test]
    fn what_is_said_to_be_followed_is_lost_with_the_app_and_the_service_says_so() {
        let f = fx();
        let w = with_tool_and_run(&f);
        let run = f.svc.tool_start_run(&request(&w)).unwrap().run.run;
        let restarted = service(&f.base, Box::new(FakeLauncher::new()));
        assert!(restarted.tools_view().unwrap().running.is_empty());
        let e = restarted.tool_poll_run(run).unwrap_err().to_string();
        assert!(e.contains("restart"), "{e}");
    }
}
