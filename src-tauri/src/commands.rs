//! IPC commands for the game library and Steam launching.
//!
//! These are thin wrappers: every rule (which ticks are allowed, how a scan is merged, what a launch
//! does) lives in `sul-core` where it is tested without a window. The commands only move data
//! between the webview and the core, and run slow or blocking work off the UI thread.
//!
//! Errors reach the webview as plain text, which the UI shows as is.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sul_core::adapters::AdapterRegistry;
use sul_core::builder::recipe::{Recipe, RunPlan, RunRecord};
use sul_core::builder::tools::{StoredRunRecords, ToolRunPlan};
use sul_core::catalogue::GameDetail;
use sul_core::manager::{
    BuildSource, BuilderView, BuiltView, CatalogueView, Deployed, Edited, GameRef, LaunchView,
    ManagerService, ManagerView, ModFileList, ModSource, ModVerified, ModsView, ProfileEdit,
    Purged, PurgedAndDeployed, RecipeRan, Refreshed, STATE_FOLDER, Started, ToolChecked,
    ToolPolled, ToolRegisterRequest, ToolRunRequest, ToolStarted, ToolsView, Undone, VerifiedView,
    WorkspaceRef,
};
use sul_core::manifest::{Variant, VerifyReport};
use sul_core::mods::deploy::{DeployPlan, PurgePlan};
use sul_core::process::{ProcessLauncher, StdLauncher};
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

    /// As [`open`](Self::open), with the given Steam manager (tests). The tools the owner records are started
    /// with the real launcher: this is the one constructor, with `open`, that can start a program.
    pub fn with_steam(library_file: &Path, steam: SteamManager) -> Self {
        Self::with_parts_and_launcher(
            library_file,
            steam,
            Box::new(SystemDiskSpace),
            AdapterRegistry::new(),
            Box::new(StdLauncher),
        )
    }

    /// As [`with_steam`](Self::with_steam), with the given free-space reader (tests; the real one needs Windows), the
    /// game adapters this build knows and what starts the programs of the Tools tab. The app registers no adapter:
    /// no adapter for a real game exists yet, so no mod is deployed for any game.
    pub fn with_parts_and_launcher(
        library_file: &Path,
        steam: SteamManager,
        disk: Box<dyn DiskSpace + Send>,
        adapters: AdapterRegistry,
        launcher: Box<dyn ProcessLauncher + Send>,
    ) -> Self {
        Self::assemble(library_file, steam, |dir| {
            ManagerService::with_parts_and_launcher(dir, disk, adapters, launcher)
        })
    }

    fn assemble(
        library_file: &Path,
        steam: SteamManager,
        open_manager: impl FnOnce(&Path) -> sul_core::Result<ManagerService>,
    ) -> Self {
        let library = LibraryService::open(library_file)
            .map(Arc::new)
            .map_err(|e| format!("the game library could not be opened: {e}"));
        let state_dir = library_file
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(STATE_FOLDER);
        let manager = open_manager(&state_dir)
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

    /// The games the library lists as `(id, name)`: what the catalogue is told, so the names on its pages are the
    /// library's and never the page's.
    fn game_refs(&self) -> Result<Vec<GameRef>, String> {
        Ok(self.games()?.into_iter().map(|g| (g.id, g.name)).collect())
    }

    /// One game the library lists. A game the library does not list is refused, so no catalogue entry is ever made
    /// for a name nobody has seen.
    fn game_ref(&self, game_id: &str) -> Result<GameRef, String> {
        self.game_refs()?
            .into_iter()
            .find(|(id, _)| id == game_id)
            .ok_or_else(|| format!("the library has no game {game_id:?}"))
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

// ---- Mods (spec 2.1) ----

/// The mod store, the profiles, and the mod state of every managed build.
#[tauri::command]
pub async fn mods_view(state: State<'_, AppState>) -> Result<ModsView, String> {
    let app = state.inner().clone();
    blocking(move || app.manager()?.mods_view().map_err(|e| e.to_string())).await
}

/// Copy a mod folder into a mod library folder as a verified copy. The folder is only read.
#[tauri::command]
pub async fn mods_import(
    source: ModSource,
    root_id: String,
    state: State<'_, AppState>,
) -> Result<ModsView, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .import_mod(&source, &root_id)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Compare a stored mod with its own inventory. Reads only.
#[tauri::command]
pub async fn mods_verify(
    game_id: String,
    mod_id: String,
    version: String,
    state: State<'_, AppState>,
) -> Result<ModVerified, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .verify_mod(&game_id, &mod_id, &version)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Make a profile for one game and one of its builds. It edits SUL's profile file and nothing else.
#[tauri::command]
pub async fn mods_create_profile(
    id: String,
    name: String,
    game_id: String,
    build_id: String,
    state: State<'_, AppState>,
) -> Result<ModsView, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .create_profile(&id, &name, &game_id, &build_id)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Change one profile. Nothing is deployed or purged and no game file is touched.
#[tauri::command]
pub async fn mods_edit_profile(
    profile_id: String,
    edit: ProfileEdit,
    state: State<'_, AppState>,
) -> Result<ModsView, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .edit_profile(&profile_id, &edit)
            .map_err(|e| e.to_string())
    })
    .await
}

