//! Starting a program: the one place (besides the Steam launcher) where SUL runs another program.
//!
//! Everything that starts a program on the owner's behalf goes through [`ProcessLauncher`]: a game
//! build (spec 1.4) and, later, a registered external tool (spec 2.4). Keeping it in one small module
//! means the rules are written once and a test can fail if any other file starts a program.
//!
//! The rules:
//!
//! * **No shell.** The program is started directly with an argument list, never through `cmd /C` or
//!   a command string, so an argument can never be read as a second command.
//! * **Inside the folder.** A plan names its program and working folder relative to the build or
//!   workspace folder; each is checked to stay inside it (no `..`, no absolute path, no symlink out)
//!   and the program must be an existing `.exe` file.
//! * **Bounded input.** Argument count and length, and environment names, are limited; names that
//!   would change how Windows finds programs or where it keeps user data are refused.
//! * **Output is opt-in and capped.** Unless asked, the program's output is discarded. When asked, at
//!   most [`DEFAULT_CAPTURE_BYTES`] are kept in memory, to be redacted and written by the caller.
//!
//! What this does not do: sandbox the program. It runs with the owner's own permissions; SUL does not
//! restrict what it can do, and nothing here is anti-cheat or access-control related.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::adapters::LaunchPlan;
use crate::{Error, Result, error::io, safepath};

pub const MAX_ARGS: usize = 64;
pub const MAX_ARG_LEN: usize = 4096;
pub const MAX_ENV: usize = 32;
pub const MAX_ENV_VALUE_LEN: usize = 8192;
/// How much of a program's output is kept when the owner opts in: 1 MiB.
pub const DEFAULT_CAPTURE_BYTES: usize = 1024 * 1024;

/// Environment variables a plan may not set: they decide how Windows finds programs and where it keeps
/// the owner's data, so a plan that changed them could redirect more than the one program it starts.
const BLOCKED_ENV: &[&str] = &[
    "PATH",
    "PATHEXT",
    "COMSPEC",
    "SYSTEMROOT",
    "SYSTEMDRIVE",
    "WINDIR",
    "TEMP",
    "TMP",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "PROGRAMDATA",
    "PROGRAMFILES",
    "PROGRAMFILES(X86)",
    "HOMEDRIVE",
    "HOMEPATH",
];

/// A fully checked request to start a program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSpec {
    pub exe: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    /// `Some(n)`: keep up to `n` bytes of output. `None`: discard it.
    pub capture_bytes: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExitInfo {
    pub code: Option<i32>,
    pub success: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Captured {
    pub text: String,
    /// True when the program wrote more than was kept.
    pub truncated: bool,
}

/// A started program.
pub trait Child: Send {
    fn pid(&self) -> u32;
    /// `Ok(None)` while it is still running.
    fn poll(&mut self) -> Result<Option<ExitInfo>>;
    /// What was captured so far, or `None` when output was not requested.
    fn captured(&self) -> Option<Captured>;
}

/// The seam between SUL's rules and the operating system. Tests use a fake; the app uses [`StdLauncher`].
pub trait ProcessLauncher: Send {
    fn start(&self, spec: &ProcessSpec) -> Result<Box<dyn Child>>;
}

fn valid_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && name.len() <= 128
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '(' || c == ')')
}

/// Check the arguments and environment a plan asks for.
pub fn check_args_and_env(args: &[String], env: &[(String, String)]) -> Result<()> {
    if args.len() > MAX_ARGS {
        return Err(Error::Invalid(format!(
            "{} arguments; at most {MAX_ARGS} are allowed",
            args.len()
        )));
    }
    for a in args {
        if a.len() > MAX_ARG_LEN || a.contains('\0') {
            return Err(Error::Invalid(
                "an argument is too long or contains a NUL character".into(),
            ));
        }
    }
    if env.len() > MAX_ENV {
        return Err(Error::Invalid(format!(
            "{} environment variables; at most {MAX_ENV} are allowed",
            env.len()
        )));
    }
    for (k, v) in env {
        if !valid_env_name(k) {
            return Err(Error::Invalid(format!(
                "environment variable name {k:?} is not allowed"
            )));
        }
        if BLOCKED_ENV.iter().any(|b| b.eq_ignore_ascii_case(k)) {
            return Err(Error::Invalid(format!(
                "environment variable {k} may not be set by a launch plan"
            )));
        }
        if v.len() > MAX_ENV_VALUE_LEN || v.contains('\0') {
            return Err(Error::Invalid(format!(
                "the value of {k} is too long or contains a NUL character"
            )));
        }
    }
    Ok(())
}

