//! IPC commands for the game library and Steam launching.
//!
//! These are thin wrappers: every rule (which ticks are allowed, how a scan is merged, what a launch
//! does) lives in `sul-core` where it is tested without a window. The commands only move data
//! between the webview and the core, and run slow or blocking work off the UI thread.
//!
//! Errors reach the webview as plain text, which the UI shows as is.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sul_core::manager::{
    BuildSource, BuiltView, ManagerService, ManagerView, STATE_FOLDER, VerifiedView,
};
use sul_core::manifest::Variant;
use sul_core::registry::{GameScanner, ScanOptions, system_scanner};
use sul_core::roots::{DiskSpace, RootKind, SystemDiskSpace};
use sul_core::service::{GameView, LibraryService, MatrixView, ScanView};
use sul_core::steam::SteamManager;
use tauri::State;

/// What the commands share, built once at startup. Cloning is cheap and every clone shares the
/// same library and Steam manager, which is how a command hands it to the blocking thread pool.
#[derive(Clone)]
pub struct AppState {
    /// The library, or why it could not be opened. The app still starts without it, and each
    /// command then reports the reason instead of the whole app failing.
    library: Result<Arc<LibraryService>, String>,
    /// Library folders and managed builds (spec Module 1), kept in `managed/` beside the library file.
    manager: Result<Arc<ManagerService>, String>,
    steam: SteamManager,
}

impl AppState {
    /// Open the library stored at `library_file` and wire Steam to the live system.
    pub fn open(library_file: &Path) -> Self {
        Self::with_steam(library_file, SteamManager::system())
    }

    /// As [`open`](Self::open), with the given Steam manager (tests).
    pub fn with_steam(library_file: &Path, steam: SteamManager) -> Self {
        Self::with_parts(library_file, steam, Box::new(SystemDiskSpace))
    }

    /// As [`with_steam`](Self::with_steam), with the given free-space reader (tests; the real one needs Windows).
    pub fn with_parts(
        library_file: &Path,
        steam: SteamManager,
        disk: Box<dyn DiskSpace + Send>,
    ) -> Self {
        let library = LibraryService::open(library_file)
            .map(Arc::new)
            .map_err(|e| format!("the game library could not be opened: {e}"));
        let state_dir = library_file
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(STATE_FOLDER);
        let manager = ManagerService::with_disk(&state_dir, disk)
            .map(Arc::new)
            .map_err(|e| format!("the build manager could not be opened: {e}"));
        let state = Self {
            library,
            manager,
            steam,
        };
        state.protect_library_installs();
        state
    }

    fn manager(&self) -> Result<&ManagerService, String> {
        self.manager.as_deref().map_err(Clone::clone)
    }

    /// Tell the build manager which folders the library lists as installs, so they are protected before
    /// anything is read or written near them. Runs at start-up and after every scan.
    fn protect_library_installs(&self) {
        if let (Ok(library), Ok(manager)) = (self.library(), self.manager()) {
            let dirs: Vec<PathBuf> = library
                .games()
                .iter()
                .map(|g| PathBuf::from(&g.install_dir))
                .collect();
            let _ = manager.protect_installs(&dirs);
        }
    }

    /// The folder and name of a game the last scan found: what an import or a register-in-place is given.
    /// A game the last scan did not find is refused, because its folder may be gone or may now be something else.
    fn build_source(
        &self,
        game_id: &str,
        variant: Variant,
        base_version: String,
        source_description: String,
    ) -> Result<BuildSource, String> {
        let game = self
            .games()?
            .into_iter()
            .find(|g| g.id == game_id)
            .ok_or_else(|| format!("the library has no game {game_id:?}"))?;
        if !game.present {
            return Err(format!(
                "the last scan did not find {}; scan again before using its folder",
                game.name
            ));
        }
        Ok(BuildSource {
            game_id: game.id,
            display_name: game.name,
            install_dir: PathBuf::from(game.install_dir),
            variant,
            base_version,
            source_description,
        })
    }

    fn library(&self) -> Result<&LibraryService, String> {
        self.library.as_deref().map_err(Clone::clone)
    }

    fn games(&self) -> Result<Vec<GameView>, String> {
        Ok(self.library()?.games())
    }

    fn scan(&self) -> Result<ScanView, String> {
        let scanner = system_scanner(ScanOptions::default()).map_err(|e| e.to_string())?;
        self.scan_with(scanner.as_ref())
    }