/// The dry run of a deploy. Changes nothing.
#[tauri::command]
pub async fn mods_plan_deploy(
    profile_id: String,
    state: State<'_, AppState>,
) -> Result<DeployPlan, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .plan_deploy(&profile_id)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Put the enabled mods of a profile into its build. The core plans again first and refuses a blocked plan.
#[tauri::command]
pub async fn mods_deploy(
    profile_id: String,
    state: State<'_, AppState>,
) -> Result<Deployed, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .deploy(&profile_id)
            .map_err(|e| e.to_string())
    })
    .await
}

/// The dry run of a purge. Changes nothing.
#[tauri::command]
pub async fn mods_plan_purge(
    build_id: String,
    state: State<'_, AppState>,
) -> Result<PurgePlan, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .plan_purge(&build_id)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Take what is deployed out of a build, touching only what the ownership ledger proves SUL put there.
#[tauri::command]
pub async fn mods_purge(build_id: String, state: State<'_, AppState>) -> Result<Purged, String> {
    let app = state.inner().clone();
    blocking(move || app.manager()?.purge(&build_id).map_err(|e| e.to_string())).await
}

/// Put back what the latest purge took out.
#[tauri::command]
pub async fn mods_undo_purge(
    build_id: String,
    purge_id: String,
    state: State<'_, AppState>,
) -> Result<Undone, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .undo_purge(&build_id, &purge_id)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Purge what is deployed in the profile's build, then deploy the profile.
#[tauri::command]
pub async fn mods_purge_and_deploy(
    profile_id: String,
    state: State<'_, AppState>,
) -> Result<PurgedAndDeployed, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .purge_and_deploy(&profile_id)
            .map_err(|e| e.to_string())
    })
    .await
}

/// The saved recipes, the mods they can layer, the mod library folders and which games can be planned.
#[tauri::command]
pub async fn builder_view(state: State<'_, AppState>) -> Result<BuilderView, String> {
    let app = state.inner().clone();
    blocking(move || app.manager()?.builder_view().map_err(|e| e.to_string())).await
}

/// Keep a recipe: a new one (`replaces` empty) needs an unused id; a changed one names the SHA-256 of the
/// recipe it started from. Reads no mod and touches no game file.
#[tauri::command]
pub async fn builder_save_recipe(
    recipe: Recipe,
    replaces: Option<String>,
    state: State<'_, AppState>,
) -> Result<BuilderView, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .recipe_save(&recipe, replaces.as_deref())
            .map_err(|e| e.to_string())
    })
    .await
}

/// What a saved recipe would build. Writes nothing.
#[tauri::command]
pub async fn builder_plan_recipe(
    recipe_id: String,
    state: State<'_, AppState>,
) -> Result<RunPlan, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .recipe_plan(&recipe_id)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Build a saved recipe into a new run folder in a mod library's workspace.
#[tauri::command]
pub async fn builder_run_recipe(
    recipe_id: String,
    root_id: String,
    state: State<'_, AppState>,
) -> Result<RecipeRan, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .recipe_run(&recipe_id, &root_id)
            .map_err(|e| e.to_string())
    })
    .await
}

/// The provenance record of a finished run. Reads only.
#[tauri::command]
pub async fn builder_read_run(
    recipe_id: String,
    root_id: String,
    run_id: String,
    state: State<'_, AppState>,
) -> Result<RunRecord, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .recipe_read_run(&recipe_id, &root_id, &run_id)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Compare a finished run's files with its record. Reads only.
#[tauri::command]
pub async fn builder_verify_run(
    recipe_id: String,
    root_id: String,
    run_id: String,
    state: State<'_, AppState>,
) -> Result<VerifyReport, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .recipe_verify_run(&recipe_id, &root_id, &run_id)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Some of a stored mod's files with their hashes, to help write a patch step. Reads only.
#[tauri::command]
pub async fn builder_mod_files(
    game_id: String,
    mod_id: String,
    version: String,
    filter: String,
    state: State<'_, AppState>,
) -> Result<ModFileList, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .mod_files(&game_id, &mod_id, &version, &filter)
            .map_err(|e| e.to_string())
    })
    .await
}

/// The recorded tools, what is known about the two tools spec 2.4 names, the finished recipe runs a tool can work
/// in, and the tools being followed. Reads no tool file.
#[tauri::command]
pub async fn tools_view(state: State<'_, AppState>) -> Result<ToolsView, String> {
    let app = state.inner().clone();
    blocking(move || app.manager()?.tools_view().map_err(|e| e.to_string())).await
}