/// Turn a [`LaunchPlan`] into a [`ProcessSpec`] for the folder `base`, or say why not.
pub fn resolve_plan(
    base: &Path,
    plan: &LaunchPlan,
    capture_bytes: Option<usize>,
) -> Result<ProcessSpec> {
    resolve_parts(
        base,
        &plan.exe,
        &plan.working_dir,
        &plan.args,
        &plan.env,
        capture_bytes,
    )
}

/// The same checks for a program named by its parts (used for external tools too).
pub fn resolve_parts(
    base: &Path,
    exe_rel: &str,
    working_dir_rel: &str,
    args: &[String],
    env: &[(String, String)],
    capture_bytes: Option<usize>,
) -> Result<ProcessSpec> {
    let exe = safepath::join_within(base, exe_rel)?;
    if !exe.is_file() {
        return Err(Error::Invalid(format!(
            "the program {exe_rel} does not exist in the folder"
        )));
    }
    let is_exe = exe
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("exe"));
    if !is_exe {
        return Err(Error::Invalid(format!(
            "{exe_rel} is not an .exe file; only programs are started"
        )));
    }
    let cwd = if working_dir_rel.is_empty() {
        base.canonicalize().map_err(io(base))?
    } else {
        let c = safepath::join_within(base, working_dir_rel)?;
        if !c.is_dir() {
            return Err(Error::Invalid(format!(
                "the working folder {working_dir_rel} does not exist"
            )));
        }
        c
    };
    check_args_and_env(args, env)?;
    Ok(ProcessSpec {
        exe,
        args: args.to_vec(),
        cwd,
        env: env.to_vec(),
        capture_bytes,
    })
}

// ---- the real launcher ----

/// Starts programs with `std::process::Command`: no shell, null stdin, output discarded unless captured.
#[derive(Debug, Default, Clone, Copy)]
pub struct StdLauncher;

#[derive(Default)]
struct Buffer {
    bytes: Vec<u8>,
    truncated: bool,
}

struct StdChild {
    child: std::process::Child,
    buffer: Option<Arc<Mutex<Buffer>>>,
}

fn drain(mut pipe: impl Read + Send + 'static, buffer: Arc<Mutex<Buffer>>, cap: usize) {
    std::thread::spawn(move || {
        let mut chunk = [0u8; 8192];
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if let Ok(mut b) = buffer.lock() {
                        let room = cap.saturating_sub(b.bytes.len());
                        if room < n {
                            b.truncated = true;
                        }
                        let take = room.min(n);
                        b.bytes.extend_from_slice(&chunk[..take]);
                    }
                }
            }
        }
    });
}

impl ProcessLauncher for StdLauncher {
    fn start(&self, spec: &ProcessSpec) -> Result<Box<dyn Child>> {
        let mut cmd = Command::new(&spec.exe);
        cmd.args(&spec.args)
            .current_dir(&spec.cwd)
            .stdin(Stdio::null());
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        if spec.capture_bytes.is_some() {
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        } else {
            cmd.stdout(Stdio::null()).stderr(Stdio::null());
        }
        let mut child = cmd.spawn().map_err(io(&spec.exe))?;
        let buffer = spec.capture_bytes.map(|cap| {
            let buf = Arc::new(Mutex::new(Buffer::default()));
            if let Some(out) = child.stdout.take() {
                drain(out, Arc::clone(&buf), cap);
            }
            if let Some(err) = child.stderr.take() {
                drain(err, Arc::clone(&buf), cap);
            }
            buf
        });
        Ok(Box::new(StdChild { child, buffer }))
    }
}

impl Child for StdChild {
    fn pid(&self) -> u32 {
        self.child.id()
    }

    fn poll(&mut self) -> Result<Option<ExitInfo>> {
        match self.child.try_wait() {
            Ok(None) => Ok(None),
            Ok(Some(status)) => Ok(Some(ExitInfo {
                code: status.code(),
                success: status.success(),
            })),
            Err(e) => Err(Error::Io {
                path: PathBuf::from("<child process>"),
                source: e,
            }),
        }
    }

