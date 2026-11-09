//! The Play half of the service (spec 1.4): start a build SUL knows, follow it while it runs, and show what happened.
//!
//! As in [`super`], the rules live elsewhere ([`crate::runtime`], [`crate::process`]); this file wires them to the
//! builds the service holds and returns plain views. It adds no new way to start a program or to touch a file.
//!
//! * **Only the runtime manager starts a game**, and only a build that is Ready, through its game's adapter, the
//!   program that adapter names, checked to lie inside the build folder. A game with no adapter is refused with
//!   that reason. No adapter for a real game exists yet, so in the app every Play of a build is refused today.
//! * **A refusal is an answer, not an error.** The core's own sentence comes back inside the record, and the record
//!   is kept in the history, so the page can show why nothing started.
//! * **One running copy per game.** A second start of the same game is refused, naming the build that runs.
//! * **Reading does not poll.** [`ManagerService::launch_view`] only reads; [`ManagerService::launch_refresh`] asks
//!   each running program whether it ended and says which ones did.
//! * **Diagnostic output is opt-in** for each start, capped, passed through the log redactor and written to its own
//!   file under `launch-logs/`. By default the program's output is discarded.
//!
//! Not here: stopping a started program, limiting what it can do, checking that a game works, any network or launcher
//! integration, or the game-file risk policy ([`crate::trust::policy`]), which is not connected to launching.

use serde::Serialize;

use super::{Inner, ManagerService};
use crate::builds::Location;
use crate::manifest::State;
use crate::runtime::{LaunchRecord, LaunchRequest, LaunchState};
use crate::{Error, Result};

/// A build the Play panel can offer, with what decides whether starting it can work at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlayBuild {
    pub build_id: String,
    pub game_id: String,
    pub slot: String,
    pub state: State,
    /// True for a copy SUL manages, false for an install that stays where it is.
    pub managed: bool,
    /// Whether it is the active build of its slot.
    pub active: bool,
    /// Whether SUL has an adapter for the game. Without one nothing can be started, and the core says so.
    pub has_adapter: bool,
}

/// One game's launch state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GameLaunch {
    pub game_id: String,
    pub state: LaunchState,
}

/// Everything the Play panel shows.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LaunchView {
    pub builds: Vec<PlayBuild>,
    /// One entry for every game that has a build, by game id.
    pub games: Vec<GameLaunch>,
    /// Newest first. The core keeps the last few dozen.
    pub history: Vec<LaunchRecord>,
}

/// What a start returned: the record (a refusal is a record whose outcome is `not_started`), and the view after it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Started {
    pub record: LaunchRecord,
    pub view: LaunchView,
}

/// What a poll returned: the launches that ended since the last poll, and the view after it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Refreshed {
    pub finished: Vec<LaunchRecord>,
    pub view: LaunchView,
}

fn launch_view_of(g: &Inner) -> LaunchView {
    let mut builds: Vec<PlayBuild> = g
        .builds
        .builds()
        .iter()
        .map(|b| PlayBuild {
            build_id: b.id.clone(),
            game_id: b.game_id.clone(),
            slot: b.slot.clone(),
            state: b.state.clone(),
            managed: matches!(b.location, Location::Managed { .. }),
            active: g
                .builds
                .active(&b.game_id, &b.slot)
                .is_some_and(|a| a.build_id == b.id),
            has_adapter: g.adapters.get(&b.game_id).is_some(),
        })
        .collect();
    builds.sort_by(|a, b| {
        (&a.game_id, &a.slot, &a.build_id).cmp(&(&b.game_id, &b.slot, &b.build_id))
    });
    let mut game_ids: Vec<&str> = builds.iter().map(|b| b.game_id.as_str()).collect();
    game_ids.dedup();
    let games = game_ids
        .into_iter()
        .map(|id| GameLaunch {
            game_id: id.to_owned(),
            state: g.runtime.state_of(id),
        })
        .collect();
    let mut history = g.runtime.history().to_vec();
    history.reverse();
    LaunchView {
        builds,
        games,
        history,
    }
}

impl ManagerService {
    /// Everything the Play panel shows. Reads only: it does not ask a running program whether it ended.
    pub fn launch_view(&self) -> Result<LaunchView> {
        let g = self.lock()?;
        Ok(launch_view_of(&g))
    }