/// Record a program the owner installed: its full path, hashed now. Nothing is copied, downloaded or started.
#[tauri::command]
pub async fn tools_register(
    request: ToolRegisterRequest,
    state: State<'_, AppState>,
) -> Result<ToolsView, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .tool_register(&request)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Whether a tool's file is still the recorded one. Reads the file and changes nothing.
#[tauri::command]
pub async fn tools_check(id: String, state: State<'_, AppState>) -> Result<ToolChecked, String> {
    let app = state.inner().clone();
    blocking(move || app.manager()?.tool_check(&id).map_err(|e| e.to_string())).await
}

/// Accept the changed file as the tool, at a version the owner types. Names the hash the check showed.
#[tauri::command]
pub async fn tools_accept_change(
    id: String,
    new_version: String,
    seen_sha256: String,
    state: State<'_, AppState>,
) -> Result<ToolsView, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .tool_accept_change(&id, &new_version, &seen_sha256)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Exactly what a run would start, and anything that would stop it. Starts nothing.
#[tauri::command]
pub async fn tools_plan_run(
    request: ToolRunRequest,
    state: State<'_, AppState>,
) -> Result<ToolRunPlan, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .tool_plan_run(&request)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Start a tool on a finished recipe run's files. Returns at once; poll the run until it has finished.
#[tauri::command]
pub async fn tools_start_run(
    request: ToolRunRequest,
    state: State<'_, AppState>,
) -> Result<ToolStarted, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .tool_start_run(&request)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Has a followed tool finished? Once it has, its record is kept beside the run.
#[tauri::command]
pub async fn tools_poll_run(run: u64, state: State<'_, AppState>) -> Result<ToolPolled, String> {
    let app = state.inner().clone();
    blocking(move || app.manager()?.tool_poll_run(run).map_err(|e| e.to_string())).await
}

/// The records kept for a finished recipe run, newest first. Reads only.
#[tauri::command]
pub async fn tools_run_records(
    workspace: WorkspaceRef,
    state: State<'_, AppState>,
) -> Result<StoredRunRecords, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .tool_run_records(&workspace)
            .map_err(|e| e.to_string())
    })
    .await
}

/// The builds the Play panel can offer, whether each game has an adapter, what is running and the last launches.
/// Reads only: it does not ask a running program whether it ended.
#[tauri::command]
pub async fn launch_view(state: State<'_, AppState>) -> Result<LaunchView, String> {
    let app = state.inner().clone();
    blocking(move || app.manager()?.launch_view().map_err(|e| e.to_string())).await
}

/// Start a build SUL knows, through its game's adapter. A refusal (no adapter, not ready, already running, a program
/// that no longer matches its record) comes back as a record with the reason, not as an error. `capture_output` keeps
/// the program's output, capped and with secrets masked, in a log file of its own.
#[tauri::command]
pub async fn launch_start(
    build_id: String,
    capture_output: bool,
    state: State<'_, AppState>,
) -> Result<Started, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .launch_start(&build_id, capture_output)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Ask every running game whether it ended; returns the ones that did and the view after.
#[tauri::command]
pub async fn launch_refresh(state: State<'_, AppState>) -> Result<Refreshed, String> {
    let app = state.inner().clone();
    blocking(move || app.manager()?.launch_refresh().map_err(|e| e.to_string())).await
}

/// The categories in use and which games of the library carry them. Reads only.
#[tauri::command]
pub async fn catalogue_view(state: State<'_, AppState>) -> Result<CatalogueView, String> {
    let app = state.inner().clone();
    blocking(move || {
        app.manager()?
            .catalogue_view(&app.game_refs()?)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Everything SUL knows about one game of the library: builds, adapter, capability label, launch state, categories,
/// the artwork reference and plain notes about what is missing. Reads only.
#[tauri::command]
pub async fn game_detail(
    game_id: String,
    state: State<'_, AppState>,
) -> Result<GameDetail, String> {
    let app = state.inner().clone();
    blocking(move || {
        let game = app.game_ref(&game_id)?;
        app.manager()?.game_detail(&game).map_err(|e| e.to_string())
    })
    .await
}

/// Replace a game's categories. The game must be one the library lists. Returns the game's detail and the catalogue
/// view after the change.
#[tauri::command]
pub async fn catalogue_set_categories(
    game_id: String,
    categories: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Edited, String> {
    let app = state.inner().clone();
    blocking(move || {
        let game = app.game_ref(&game_id)?;
        app.manager()?
            .catalogue_set_categories(&game, &app.game_refs()?, &categories)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Point a game at an image file the owner already has (`path`), or clear the reference (`path` null). The file is
/// checked and referenced, never copied. Returns the game's detail and the catalogue view after the change.
#[tauri::command]
pub async fn catalogue_set_artwork(
    game_id: String,
    path: Option<String>,
    state: State<'_, AppState>,
) -> Result<Edited, String> {
    let app = state.inner().clone();
    blocking(move || {
        let game = app.game_ref(&game_id)?;
        app.manager()?
            .catalogue_set_artwork(&game, &app.game_refs()?, path.as_deref())
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