    fn captured(&self) -> Option<Captured> {
        let b = self.buffer.as_ref()?.lock().ok()?;
        Some(Captured {
            text: String::from_utf8_lossy(&b.bytes).into_owned(),
            truncated: b.truncated,
        })
    }
}

#[cfg(test)]
pub(crate) mod fake {
    //! A launcher that starts nothing, for tests of the code around it.
    use super::*;
    use std::collections::VecDeque;

    /// What a fake child does when polled.
    pub struct Script {
        pub polls_before_exit: usize,
        pub exit: ExitInfo,
        pub output: Option<String>,
    }

    pub struct FakeLauncher {
        pub started: Arc<Mutex<Vec<ProcessSpec>>>,
        pub scripts: Arc<Mutex<VecDeque<Script>>>,
        pub fail_with: Arc<Mutex<Option<String>>>,
    }

    impl FakeLauncher {
        pub fn new() -> Self {
            Self {
                started: Arc::default(),
                scripts: Arc::default(),
                fail_with: Arc::default(),
            }
        }
    }

    struct FakeChild {
        polls: usize,
        script: Script,
        capture: bool,
    }

    impl Child for FakeChild {
        fn pid(&self) -> u32 {
            4242
        }
        fn poll(&mut self) -> Result<Option<ExitInfo>> {
            if self.polls >= self.script.polls_before_exit {
                Ok(Some(self.script.exit.clone()))
            } else {
                self.polls += 1;
                Ok(None)
            }
        }
        fn captured(&self) -> Option<Captured> {
            if !self.capture {
                return None;
            }
            Some(Captured {
                text: self.script.output.clone().unwrap_or_default(),
                truncated: false,
            })
        }
    }