    /// Scan with `scanner`, then protect every install folder the library now lists (a game found by this scan
    /// is protected before anything is built near it). The real scan is [`scan`](Self::scan); tests give their own.
    pub(crate) fn scan_with(&self, scanner: &dyn GameScanner) -> Result<ScanView, String> {
        let scan = self.library()?.scan(scanner).map_err(|e| e.to_string())?;
        self.protect_library_installs();
        Ok(scan)
    }

    fn matrix(&self, game_id: &str) -> Result<MatrixView, String> {
        self.library()?.matrix(game_id).map_err(|e| e.to_string())
    }

    fn tick(&self, game_id: &str, feature: &str, enabled: bool) -> Result<MatrixView, String> {
        self.library()?
            .set_tick(game_id, feature, enabled)
            .map_err(|e| e.to_string())
    }

    fn launch(&self, app_id: u32) -> Result<(), String> {
        self.steam.launch_steam_game(app_id)
    }
}

/// Run `work` on the blocking thread pool so a slow scan or a file write cannot freeze the window.
async fn blocking<T, F>(work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| format!("background task failed: {e}"))?
}

/// The games the library already knows, without scanning. Games the last scan did not find are
/// included and marked `present: false`.
#[tauri::command]
pub async fn list_games(state: State<'_, AppState>) -> Result<Vec<GameView>, String> {
    state.games()
}

/// Scan the Windows registry, every Steam library (`libraryfolders.vdf`, on any drive) and the
/// fixed drives, merge the result into the library and return the library afterwards.
/// Errors on other platforms.
#[tauri::command]
pub async fn scan_games_command(state: State<'_, AppState>) -> Result<ScanView, String> {
    let app = state.inner().clone();
    blocking(move || app.scan()).await
}

/// The persisted tick-and-compatibility matrix of one game.
#[tauri::command]
pub async fn get_game_matrix(
    game_id: String,
    state: State<'_, AppState>,
) -> Result<MatrixView, String> {
    state.matrix(&game_id)
}

/// Tick or untick one feature (`feature` is a feature id such as `steam_headless`) and return
/// the game's updated matrix. The core refuses ticks the matrix does not allow, for example
/// Steam headless launch on a game that is not from Steam, and nothing is changed then.
#[tauri::command]
pub async fn update_feature_tick(
    game_id: String,
    feature: String,
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<MatrixView, String> {
    let app = state.inner().clone();
    blocking(move || app.tick(&game_id, &feature, enabled)).await
}

/// Ask Steam to start the game with this app id. Needs no Steam password: the request goes to the
/// running client, or starts Steam first when it is closed. This asks; it does not wait for the game.
#[tauri::command]
pub async fn launch_game_command(app_id: u32, state: State<'_, AppState>) -> Result<(), String> {
    let app = state.inner().clone();
    blocking(move || app.launch(app_id)).await
}

// ---------------------------------------------------------------------------------------------
// Module 1: library folders and managed builds (spec 1.2, 1.3, 1.5)
// ---------------------------------------------------------------------------------------------

/// The library folders, managed builds, active slots, old copies and operations that never finished.
#[tauri::command]
pub async fn manager_view(state: State<'_, AppState>) -> Result<ManagerView, String> {
    state.manager()?.view().map_err(|e| e.to_string())
}

/// Register a folder as a library folder. `kind` is `builds` or `mods`. The folder must exist.
#[tauri::command]
pub async fn manager_add_root(
    id: String,
    label: String,
    path: String,
    kind: RootKind,
    state: State<'_, AppState>,
) -> Result<ManagerView, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .add_root(&id, &label, Path::new(&path), kind)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Point a library folder at its place again after it moved. Only a folder with the right marker is accepted.
#[tauri::command]
pub async fn manager_relink_root(
    id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<ManagerView, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .relink_root(&id, Path::new(&path))
            .map_err(|e| e.to_string())
    })
    .await
}

/// Stop listing a library folder. Nothing in the folder is touched. Refused while a build is stored there.
#[tauri::command]
pub async fn manager_forget_root(
    id: String,
    state: State<'_, AppState>,
) -> Result<ManagerView, String> {
    let app = state.inner().clone();
    blocking(move || app.manager()?.forget_root(&id).map_err(|e| e.to_string())).await
}

