//! Launch and runtime manager (spec 1.4): start a chosen build, follow it while it runs, and say
//! plainly why it did not start or why it ended.
//!
//! What it does:
//!
//! * Starts only a build that is **Ready** in the registry, through its game's adapter, and only the
//!   program the adapter's plan names, checked to lie inside the build folder (see [`crate::process`]).
//! * For a build SUL manages, the program must still match the hash recorded for it; a changed program
//!   is refused with an explanation. For an install that stays where it is (the owner's original
//!   copy), a program that differs from the recorded baseline is reported as a warning and the launch
//!   goes ahead: the game may simply have updated. (The game-file risk policy in [`crate::trust::policy`]
//!   is not connected here yet; see decision 0007.)
//! * One running instance per game. A second launch of the same game is refused, with the build that
//!   is running named.
//! * Follows the process and keeps a short history. When a program ends it records the exit code and,
//!   if it ended quickly with an error, an explanation that says what is known and what is not.
//! * **Diagnostic output is opt-in** per launch, capped, passed through the log redactor and written to
//!   its own file; by default the program's output is discarded.
//!
//! What it does not do: stop a program, restrict what it can do, check that the game works, or decide
//! anything about network or launcher integration. Per-build network integration is declared only when
//! implemented and tested (spec 1.4); none is.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::adapters::AdapterRegistry;
use crate::builds::{BuildRegistry, Ctx, Location};
use crate::fsops::{self, ProtectedRoots};
use crate::journal::fresh_id;
use crate::logging::{Logger, redact_text};
use crate::manifest::{State, hash_file, now_unix};
use crate::process::{
    Child, DEFAULT_CAPTURE_BYTES, ExitInfo, ProcessLauncher, ProcessSpec, resolve_plan,
};
use crate::{Error, Result};

/// How many finished launches are remembered.
pub const HISTORY_LIMIT: usize = 50;
/// A program that fails within this many seconds is described as failing at start-up.
pub const EARLY_EXIT_SECONDS: u64 = 15;

#[derive(Debug, Clone)]
pub struct LaunchRequest {
    pub build_id: String,
    /// Keep the program's output (capped, redacted) in a log file. Off unless the owner asks.
    pub capture_output: bool,
}

/// Why a launch did not happen. Each has a sentence for the owner ([`NotStarted::explain`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum NotStarted {
    NoSuchBuild,
    AlreadyRunning { build_id: String },
    BuildNotReady { state: State },
    NoAdapter,
    AdapterRefused { why: String },
    PlanRejected { why: String },
    ProgramChanged { why: String },
    CouldNotStart { why: String },
}