    impl ProcessLauncher for FakeLauncher {
        fn start(&self, spec: &ProcessSpec) -> Result<Box<dyn Child>> {
            if let Some(msg) = self.fail_with.lock().unwrap().clone() {
                return Err(Error::Invalid(msg));
            }
            self.started.lock().unwrap().push(spec.clone());
            let script = self.scripts.lock().unwrap().pop_front().unwrap_or(Script {
                polls_before_exit: 0,
                exit: ExitInfo {
                    code: Some(0),
                    success: true,
                },
                output: None,
            });
            Ok(Box::new(FakeChild {
                polls: 0,
                script,
                capture: spec.capture_bytes.is_some(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(exe: &str) -> LaunchPlan {
        LaunchPlan {
            exe: exe.into(),
            args: vec![],
            working_dir: String::new(),
            env: vec![],
            explanation: String::new(),
        }
    }

    fn folder_with_exe() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        std::fs::write(base.join("game.exe"), "x").unwrap();
        std::fs::create_dir_all(base.join("bin")).unwrap();
        std::fs::write(base.join("bin/other.EXE"), "x").unwrap();
        std::fs::write(base.join("readme.txt"), "x").unwrap();
        (dir, base)
    }

    #[test]
    fn a_good_plan_resolves_to_paths_inside_the_folder() {
        let (_d, base) = folder_with_exe();
        let mut p = plan("bin/other.EXE");
        p.working_dir = "bin".into();
        p.args = vec!["--a".into()];
        let spec = resolve_plan(&base, &p, None).unwrap();
        assert!(spec.exe.starts_with(&base) && spec.cwd.starts_with(&base));
        assert_eq!(spec.args, ["--a"]);
        assert_eq!(spec.capture_bytes, None);
    }

    #[test]
    fn a_program_outside_the_folder_or_not_an_exe_or_missing_is_refused() {
        let (_d, base) = folder_with_exe();
        for bad in ["../game.exe", "/windows/system32/cmd.exe", "C:\\x.exe", ""] {
            assert!(resolve_plan(&base, &plan(bad), None).is_err(), "{bad:?}");
        }
        assert!(resolve_plan(&base, &plan("readme.txt"), None).is_err());
        assert!(resolve_plan(&base, &plan("missing.exe"), None).is_err());
        assert!(resolve_plan(&base, &plan("bin"), None).is_err());
        let mut p = plan("game.exe");
        p.working_dir = "../elsewhere".into();
        assert!(resolve_plan(&base, &p, None).is_err());
        p.working_dir = "nodir".into();
        assert!(resolve_plan(&base, &p, None).is_err());
    }

    #[test]
    fn argument_limits_are_exact() {
        let ok: Vec<String> = (0..MAX_ARGS).map(|i| i.to_string()).collect();
        assert!(check_args_and_env(&ok, &[]).is_ok());
        let mut many = ok.clone();
        many.push("x".into());
        assert!(check_args_and_env(&many, &[]).is_err());
        assert!(check_args_and_env(&["a".repeat(MAX_ARG_LEN)], &[]).is_ok());
        assert!(check_args_and_env(&["a".repeat(MAX_ARG_LEN + 1)], &[]).is_err());
        assert!(check_args_and_env(&["a\0b".into()], &[]).is_err());
    }

    #[test]
    fn environment_names_and_values_are_checked() {
        let e = |k: &str, v: &str| vec![(k.to_owned(), v.to_owned())];
        assert!(check_args_and_env(&[], &e("MY_FLAG", "1")).is_ok());
        for bad in [
            "", "1X", "A B", "A=B", "A\0B", "A-B", "PATH", "path", "ComSpec",
        ] {
            assert!(check_args_and_env(&[], &e(bad, "v")).is_err(), "{bad:?}");
        }
        assert!(check_args_and_env(&[], &e("X", &"v".repeat(MAX_ENV_VALUE_LEN))).is_ok());
        assert!(check_args_and_env(&[], &e("X", &"v".repeat(MAX_ENV_VALUE_LEN + 1))).is_err());
        assert!(check_args_and_env(&[], &e("X", "a\0b")).is_err());
        let many: Vec<_> = (0..=MAX_ENV)
            .map(|i| (format!("V{i}"), "1".into()))
            .collect();
        assert!(check_args_and_env(&[], &many).is_err());
        assert!(check_args_and_env(&[], &many[..MAX_ENV]).is_ok());
    }

    /// Copying a program and starting it must not overlap with another test's start: a start in another
    /// thread can briefly hold the copy's write handle, and Linux then refuses to run the file ("text file busy").
    static START_LOCK: Mutex<()> = Mutex::new(());

    /// Copy the running test program into a fresh folder as `game.exe` and start it with `args`, with no shell.
    fn copy_and_start(
        args: &[&str],
        capture_bytes: Option<usize>,
    ) -> (tempfile::TempDir, Box<dyn Child>) {
        let _held = START_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        std::fs::copy(std::env::current_exe().unwrap(), base.join("game.exe")).unwrap();
        let mut p = plan("game.exe");
        p.args = args.iter().map(|a| a.to_string()).collect();
        let spec = resolve_plan(&base, &p, capture_bytes).unwrap();
        let child = StdLauncher.start(&spec).unwrap();
        (dir, child)
    }

    fn wait_for_exit(child: &mut Box<dyn Child>) -> ExitInfo {
        for _ in 0..600 {
            if let Some(x) = child.poll().unwrap() {
                return x;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        panic!("the program should have ended");
    }

    /// A real start: the running test program, asked to list its tests.
    #[test]
    fn the_real_launcher_starts_a_program_and_reports_how_it_ended() {
        let (_dir, mut child) = copy_and_start(&["--list"], Some(DEFAULT_CAPTURE_BYTES));
        assert!(child.pid() > 0);
        let exit = wait_for_exit(&mut child);
        assert!(exit.success, "{exit:?}");
        // the output reader runs on its own thread; give it a moment to catch up
        let mut text = String::new();
        for _ in 0..100 {
            text = child.captured().unwrap().text;
            if text.contains("tests") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(text.contains(": test"), "captured: {text:?}");
    }

    #[test]
    fn output_is_discarded_unless_asked_for_and_capped_when_asked() {
        let (_d1, mut quiet) = copy_and_start(&["--list"], None);
        assert!(quiet.captured().is_none());
        wait_for_exit(&mut quiet);

        let (_d2, mut tiny) = copy_and_start(&["--list"], Some(10));
        wait_for_exit(&mut tiny);
        let mut got = tiny.captured().unwrap();
        for _ in 0..100 {
            if got.truncated {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
            got = tiny.captured().unwrap();
        }
        assert!(got.truncated);
        assert!(got.text.len() <= 10, "kept {} bytes", got.text.len());
    }

    #[test]
    fn a_program_that_cannot_start_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        // present, an .exe by name, but not a program
        std::fs::write(base.join("game.exe"), "not a program").unwrap();
        let spec = resolve_plan(&base, &plan("game.exe"), None).unwrap();
        assert!(StdLauncher.start(&spec).is_err());
    }
}
