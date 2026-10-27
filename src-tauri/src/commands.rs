//! IPC commands for the game library and Steam launching.
//!
//! These are thin wrappers: every rule (which ticks are allowed, how a scan is merged, what a launch
//! does) lives in `sul-core` where it is tested without a window. The commands only move data
//! between the webview and the core, and run slow or blocking work off the UI thread.
//!
//! Errors reach the webview as plain text, which the UI shows as is.

use std::path::Path;
use std::sync::Arc;

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
    steam: SteamManager,
}

impl AppState {
    /// Open the library stored at `library_file` and wire Steam to the live system.
    pub fn open(library_file: &Path) -> Self {
        Self::with_steam(library_file, SteamManager::system())
    }

    /// As [`open`](Self::open), with the given Steam manager (tests).
    pub fn with_steam(library_file: &Path, steam: SteamManager) -> Self {
        let library = LibraryService::open(library_file)
            .map(Arc::new)
            .map_err(|e| format!("the game library could not be opened: {e}"));
        Self { library, steam }
    }

    fn library(&self) -> Result<&LibraryService, String> {
        self.library.as_deref().map_err(Clone::clone)
    }

    fn games(&self) -> Result<Vec<GameView>, String> {
        Ok(self.library()?.games())
    }

    fn scan(&self) -> Result<ScanView, String> {
        self.library()?.scan_system().map_err(|e| e.to_string())
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
        let state = AppState::with_steam(
            dir.path(),
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