    /// Start a build. Expected refusals (no such build, one is already running, not ready, no adapter, a plan that
    /// is unsafe, a program that no longer matches its record) come back as a record with the core's sentence;
    /// only something unexpected is an `Err`. `capture_output` keeps the program's output in a capped, redacted log.
    pub fn launch_start(&self, build_id: &str, capture_output: bool) -> Result<Started> {
        if build_id.trim().is_empty() {
            return Err(Error::Invalid("pick a build to start".into()));
        }
        let mut g = self.lock()?;
        let Inner {
            roots,
            protected,
            journal,
            disk,
            builds,
            adapters,
            runtime,
            ..
        } = &mut *g;
        let ctx = crate::builds::Ctx {
            roots,
            protected,
            journal,
            disk: &**disk,
        };
        let record = runtime.launch(
            &ctx,
            builds,
            adapters,
            &LaunchRequest {
                build_id: build_id.to_owned(),
                capture_output,
            },
        )?;
        Ok(Started {
            record,
            view: launch_view_of(&g),
        })
    }

    /// Ask every running program whether it ended, and return the ones that did.
    pub fn launch_refresh(&self) -> Result<Refreshed> {
        let mut g = self.lock()?;
        let finished = g.runtime.refresh()?;
        Ok(Refreshed {
            finished,
            view: launch_view_of(&g),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::VecDeque;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::adapters::AdapterRegistry;
    use crate::adapters::fixtures::MadeUpGame;
    use crate::manager::BuildSource;
    use crate::manifest::Variant;
    use crate::process::fake::{FakeLauncher, Script};
    use crate::process::{ExitInfo, ProcessSpec};
    use crate::roots::{DiskSpace, RootKind};
    use crate::runtime::{NotStarted, Outcome};

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
        started: Arc<Mutex<Vec<ProcessSpec>>>,
        scripts: Arc<Mutex<VecDeque<Script>>>,
    }

    fn put(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn adapters(with_adapter: bool) -> AdapterRegistry {
        let mut a = AdapterRegistry::new();
        if with_adapter {
            a.register(Box::new(MadeUpGame::new())).unwrap();
        }
        a
    }

    fn script(polls_before_exit: usize, code: i32, output: Option<&str>) -> Script {
        Script {
            polls_before_exit,
            exit: ExitInfo {
                code: Some(code),
                success: code == 0,
            },
            output: output.map(str::to_owned),
        }
    }

    /// A service whose launcher is a fake (it starts nothing), with one managed build of a made-up install.
    fn fx_with(with_adapter: bool, variant: Variant) -> Fx {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(base.join("ssd")).unwrap();
        let install = base.join("original_install");
        put(&install.join("game.exe"), "exe");
        put(&install.join("data/a.bin"), "AAAA");
        let launcher = FakeLauncher::new();
        let started = Arc::clone(&launcher.started);
        let scripts = Arc::clone(&launcher.scripts);
        let svc = ManagerService::with_parts_and_launcher(
            &base.join("state"),
            Box::new(Disk(Cell::new(u64::MAX / 2))),
            adapters(with_adapter),
            Box::new(launcher),
        )
        .unwrap();
        svc.add_root("ssd", "Fast drive", &base.join("ssd"), RootKind::Builds)
            .unwrap();
        svc.protect_installs(std::slice::from_ref(&install))
            .unwrap();
        let built = svc
            .import_build(
                &BuildSource {
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
            started,
            scripts,
        }
    }

    fn fx() -> Fx {
        fx_with(true, Variant::Experimental)
    }

    fn explained(r: &LaunchRecord) -> String {
        match &r.outcome {
            Outcome::NotStarted { explanation, .. } => explanation.clone(),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// The `game.exe` inside the managed copy (found by looking, so the test does not assume how the folder is named).
    fn managed_exe(f: &Fx) -> PathBuf {
        fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.file_name().is_some_and(|n| n == "game.exe") {
                    out.push(p);
                }
            }
        }
        let mut found = vec![];
        walk(&f.base.join("ssd"), &mut found);
        assert_eq!(found.len(), 1, "{found:?}");
        found.remove(0)
    }

    #[test]
    fn the_view_lists_every_build_with_what_decides_whether_it_can_start() {
        let f = fx();
        let v = f.svc.launch_view().unwrap();
        assert_eq!(v.builds.len(), 1);
        let b = &v.builds[0];
        assert_eq!(
            (b.build_id.as_str(), b.game_id.as_str()),
            (f.build.as_str(), GAME)
        );
        assert!(b.managed && b.has_adapter);
        assert_eq!(b.state, State::Ready);
        assert_eq!(v.games.len(), 1);
        assert_eq!(v.games[0].state, LaunchState::NotRunning);
        assert!(v.history.is_empty());

        let none = fx_with(false, Variant::Experimental);
        assert!(!none.svc.launch_view().unwrap().builds[0].has_adapter);
    }

    #[test]
    fn two_builds_of_one_game_list_the_game_once_and_only_the_activated_build_is_active() {
        let f = fx();
        let second = f
            .svc
            .import_build(
                &BuildSource {
                    game_id: GAME.into(),
                    display_name: "Made-up Game".into(),
                    install_dir: f.install.clone(),
                    variant: Variant::Experimental,
                    base_version: "1.0.0".into(),
                    source_description: "a second copy".into(),
                },
                "ssd",
            )
            .unwrap()
            .build_id;
        assert_ne!(second, f.build);
        let active_of = |svc: &ManagerService| -> Vec<String> {
            svc.launch_view()
                .unwrap()
                .builds
                .into_iter()
                .filter(|b| b.active)
                .map(|b| b.build_id)
                .collect()
        };
        let v = f.svc.launch_view().unwrap();
        assert_eq!(v.builds.len(), 2);
        assert_eq!(
            v.games.len(),
            1,
            "one entry for the game, not one per build"
        );
        assert!(
            v.builds.windows(2).all(|w| w[0].build_id <= w[1].build_id),
            "builds are listed in a fixed order"
        );
        // A new build is not active until the owner activates it; activating one moves the flag and only that one has it.
        assert!(active_of(&f.svc).is_empty());
        f.svc.activate_build(&f.build).unwrap();
        assert_eq!(active_of(&f.svc), std::slice::from_ref(&f.build));
        f.svc.activate_build(&second).unwrap();
        assert_eq!(active_of(&f.svc), std::slice::from_ref(&second));
    }

    #[test]
    fn a_ready_build_starts_the_program_the_adapter_named_inside_the_build_and_the_view_says_it_runs()
     {
        let f = fx();
        f.scripts.lock().unwrap().push_back(script(100, 0, None));
        let s = f.svc.launch_start(&f.build, false).unwrap();
        assert!(
            matches!(s.record.outcome, Outcome::Running { pid: 4242 }),
            "{:?}",
            s.record
        );
        let spec = f.started.lock().unwrap()[0].clone();
        assert_eq!(
            spec.exe,
            managed_exe(&f),
            "the managed copy's program, not the original install's"
        );
        assert!(!spec.exe.starts_with(&f.install));
        assert_eq!(spec.capture_bytes, None, "output is not kept unless asked");
        assert!(matches!(
            s.view.games[0].state,
            LaunchState::Running { pid: 4242, .. }
        ));
        assert_eq!(
            s.view.history.len(),
            0,
            "a running launch is not history yet"
        );
    }

    #[test]
    fn reading_the_view_does_not_poll_and_a_refresh_reports_the_launch_that_ended() {
        let f = fx();
        f.scripts.lock().unwrap().push_back(script(1, 0, None));
        f.svc.launch_start(&f.build, false).unwrap();
        for _ in 0..4 {
            let v = f.svc.launch_view().unwrap();
            assert!(
                matches!(v.games[0].state, LaunchState::Running { .. }),
                "reading must not end it"
            );
        }
        let first = f.svc.launch_refresh().unwrap();
        assert!(first.finished.is_empty());
        assert!(matches!(
            first.view.games[0].state,
            LaunchState::Running { .. }
        ));
        let second = f.svc.launch_refresh().unwrap();
        assert_eq!(second.finished.len(), 1);
        assert!(matches!(
            second.finished[0].outcome,
            Outcome::Exited {
                code: Some(0),
                success: true,
                ..
            }
        ));
        assert!(matches!(
            second.view.games[0].state,
            LaunchState::LastExited { success: true, .. }
        ));
        assert_eq!(second.view.history.len(), 1);
        assert!(
            f.svc.launch_refresh().unwrap().finished.is_empty(),
            "an ended launch is reported once"
        );
    }

    #[test]
    fn a_failed_exit_is_explained_without_claiming_a_cause() {
        let f = fx();
        f.scripts.lock().unwrap().push_back(script(0, 3, None));
        f.svc.launch_start(&f.build, false).unwrap();
        let done = f.svc.launch_refresh().unwrap();
        let Outcome::Exited {
            code,
            success,
            explanation,
        } = &done.finished[0].outcome
        else {
            panic!("{:?}", done.finished[0])
        };
        assert_eq!((*code, *success), (Some(3), false));
        let text = explanation.clone().unwrap();
        assert!(
            text.contains("exit code 3") && text.contains("cannot tell why"),
            "{text}"
        );
    }

    #[test]
    fn a_game_with_no_adapter_is_refused_in_the_cores_words_and_nothing_starts() {
        let f = fx_with(false, Variant::Experimental);
        let s = f.svc.launch_start(&f.build, false).unwrap();
        assert!(
            matches!(
                &s.record.outcome,
                Outcome::NotStarted {
                    why: NotStarted::NoAdapter,
                    ..
                }
            ),
            "{:?}",
            s.record
        );
        assert!(explained(&s.record).contains("no adapter registered"));
        assert!(f.started.lock().unwrap().is_empty());
        assert_eq!(
            s.view.history.len(),
            1,
            "a refusal is kept so the page can show why"
        );
        assert_eq!(s.view.history[0].launch_id, s.record.launch_id);
        assert!(matches!(
            s.view.games[0].state,
            LaunchState::LastNotStarted { .. }
        ));
    }

    #[test]
    fn a_second_start_of_a_running_game_is_refused_naming_the_running_build() {
        let f = fx();
        f.scripts.lock().unwrap().push_back(script(100, 0, None));
        f.svc.launch_start(&f.build, false).unwrap();
        let second = f.svc.launch_start(&f.build, false).unwrap();
        let text = explained(&second.record);
        assert!(
            text.contains("already running") && text.contains(&f.build),
            "{text}"
        );
        assert_eq!(f.started.lock().unwrap().len(), 1);
    }

    #[test]
    fn an_unknown_build_is_a_refusal_and_a_blank_id_is_an_error_that_records_nothing() {
        let f = fx();
        let s = f.svc.launch_start("nonexistent-1", false).unwrap();
        assert!(explained(&s.record).contains("no build"));
        let e = f.svc.launch_start("   ", false).unwrap_err().to_string();
        assert!(e.contains("pick a build"), "{e}");
        assert_eq!(f.svc.launch_view().unwrap().history.len(), 1);
        assert!(f.started.lock().unwrap().is_empty());
    }

    #[test]
    fn a_build_that_failed_its_check_is_not_started() {
        let f = fx();
        // Damage a data file of the managed copy, then let the service check the build: it is no longer Ready.
        let dir = managed_exe(&f).parent().unwrap().to_path_buf();
        put(&dir.join("data/a.bin"), "CHANGED");
        f.svc.verify_build(&f.build).unwrap();
        let v = f.svc.launch_view().unwrap();
        assert_ne!(v.builds[0].state, State::Ready, "{:?}", v.builds[0]);
        let s = f.svc.launch_start(&f.build, false).unwrap();
        assert!(explained(&s.record).contains("not ready"), "{:?}", s.record);
        assert!(f.started.lock().unwrap().is_empty());
    }

    #[test]
    fn a_managed_program_that_changed_since_it_was_recorded_is_not_started() {
        let f = fx();
        put(&managed_exe(&f), "a different program");
        let s = f.svc.launch_start(&f.build, false).unwrap();
        assert!(
            matches!(
                &s.record.outcome,
                Outcome::NotStarted {
                    why: NotStarted::ProgramChanged { .. },
                    ..
                }
            ),
            "{:?}",
            s.record
        );
        assert!(explained(&s.record).contains("no longer matches"));
        assert!(f.started.lock().unwrap().is_empty());
    }

    #[test]
    fn a_service_made_without_a_launcher_starts_nothing_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(base.join("ssd")).unwrap();
        let install = base.join("original_install");
        put(&install.join("game.exe"), "exe");
        let svc = ManagerService::with_parts(
            &base.join("state"),
            Box::new(Disk(Cell::new(u64::MAX / 2))),
            adapters(true),
        )
        .unwrap();
        svc.add_root("ssd", "Fast drive", &base.join("ssd"), RootKind::Builds)
            .unwrap();
        svc.protect_installs(std::slice::from_ref(&install))
            .unwrap();
        let built = svc
            .import_build(
                &BuildSource {
                    game_id: GAME.into(),
                    display_name: "Made-up Game".into(),
                    install_dir: install,
                    variant: Variant::Experimental,
                    base_version: "1".into(),
                    source_description: "x".into(),
                },
                "ssd",
            )
            .unwrap();
        let s = svc.launch_start(&built.build_id, false).unwrap();
        assert!(
            matches!(
                &s.record.outcome,
                Outcome::NotStarted {
                    why: NotStarted::CouldNotStart { .. },
                    ..
                }
            ),
            "{:?}",
            s.record
        );
        assert!(explained(&s.record).contains("without a way to start programs"));
    }

    #[test]
    fn output_is_not_kept_unless_asked_and_then_it_is_redacted_in_its_own_file_under_launch_logs() {
        let f = fx();
        f.scripts
            .lock()
            .unwrap()
            .push_back(script(0, 0, Some("hello")));
        f.svc.launch_start(&f.build, false).unwrap();
        let done = f.svc.launch_refresh().unwrap();
        assert!(done.finished[0].log_file.is_none());
        assert!(!f.base.join("state/launch-logs").exists());

        let hook = "https://discord.com/api/webhooks/123456/abcdefSECRET";
        f.scripts.lock().unwrap().push_back(script(
            0,
            0,
            Some(&format!("started ok\nposting to {hook} now\n")),
        ));
        let s = f.svc.launch_start(&f.build, true).unwrap();
        assert!(f.started.lock().unwrap()[1].capture_bytes.is_some());
        assert!(matches!(s.record.outcome, Outcome::Running { .. }));
        let done = f.svc.launch_refresh().unwrap();
        let file = done.finished[0].log_file.clone().expect("a log file");
        assert!(
            file.starts_with(f.base.join("state/launch-logs")),
            "{file:?}"
        );
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("started ok") && text.contains("[REDACTED]"));
        assert!(!text.contains("SECRET") && !text.contains("123456"));
    }

    #[test]
    fn a_log_is_not_written_into_an_install_that_was_protected_after_the_service_was_opened() {
        let f = fx();
        let logs = f.base.join("state/launch-logs");
        std::fs::create_dir_all(&logs).unwrap();
        f.svc.protect_installs(std::slice::from_ref(&logs)).unwrap();
        f.scripts
            .lock()
            .unwrap()
            .push_back(script(0, 0, Some("hello")));
        f.svc.launch_start(&f.build, true).unwrap();
        let done = f.svc.launch_refresh().unwrap();
        assert!(
            done.finished[0].log_file.is_none(),
            "{:?}",
            done.finished[0]
        );
        assert!(
            done.finished[0]
                .warnings
                .iter()
                .any(|w| w.contains("could not be saved"))
        );
        assert!(std::fs::read_dir(&logs).unwrap().next().is_none());
    }

    #[test]
    fn the_history_lists_the_newest_launch_first() {
        let f = fx();
        let a = f.svc.launch_start("nonexistent-1", false).unwrap().record;
        let b = f.svc.launch_start("nonexistent-2", false).unwrap().record;
        let v = f.svc.launch_view().unwrap();
        let ids: Vec<&str> = v.history.iter().map(|r| r.launch_id.as_str()).collect();
        assert_eq!(ids, [b.launch_id.as_str(), a.launch_id.as_str()]);
    }

    #[test]
    fn an_install_that_stays_in_place_starts_with_a_warning_when_its_program_changed() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        let install = base.join("original_install");
        put(&install.join("game.exe"), "exe");
        let launcher = FakeLauncher::new();
        let started = Arc::clone(&launcher.started);
        let svc = ManagerService::with_parts_and_launcher(
            &base.join("state"),
            Box::new(Disk(Cell::new(u64::MAX / 2))),
            adapters(true),
            Box::new(launcher),
        )
        .unwrap();
        svc.protect_installs(std::slice::from_ref(&install))
            .unwrap();
        let built = svc
            .register_in_place(&BuildSource {
                game_id: GAME.into(),
                display_name: "Made-up Game".into(),
                install_dir: install.clone(),
                variant: Variant::Original,
                base_version: "1".into(),
                source_description: "x".into(),
            })
            .unwrap();
        // The game updates itself: its program is no longer the one recorded.
        put(&install.join("game.exe"), "updated by the game");
        let s = svc.launch_start(&built.build_id, false).unwrap();
        assert!(
            matches!(s.record.outcome, Outcome::Running { .. }),
            "{:?}",
            s.record
        );
        assert!(!s.record.warnings.is_empty(), "the owner is told");
        assert_eq!(started.lock().unwrap().len(), 1);
        assert!(!s.view.builds[0].managed);
    }
}
