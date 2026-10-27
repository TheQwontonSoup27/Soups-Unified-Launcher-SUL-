//! Steam integration (spec section 1).
//!
//! # Account safety
//!
//! SUL never sees, stores or manages a Steam password, token or any other credential. Steam stays
//! logged in; SUL only talks to the client that is already running on the machine. Public player
//! data (owned games, playtime) is meant to come from Steam's public Web API using the user's
//! SteamID64, not from the client; that HTTP side is not built yet.
//!
//! # What `steamworks-rs` can and cannot do
//!
//! These limits were read from the crate source (steamworks 0.13.1), and they decide the design:
//!
//! * It attaches to a Steam client that is **already running**. It does not log in.
//! * `Client::init` always needs an app id, and Steam then treats this process as *running that
//!   app* (friends see "Playing ..."). It also fails if the user does not own that app. Attaching
//!   with a game's own id would therefore claim the game is running without starting it, so
//!   [`SteamManager::connect`] attaches as Valve's public test app ([`ATTACH_APP_ID`]) and only when
//!   asked to. Nothing attaches implicitly.
//! * It has **no call that launches another game**. Launching is the `steam://run/<id>` URL, which
//!   the operating system hands to the running Steam client, or starts Steam first if it is closed.
//!
//! So [`SteamManager::launch_steam_game`] checks whether a client is up (a connected session, or the
//! "active process" value Steam keeps in the registry), reports which situation it found, and
//! hands the URL to the system either way. `-silent` (tray only, see [`STEAM_SILENT_ARG`]) is not
//! used on the cold-start path yet, so a closed Steam shows its window.
//!
//! The real attachment is behind the `steam` feature, which is off by default: on Windows the
//! executable then links `steam_api64.dll` and will not start without it. Nothing here writes into
//! Steam's folders.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};

use crate::registry::{Hive, KeyPath, RegistryAccess, View};
use crate::safepath::resolve_lenient;
use crate::{Error, Result};

/// Command-line flag that starts the Steam client minimised to the tray.
pub const STEAM_SILENT_ARG: &str = "-silent";

/// The app id [`SteamManager::connect`] attaches as: Spacewar, Valve's public test app. Any
/// account can initialise it, and it does not pretend a real game is running. While attached,
/// Steam shows the user as playing it.
pub const ATTACH_APP_ID: u32 = 480;

/// What an `appmanifest_<id>.acf` says about the installed build.
///
/// Both fields are informational: SUL records them (the build id is the natural baseline
/// for "which version is this?") and never edits them. Editing `AutoUpdateBehavior` would
/// modify the original installation, and Steam can reset it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SteamAppInfo {
    /// `buildid`: changes with every published Steam build.
    #[serde(default)]
    pub build_id: Option<String>,
    /// `AutoUpdateBehavior`: 0 = always update, 1 = only on launch, 2 = high priority.
    #[serde(default)]
    pub auto_update_behavior: Option<u32>,
}

/// URL that asks the logged-in Steam client to start a game. No credentials involved.
pub fn launch_url(app_id: u32) -> String {
    format!("steam://run/{app_id}")
}

/// The app id in `url` if, and only if, `url` is exactly what [`launch_url`] produces.
fn app_id_of_launch_url(url: &str) -> Option<u32> {
    let id: u32 = url.strip_prefix("steam://run/")?.parse().ok()?;
    (launch_url(id) == url).then_some(id)
}

/// Likely Steam install folders on Windows, used when the registry does not say where Steam is.
///
/// Candidates only: callers must check that they exist.
pub fn default_steam_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for var in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(base) = std::env::var_os(var).filter(|v| !v.is_empty()) {
            dirs.push(PathBuf::from(base).join("Steam"));
        }
    }
    dirs.push(PathBuf::from(r"C:\Program Files (x86)\Steam"));
    dirs.dedup();
    dirs
}

/// True if `candidate` lies inside the `steamapps` folder of any Steam library.
///
/// A managed legacy build must **not** live there: Steam updates and verifies the games
/// inside its libraries. Paths are canonicalised where they exist, so symlinks and `..`
/// cannot hide a location inside a library.
pub fn is_inside_steam_library(candidate: &Path, library_roots: &[PathBuf]) -> bool {
    let candidate = resolve_lenient(candidate);
    library_roots
        .iter()
        .any(|root| candidate.starts_with(resolve_lenient(&root.join("steamapps"))))
}