/// Copy a library game's install into a library folder as a managed build. The original is only read.
/// This can take a long time for a large game, and nothing reports progress while it runs.
#[tauri::command]
pub async fn manager_import_build(
    game_id: String,
    variant: Variant,
    base_version: String,
    source_description: String,
    root_id: String,
    state: State<'_, AppState>,
) -> Result<BuiltView, String> {
    let app = state.inner().clone();
    blocking(move || {
        let source = app.build_source(&game_id, variant, base_version, source_description)?;
        app.manager()?
            .import_build(&source, &root_id)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Record a library game's install, where it is, as a read-only baseline. Nothing is copied or written.
#[tauri::command]
pub async fn manager_register_in_place(
    game_id: String,
    variant: Variant,
    base_version: String,
    source_description: String,
    state: State<'_, AppState>,
) -> Result<BuiltView, String> {
    let app = state.inner().clone();
    blocking(move || {
        let source = app.build_source(&game_id, variant, base_version, source_description)?;
        app.manager()?
            .register_in_place(&source)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Compare a build's files with its manifest. Changes nothing but the recorded result.
#[tauri::command]
pub async fn manager_verify_build(
    build_id: String,
    state: State<'_, AppState>,
) -> Result<VerifiedView, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .verify_build(&build_id)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Make a build the active one of its slot. It is verified first and must match its manifest.
#[tauri::command]
pub async fn manager_activate_build(
    build_id: String,
    state: State<'_, AppState>,
) -> Result<ManagerView, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .activate_build(&build_id)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Return a slot to the most recent earlier build that still verifies.
#[tauri::command]
pub async fn manager_rollback_slot(
    game_id: String,
    slot: String,
    state: State<'_, AppState>,
) -> Result<ManagerView, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .rollback_slot(&game_id, &slot)
            .map_err(|e| e.to_string())
    })
    .await
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use sul_core::steam::{ClientProbe, UrlOpener};

    use super::*;

    #[derive(Clone, Default)]
    struct Opened(Arc<Mutex<Vec<String>>>);

    impl UrlOpener for Opened {
        fn open(&self, url: &str) -> sul_core::Result<()> {
            self.0.lock().unwrap().push(url.to_owned());
            Ok(())
        }
    }

    struct Closed;

    impl ClientProbe for Closed {
        fn client_running(&self) -> bool {
            false
        }
    }

    fn state_with(opened: &Opened) -> (tempfile::TempDir, AppState) {
        let dir = tempfile::tempdir().unwrap();
        let steam = SteamManager::with_parts(Box::new(Closed), Box::new(opened.clone()));
        let state = AppState::with_steam(&dir.path().join("library.json"), steam);
        (dir, state)
    }

    #[test]
    fn a_fresh_library_is_empty_and_has_no_matrices() {
        let (_dir, state) = state_with(&Opened::default());
        assert!(state.games().unwrap().is_empty());
        let error = state.matrix("steam-553850").unwrap_err();
        assert!(error.contains("steam-553850"), "{error}");
    }

    #[test]
    fn ticks_for_unknown_games_or_features_are_refused() {
        let (_dir, state) = state_with(&Opened::default());
        assert!(state.tick("steam-553850", "mods", true).is_err());
        let error = state
            .tick("steam-553850", "no_such_feature", true)
            .unwrap_err();
        assert!(error.contains("no_such_feature"), "{error}");
    }

    #[test]
    fn launching_goes_through_the_steam_manager() {
        let opened = Opened::default();
        let (_dir, state) = state_with(&opened);

        state.launch(553850).unwrap();
        assert_eq!(*opened.0.lock().unwrap(), ["steam://run/553850"]);

        assert!(state.launch(0).is_err(), "app id 0 is not a game");
        assert_eq!(
            opened.0.lock().unwrap().len(),
            1,
            "nothing was opened for it"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn scanning_reports_that_it_needs_windows() {
        let (_dir, state) = state_with(&Opened::default());
        let error = state.scan().unwrap_err();
        assert!(error.to_lowercase().contains("windows"), "{error}");
    }

    #[test]
    fn a_library_that_cannot_open_is_reported_by_every_command_not_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        // A folder where the library file should be: it can be neither read nor replaced as a file.
        let library_file = dir.path().join("library.json");
        std::fs::create_dir(&library_file).unwrap();
        let state = AppState::with_steam(
            &library_file,
            SteamManager::with_parts(Box::new(Closed), Box::new(Opened::default())),
        );
        for error in [
            state.games().unwrap_err(),
            state.matrix("x").unwrap_err(),
            state.tick("x", "mods", true).unwrap_err(),
        ] {
            assert!(
                error.starts_with("the game library could not be opened"),
                "{error}"
            );
        }
        // Launching does not depend on the library.
        assert!(state.launch(480).is_ok());
    }
}