impl NotStarted {
    pub fn explain(&self) -> String {
        match self {
            Self::NoSuchBuild => "SUL has no build with that id.".into(),
            Self::AlreadyRunning { build_id } => format!(
                "This game is already running (build {build_id}). SUL starts one copy of a game at a time."
            ),
            Self::BuildNotReady { state } => format!(
                "The build is not ready (it is {state:?}). Verify or repair it first; SUL only starts a build that has been checked."
            ),
            Self::NoAdapter => "There is no adapter registered for this game, so SUL does not know how to start it.".into(),
            Self::AdapterRefused { why } => format!("The game's adapter could not make a launch plan: {why}"),
            Self::PlanRejected { why } => format!("The launch plan was refused as unsafe or unusable: {why}"),
            Self::ProgramChanged { why } => format!(
                "The game's program no longer matches what SUL recorded, so it was not started: {why} Verify the build to see what changed."
            ),
            Self::CouldNotStart { why } => format!("Windows could not start the program: {why}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outcome {
    Running {
        pid: u32,
    },
    Exited {
        code: Option<i32>,
        success: bool,
        /// Set when it failed; says what is known and what is not.
        explanation: Option<String>,
    },
    NotStarted {
        why: NotStarted,
        explanation: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaunchRecord {
    pub launch_id: String,
    pub game_id: String,
    pub build_id: String,
    pub started_unix: u64,
    pub ended_unix: Option<u64>,
    pub outcome: Outcome,
    /// Where the diagnostic output was written, when the owner asked for it and there was some.
    pub log_file: Option<PathBuf>,
    /// Things worth knowing that did not stop the launch.
    pub warnings: Vec<String>,
}

/// What the library shows next to a game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LaunchState {
    NotRunning,
    Running {
        pid: u32,
        build_id: String,
        since_unix: u64,
    },
    LastExited {
        code: Option<i32>,
        success: bool,
        ended_unix: u64,
    },
    LastNotStarted {
        explanation: String,
    },
}

struct Active {
    record: LaunchRecord,
    child: Box<dyn Child>,
}

pub struct RuntimeManager {
    launcher: Box<dyn ProcessLauncher>,
    protected: ProtectedRoots,
    logs_dir: PathBuf,
    logger: Option<Logger>,
    active: BTreeMap<String, Active>,
    history: Vec<LaunchRecord>,
}

impl RuntimeManager {
    pub fn new(
        launcher: Box<dyn ProcessLauncher>,
        protected: ProtectedRoots,
        logs_dir: &Path,
    ) -> Self {
        Self {
            launcher,
            protected,
            logs_dir: logs_dir.to_path_buf(),
            logger: None,
            active: BTreeMap::new(),
            history: Vec::new(),
        }
    }

    /// Record one structured line per launch event. Argument and environment values are never logged.
    pub fn with_logger(mut self, logger: Logger) -> Self {
        self.logger = Some(logger);
        self
    }

    fn log(&self, level: &str, event: &str, fields: serde_json::Value) {
        if let Some(l) = &self.logger {
            let _ = l.log(level, event, fields);
        }
    }

    fn remember(&mut self, rec: LaunchRecord) {
        self.history.push(rec);
        if self.history.len() > HISTORY_LIMIT {
            self.history.remove(0);
        }
    }

    fn refuse(
        &mut self,
        launch_id: String,
        game_id: &str,
        build_id: &str,
        started: u64,
        why: NotStarted,
    ) -> LaunchRecord {
        let explanation = why.explain();
        self.log(
            "warn",
            "launch.not_started",
            serde_json::json!({"launch_id": launch_id, "game_id": game_id, "build_id": build_id, "explanation": explanation}),
        );
        let rec = LaunchRecord {
            launch_id,
            game_id: game_id.to_owned(),
            build_id: build_id.to_owned(),
            started_unix: started,
            ended_unix: Some(started),
            outcome: Outcome::NotStarted { why, explanation },
            log_file: None,
            warnings: vec![],
        };
        self.remember(rec.clone());
        rec
    }

    /// Start a build. Expected refusals come back as a record whose outcome is `NotStarted`, with an
    /// explanation; an `Err` means something unexpected went wrong (a registry or disk error).
    pub fn launch(
        &mut self,
        ctx: &Ctx,
        registry: &BuildRegistry,
        adapters: &AdapterRegistry,
        req: &LaunchRequest,
    ) -> Result<LaunchRecord> {
        self.refresh()?;
        let launch_id = fresh_id("launch");
        let started = now_unix();

        let Some(build) = registry.get(&req.build_id) else {
            return Ok(self.refuse(
                launch_id,
                "",
                &req.build_id,
                started,
                NotStarted::NoSuchBuild,
            ));
        };
        let game_id = build.game_id.clone();
        if let Some(a) = self.active.get(&game_id) {
            let running = a.record.build_id.clone();
            return Ok(self.refuse(
                launch_id,
                &game_id,
                &req.build_id,
                started,
                NotStarted::AlreadyRunning { build_id: running },
            ));
        }
        if build.state != State::Ready {
            let state = build.state.clone();
            return Ok(self.refuse(
                launch_id,
                &game_id,
                &req.build_id,
                started,
                NotStarted::BuildNotReady { state },
            ));
        }
        let Some(adapter) = adapters.get(&game_id) else {
            return Ok(self.refuse(
                launch_id,
                &game_id,
                &req.build_id,
                started,
                NotStarted::NoAdapter,
            ));
        };

        let dir = registry.build_dir(ctx, &req.build_id)?;
        let manifest = registry.manifest(ctx, &req.build_id)?;
        let plan = match adapter.launch_plan(&dir, &manifest.base_version) {
            Ok(p) => p,
            Err(e) => {
                return Ok(self.refuse(
                    launch_id,
                    &game_id,
                    &req.build_id,
                    started,
                    NotStarted::AdapterRefused { why: e.to_string() },
                ));
            }
        };
        let capture = req.capture_output.then_some(DEFAULT_CAPTURE_BYTES);
        let spec: ProcessSpec = match resolve_plan(&dir, &plan, capture) {
            Ok(s) => s,
            Err(e) => {
                return Ok(self.refuse(
                    launch_id,
                    &game_id,
                    &req.build_id,
                    started,
                    NotStarted::PlanRejected { why: e.to_string() },
                ));
            }
        };

        // The program must still be the one SUL recorded.
        let mut warnings = Vec::new();
        let wanted = plan.exe.replace('\\', "/");
        let wanted = wanted.trim_start_matches("./");
        let entry = manifest
            .files
            .iter()
            .find(|f| f.path.eq_ignore_ascii_case(wanted));
        let managed = matches!(build.location, Location::Managed { .. });
        let (hash, _) = hash_file(&spec.exe)?;
        match entry {
            Some(e) if e.sha256 == hash => {}
            Some(_) if managed => {
                return Ok(self.refuse(
                    launch_id,
                    &game_id,
                    &req.build_id,
                    started,
                    NotStarted::ProgramChanged {
                        why: format!("{wanted} has changed since it was recorded."),
                    },
                ));
            }
            None if managed => {
                return Ok(self.refuse(
                    launch_id,
                    &game_id,
                    &req.build_id,
                    started,
                    NotStarted::ProgramChanged {
                        why: format!("{wanted} is not in the build's list of files."),
                    },
                ));
            }
            Some(_) => warnings.push(format!(
                "{wanted} differs from the baseline SUL recorded for this install. The game may have updated. The launch was not blocked."
            )),
            None => warnings.push(format!(
                "{wanted} is not in the baseline SUL recorded for this install. The launch was not blocked."
            )),
        }

        match self.launcher.start(&spec) {
            Err(e) => Ok(self.refuse(
                launch_id,
                &game_id,
                &req.build_id,
                started,
                NotStarted::CouldNotStart { why: e.to_string() },
            )),
            Ok(child) => {
                let rec = LaunchRecord {
                    launch_id: launch_id.clone(),
                    game_id: game_id.clone(),
                    build_id: req.build_id.clone(),
                    started_unix: started,
                    ended_unix: None,
                    outcome: Outcome::Running { pid: child.pid() },
                    log_file: None,
                    warnings,
                };
                self.log(
                    "info",
                    "launch.started",
                    serde_json::json!({
                        "launch_id": launch_id, "game_id": game_id, "build_id": req.build_id,
                        "pid": child.pid(), "program": wanted, "argument_count": spec.args.len(),
                        "environment_count": spec.env.len(), "capture_output": req.capture_output,
                    }),
                );
                self.active.insert(
                    game_id,
                    Active {
                        record: rec.clone(),
                        child,
                    },
                );
                Ok(rec)
            }
        }
    }

    /// Poll every running program. Returns the launches that ended since the last call.
    pub fn refresh(&mut self) -> Result<Vec<LaunchRecord>> {
        let mut finished = Vec::new();
        let games: Vec<String> = self.active.keys().cloned().collect();
        for game in games {
            let polled = match self.active.get_mut(&game) {
                Some(a) => a.child.poll(),
                None => continue,
            };
            let exit = match polled {
                Ok(None) => continue,
                Ok(Some(x)) => x,
                Err(e) => {
                    // The process is gone or cannot be asked; do not keep claiming it runs.
                    let a = self.active.get(&game).map(|a| a.record.clone());
                    self.log(
                        "warn",
                        "launch.status_unreadable",
                        serde_json::json!({"game_id": game, "error": e.to_string(), "launch_id": a.map(|r| r.launch_id)}),
                    );
                    ExitInfo {
                        code: None,
                        success: false,
                    }
                }
            };
            let Some(mut a) = self.active.remove(&game) else {
                continue;
            };
            let ended = now_unix();
            let seconds = ended.saturating_sub(a.record.started_unix);
            let explanation = (!exit.success).then(|| explain_exit(&exit, seconds));

            if let Some(c) = a.child.captured() {
                let mut text = redact_text(&c.text);
                if c.truncated {
                    text.push_str("\n[SUL kept only the first part of the output]\n");
                }
                let file = self.logs_dir.join(format!("{}.log", a.record.launch_id));
                match fsops::atomic_write(&self.protected, &file, text.as_bytes()) {
                    Ok(()) => a.record.log_file = Some(file),
                    Err(e) => a
                        .record
                        .warnings
                        .push(format!("The diagnostic output could not be saved: {e}")),
                }
            }
            a.record.ended_unix = Some(ended);
            a.record.outcome = Outcome::Exited {
                code: exit.code,
                success: exit.success,
                explanation,
            };
            self.log(
                if exit.success { "info" } else { "warn" },
                "launch.exited",
                serde_json::json!({"launch_id": a.record.launch_id, "game_id": game,
                    "code": exit.code, "success": exit.success, "seconds": seconds}),
            );
            self.remember(a.record.clone());
            finished.push(a.record);
        }
        Ok(finished)
    }

    pub fn state_of(&self, game_id: &str) -> LaunchState {
        if let Some(a) = self.active.get(game_id) {
            if let Outcome::Running { pid } = a.record.outcome {
                return LaunchState::Running {
                    pid,
                    build_id: a.record.build_id.clone(),
                    since_unix: a.record.started_unix,
                };
            }
        }
        match self.history.iter().rev().find(|r| r.game_id == game_id) {
            None => LaunchState::NotRunning,
            Some(r) => match &r.outcome {
                Outcome::Exited { code, success, .. } => LaunchState::LastExited {
                    code: *code,
                    success: *success,
                    ended_unix: r.ended_unix.unwrap_or(r.started_unix),
                },
                Outcome::NotStarted { explanation, .. } => LaunchState::LastNotStarted {
                    explanation: explanation.clone(),
                },
                Outcome::Running { .. } => LaunchState::NotRunning,
            },
        }
    }

    pub fn history(&self) -> &[LaunchRecord] {
        &self.history
    }

    pub fn is_running(&self, game_id: &str) -> bool {
        self.active.contains_key(game_id)
    }
}

/// What can honestly be said about a failed exit. SUL does not know why a game failed.
fn explain_exit(exit: &ExitInfo, seconds: u64) -> String {
    let code = match exit.code {
        Some(c) => format!("exit code {c}"),
        None => "no exit code (it was ended by the system)".to_owned(),
    };
    if seconds < EARLY_EXIT_SECONDS {
        format!(
            "The program ended with {code} after {seconds} s, which is very soon after it started, so it most likely failed during start-up. SUL cannot tell why. If you turned on diagnostic output for the launch, its log may say."
        )
    } else {
        format!(
            "The program ended with {code} after {seconds} s. SUL cannot tell whether that was a crash or a normal way for this game to close."
        )
    }
}

/// A convenience error for callers that want a refusal as an `Err`.
pub fn refusal_as_error(rec: &LaunchRecord) -> Option<Error> {
    match &rec.outcome {
        Outcome::NotStarted { explanation, .. } => Some(Error::Invalid(explanation.clone())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fixtures::MadeUpGame;
    use crate::manifest::Variant;
    use crate::process::fake::{FakeLauncher, Script};
    use crate::testkit::{GAME, World, put};
    use std::sync::{Arc, Mutex};

    struct Rig {
        w: World,
        reg: BuildRegistry,
        adapters: AdapterRegistry,
        build: String,
        started: Arc<Mutex<Vec<ProcessSpec>>>,
        scripts: Arc<Mutex<std::collections::VecDeque<Script>>>,
        fail: Arc<Mutex<Option<String>>>,
        mgr: RuntimeManager,
    }

    fn rig() -> Rig {
        let w = World::new();
        let mut reg = w.registry();
        let build = w.import(&mut reg, Variant::Original);
        let mut adapters = AdapterRegistry::new();
        adapters.register(Box::new(MadeUpGame::new())).unwrap();
        let launcher = FakeLauncher::new();
        let (started, scripts, fail) = (
            Arc::clone(&launcher.started),
            Arc::clone(&launcher.scripts),
            Arc::clone(&launcher.fail_with),
        );
        let mgr = RuntimeManager::new(
            Box::new(launcher),
            w.protected.clone(),
            &w.base.join("logs"),
        );
        Rig {
            w,
            reg,
            adapters,
            build,
            started,
            scripts,
            fail,
            mgr,
        }
    }

    impl Rig {
        fn launch(&mut self, capture: bool) -> LaunchRecord {
            self.launch_build(&self.build.clone(), capture)
        }
        fn launch_build(&mut self, build: &str, capture: bool) -> LaunchRecord {
            let ctx = self.w.ctx();
            self.mgr
                .launch(
                    &ctx,
                    &self.reg,
                    &self.adapters,
                    &LaunchRequest {
                        build_id: build.into(),
                        capture_output: capture,
                    },
                )
                .unwrap()
        }
    }

    fn explained(r: &LaunchRecord) -> String {
        match &r.outcome {
            Outcome::NotStarted { explanation, .. } => explanation.clone(),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_ready_build_starts_and_runs_the_program_the_adapter_named_inside_the_build() {
        let mut r = rig();
        let rec = r.launch(false);
        assert!(
            matches!(rec.outcome, Outcome::Running { pid: 4242 }),
            "{rec:?}"
        );
        let spec = r.started.lock().unwrap()[0].clone();
        let dir = r.reg.build_dir(&r.w.ctx(), &r.build).unwrap();
        assert!(spec.exe.starts_with(&dir) && spec.exe.ends_with("game.exe"));
        assert_eq!(spec.args, ["--list"]);
        assert_eq!(spec.capture_bytes, None);
        assert!(r.mgr.is_running(GAME));
        assert!(matches!(
            r.mgr.state_of(GAME),
            LaunchState::Running { pid: 4242, .. }
        ));
    }

    #[test]
    fn a_second_launch_of_a_running_game_is_refused_and_names_the_running_build() {
        let mut r = rig();
        r.scripts.lock().unwrap().push_back(Script {
            polls_before_exit: 100,
            exit: ExitInfo {
                code: Some(0),
                success: true,
            },
            output: None,
        });
        r.launch(false);
        let second = r.launch(false);
        let text = explained(&second);
        assert!(
            text.contains("already running") && text.contains(&r.build),
            "{text}"
        );
        assert_eq!(r.started.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_build_that_is_not_ready_is_not_started() {
        let mut r = rig();
        // damage the managed copy so verification marks it failed
        let dir = r.reg.build_dir(&r.w.ctx(), &r.build).unwrap();
        put(&dir.join("data/a.bin"), "CHANGED");
        {
            let ctx = r.w.ctx();
            r.reg.verify(&ctx, &r.build).unwrap();
        }
        let rec = r.launch(false);
        assert!(explained(&rec).contains("not ready"), "{rec:?}");
        assert!(r.started.lock().unwrap().is_empty());
    }

    #[test]
    fn an_unknown_build_and_a_game_with_no_adapter_are_explained() {
        let mut r = rig();
        let rec = r.launch_build("nonexistent-1", false);
        assert!(explained(&rec).contains("no build"));
        r.adapters = AdapterRegistry::new();
        let rec = r.launch(false);
        assert!(explained(&rec).contains("no adapter registered"), "{rec:?}");
        assert!(r.started.lock().unwrap().is_empty());
    }

    #[test]
    fn a_changed_program_in_a_managed_build_is_refused() {
        let mut r = rig();
        let dir = r.reg.build_dir(&r.w.ctx(), &r.build).unwrap();
        put(&dir.join("game.exe"), "swapped");
        let rec = r.launch(false);
        let text = explained(&rec);
        assert!(
            text.contains("game.exe") && text.contains("changed"),
            "{text}"
        );
        assert!(r.started.lock().unwrap().is_empty());
    }

    #[test]
    fn a_missing_program_is_a_rejected_plan_not_a_crash() {
        let mut r = rig();
        let dir = r.reg.build_dir(&r.w.ctx(), &r.build).unwrap();
        std::fs::rename(dir.join("game.exe"), dir.join("moved.bin")).unwrap();
        let rec = r.launch(false);
        assert!(explained(&rec).contains("unsafe or unusable"), "{rec:?}");
    }

    #[test]
    fn an_install_that_stays_in_place_launches_with_a_warning_when_its_program_changed() {
        let mut r = rig();
        let ext = r.w.base.join("steam_copy");
        for f in ["game.exe", "madeup.txt"] {
            put(&ext.join(f), "orig");
        }
        let mut prot = r.w.protected.clone();
        prot.add(&ext).unwrap();
        // register the install, then change its program (as a game update would)
        let reg_file = r.w.base.join("state/external.json");
        let mut reg = BuildRegistry::open(&reg_file).unwrap();
        let ctx = Ctx {
            roots: &r.w.roots,
            protected: &prot,
            journal: &r.w.journal,
            disk: &r.w.disk,
        };
        let id = reg
            .register_in_place(
                &ctx,
                &crate::builds::RegisterRequest {
                    game_id: GAME.into(),
                    display_name: "Made-up Game".into(),
                    variant: Variant::Original,
                    base_version: "1.0.0".into(),
                    source_description: "steam".into(),
                    path: ext.clone(),
                },
            )
            .unwrap();
        std::fs::write(ext.join("game.exe"), "updated by the game").unwrap();
        let rec = r
            .mgr
            .launch(
                &ctx,
                &reg,
                &r.adapters,
                &LaunchRequest {
                    build_id: id,
                    capture_output: false,
                },
            )
            .unwrap();
        assert!(matches!(rec.outcome, Outcome::Running { .. }), "{rec:?}");
        assert!(
            rec.warnings
                .iter()
                .any(|w| w.contains("differs from the baseline"))
        );
    }

    #[test]
    fn a_failure_to_start_is_explained_and_the_game_is_not_left_marked_running() {
        let mut r = rig();
        *r.fail.lock().unwrap() = Some("access is denied".into());
        let rec = r.launch(false);
        assert!(explained(&rec).contains("access is denied"));
        assert!(!r.mgr.is_running(GAME));
        assert!(matches!(
            r.mgr.state_of(GAME),
            LaunchState::LastNotStarted { .. }
        ));
        *r.fail.lock().unwrap() = None;
        assert!(matches!(r.launch(false).outcome, Outcome::Running { .. }));
    }

    #[test]
    fn a_program_that_ends_is_recorded_with_its_code_and_an_honest_explanation() {
        let mut r = rig();
        r.scripts.lock().unwrap().push_back(Script {
            polls_before_exit: 1,
            exit: ExitInfo {
                code: Some(3),
                success: false,
            },
            output: None,
        });
        r.launch(false);
        assert!(r.mgr.refresh().unwrap().is_empty()); // still running on the first poll
        let done = r.mgr.refresh().unwrap();
        assert_eq!(done.len(), 1);
        match &done[0].outcome {
            Outcome::Exited {
                code,
                success,
                explanation,
            } => {
                assert_eq!(*code, Some(3));
                assert!(!success);
                let e = explanation.clone().unwrap();
                assert!(
                    e.contains("exit code 3")
                        && e.contains("start-up")
                        && e.contains("cannot tell why"),
                    "{e}"
                );
            }
            o => panic!("{o:?}"),
        }
        assert!(!r.mgr.is_running(GAME));
        assert!(matches!(
            r.mgr.state_of(GAME),
            LaunchState::LastExited {
                code: Some(3),
                success: false,
                ..
            }
        ));
        // and the game can be started again
        assert!(matches!(r.launch(false).outcome, Outcome::Running { .. }));
    }

    #[test]
    fn a_clean_exit_has_no_failure_explanation() {
        let mut r = rig();
        r.launch(false);
        let done = r.mgr.refresh().unwrap();
        assert!(matches!(
            done[0].outcome,
            Outcome::Exited {
                success: true,
                explanation: None,
                ..
            }
        ));
    }

    #[test]
    fn the_exit_explanation_depends_on_how_soon_it_failed_and_never_claims_a_cause() {
        let e = ExitInfo {
            code: Some(1),
            success: false,
        };
        let soon = explain_exit(&e, EARLY_EXIT_SECONDS - 1);
        let late = explain_exit(&e, EARLY_EXIT_SECONDS);
        assert!(soon.contains("start-up") && !late.contains("start-up"));
        assert!(late.contains("cannot tell whether"));
        let none = explain_exit(
            &ExitInfo {
                code: None,
                success: false,
            },
            1,
        );
        assert!(none.contains("no exit code"));
    }

    #[test]
    fn diagnostic_output_is_off_by_default() {
        let mut r = rig();
        r.scripts.lock().unwrap().push_back(Script {
            polls_before_exit: 0,
            exit: ExitInfo {
                code: Some(0),
                success: true,
            },
            output: Some("hello".into()),
        });
        r.launch(false);
        let done = r.mgr.refresh().unwrap();
        assert!(done[0].log_file.is_none());
        assert!(!r.w.base.join("logs").exists());
    }

    #[test]
    fn opted_in_output_is_redacted_and_saved_to_its_own_file() {
        let mut r = rig();
        let hook = "https://discord.com/api/webhooks/123456/abcdefSECRET";
        r.scripts.lock().unwrap().push_back(Script {
            polls_before_exit: 0,
            exit: ExitInfo {
                code: Some(0),
                success: true,
            },
            output: Some(format!("started ok\nposting to {hook} now\n")),
        });
        r.launch(true);
        assert_eq!(
            r.started.lock().unwrap()[0].capture_bytes,
            Some(DEFAULT_CAPTURE_BYTES)
        );
        let done = r.mgr.refresh().unwrap();
        let file = done[0].log_file.clone().expect("a log file");
        assert!(file.starts_with(r.w.base.join("logs")));
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("started ok") && text.contains("[REDACTED]"));
        assert!(!text.contains("SECRET") && !text.contains("123456"));
    }

    #[test]
    fn history_is_capped_and_keeps_the_newest() {
        let mut r = rig();
        for _ in 0..(HISTORY_LIMIT + 5) {
            r.launch_build("nonexistent-1", false);
        }
        assert_eq!(r.mgr.history().len(), HISTORY_LIMIT);
    }

    #[test]
    fn the_log_names_what_happened_but_never_the_argument_or_environment_values() {
        let w = World::new();
        let mut reg = w.registry();
        let build = w.import(&mut reg, Variant::Original);
        let mut adapters = AdapterRegistry::new();
        adapters.register(Box::new(MadeUpGame::new())).unwrap();
        let log_file = w.base.join("sul.log");
        let mut mgr = RuntimeManager::new(
            Box::new(FakeLauncher::new()),
            w.protected.clone(),
            &w.base.join("logs"),
        )
        .with_logger(Logger::open(&log_file).unwrap());
        let ctx = w.ctx();
        mgr.launch(
            &ctx,
            &reg,
            &adapters,
            &LaunchRequest {
                build_id: build,
                capture_output: false,
            },
        )
        .unwrap();
        mgr.refresh().unwrap();
        let text = std::fs::read_to_string(&log_file).unwrap();
        assert!(text.contains("launch.started") && text.contains("launch.exited"));
        assert!(text.contains("argument_count"));
        assert!(!text.contains("--list"));
    }

    #[test]
    fn a_refusal_can_be_turned_into_an_error_for_callers_that_want_one() {
        let mut r = rig();
        let refusal = r.launch_build("nonexistent-1", false);
        assert!(refusal_as_error(&refusal).is_some());
        let ok = r.launch(false);
        assert!(refusal_as_error(&ok).is_none());
    }
}