// ---------------------------------------------------------------------------
// Seams: the operating system and the registry
// ---------------------------------------------------------------------------

/// Hands a `steam://` URL to the operating system.
pub trait UrlOpener: Send + Sync {
    fn open(&self, url: &str) -> Result<()>;
}

/// The real opener: `cmd /C start` on Windows, unsupported elsewhere.
///
/// It only ever passes on the exact URL [`launch_url`] builds (digits only), so it cannot be used
/// to run an arbitrary command line.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemOpener;

impl UrlOpener for SystemOpener {
    fn open(&self, url: &str) -> Result<()> {
        if app_id_of_launch_url(url).is_none() {
            return Err(Error::Invalid(format!("not a Steam launch URL: {url:?}")));
        }
        open_with_shell(url)
    }
}

#[cfg(windows)]
fn open_with_shell(url: &str) -> Result<()> {
    use std::os::windows::process::CommandExt;

    /// Do not flash a console window.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    // `start` takes the first quoted argument as a window title, hence the empty one.
    let status = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| Error::Steam(format!("could not run cmd: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::Steam(format!("cmd /C start failed: {status}")))
    }
}

#[cfg(not(windows))]
fn open_with_shell(_url: &str) -> Result<()> {
    Err(Error::Unsupported(
        "launching Steam games is only implemented for Windows",
    ))
}

/// Answers "is a Steam client running on this machine?" without touching it.
pub trait ClientProbe: Send + Sync {
    fn client_running(&self) -> bool;
}

/// Reads the `pid` value Steam keeps under `HKCU\SOFTWARE\Valve\Steam\ActiveProcess`.
///
/// Read-only and free of side effects (unlike attaching with `steamworks`). As far as is known,
/// Steam sets `pid` to 0 when it exits cleanly, so after a crash the value can be stale; the
/// consequence is only that [`LaunchRoute::RunningClient`] is reported for a client that is gone.
/// The launch itself still starts Steam. Unverified on a real machine.
#[derive(Debug, Clone, Copy, Default)]
pub struct RegistryProbe<R>(pub R);

impl<R: RegistryAccess + Send + Sync> ClientProbe for RegistryProbe<R> {
    fn client_running(&self) -> bool {
        let key = KeyPath::new(
            Hive::CurrentUser,
            View::Registry64,
            r"SOFTWARE\Valve\Steam\ActiveProcess",
        );
        match self.0.key(&key) {
            Ok(Some(key)) => key.dword("pid").is_some_and(|pid| pid != 0),
            Ok(None) => false,
            Err(error) => {
                tracing::debug!(%error, "could not read Steam's active process");
                false
            }
        }
    }
}

/// For hosts without a Steam registry (anything but Windows): never running.
#[cfg(not(windows))]
#[derive(Debug, Clone, Copy, Default)]
struct NeverRunning;

#[cfg(not(windows))]
impl ClientProbe for NeverRunning {
    fn client_running(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// The manager
// ---------------------------------------------------------------------------

/// Which situation a launch request found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchRoute {
    /// A Steam client was up; the request went to it.
    RunningClient,
    /// No client was up; the request makes the system start Steam, which then launches the game.
    StartedSteam,
}

/// Who the attached Steam client is signed in as. Public facts only; no credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SteamIdentity {
    /// SteamID64 as text: the value does not fit a JavaScript number.
    pub steam_id64: String,
    pub persona_name: String,
}

/// The live attachment. Without the `steam` feature no value of this type can exist.
#[cfg(feature = "steam")]
type Attachment = steamworks::Client;
#[cfg(not(feature = "steam"))]
type Attachment = std::convert::Infallible;

struct Inner {
    probe: Box<dyn ClientProbe>,
    opener: Box<dyn UrlOpener>,
    /// The explicit `steamworks` attachment, if [`SteamManager::connect`] made one.
    session: Mutex<Option<Attachment>>,
    /// Held for the whole of a launch so two quick clicks cannot interleave.
    launching: Mutex<()>,
}

/// Thread-safe Steam manager: cheap to clone, every clone shares one state.
#[derive(Clone)]
pub struct SteamManager {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for SteamManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SteamManager")
            .field("connected", &self.is_connected())
            .finish_non_exhaustive()
    }
}

impl Default for SteamManager {
    fn default() -> Self {
        Self::system()
    }
}

/// A poisoned lock only means another thread panicked while holding it; the data is still usable.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl SteamManager {
    /// Wired to the live system.
    pub fn system() -> Self {
        #[cfg(windows)]
        let probe: Box<dyn ClientProbe> = Box::new(RegistryProbe(crate::registry::WindowsRegistry));
        #[cfg(not(windows))]
        let probe: Box<dyn ClientProbe> = Box::new(NeverRunning);
        Self::with_parts(probe, Box::new(SystemOpener))
    }

    /// Wired to the given parts: tests, or a host with its own way of opening URLs.
    pub fn with_parts(probe: Box<dyn ClientProbe>, opener: Box<dyn UrlOpener>) -> Self {
        Self {
            inner: Arc::new(Inner {
                probe,
                opener,
                session: Mutex::new(None),
                launching: Mutex::new(()),
            }),
        }
    }

    /// Whether [`connect`](Self::connect) has attached and not yet been undone.
    pub fn is_connected(&self) -> bool {
        lock(&self.inner.session).is_some()
    }

    /// Attach to the running Steam client through `steamworks` and report who is signed in.
    ///
    /// Opt-in and side-effecting: Steam shows the user as playing [`ATTACH_APP_ID`] until
    /// [`disconnect`](Self::disconnect). Fails when Steam is not running. Needs the `steam`
    /// feature. Calling it again while attached just re-reads the identity.
    #[cfg(feature = "steam")]
    pub fn connect(&self) -> Result<SteamIdentity> {
        let mut slot = lock(&self.inner.session);
        let client = match slot.as_ref() {
            Some(client) => client.clone(),
            None => {
                let client = steamworks::Client::init_app(ATTACH_APP_ID)
                    .map_err(|e| Error::Steam(e.to_string()))?;
                *slot = Some(client.clone());
                client
            }
        };
        Ok(SteamIdentity {
            steam_id64: client.user().steam_id().raw().to_string(),
            persona_name: client.friends().name(),
        })
    }

    /// Without the `steam` feature there is nothing to attach to.
    #[cfg(not(feature = "steam"))]
    pub fn connect(&self) -> Result<SteamIdentity> {
        Err(Error::Unsupported("built without the `steam` feature"))
    }

    /// Let go of the attachment. Steam stops showing the user as playing [`ATTACH_APP_ID`].
    pub fn disconnect(&self) {
        lock(&self.inner.session).take();
    }

    /// Whether a Steam client is up: an attached session, or the registry's active process.
    pub fn client_running(&self) -> bool {
        self.is_connected() || self.inner.probe.client_running()
    }

    /// Ask Steam to start a game and say how the request was routed.
    ///
    /// This asks; it does not wait for the game. Whether the game actually starts depends on Steam
    /// (ownership, installation, updates) and is not known here.
    pub fn launch(&self, app_id: u32) -> Result<LaunchRoute> {
        if app_id == 0 {
            return Err(Error::Invalid("Steam app id 0 is not a game".to_owned()));
        }
        let _one_launch_at_a_time = lock(&self.inner.launching);
        let route = if self.client_running() {
            LaunchRoute::RunningClient
        } else {
            LaunchRoute::StartedSteam
        };
        self.inner.opener.open(&launch_url(app_id))?;
        tracing::info!(app_id, ?route, "asked Steam to start a game");
        Ok(route)
    }

    /// [`launch`](Self::launch) with the error as text, for callers that only show a message.
    pub fn launch_steam_game(&self, app_id: u32) -> std::result::Result<(), String> {
        self.launch(app_id).map(|_| ()).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::registry::{RawKey, RegValue};

    // --- helpers ---------------------------------------------------------

    #[derive(Default)]
    struct Recorder {
        urls: Mutex<Vec<String>>,
        fail: bool,
        in_flight: AtomicUsize,
        max_in_flight: AtomicUsize,
    }

    impl UrlOpener for Arc<Recorder> {
        fn open(&self, url: &str) -> Result<()> {
            let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_in_flight.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(2));
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            if self.fail {
                return Err(Error::Steam("no protocol handler".to_owned()));
            }
            self.urls.lock().unwrap().push(url.to_owned());
            Ok(())
        }
    }

    struct FixedProbe(bool);

    impl ClientProbe for FixedProbe {
        fn client_running(&self) -> bool {
            self.0
        }
    }

    fn manager(running: bool, recorder: &Arc<Recorder>) -> SteamManager {
        SteamManager::with_parts(Box::new(FixedProbe(running)), Box::new(recorder.clone()))
    }

    enum Fake {
        Missing,
        Pid(u32),
        Fails,
    }

    impl RegistryAccess for Fake {
        fn subkeys(&self, _: &KeyPath) -> Result<Vec<RawKey>> {
            Ok(Vec::new())
        }

        fn key(&self, path: &KeyPath) -> Result<Option<RawKey>> {
            assert_eq!(path.path, r"SOFTWARE\Valve\Steam\ActiveProcess");
            assert_eq!(path.hive, Hive::CurrentUser);
            match self {
                Fake::Missing => Ok(None),
                Fake::Pid(pid) => {
                    let mut key = RawKey::new("ActiveProcess");
                    key.insert("pid", RegValue::Dword(*pid));
                    Ok(Some(key))
                }
                Fake::Fails => Err(Error::Registry {
                    key: path.path.clone(),
                    source: std::io::Error::other("denied"),
                }),
            }
        }
    }

    // --- thread safety ---------------------------------------------------

    #[test]
    fn the_manager_can_be_shared_between_threads() {
        fn assert_send_sync<T: Send + Sync + Clone + 'static>() {}
        assert_send_sync::<SteamManager>();
    }

    #[test]
    fn launches_from_many_threads_are_serialised_and_all_delivered() {
        let recorder = Arc::new(Recorder::default());
        let steam = manager(true, &recorder);
        let threads: Vec<_> = (1..=8)
            .map(|id| {
                let steam = steam.clone();
                std::thread::spawn(move || steam.launch(id).unwrap())
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(
            recorder.max_in_flight.load(Ordering::SeqCst),
            1,
            "two launches overlapped"
        );
        let mut urls = recorder.urls.lock().unwrap().clone();
        urls.sort();
        let mut expected: Vec<_> = (1..=8).map(launch_url).collect();
        expected.sort();
        assert_eq!(urls, expected);
    }

    #[test]
    fn a_panic_while_holding_the_lock_does_not_wedge_the_manager() {
        let recorder = Arc::new(Recorder::default());
        let steam = manager(true, &recorder);
        let poisoner = steam.clone();
        let _ = std::thread::spawn(move || {
            let _held = lock(&poisoner.inner.launching);
            panic!("simulated crash in a launch");
        })
        .join();
        assert!(
            steam.launch(480).is_ok(),
            "the poisoned lock must be recovered"
        );
    }

    // --- launching -------------------------------------------------------

    #[test]
    fn launch_url_uses_the_steam_protocol_only() {
        assert_eq!(launch_url(553850), "steam://run/553850");
    }

    #[test]
    fn a_running_client_gets_the_url_and_is_reported() {
        let recorder = Arc::new(Recorder::default());
        let route = manager(true, &recorder).launch(553850).unwrap();
        assert_eq!(route, LaunchRoute::RunningClient);
        assert_eq!(*recorder.urls.lock().unwrap(), ["steam://run/553850"]);
    }

    #[test]
    fn a_closed_client_falls_back_to_starting_steam_through_the_url() {
        let recorder = Arc::new(Recorder::default());
        let route = manager(false, &recorder).launch(553850).unwrap();
        assert_eq!(route, LaunchRoute::StartedSteam);
        // Same URL either way: that is what makes the system start Steam first.
        assert_eq!(*recorder.urls.lock().unwrap(), ["steam://run/553850"]);
    }

    #[test]
    fn app_id_zero_is_refused_without_calling_the_system() {
        let recorder = Arc::new(Recorder::default());
        let error = manager(true, &recorder).launch(0).unwrap_err();
        assert!(matches!(error, Error::Invalid(_)));
        assert!(recorder.urls.lock().unwrap().is_empty());
    }

    #[test]
    fn a_failing_system_call_is_reported_not_swallowed() {
        let recorder = Arc::new(Recorder {
            fail: true,
            ..Recorder::default()
        });
        let steam = manager(false, &recorder);
        assert!(matches!(steam.launch(480), Err(Error::Steam(_))));
        let message = steam.launch_steam_game(480).unwrap_err();
        assert!(message.contains("no protocol handler"), "{message}");
    }

    #[test]
    fn launch_steam_game_returns_unit_on_success() {
        let recorder = Arc::new(Recorder::default());
        assert_eq!(manager(true, &recorder).launch_steam_game(220), Ok(()));
    }

    // --- the real opener's guard ----------------------------------------

    #[test]
    fn only_the_exact_launch_url_is_accepted() {
        assert_eq!(app_id_of_launch_url("steam://run/1"), Some(1));
        assert_eq!(
            app_id_of_launch_url("steam://run/4294967295"),
            Some(u32::MAX)
        );
        for bad in [
            "",
            "steam://run/",
            "steam://run/007",       // not canonical
            "steam://run/+7",        // parse() would accept the sign
            "steam://run/12 & calc", // shell metacharacters
            "steam://run/12\"",
            "steam://run/4294967296", // overflows u32
            "steam://rungameid/12",
            "https://example.com/steam://run/12",
            " steam://run/12",
        ] {
            assert_eq!(app_id_of_launch_url(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_system_opener_refuses_anything_but_a_launch_url() {
        for bad in ["calc.exe", "steam://run/1 & calc", "https://example.com"] {
            assert!(
                matches!(SystemOpener.open(bad), Err(Error::Invalid(_))),
                "{bad:?}"
            );
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn off_windows_the_system_opener_says_it_is_unsupported() {
        assert!(matches!(
            SystemOpener.open(&launch_url(480)),
            Err(Error::Unsupported(_))
        ));
    }

    // --- the registry probe ---------------------------------------------

    #[test]
    fn the_probe_reads_steams_active_process() {
        assert!(RegistryProbe(Fake::Pid(4242)).client_running());
        assert!(
            !RegistryProbe(Fake::Pid(0)).client_running(),
            "pid 0 = exited"
        );
        assert!(!RegistryProbe(Fake::Missing).client_running());
        assert!(
            !RegistryProbe(Fake::Fails).client_running(),
            "unreadable = not running"
        );
    }

    // --- the attachment --------------------------------------------------

    #[cfg(not(feature = "steam"))]
    #[test]
    fn connecting_requires_the_steam_feature() {
        let recorder = Arc::new(Recorder::default());
        let steam = manager(true, &recorder);
        assert!(matches!(steam.connect(), Err(Error::Unsupported(_))));
        assert!(!steam.is_connected());
        steam.disconnect(); // harmless when never connected
    }

    #[test]
    fn identity_serialises_the_steam_id_as_text() {
        let identity = SteamIdentity {
            steam_id64: "76561197960287930".to_owned(),
            persona_name: "Gabe".to_owned(),
        };
        let json = serde_json::to_value(&identity).unwrap();
        assert_eq!(json["steam_id64"], "76561197960287930");
        // Above 2^53: squeezed through a double (what a JSON number becomes in JavaScript) the
        // value changes, which is why it travels as text.
        let id: u64 = identity.steam_id64.parse().unwrap();
        assert_ne!(id, (id as f64) as u64);
    }

    #[test]
    fn routes_serialise_in_snake_case() {
        assert_eq!(
            serde_json::to_value(LaunchRoute::RunningClient).unwrap(),
            "running_client"
        );
        assert_eq!(
            serde_json::to_value(LaunchRoute::StartedSteam).unwrap(),
            "started_steam"
        );
    }

    // --- read-only helpers (unchanged behaviour) --------------------------

    #[test]
    fn default_dirs_always_offer_the_standard_location() {
        let dirs = default_steam_dirs();
        assert!(dirs.contains(&PathBuf::from(r"C:\Program Files (x86)\Steam")));
        // Compared as text: on non-Windows hosts a backslash path is a single component.
        assert!(dirs.iter().all(|d| d.to_string_lossy().ends_with("Steam")));
    }

    #[test]
    fn managed_builds_must_stay_outside_steam_libraries() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join("steamapps").join("common").join("game")).unwrap();
        fs::create_dir_all(root.join("SUL-builds").join("legacy")).unwrap();
        let libraries = vec![root.clone()];

        let inside = root.join("steamapps").join("common").join("game");
        assert!(is_inside_steam_library(&inside, &libraries));
        // A not-yet-created folder under steamapps is still inside.
        assert!(is_inside_steam_library(
            &inside.join("new_build"),
            &libraries
        ));
        // `..` cannot be used to sneak back in.
        let sneaky = root
            .join("SUL-builds")
            .join("..")
            .join("steamapps")
            .join("x");
        assert!(is_inside_steam_library(&sneaky, &libraries));

        assert!(!is_inside_steam_library(
            &root.join("SUL-builds").join("legacy"),
            &libraries
        ));
        assert!(!is_inside_steam_library(
            Path::new("/definitely/elsewhere"),
            &libraries
        ));
        assert!(!is_inside_steam_library(&inside, &[]));
    }
}
