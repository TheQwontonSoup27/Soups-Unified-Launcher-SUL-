//! The commands, called the way the webview calls them: through Tauri's IPC, with JSON arguments.
//!
//! This is the other half of `ui/tests/dom-wire.test.tsx`. That test checks what the page sends; this
//! one checks that the shell accepts exactly that, answers with the shapes the page reads, and that
//! a refused request reaches the page as plain text. The mock runtime opens no window.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use sul_core::registry::{GameScanner, GameSource, InstalledGame};
use sul_core::service::LibraryService;
use sul_core::steam::{ClientProbe, SteamManager, UrlOpener};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{
    INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
};
use tauri::webview::InvokeRequest;

use sul_core::adapters::{AdapterRegistry, GameAdapter, LaunchPlan, ModLayout, Recognition};
use sul_core::capability::CapabilityEntry;
use sul_core::process::{Child, ExitInfo, ProcessLauncher, ProcessSpec};
use sul_core::roots::DiskSpace;

use crate::{DiscordState, commands::AppState, handlers};

/// A disk with 500 GB free, because the real free-space reader needs Windows.
struct Plenty;

impl DiskSpace for Plenty {
    fn free_bytes(&self, _path: &std::path::Path) -> sul_core::Result<u64> {
        Ok(500_000_000_000)
    }
}

#[derive(Clone, Default)]
struct Opened(Arc<Mutex<Vec<String>>>);

impl UrlOpener for Opened {
    fn open(&self, url: &str) -> sul_core::Result<()> {
        self.0.lock().unwrap().push(url.to_owned());
        Ok(())
    }
}

struct SteamClosed;

impl ClientProbe for SteamClosed {
    fn client_running(&self) -> bool {
        false
    }
}

/// A scan result fixed in advance, so the library has games without reading this machine.
struct Found(Vec<InstalledGame>);

impl GameScanner for Found {
    fn scan(&self) -> sul_core::Result<Vec<InstalledGame>> {
        Ok(self.0.clone())
    }
}

fn game(id: &str, name: &str, source: GameSource, store_id: Option<&str>) -> InstalledGame {
    InstalledGame {
        id: id.into(),
        name: name.into(),
        install_dir: PathBuf::from(format!(r"D:\Games\{name}")),
        source,
        store_id: store_id.map(Into::into),
        publisher: None,
        steam: None,
    }
}

/// A launcher that starts nothing: it remembers what it was asked to start, and the "program" it pretends to start
/// finishes after the number of polls queued in `scripts` (none queued: at once, with code 0).
#[derive(Clone, Default)]
struct Launched {
    started: Arc<Mutex<Vec<ProcessSpec>>>,
    /// (polls before the program exits, its exit code)
    scripts: Arc<Mutex<VecDeque<(usize, i32)>>>,
}

struct PretendChild {
    polls: usize,
    exits_after: usize,
    code: i32,
}

impl Child for PretendChild {
    fn pid(&self) -> u32 {
        7777
    }
    fn poll(&mut self) -> sul_core::Result<Option<ExitInfo>> {
        if self.polls >= self.exits_after {
            return Ok(Some(ExitInfo {
                code: Some(self.code),
                success: self.code == 0,
            }));
        }
        self.polls += 1;
        Ok(None)
    }
    fn captured(&self) -> Option<sul_core::process::Captured> {
        None
    }
}

impl ProcessLauncher for Launched {
    fn start(&self, spec: &ProcessSpec) -> sul_core::Result<Box<dyn Child>> {
        self.started.lock().unwrap().push(spec.clone());
        let (exits_after, code) = self.scripts.lock().unwrap().pop_front().unwrap_or((0, 0));
        Ok(Box::new(PretendChild {
            polls: 0,
            exits_after,
            code,
        }))
    }
}

struct Shell {
    webview: tauri::WebviewWindow<MockRuntime>,
    opened: Opened,
    /// What the Tools tab was asked to start. No test in this file starts a real program.
    launched: Launched,
    // Keep the app and the folder alive as long as the webview is used.
    _app: tauri::App<MockRuntime>,
    _dir: tempfile::TempDir,
}

/// The app with the real command list, a library holding `games`, and a Steam that records what it is asked to start.
fn shell(games: Vec<InstalledGame>) -> Shell {
    shell_in(tempfile::tempdir().unwrap(), games)
}

/// As [`shell`], with the library kept in `dir` (so a test can make real folders beside it first).
fn shell_in(dir: tempfile::TempDir, games: Vec<InstalledGame>) -> Shell {
    shell_with(dir, games, AdapterRegistry::new())
}

/// As [`shell_in`], with game adapters registered. The real app registers none.
fn shell_with(
    dir: tempfile::TempDir,
    games: Vec<InstalledGame>,
    adapters: AdapterRegistry,
) -> Shell {
    let file = dir.path().join("library.json");
    if !games.is_empty() {
        LibraryService::open(&file)
            .unwrap()
            .scan(&Found(games))
            .unwrap();
    }
    let opened = Opened::default();
    let launched = Launched::default();
    let steam = SteamManager::with_parts(Box::new(SteamClosed), Box::new(opened.clone()));
    let app = mock_builder()
        .manage(DiscordState::default())
        .manage(AppState::with_parts_and_launcher(
            &file,
            steam,
            Box::new(Plenty),
            adapters,
            Box::new(launched.clone()),
        ))
        .invoke_handler(handlers())
        .build(mock_context(noop_assets()))
        .expect("the mock app builds");
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    Shell {
        webview,
        opened,
        launched,
        _app: app,
        _dir: dir,
    }
}

impl Shell {
    /// `Ok` is what the page's `invoke` resolves with; `Err` is what it rejects with.
    fn call(&self, cmd: &str, args: Value) -> Result<Value, Value> {
        let request = InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: if cfg!(windows) {
                "http://tauri.localhost"
            } else {
                "tauri://localhost"
            }
            .parse()
            .unwrap(),
            body: InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        };
        get_ipc_response(&self.webview, request).map(|body| body.deserialize().unwrap())
    }
}

fn keys(value: &Value) -> Vec<&str> {
    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}

fn steam_and_gog() -> Vec<InstalledGame> {
    vec![
        game(
            "steam-553850",
            "Helldivers 2",
            GameSource::Steam,
            Some("553850"),
        ),
        game("gog-1", "Alpha", GameSource::Gog, Some("1")),
    ]
}

#[test]
fn every_command_the_page_knows_is_reachable() {
    let shell = shell(vec![]);
    for (cmd, args) in [
        ("app_info", json!({})),
        ("list_games", json!({})),
        ("get_game_matrix", json!({ "gameId": "x" })),
        (
            "update_feature_tick",
            json!({ "gameId": "x", "feature": "mods", "enabled": true }),
        ),
        ("launch_game_command", json!({ "appId": 480 })),
        ("discord_set_enabled", json!({ "enabled": false })),
        ("manager_view", json!({})),
        (
            "manager_add_root",
            json!({ "id": "x", "label": "X", "path": "/nope", "kind": "builds" }),
        ),
        ("manager_relink_root", json!({ "id": "x", "path": "/nope" })),
        ("manager_forget_root", json!({ "id": "x" })),
        (
            "manager_import_build",
            json!({ "gameId": "x", "variant": "original", "baseVersion": "1", "sourceDescription": "d", "rootId": "x" }),
        ),
        (
            "manager_register_in_place",
            json!({ "gameId": "x", "variant": "original", "baseVersion": "1", "sourceDescription": "d" }),
        ),
        ("manager_verify_build", json!({ "buildId": "x" })),
        ("manager_activate_build", json!({ "buildId": "x" })),
        (
            "manager_rollback_slot",
            json!({ "gameId": "x", "slot": "original" }),
        ),
        ("mods_view", json!({})),
        (
            "mods_import",
            json!({ "source": { "game_id": "x", "mod_id": "m", "name": "M", "version": "1", "path": "/nope" }, "rootId": "x" }),
        ),
        (
            "mods_verify",
            json!({ "gameId": "x", "modId": "m", "version": "1" }),
        ),
        (
            "mods_create_profile",
            json!({ "id": "p", "name": "P", "gameId": "x", "buildId": "b" }),
        ),
        (
            "mods_edit_profile",
            json!({ "profileId": "p", "edit": { "action": "rename", "name": "Q" } }),
        ),
        ("mods_plan_deploy", json!({ "profileId": "p" })),
        ("mods_deploy", json!({ "profileId": "p" })),
        ("mods_plan_purge", json!({ "buildId": "b" })),
        ("mods_purge", json!({ "buildId": "b" })),
        ("mods_undo_purge", json!({ "buildId": "b", "purgeId": "x" })),
        ("mods_purge_and_deploy", json!({ "profileId": "p" })),
        ("builder_view", json!({})),
        (
            "builder_save_recipe",
            json!({ "recipe": { "schema": 1, "id": "r", "name": "R", "game_id": "x", "description": "", "steps": [] }, "replaces": null }),
        ),
        ("builder_plan_recipe", json!({ "recipeId": "r" })),
        (
            "builder_run_recipe",
            json!({ "recipeId": "r", "rootId": "x" }),
        ),
        (
            "builder_read_run",
            json!({ "recipeId": "r", "rootId": "x", "runId": "run" }),
        ),
        (
            "builder_verify_run",
            json!({ "recipeId": "r", "rootId": "x", "runId": "run" }),
        ),
        (
            "builder_mod_files",
            json!({ "gameId": "x", "modId": "m", "version": "1", "filter": "" }),
        ),
        ("tools_view", json!({})),
        (
            "tools_register",
            json!({ "request": { "id": "t", "display_name": "T", "version": "1", "exe_path": "/nope",
                                 "redistribution": "unknown", "license_note": "", "source_note": "" } }),
        ),
        ("tools_check", json!({ "id": "t" })),
        (
            "tools_accept_change",
            json!({ "id": "t", "newVersion": "2", "seenSha256": "x" }),
        ),
        (
            "tools_plan_run",
            json!({ "request": { "tool_id": "t", "args": [], "settings": {},
                                 "workspace": { "recipe_id": "r", "root_id": "x", "run_id": "run" } } }),
        ),
        (
            "tools_start_run",
            json!({ "request": { "tool_id": "t", "args": [], "settings": {},
                                 "workspace": { "recipe_id": "r", "root_id": "x", "run_id": "run" } } }),
        ),
        ("tools_poll_run", json!({ "run": 1 })),
        (
            "tools_run_records",
            json!({ "workspace": { "recipe_id": "r", "root_id": "x", "run_id": "run" } }),
        ),
        ("launch_view", json!({})),
        (
            "launch_start",
            json!({ "buildId": "x", "captureOutput": false }),
        ),
        ("launch_refresh", json!({})),
        ("catalogue_view", json!({})),
        ("game_detail", json!({ "gameId": "x" })),
        (
            "catalogue_set_categories",
            json!({ "gameId": "x", "categories": [] }),
        ),
        (
            "catalogue_set_artwork",
            json!({ "gameId": "x", "path": null }),
        ),
    ] {
        // Whatever the answer is, it must not be Tauri's "not registered" refusal.
        let answer = format!("{:?}", shell.call(cmd, args));
        assert!(
            !answer.contains(&format!("Command {cmd} not found")),
            "{cmd}: {answer}"
        );
    }
}

#[test]
fn an_unknown_command_is_refused() {
    let shell = shell(vec![]);
    let refusal = shell.call("format_disk", json!({})).unwrap_err();
    assert!(
        refusal.as_str().unwrap().contains("format_disk"),
        "{refusal}"
    );
}

#[test]
fn app_info_has_the_fields_and_vocabulary_the_page_reads() {
    let info = shell(vec![]).call("app_info", json!({})).unwrap();
    assert_eq!(keys(&info), ["app_root", "install_mode", "name", "version"]);
    assert_eq!(info["name"], "Soups Unified Launcher");
    assert_eq!(info["version"], env!("CARGO_PKG_VERSION"));
    assert!(["standard", "portable"].contains(&info["install_mode"].as_str().unwrap()));
    assert!(!info["app_root"].as_str().unwrap().is_empty());
}

#[test]
fn a_fresh_library_lists_nothing() {
    assert_eq!(shell(vec![]).call("list_games", json!({})), Ok(json!([])));
}

#[test]
fn listed_games_have_exactly_the_fields_the_page_reads() {
    let list = shell(steam_and_gog())
        .call("list_games", json!({}))
        .unwrap();
    let games = list.as_array().unwrap();
    assert_eq!(games.len(), 2);
    for g in games {
        assert_eq!(
            keys(g),
            [
                "headline",
                "id",
                "install_dir",
                "name",
                "present",
                "publisher",
                "source",
                "steam_app_id",
                "store_id"
            ]
        );
    }
    let steam = games.iter().find(|g| g["id"] == "steam-553850").unwrap();
    assert_eq!(steam["source"], "steam");
    assert_eq!(
        steam["steam_app_id"], 553850,
        "a number, not text: the Play button hands it to Steam"
    );
    assert_eq!(steam["present"], true);
    assert_eq!(steam["headline"], "not_investigated");
    let gog = games.iter().find(|g| g["id"] == "gog-1").unwrap();
    assert_eq!(gog["steam_app_id"], Value::Null);
}

#[test]
fn a_matrix_has_eight_features_in_order_with_the_fields_the_page_reads() {
    let matrix = shell(steam_and_gog())
        .call("get_game_matrix", json!({ "gameId": "steam-553850" }))
        .unwrap();
    assert_eq!(keys(&matrix), ["game_id", "game_name", "headline", "rows"]);
    let rows = matrix["rows"].as_array().unwrap();
    let order: Vec<&str> = rows
        .iter()
        .map(|r| r["feature"].as_str().unwrap())
        .collect();
    assert_eq!(
        order,
        [
            "save_isolation",
            "config_override",
            "steam_headless",
            "controller_remap",
            "overlay",
            "discord_rpc",
            "mods",
            "diagnostics"
        ]
    );
    for row in rows {
        assert_eq!(
            keys(row),
            [
                "can_enable",
                "compatibility",
                "enabled",
                "feature",
                "label",
                "note",
                "status"
            ]
        );
        assert_eq!(row["enabled"], false, "nothing is ticked on a new game");
    }
}

#[test]
fn ticking_is_saved_and_the_answer_is_the_saved_matrix() {
    let shell = shell(steam_and_gog());
    let args =
        |enabled: bool| json!({ "gameId": "steam-553850", "feature": "mods", "enabled": enabled });
    let on = shell.call("update_feature_tick", args(true)).unwrap();
    let mods = |m: &Value| {
        m["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["feature"] == "mods")
            .unwrap()["enabled"]
            .clone()
    };
    assert_eq!(mods(&on), true);
    let read_back = shell
        .call("get_game_matrix", json!({ "gameId": "steam-553850" }))
        .unwrap();
    assert_eq!(mods(&read_back), true, "it was saved, not just echoed");
    let off = shell.call("update_feature_tick", args(false)).unwrap();
    assert_eq!(mods(&off), false);
}

#[test]
fn the_rules_refuse_steam_headless_on_a_game_that_is_not_from_steam() {
    let shell = shell(steam_and_gog());
    let refused = shell
        .call(
            "update_feature_tick",
            json!({ "gameId": "gog-1", "feature": "steam_headless", "enabled": true }),
        )
        .unwrap_err();
    assert!(
        refused.is_string(),
        "errors reach the page as plain text: {refused}"
    );
    let matrix = shell
        .call("get_game_matrix", json!({ "gameId": "gog-1" }))
        .unwrap();
    let row = matrix["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["feature"] == "steam_headless")
        .unwrap()
        .clone();
    assert_eq!(row["enabled"], false, "nothing changed");
    assert_eq!(row["compatibility"], "not_applicable");
    assert_eq!(row["can_enable"], false);
}

#[test]
fn unknown_games_and_features_come_back_as_text_that_names_them() {
    let shell = shell(steam_and_gog());
    let e = shell
        .call("get_game_matrix", json!({ "gameId": "nope" }))
        .unwrap_err();
    assert!(e.as_str().unwrap().contains("nope"), "{e}");
    let e = shell
        .call(
            "update_feature_tick",
            json!({ "gameId": "steam-553850", "feature": "teleport", "enabled": true }),
        )
        .unwrap_err();
    assert!(e.as_str().unwrap().contains("teleport"), "{e}");
}

#[test]
fn arguments_must_be_camel_case_and_of_the_right_type() {
    let shell = shell(steam_and_gog());
    // The page sends `gameId`; a snake_case key is not accepted, so a typo cannot silently work.
    assert!(
        shell
            .call("get_game_matrix", json!({ "game_id": "steam-553850" }))
            .is_err()
    );
    assert!(shell.call("get_game_matrix", json!({})).is_err());
    assert!(
        shell
            .call("launch_game_command", json!({ "appId": "480" }))
            .is_err(),
        "text is not a number"
    );
    assert!(
        shell
            .call("launch_game_command", json!({ "appId": -1 }))
            .is_err()
    );
    assert!(
        shell
            .call("launch_game_command", json!({ "appId": 4_294_967_296_u64 }))
            .is_err(),
        "bigger than u32"
    );
    assert!(
        shell.opened.0.lock().unwrap().is_empty(),
        "nothing was started by a malformed request"
    );
}

#[test]
fn play_asks_steam_for_that_app_and_only_that() {
    let shell = shell(vec![]);
    assert_eq!(
        shell.call("launch_game_command", json!({ "appId": 553850 })),
        Ok(Value::Null)
    );
    assert_eq!(*shell.opened.0.lock().unwrap(), ["steam://run/553850"]);
    assert!(
        shell
            .call("launch_game_command", json!({ "appId": 0 }))
            .is_err()
    );
    assert_eq!(shell.opened.0.lock().unwrap().len(), 1);
}

#[cfg(not(windows))]
#[test]
fn scanning_off_windows_says_so_instead_of_returning_an_empty_library() {
    let e = shell(vec![])
        .call("scan_games_command", json!({}))
        .unwrap_err();
    assert!(
        e.as_str().unwrap().to_lowercase().contains("windows"),
        "{e}"
    );
}

/// On the Windows runner this scans the real machine through the real command.
#[cfg(windows)]
#[test]
fn scanning_on_windows_answers_with_the_summary_the_page_reads() {
    let shell = shell(vec![]);
    let scan = shell.call("scan_games_command", json!({})).unwrap();
    assert_eq!(
        keys(&scan),
        ["added", "games", "missing", "returned", "updated"]
    );
    let listed = shell.call("list_games", json!({})).unwrap();
    assert_eq!(
        listed, scan["games"],
        "the scan was saved: listing afterwards gives the same games"
    );
}

#[test]
fn presence_is_off_until_asked_and_switching_it_off_needs_no_discord() {
    let shell = shell(vec![]);
    assert_eq!(
        shell.call("discord_set_enabled", json!({ "enabled": false })),
        Ok(json!("off"))
    );
    assert_eq!(
        shell.call(
            "discord_set_enabled",
            json!({ "enabled": false, "smallImage": "corner" })
        ),
        Ok(json!("off"))
    );
}

#[test]
fn presence_without_an_application_id_explains_what_is_missing_and_connects_to_nothing() {
    if std::env::var_os("SUL_DISCORD_CLIENT_ID").is_some() {
        return; // On a machine set up for Discord this would really try to connect: not a unit test.
    }
    let e = shell(vec![])
        .call("discord_set_enabled", json!({ "enabled": true }))
        .unwrap_err();
    let text = e.as_str().unwrap();
    assert!(
        text.contains("SUL_DISCORD_CLIENT_ID") && text.contains("docs/discord.md"),
        "{text}"
    );
}

// ---------------------------------------------------------------------------------------------
// Module 1: library folders and managed builds
// ---------------------------------------------------------------------------------------------

/// A library whose one game lives in a real folder under `dir`, scanned once so the library lists it.
fn real_game_shell(also_missing: bool) -> (Shell, PathBuf, PathBuf) {
    real_game_shell_with(also_missing, AdapterRegistry::new())
}

fn real_game_shell_with(
    also_missing: bool,
    adapters: AdapterRegistry,
) -> (Shell, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().canonicalize().unwrap();
    let install = base.join("Games").join("Made-up Game");
    std::fs::create_dir_all(install.join("data")).unwrap();
    std::fs::write(install.join("game.exe"), "exe").unwrap();
    std::fs::write(install.join("data/a.bin"), "AAAA").unwrap();
    let store = base.join("store");
    std::fs::create_dir_all(&store).unwrap();
    let mut here = game("gog-1", "Made-up Game", GameSource::Gog, Some("1"));
    here.install_dir = install.clone();
    let mut lost = game("gog-2", "Gone Game", GameSource::Gog, Some("2"));
    lost.install_dir = base.join("Games").join("Gone Game");
    let file = dir.path().join("library.json");
    let service = LibraryService::open(&file).unwrap();
    if also_missing {
        service
            .scan(&Found(vec![here.clone(), lost.clone()]))
            .unwrap();
    }
    service.scan(&Found(vec![here])).unwrap();
    (shell_with(dir, vec![], adapters), install, store)
}

#[test]
fn the_manager_view_has_exactly_the_fields_the_page_reads() {
    let view = shell(vec![]).call("manager_view", json!({})).unwrap();
    assert_eq!(
        keys(&view),
        ["builds", "interrupted", "retired", "roots", "slots"]
    );
    for k in ["builds", "interrupted", "retired", "roots", "slots"] {
        assert_eq!(view[k], json!([]), "{k}");
    }
}

#[test]
fn a_folder_is_added_listed_with_the_fields_the_page_reads_and_forgotten() {
    let (shell, _install, store) = real_game_shell(false);
    let view = shell
        .call(
            "manager_add_root",
            json!({ "id": "ssd", "label": "Fast drive", "path": store.to_str().unwrap(), "kind": "builds" }),
        )
        .unwrap();
    let roots = view["roots"].as_array().unwrap();
    assert_eq!(roots.len(), 1);
    assert_eq!(
        keys(&roots[0]),
        [
            "free_bytes",
            "id",
            "kind",
            "label",
            "marker_ok",
            "path",
            "present"
        ]
    );
    assert_eq!(roots[0]["kind"], "builds");
    assert_eq!(roots[0]["present"], true);
    assert_eq!(roots[0]["marker_ok"], true);
    assert_eq!(roots[0]["free_bytes"], 500_000_000_000u64);
    assert!(store.join(".sul-root.json").is_file());

    let view = shell
        .call("manager_forget_root", json!({ "id": "ssd" }))
        .unwrap();
    assert_eq!(view["roots"], json!([]));
    assert!(
        store.join(".sul-root.json").is_file(),
        "the folder is left alone"
    );
}

#[test]
fn a_game_is_imported_from_the_library_activated_and_checked_and_the_original_is_never_changed() {
    let (shell, install, store) = real_game_shell(false);
    shell
        .call(
            "manager_add_root",
            json!({ "id": "ssd", "label": "Fast drive", "path": store.to_str().unwrap(), "kind": "builds" }),
        )
        .unwrap();
    let before: Vec<_> = std::fs::read_dir(&install)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();

    let built = shell
        .call(
            "manager_import_build",
            json!({ "gameId": "gog-1", "variant": "original", "baseVersion": "1.0.0",
                    "sourceDescription": "my own install", "rootId": "ssd" }),
        )
        .unwrap();
    assert_eq!(keys(&built), ["build_id", "view"]);
    let id = built["build_id"].as_str().unwrap().to_owned();
    let record = &built["view"]["builds"][0];
    assert_eq!(
        keys(record),
        [
            "active", "game_id", "id", "location", "slot", "state", "variant"
        ]
    );
    assert_eq!(record["state"], "ready");
    assert_eq!(record["active"], false);
    assert_eq!(record["location"]["kind"], "managed");

    let view = shell
        .call("manager_activate_build", json!({ "buildId": id }))
        .unwrap();
    assert_eq!(view["builds"][0]["active"], true);
    assert_eq!(view["slots"].as_array().unwrap().len(), 1);

    let checked = shell
        .call("manager_verify_build", json!({ "buildId": id }))
        .unwrap();
    assert_eq!(keys(&checked), ["check", "view"]);
    assert_eq!(checked["check"]["state"], "ready");
    assert_eq!(
        keys(&checked["check"]["report"]),
        ["extra", "missing", "modified"]
    );

    let after: Vec<_> = std::fs::read_dir(&install)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(
        before, after,
        "nothing was added to or removed from the original"
    );
    assert_eq!(
        std::fs::read_to_string(install.join("data/a.bin")).unwrap(),
        "AAAA"
    );
}

#[test]
fn a_named_variant_and_a_register_in_place_use_the_shapes_the_page_sends() {
    let (shell, _install, store) = real_game_shell(false);
    shell
        .call(
            "manager_add_root",
            json!({ "id": "ssd", "label": "Fast drive", "path": store.to_str().unwrap(), "kind": "builds" }),
        )
        .unwrap();
    let built = shell
        .call(
            "manager_import_build",
            json!({ "gameId": "gog-1", "variant": { "custom": "tuned" }, "baseVersion": "1.0.0",
                    "sourceDescription": "d", "rootId": "ssd" }),
        )
        .unwrap();
    assert_eq!(built["view"]["builds"][0]["slot"], "tuned");

    let placed = shell
        .call(
            "manager_register_in_place",
            json!({ "gameId": "gog-1", "variant": "legacy", "baseVersion": "1.0.0", "sourceDescription": "d" }),
        )
        .unwrap();
    let in_place = placed["view"]["builds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == placed["build_id"])
        .unwrap();
    assert_eq!(in_place["location"]["kind"], "external_install");
    assert_eq!(
        in_place["slot"], "legacy",
        "the slot the page chose is the slot recorded"
    );
}

#[test]
fn a_game_the_library_does_not_have_or_did_not_find_is_refused_and_nothing_is_copied() {
    let (shell, _install, store) = real_game_shell(true);
    shell
        .call(
            "manager_add_root",
            json!({ "id": "ssd", "label": "Fast drive", "path": store.to_str().unwrap(), "kind": "builds" }),
        )
        .unwrap();
    for (game_id, expect) in [("nope", "no game"), ("gog-2", "did not find")] {
        let refusal = shell
            .call(
                "manager_import_build",
                json!({ "gameId": game_id, "variant": "original", "baseVersion": "1",
                        "sourceDescription": "d", "rootId": "ssd" }),
            )
            .unwrap_err();
        assert!(refusal.as_str().unwrap().contains(expect), "{refusal}");
        let refusal = shell
            .call(
                "manager_register_in_place",
                json!({ "gameId": game_id, "variant": "original", "baseVersion": "1", "sourceDescription": "d" }),
            )
            .unwrap_err();
        assert!(refusal.as_str().unwrap().contains(expect), "{refusal}");
    }
    let view = shell.call("manager_view", json!({})).unwrap();
    assert_eq!(view["builds"], json!([]));
    let files: Vec<_> = std::fs::read_dir(&store)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        files,
        [".sul-root.json"],
        "only the marker is in the folder"
    );
}

#[test]
fn the_librarys_install_folders_are_protected_so_a_root_inside_one_is_refused() {
    let (shell, install, _store) = real_game_shell(false);
    let refusal = shell
        .call(
            "manager_add_root",
            json!({ "id": "bad", "label": "Bad", "path": install.join("data").to_str().unwrap(), "kind": "builds" }),
        )
        .unwrap_err();
    assert!(
        refusal.as_str().unwrap().to_lowercase().contains("protect"),
        "{refusal}"
    );
    assert!(!install.join("data/.sul-root.json").exists());
}

#[test]
fn a_refusal_reaches_the_page_as_plain_text_and_the_arguments_must_be_camel_case() {
    let shell = shell(vec![]);
    let refusal = shell
        .call(
            "manager_add_root",
            json!({ "id": "x", "label": "X", "path": "/this/is/not/there", "kind": "builds" }),
        )
        .unwrap_err();
    assert!(refusal.is_string(), "{refusal}");

    let bad = shell
        .call(
            "manager_import_build",
            json!({ "game_id": "x", "variant": "original", "base_version": "1",
                    "source_description": "d", "root_id": "x" }),
        )
        .unwrap_err();
    assert!(bad.as_str().unwrap().contains("gameId"), "{bad}");
    let bad_kind = shell
        .call(
            "manager_add_root",
            json!({ "id": "x", "label": "X", "path": "/p", "kind": "everything" }),
        )
        .unwrap_err();
    assert!(bad_kind.is_string(), "{bad_kind}");
}

#[test]
fn a_moved_folder_is_found_again_through_the_relink_command() {
    let (shell, _install, store) = real_game_shell(false);
    shell
        .call(
            "manager_add_root",
            json!({ "id": "ssd", "label": "Fast drive", "path": store.to_str().unwrap(), "kind": "builds" }),
        )
        .unwrap();
    let moved = store.with_file_name("store-moved");
    std::fs::rename(&store, &moved).unwrap();
    let view = shell.call("manager_view", json!({})).unwrap();
    assert_eq!(view["roots"][0]["present"], false);

    // A folder that is not it is refused, and the list is as it was.
    let other = store.with_file_name("some-other-folder");
    std::fs::create_dir(&other).unwrap();
    let refusal = shell
        .call(
            "manager_relink_root",
            json!({ "id": "ssd", "path": other.to_str().unwrap() }),
        )
        .unwrap_err();
    assert!(refusal.is_string(), "{refusal}");
    assert!(
        !other.join(".sul-root.json").exists(),
        "nothing is written there"
    );

    let view = shell
        .call(
            "manager_relink_root",
            json!({ "id": "ssd", "path": moved.to_str().unwrap() }),
        )
        .unwrap();
    assert_eq!(view["roots"][0]["present"], true);
    assert_eq!(view["roots"][0]["marker_ok"], true);
    assert_eq!(view["roots"][0]["path"], moved.to_str().unwrap());
}

#[test]
fn a_slot_is_rolled_back_through_the_command_to_the_build_that_was_active_before() {
    let (shell, _install, store) = real_game_shell(false);
    shell
        .call(
            "manager_add_root",
            json!({ "id": "ssd", "label": "Fast drive", "path": store.to_str().unwrap(), "kind": "builds" }),
        )
        .unwrap();
    let mut ids = Vec::new();
    for version in ["1.0.0", "1.0.1"] {
        let built = shell
            .call(
                "manager_import_build",
                json!({ "gameId": "gog-1", "variant": "original", "baseVersion": version,
                        "sourceDescription": "d", "rootId": "ssd" }),
            )
            .unwrap();
        let id = built["build_id"].as_str().unwrap().to_owned();
        shell
            .call("manager_activate_build", json!({ "buildId": id }))
            .unwrap();
        ids.push(id);
    }
    let active_of = |view: &Value| -> Vec<String> {
        view["builds"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|b| b["active"] == true)
            .map(|b| b["id"].as_str().unwrap().to_owned())
            .collect()
    };
    let now = shell.call("manager_view", json!({})).unwrap();
    assert_eq!(active_of(&now), [ids[1].clone()]);
    assert_eq!(now["slots"][0]["history"].as_array().unwrap().len(), 1);

    let view = shell
        .call(
            "manager_rollback_slot",
            json!({ "gameId": "gog-1", "slot": "original" }),
        )
        .unwrap();
    assert_eq!(active_of(&view), [ids[0].clone()]);

    // Nothing earlier is left to go back to: refused as text.
    let refusal = shell
        .call(
            "manager_rollback_slot",
            json!({ "gameId": "gog-1", "slot": "original" }),
        )
        .unwrap_err();
    assert!(refusal.is_string(), "{refusal}");
}

#[test]
fn a_game_found_by_a_scan_is_protected_at_once_so_a_root_inside_it_is_refused() {
    use tauri::Manager;
    // The library starts empty, so only the protection after the scan can know this folder.
    let shell = shell(vec![]);
    let base = shell._dir.path().canonicalize().unwrap();
    let install = base.join("Games").join("Found Later");
    std::fs::create_dir_all(install.join("data")).unwrap();
    let mut found = game("gog-9", "Found Later", GameSource::Gog, Some("9"));
    found.install_dir = install.clone();
    shell
        ._app
        .state::<AppState>()
        .scan_with(&Found(vec![found]))
        .unwrap();

    let refusal = shell
        .call(
            "manager_add_root",
            json!({ "id": "bad", "label": "Bad", "path": install.join("data").to_str().unwrap(), "kind": "builds" }),
        )
        .unwrap_err();
    assert!(
        refusal.as_str().unwrap().to_lowercase().contains("protect"),
        "{refusal}"
    );
    assert!(!install.join("data/.sul-root.json").exists());
}

// ---- Mods (spec 2.1) ----

/// A made-up adapter for the made-up game of `real_game_shell`. It is not a real game's adapter.
struct TinyAdapter;

impl GameAdapter for TinyAdapter {
    fn id(&self) -> &str {
        "gog-1"
    }
    fn display_name(&self) -> &str {
        "Made-up Game"
    }
    fn version(&self) -> &str {
        "0.0-test"
    }
    fn recognise(&self, _folder: &std::path::Path) -> sul_core::Result<Recognition> {
        Ok(Recognition::not_this_game(
            "a test adapter recognises nothing",
        ))
    }
    fn launch_plan(&self, _dir: &std::path::Path, _v: &str) -> sul_core::Result<LaunchPlan> {
        Err(sul_core::Error::Invalid(
            "a test adapter does not launch".into(),
        ))
    }
    fn mod_layout(&self) -> ModLayout {
        ModLayout {
            allowed_prefixes: vec!["data".into(), "mods".into()],
            forbidden_prefixes: vec!["data/anticheat".into()],
            case_insensitive: true,
        }
    }
    fn capabilities(&self) -> Vec<CapabilityEntry> {
        vec![]
    }
}

fn tiny_adapters() -> AdapterRegistry {
    let mut a = AdapterRegistry::new();
    a.register(Box::new(TinyAdapter)).unwrap();
    a
}

/// A shell with a build folder, a mod folder, an experimental build of the made-up game and a mod folder on disk.
/// Returns the shell, the original install, the build's id and the folder the mod came from.
fn mods_shell(adapters: AdapterRegistry) -> (Shell, PathBuf, String, PathBuf) {
    let (shell, install, store) = real_game_shell_with(false, adapters);
    let base = store.parent().unwrap().to_path_buf();
    let modlib = base.join("modlib");
    std::fs::create_dir_all(&modlib).unwrap();
    for (id, label, path, kind) in [
        ("ssd", "Fast drive", &store, "builds"),
        ("modlib", "Mod library", &modlib, "mods"),
    ] {
        shell
            .call(
                "manager_add_root",
                json!({ "id": id, "label": label, "path": path.to_str().unwrap(), "kind": kind }),
            )
            .unwrap();
    }
    let built = shell
        .call(
            "manager_import_build",
            json!({ "gameId": "gog-1", "variant": "experimental", "baseVersion": "1.0.0",
                    "sourceDescription": "my own install", "rootId": "ssd" }),
        )
        .unwrap();
    let build = built["build_id"].as_str().unwrap().to_owned();
    let src = base.join("mod_src");
    std::fs::create_dir_all(src.join("data")).unwrap();
    std::fs::create_dir_all(src.join("mods")).unwrap();
    std::fs::write(src.join("data/a.bin"), "NEW-A").unwrap();
    std::fs::write(src.join("mods/ui.txt"), "ui").unwrap();
    (shell, install, build, src)
}

fn import_mod_and_make_profile(shell: &Shell, build: &str, src: &std::path::Path) -> Value {
    shell
        .call(
            "mods_import",
            json!({ "source": { "game_id": "gog-1", "mod_id": "ui", "name": "UI mod", "version": "1.0",
                                "requires": [], "compatible_versions": ["1.0.0"],
                                "source": "made by hand", "license_note": "mine",
                                "path": src.to_str().unwrap() },
                    "rootId": "modlib" }),
        )
        .unwrap();
    shell
        .call(
            "mods_create_profile",
            json!({ "id": "main", "name": "Main", "gameId": "gog-1", "buildId": build }),
        )
        .unwrap();
    shell
        .call(
            "mods_edit_profile",
            json!({ "profileId": "main",
                    "edit": { "action": "add_mod", "mod_id": "ui", "version": "1.0", "position": null } }),
        )
        .unwrap()
}

fn managed_file(shell: &Shell, build: &str, rel: &str) -> PathBuf {
    // The build's folder is the one inside the build library that holds the made-up game's executable.
    let view = shell.call("manager_view", json!({})).unwrap();
    let rec = view["builds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == build)
        .unwrap();
    let root = &rec["location"]["root_id"];
    let roots = view["roots"].as_array().unwrap();
    let root_path = roots.iter().find(|r| &r["id"] == root).unwrap()["path"]
        .as_str()
        .unwrap()
        .to_owned();
    PathBuf::from(root_path)
        .join(rec["location"]["dir"].as_str().unwrap())
        .join(rel)
}

#[test]
fn the_mods_view_has_exactly_the_fields_the_page_reads() {
    let view = shell(vec![]).call("mods_view", json!({})).unwrap();
    assert_eq!(
        keys(&view),
        [
            "builds",
            "deployable_games",
            "mod_roots",
            "mods",
            "profiles"
        ]
    );
    for k in [
        "builds",
        "deployable_games",
        "mod_roots",
        "mods",
        "profiles",
    ] {
        assert_eq!(view[k], json!([]), "{k}");
    }
}

#[test]
fn a_mod_is_imported_put_in_a_profile_deployed_purged_and_undone_through_the_commands() {
    let (shell, install, build, src) = mods_shell(tiny_adapters());
    let view = import_mod_and_make_profile(&shell, &build, &src);

    assert_eq!(view["deployable_games"], json!(["gog-1"]));
    let m = &view["mods"][0];
    assert_eq!(
        keys(m),
        [
            "compatible_versions",
            "file_count",
            "game_id",
            "imported_unix",
            "license_note",
            "mod_id",
            "name",
            "requires",
            "root_id",
            "source",
            "total_bytes",
            "version"
        ]
    );
    assert_eq!(
        (m["file_count"].as_u64(), m["total_bytes"].as_u64()),
        (Some(2), Some(7))
    );
    let p = &view["profiles"][0];
    assert_eq!(
        keys(p),
        [
            "build_id",
            "config",
            "created_unix",
            "game_id",
            "id",
            "mods",
            "name",
            "notes",
            "problems",
            "updated_unix"
        ]
    );
    assert_eq!(keys(&p["mods"][0]), ["enabled", "mod_id", "version"]);
    assert_eq!(
        keys(&view["mod_roots"][0]),
        [
            "free_bytes",
            "id",
            "kind",
            "label",
            "marker_ok",
            "path",
            "present"
        ]
    );
    assert_eq!(
        keys(&view["builds"][0]),
        [
            "build_id",
            "deployment",
            "error",
            "game_id",
            "history",
            "slot",
            "undoable_purge"
        ]
    );
    assert_eq!(
        view["builds"][0]["slot"], "experimental",
        "the page names a build by its slot"
    );

    let a_bin = managed_file(&shell, &build, "data/a.bin");
    let checked = shell
        .call(
            "mods_verify",
            json!({ "gameId": "gog-1", "modId": "ui", "version": "1.0" }),
        )
        .unwrap();
    assert_eq!(keys(&checked), ["report", "view"]);
    assert_eq!(keys(&checked["report"]), ["extra", "missing", "modified"]);

    // The preview.
    let plan = shell
        .call("mods_plan_deploy", json!({ "profileId": "main" }))
        .unwrap();
    assert_eq!(
        keys(&plan),
        ["build_id", "files", "problems", "profile_id", "total_bytes"]
    );
    assert_eq!(plan["problems"], json!([]));
    assert_eq!(
        keys(&plan["files"][0]),
        [
            "action",
            "mod_id",
            "mod_version",
            "overrides",
            "path",
            "replaces",
            "sha256",
            "size"
        ]
    );
    assert_eq!(
        std::fs::read_to_string(&a_bin).unwrap(),
        "AAAA",
        "a preview changes nothing"
    );

    // The deploy.
    let deployed = shell
        .call("mods_deploy", json!({ "profileId": "main" }))
        .unwrap();
    assert_eq!(keys(&deployed), ["report", "view"]);
    assert_eq!(
        keys(&deployed["report"]),
        ["added", "deployment_id", "kept", "replaced", "warnings"]
    );
    assert_eq!(deployed["report"]["replaced"], json!(["data/a.bin"]));
    assert_eq!(std::fs::read_to_string(&a_bin).unwrap(), "NEW-A");
    let dep = &deployed["view"]["builds"][0]["deployment"];
    assert_eq!(
        keys(dep),
        [
            "files",
            "finished_unix",
            "id",
            "profile_id",
            "profile_name",
            "started_unix",
            "status"
        ]
    );
    assert_eq!(dep["status"], "deployed");

    // The purge preview, then the purge.
    let pp = shell
        .call("mods_plan_purge", json!({ "buildId": build }))
        .unwrap();
    assert_eq!(keys(&pp), ["build_id", "deployment", "items"]);
    assert_eq!(
        keys(&pp["items"][0]),
        ["action", "detail", "mod_id", "path"]
    );
    assert_eq!(std::fs::read_to_string(&a_bin).unwrap(), "NEW-A");
    let purged = shell
        .call("mods_purge", json!({ "buildId": build }))
        .unwrap();
    assert_eq!(
        keys(&purged["report"]),
        [
            "forgotten",
            "left",
            "purge_id",
            "recovery_dir",
            "removed",
            "restored"
        ]
    );
    assert_eq!(std::fs::read_to_string(&a_bin).unwrap(), "AAAA");
    let undoable = &purged["view"]["builds"][0]["undoable_purge"];
    assert_eq!(keys(undoable), ["files", "id", "profile_name", "unix"]);

    // The undo.
    let purge_id = purged["report"]["purge_id"].as_str().unwrap().to_owned();
    let undone = shell
        .call(
            "mods_undo_purge",
            json!({ "buildId": build, "purgeId": purge_id }),
        )
        .unwrap();
    assert_eq!(
        keys(&undone["report"]),
        ["originals_moved_aside_again", "put_back"]
    );
    assert_eq!(std::fs::read_to_string(&a_bin).unwrap(), "NEW-A");

    // Purge & Deploy.
    let both = shell
        .call("mods_purge_and_deploy", json!({ "profileId": "main" }))
        .unwrap();
    assert_eq!(keys(&both["report"]), ["deploy", "purge"]);
    assert_eq!(std::fs::read_to_string(&a_bin).unwrap(), "NEW-A");

    assert_eq!(
        std::fs::read_to_string(install.join("data/a.bin")).unwrap(),
        "AAAA",
        "the original install is never changed"
    );
}

#[test]
fn with_no_adapter_nothing_is_deployed_and_the_refusal_says_why() {
    let (shell, _install, build, src) = mods_shell(AdapterRegistry::new());
    let view = import_mod_and_make_profile(&shell, &build, &src);
    assert_eq!(view["deployable_games"], json!([]));
    for cmd in ["mods_plan_deploy", "mods_deploy", "mods_purge_and_deploy"] {
        let e = shell.call(cmd, json!({ "profileId": "main" })).unwrap_err();
        assert!(e.as_str().unwrap().contains("no adapter"), "{cmd}: {e}");
    }
    let a_bin = managed_file(&shell, &build, "data/a.bin");
    assert_eq!(std::fs::read_to_string(a_bin).unwrap(), "AAAA");
}

#[test]
fn profile_edits_use_a_tagged_object_and_refusals_reach_the_page_as_text() {
    let (shell, _install, build, src) = mods_shell(tiny_adapters());
    import_mod_and_make_profile(&shell, &build, &src);
    for (edit, expect) in [
        (
            json!({ "action": "set_enabled", "mod_id": "ui", "enabled": false }),
            None,
        ),
        (
            json!({ "action": "move", "mod_id": "ui", "to_index": 0 }),
            None,
        ),
        (
            json!({ "action": "set_config", "key": "volume", "value": "3" }),
            None,
        ),
        (
            json!({ "action": "set_config", "key": "volume", "value": null }),
            None,
        ),
        (json!({ "action": "notes", "notes": "hello" }), None),
        (
            json!({ "action": "remove_mod", "mod_id": "ghost" }),
            Some("not in this profile"),
        ),
    ] {
        let r = shell.call(
            "mods_edit_profile",
            json!({ "profileId": "main", "edit": edit }),
        );
        match expect {
            None => {
                r.unwrap();
            }
            Some(text) => assert!(r.unwrap_err().as_str().unwrap().contains(text)),
        }
    }
    // An unknown action, and arguments spelled in snake case, are refused.
    assert!(
        shell
            .call(
                "mods_edit_profile",
                json!({ "profileId": "main", "edit": { "action": "explode" } })
            )
            .is_err()
    );
    assert!(
        shell
            .call("mods_deploy", json!({ "profile_id": "main" }))
            .is_err()
    );
    let e = shell
        .call(
            "mods_import",
            json!({ "source": { "game_id": "gog-1" }, "rootId": "modlib" }),
        )
        .unwrap_err();
    assert!(e.is_string(), "{e}");
}

#[test]
fn a_mod_import_from_a_folder_that_is_not_there_is_refused_as_text_and_adds_nothing() {
    let (shell, _install, _build, src) = mods_shell(tiny_adapters());
    let e = shell
        .call(
            "mods_import",
            json!({ "source": { "game_id": "gog-1", "mod_id": "ui", "name": "UI", "version": "1.0",
                                "path": src.join("nope").to_str().unwrap() },
                    "rootId": "modlib" }),
        )
        .unwrap_err();
    assert!(e.is_string(), "{e}");
    assert_eq!(
        shell.call("mods_view", json!({})).unwrap()["mods"],
        json!([])
    );
}

/// A recipe that layers the mod `ui` and changes the first byte of `data/a.bin` ("NEW-A") from N to O.
fn a_recipe(sha: &str) -> Value {
    json!({
        "schema": 1, "id": "brighter", "name": "Brighter UI", "game_id": "gog-1",
        "description": "the ui mod with one byte changed",
        "steps": [
            { "op": "layer_mod", "mod_id": "ui", "version": "1.0", "into": "" },
            { "op": "patch_file", "path": "data/a.bin", "patch": {
                "id": "n-to-o", "description": "N becomes O", "expect_sha256": sha,
                "hunks": [ { "offset": 0, "old_hex": "4e", "new_hex": "4f" } ] } }
        ]
    })
}

const BUILDER_VIEW_KEYS: [&str; 5] = [
    "deployable_games",
    "mod_roots",
    "mods",
    "recipes",
    "run_errors",
];

#[test]
fn the_builder_view_has_exactly_the_fields_the_page_reads() {
    let view = shell(vec![]).call("builder_view", json!({})).unwrap();
    assert_eq!(keys(&view), BUILDER_VIEW_KEYS);
    for k in BUILDER_VIEW_KEYS {
        assert_eq!(view[k], json!([]), "{k}");
    }
}

#[test]
fn a_recipe_is_saved_changed_planned_run_read_and_checked_through_the_commands() {
    let (shell, _install, build, src) = mods_shell(tiny_adapters());
    import_mod_and_make_profile(&shell, &build, &src);
    let modlib = src.parent().unwrap().join("modlib");

    // The hash a patch needs comes from the command made for that.
    let files = shell
        .call(
            "builder_mod_files",
            json!({ "gameId": "gog-1", "modId": "ui", "version": "1.0", "filter": "A.BIN" }),
        )
        .unwrap();
    assert_eq!(keys(&files), ["files", "matching", "total"]);
    assert_eq!(
        (files["matching"].as_u64(), files["total"].as_u64()),
        (Some(1), Some(2))
    );
    assert_eq!(keys(&files["files"][0]), ["path", "sha256", "size"]);
    assert_eq!(files["files"][0]["path"], "data/a.bin");
    let sha = files["files"][0]["sha256"].as_str().unwrap().to_owned();

    // Saving reads no mod and writes only the list.
    let view = shell
        .call(
            "builder_save_recipe",
            json!({ "recipe": a_recipe(&sha), "replaces": null }),
        )
        .unwrap();
    assert_eq!(keys(&view), BUILDER_VIEW_KEYS);
    assert_eq!(view["deployable_games"], json!(["gog-1"]));
    assert_eq!(view["mods"].as_array().unwrap().len(), 1);
    let saved = &view["recipes"][0];
    assert_eq!(
        keys(saved),
        ["has_runtime_code", "recipe", "runs", "sha256"]
    );
    assert_eq!(saved["has_runtime_code"], false);
    assert_eq!(saved["runs"], json!([]));
    let first_sha = saved["sha256"].as_str().unwrap().to_owned();
    assert!(
        !modlib.join(".workspaces").exists(),
        "saving made no workspace"
    );

    // The preview writes nothing.
    let plan = shell
        .call("builder_plan_recipe", json!({ "recipeId": "brighter" }))
        .unwrap();
    assert_eq!(
        keys(&plan),
        [
            "files",
            "inputs",
            "output_sha256",
            "patches",
            "problems",
            "recipe_sha256",
            "total_bytes",
            "warnings"
        ]
    );
    assert_eq!(plan["problems"], json!([]));
    assert_eq!(plan["files"].as_array().unwrap().len(), 2);
    assert_eq!(plan["recipe_sha256"], first_sha.as_str());
    assert!(
        !modlib.join(".workspaces").exists(),
        "a preview wrote nothing"
    );

    // The run builds a new folder in the mod library's workspace.
    let ran = shell
        .call(
            "builder_run_recipe",
            json!({ "recipeId": "brighter", "rootId": "modlib" }),
        )
        .unwrap();
    assert_eq!(keys(&ran), ["record", "view"]);
    let record = &ran["record"];
    assert_eq!(
        keys(record),
        [
            "files",
            "inputs",
            "output_sha256",
            "patches",
            "recipe",
            "recipe_sha256",
            "run_id",
            "schema",
            "started_unix",
            "warnings"
        ]
    );
    assert_eq!(
        record["output_sha256"], plan["output_sha256"],
        "the run built what the preview said"
    );
    let run_id = record["run_id"].as_str().unwrap().to_owned();
    assert_eq!(
        ran["view"]["recipes"][0]["runs"],
        json!([{ "root_id": "modlib", "run_id": run_id }])
    );
    let run_files = modlib
        .join(".workspaces")
        .join("brighter")
        .join(&run_id)
        .join("files");
    assert_eq!(
        std::fs::read_to_string(run_files.join("data/a.bin")).unwrap(),
        "OEW-A"
    );
    assert_eq!(
        std::fs::read_to_string(run_files.join("mods/ui.txt")).unwrap(),
        "ui"
    );
    assert_eq!(
        std::fs::read_to_string(src.join("data/a.bin")).unwrap(),
        "NEW-A",
        "the folder the mod came from was not touched"
    );

    // Reading the record and checking the files.
    let args = json!({ "recipeId": "brighter", "rootId": "modlib", "runId": run_id });
    assert_eq!(
        shell.call("builder_read_run", args.clone()).unwrap(),
        *record
    );
    let clean = shell.call("builder_verify_run", args.clone()).unwrap();
    assert_eq!(keys(&clean), ["extra", "missing", "modified"]);
    assert_eq!(
        (&clean["extra"], &clean["missing"], &clean["modified"]),
        (&json!([]), &json!([]), &json!([]))
    );
    std::fs::write(run_files.join("data/a.bin"), "changed by hand").unwrap();
    let dirty = shell.call("builder_verify_run", args).unwrap();
    assert_eq!(dirty["modified"], json!(["data/a.bin"]));

    // A change must name the recipe it started from; a stale window is refused and nothing changes.
    let mut renamed = a_recipe(&sha);
    renamed["name"] = json!("Brighter UI, v2");
    let stale = shell
        .call(
            "builder_save_recipe",
            json!({ "recipe": renamed, "replaces": "0".repeat(64) }),
        )
        .unwrap_err();
    assert!(
        stale
            .as_str()
            .unwrap()
            .contains("changed since it was opened"),
        "{stale}"
    );
    let changed = shell
        .call(
            "builder_save_recipe",
            json!({ "recipe": renamed, "replaces": first_sha }),
        )
        .unwrap();
    assert_eq!(changed["recipes"][0]["recipe"]["name"], "Brighter UI, v2");
    assert_ne!(changed["recipes"][0]["sha256"], first_sha.as_str());
    assert_eq!(
        changed["recipes"][0]["runs"].as_array().unwrap().len(),
        1,
        "the earlier run is still listed"
    );
}

#[test]
fn with_no_adapter_a_recipe_can_be_saved_but_not_previewed_or_run() {
    let (shell, _install, build, src) = mods_shell(AdapterRegistry::new());
    import_mod_and_make_profile(&shell, &build, &src);
    let modlib = src.parent().unwrap().join("modlib");
    let sha = "a".repeat(64);
    let view = shell
        .call(
            "builder_save_recipe",
            json!({ "recipe": a_recipe(&sha), "replaces": null }),
        )
        .unwrap();
    assert_eq!(view["deployable_games"], json!([]));
    assert_eq!(view["recipes"].as_array().unwrap().len(), 1);
    let plan = shell
        .call("builder_plan_recipe", json!({ "recipeId": "brighter" }))
        .unwrap_err();
    assert!(plan.as_str().unwrap().contains("no adapter"), "{plan}");
    let run = shell
        .call(
            "builder_run_recipe",
            json!({ "recipeId": "brighter", "rootId": "modlib" }),
        )
        .unwrap_err();
    assert!(run.as_str().unwrap().contains("no adapter"), "{run}");
    assert!(!modlib.join(".workspaces").exists(), "nothing was written");
}

#[test]
fn a_recipe_that_changes_game_code_is_saved_and_flagged_but_never_planned_or_run() {
    let (shell, _install, build, src) = mods_shell(tiny_adapters());
    import_mod_and_make_profile(&shell, &build, &src);
    let recipe = json!({
        "schema": 1, "id": "hook", "name": "Hook", "game_id": "gog-1", "description": "",
        "steps": [ { "op": "runtime_code", "description": "inject a library" } ]
    });
    let view = shell
        .call(
            "builder_save_recipe",
            json!({ "recipe": recipe, "replaces": null }),
        )
        .unwrap();
    assert_eq!(view["recipes"][0]["has_runtime_code"], true);
    for (cmd, args) in [
        ("builder_plan_recipe", json!({ "recipeId": "hook" })),
        (
            "builder_run_recipe",
            json!({ "recipeId": "hook", "rootId": "modlib" }),
        ),
    ] {
        let e = shell.call(cmd, args).unwrap_err();
        assert!(
            e.as_str()
                .unwrap()
                .contains("changes code that runs inside the game"),
            "{cmd}: {e}"
        );
    }
}

#[test]
fn builder_refusals_reach_the_page_as_text_and_the_arguments_must_be_camel_case() {
    let (shell, _install, build, src) = mods_shell(tiny_adapters());
    import_mod_and_make_profile(&shell, &build, &src);
    let sha = "b".repeat(64);
    shell
        .call(
            "builder_save_recipe",
            json!({ "recipe": a_recipe(&sha), "replaces": null }),
        )
        .unwrap();

    let refused = |cmd: &str, args: Value, needle: &str| {
        let e = shell.call(cmd, args).unwrap_err();
        assert!(e.as_str().unwrap().contains(needle), "{cmd}: {e}");
    };
    refused(
        "builder_save_recipe",
        json!({ "recipe": a_recipe(&sha), "replaces": null }),
        "already exists",
    );
    let mut empty = a_recipe(&sha);
    empty["id"] = json!("empty");
    empty["steps"] = json!([]);
    refused(
        "builder_save_recipe",
        json!({ "recipe": empty, "replaces": null }),
        "1 to",
    );
    let mut ghost = a_recipe(&sha);
    ghost["id"] = json!("ghost");
    refused(
        "builder_save_recipe",
        json!({ "recipe": ghost, "replaces": sha }),
        "no saved recipe",
    );
    refused(
        "builder_plan_recipe",
        json!({ "recipeId": "nope" }),
        "no saved recipe",
    );
    refused(
        "builder_run_recipe",
        json!({ "recipeId": "brighter", "rootId": "nope" }),
        "no library root",
    );
    refused(
        "builder_run_recipe",
        json!({ "recipeId": "brighter", "rootId": "ssd" }),
        "not a mod library",
    );
    refused(
        "builder_mod_files",
        json!({ "gameId": "gog-1", "modId": "ghost", "version": "1.0", "filter": "" }),
        "not in the mod store",
    );
    // A run id is a name, never a path.
    refused(
        "builder_read_run",
        json!({ "recipeId": "brighter", "rootId": "modlib", "runId": "../../x" }),
        "run id",
    );
    // The recipe that failed the plan (the patch's hash is made up) was refused and wrote nothing.
    let plan = shell
        .call("builder_plan_recipe", json!({ "recipeId": "brighter" }))
        .unwrap();
    assert!(!plan["problems"].as_array().unwrap().is_empty(), "{plan}");
    refused(
        "builder_run_recipe",
        json!({ "recipeId": "brighter", "rootId": "modlib" }),
        "cannot run",
    );
    assert!(
        !src.parent()
            .unwrap()
            .join("modlib")
            .join(".workspaces")
            .join("brighter")
            .exists()
    );

    let bad = shell
        .call("builder_plan_recipe", json!({ "recipe_id": "brighter" }))
        .unwrap_err();
    assert!(bad.as_str().unwrap().contains("recipeId"), "{bad}");
}

// ---------------------------------------------------------------------------
// Tools (slice C2)
// ---------------------------------------------------------------------------

const TOOLS_VIEW_KEYS: [&str; 6] = [
    "known",
    "mod_roots",
    "run_errors",
    "running",
    "tools",
    "workspaces",
];

const TOOL_RECORD_KEYS: [&str; 11] = [
    "display_name",
    "exe_path",
    "exe_sha256",
    "exe_size",
    "history",
    "id",
    "license_note",
    "redistribution",
    "registered_unix",
    "source_note",
    "version",
];

const TOOL_RUN_RECORD_KEYS: [&str; 15] = [
    "added",
    "args",
    "changed",
    "comparison_error",
    "env_names",
    "exit_code",
    "finished_unix",
    "limits",
    "removed",
    "settings",
    "started_unix",
    "success",
    "tool_id",
    "tool_sha256",
    "tool_version",
];

/// A shell with a mod library, a stored mod, a finished run of a recipe (the place a tool works) and a program on
/// disk that is not yet recorded. Returns the shell, the program's path and the run as the page names it.
fn tools_shell() -> (Shell, PathBuf, Value) {
    let (shell, _install, build, src) = mods_shell(tiny_adapters());
    import_mod_and_make_profile(&shell, &build, &src);
    let base = src.parent().unwrap().to_path_buf();
    shell
        .call(
            "builder_save_recipe",
            json!({ "recipe": {
                "schema": 1, "id": "plain", "name": "Plain", "game_id": "gog-1", "description": "",
                "steps": [ { "op": "layer_mod", "mod_id": "ui", "version": "1.0", "into": "" } ] },
                "replaces": null }),
        )
        .unwrap();
    let ran = shell
        .call(
            "builder_run_recipe",
            json!({ "recipeId": "plain", "rootId": "modlib" }),
        )
        .unwrap();
    let run_id = ran["record"]["run_id"].as_str().unwrap().to_owned();
    let exe = base.join("tools").join("audio.exe");
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    std::fs::write(&exe, "pretend program").unwrap();
    (
        shell,
        exe,
        json!({ "recipe_id": "plain", "root_id": "modlib", "run_id": run_id }),
    )
}

fn tool_registration(exe: &std::path::Path) -> Value {
    json!({ "request": { "id": "audio", "display_name": "Audio tool", "version": "1.0",
                         "exe_path": exe.to_str().unwrap(), "redistribution": "unknown",
                         "license_note": "", "source_note": "installed by me" } })
}

fn tool_run(workspace: &Value) -> Value {
    json!({ "request": { "tool_id": "audio", "args": ["--in", "a.bin"],
                         "settings": { "bank": "music" }, "workspace": workspace } })
}

#[test]
fn the_tools_view_has_exactly_the_fields_the_page_reads() {
    let view = shell(vec![]).call("tools_view", json!({})).unwrap();
    assert_eq!(keys(&view), TOOLS_VIEW_KEYS);
    for k in [
        "known",
        "mod_roots",
        "run_errors",
        "running",
        "tools",
        "workspaces",
    ] {
        assert!(view[k].is_array(), "{k}");
    }
    assert_eq!(view["known"].as_array().unwrap().len(), 2);
    let known = &view["known"][0];
    assert_eq!(
        keys(known),
        ["finding", "id", "name", "not_established", "status"]
    );
    assert_eq!(known["id"], "hd2-audio-modder");
    assert_eq!(known["status"], "not_investigated");
    assert_eq!(view["known"][1]["id"], "wwise");
}

#[test]
fn a_tool_is_recorded_checked_changed_accepted_run_polled_and_its_records_read_through_the_commands()
 {
    let (shell, exe, workspace) = tools_shell();

    // The finished recipe run is listed as a place a tool can work.
    let view = shell.call("tools_view", json!({})).unwrap();
    assert_eq!(
        keys(&view["workspaces"][0]),
        ["recipe_id", "recipe_name", "root_id", "run_id"]
    );
    assert_eq!(view["workspaces"][0]["run_id"], workspace["run_id"]);
    assert_eq!(view["mod_roots"][0]["id"], "modlib");

    // Recording reads and hashes the file and starts nothing.
    let view = shell
        .call("tools_register", tool_registration(&exe))
        .unwrap();
    assert_eq!(keys(&view), TOOLS_VIEW_KEYS);
    let tool = &view["tools"][0];
    assert_eq!(keys(tool), TOOL_RECORD_KEYS);
    assert_eq!(tool["exe_size"], 15);
    assert_eq!(tool["redistribution"], "unknown");
    assert_eq!(tool["history"], json!([]));
    assert!(shell.launched.started.lock().unwrap().is_empty());

    let checked = shell.call("tools_check", json!({ "id": "audio" })).unwrap();
    assert_eq!(keys(&checked), ["id", "status"]);
    assert_eq!(checked["status"], json!({ "state": "matches" }));

    // The file changes: the check says so, and the tool is neither planned as runnable nor started.
    std::fs::write(&exe, "version two").unwrap();
    let checked = shell.call("tools_check", json!({ "id": "audio" })).unwrap();
    assert_eq!(
        keys(&checked["status"]),
        ["exe_sha256", "exe_size", "state"]
    );
    assert_eq!(checked["status"]["state"], "changed");
    let seen = checked["status"]["exe_sha256"].as_str().unwrap().to_owned();
    let blocked = shell.call("tools_plan_run", tool_run(&workspace)).unwrap();
    assert!(
        !blocked["problems"].as_array().unwrap().is_empty(),
        "{blocked}"
    );
    let refused = shell
        .call("tools_start_run", tool_run(&workspace))
        .unwrap_err();
    assert!(
        refused.as_str().unwrap().contains("cannot be run"),
        "{refused}"
    );
    assert!(shell.launched.started.lock().unwrap().is_empty());

    // Accepting names the hash the check showed.
    let view = shell
        .call(
            "tools_accept_change",
            json!({ "id": "audio", "newVersion": "2.0", "seenSha256": seen }),
        )
        .unwrap();
    assert_eq!(view["tools"][0]["version"], "2.0");
    assert_eq!(view["tools"][0]["history"].as_array().unwrap().len(), 1);
    assert_eq!(
        keys(&view["tools"][0]["history"][0]),
        ["exe_sha256", "exe_size", "recorded_unix", "version"]
    );

    // The plan shows what would start, in the run's files folder, and starts nothing.
    let plan = shell.call("tools_plan_run", tool_run(&workspace)).unwrap();
    assert_eq!(
        keys(&plan),
        [
            "args",
            "cwd",
            "exe",
            "problems",
            "tool_sha256",
            "tool_version"
        ]
    );
    assert_eq!(plan["problems"], json!([]));
    assert_eq!(plan["args"], json!(["--in", "a.bin"]));
    assert_eq!(plan["tool_version"], "2.0");
    let cwd = PathBuf::from(plan["cwd"].as_str().unwrap());
    assert!(cwd.ends_with("files") && cwd.is_dir(), "{cwd:?}");
    assert!(shell.launched.started.lock().unwrap().is_empty());

    // Start (the program stays "running" for one poll), poll twice, read the record.
    shell.launched.scripts.lock().unwrap().push_back((1, 0));
    let started = shell.call("tools_start_run", tool_run(&workspace)).unwrap();
    assert_eq!(keys(&started), ["run", "view"]);
    assert_eq!(
        keys(&started["run"]),
        ["args", "pid", "run", "started_unix", "tool_id", "workspace"]
    );
    assert_eq!(started["run"]["pid"], 7777);
    assert_eq!(started["view"]["running"][0]["run"], started["run"]["run"]);
    {
        let launched = shell.launched.started.lock().unwrap();
        assert_eq!(launched.len(), 1);
        assert_eq!(launched[0].exe, exe);
        assert_eq!(launched[0].args, ["--in", "a.bin"]);
        assert_eq!(launched[0].cwd, cwd);
        assert!(launched[0].env.is_empty());
    }
    std::fs::write(cwd.join("data").join("from-tool.bin"), "made by the tool").unwrap();

    let run = started["run"]["run"].clone();
    let first = shell.call("tools_poll_run", json!({ "run": run })).unwrap();
    assert_eq!(first, json!({ "state": "running", "pid": 7777 }));
    let done = shell.call("tools_poll_run", json!({ "run": run })).unwrap();
    assert_eq!(
        keys(&done),
        ["record", "save_error", "saved_as", "state", "view"]
    );
    assert_eq!(done["state"], "finished");
    assert_eq!(keys(&done["record"]), TOOL_RUN_RECORD_KEYS);
    assert_eq!(done["record"]["success"], true);
    assert_eq!(done["record"]["tool_version"], "2.0");
    assert_eq!(done["record"]["settings"], json!({ "bank": "music" }));
    assert_eq!(done["record"]["env_names"], json!([]));
    assert_eq!(done["record"]["added"][0]["path"], "data/from-tool.bin");
    assert_eq!(done["save_error"], Value::Null);
    assert_eq!(done["view"]["running"], json!([]));
    let kept = cwd
        .parent()
        .unwrap()
        .join(".sul-tool-runs")
        .join(done["saved_as"].as_str().unwrap());
    assert!(
        kept.is_file(),
        "the record is kept beside run.json: {kept:?}"
    );

    let records = shell
        .call("tools_run_records", json!({ "workspace": workspace }))
        .unwrap();
    assert_eq!(keys(&records), ["records", "total", "unreadable"]);
    assert_eq!(records["total"], 1);
    assert_eq!(keys(&records["records"][0]), ["name", "record"]);
    assert_eq!(records["records"][0]["record"], done["record"]);

    // The run is no longer followed.
    let gone = shell
        .call("tools_poll_run", json!({ "run": run }))
        .unwrap_err();
    assert!(gone.as_str().unwrap().contains("no tool run"), "{gone}");
}

#[test]
fn tool_refusals_reach_the_page_as_text_and_the_arguments_must_be_camel_case() {
    let (shell, exe, workspace) = tools_shell();
    shell
        .call("tools_register", tool_registration(&exe))
        .unwrap();
    let refused = |cmd: &str, args: Value, needle: &str| {
        let e = shell.call(cmd, args).unwrap_err();
        assert!(e.as_str().unwrap().contains(needle), "{cmd}: {e}");
    };

    refused(
        "tools_register",
        tool_registration(&exe),
        "already recorded",
    );
    let mut relative = tool_registration(&exe);
    relative["request"]["id"] = json!("other");
    relative["request"]["exe_path"] = json!("relative/tool.exe");
    refused("tools_register", relative, "full path");
    let mut permitted = tool_registration(&exe);
    permitted["request"]["id"] = json!("other");
    permitted["request"]["redistribution"] = json!("permitted");
    refused("tools_register", permitted, "licence");
    refused("tools_check", json!({ "id": "ghost" }), "no tool");
    refused(
        "tools_accept_change",
        json!({ "id": "audio", "newVersion": "2.0", "seenSha256": "ab" }),
        "nothing to accept",
    );

    // A place that is not a finished run of a recipe is refused, and so is a name that is a path.
    let mut elsewhere = workspace.clone();
    elsewhere["run_id"] = json!("../../x");
    refused("tools_plan_run", tool_run(&elsewhere), "run id");
    refused("tools_start_run", tool_run(&elsewhere), "run id");
    refused(
        "tools_run_records",
        json!({ "workspace": elsewhere }),
        "run id",
    );
    let mut builds_root = workspace.clone();
    builds_root["root_id"] = json!("ssd");
    refused(
        "tools_start_run",
        tool_run(&builds_root),
        "not a mod library",
    );
    let mut ghost = tool_run(&workspace);
    ghost["request"]["tool_id"] = json!("ghost");
    refused("tools_start_run", ghost, "no tool");
    refused("tools_poll_run", json!({ "run": 99 }), "no tool run");
    assert!(shell.launched.started.lock().unwrap().is_empty());

    // The page cannot send an environment: whatever it sends is ignored, and nothing starts with one.
    let mut with_env = tool_run(&workspace);
    with_env["request"]["env"] = json!([["SECRET_TOKEN", "s3cret"]]);
    shell.call("tools_start_run", with_env).unwrap();
    assert!(shell.launched.started.lock().unwrap()[0].env.is_empty());

    // Top-level arguments are camelCase; a request object is snake_case and must be whole.
    let bad = shell
        .call(
            "tools_accept_change",
            json!({ "id": "audio", "new_version": "2.0", "seen_sha256": "ab" }),
        )
        .unwrap_err();
    assert!(bad.as_str().unwrap().contains("newVersion"), "{bad}");
    let bad = shell
        .call(
            "tools_start_run",
            json!({ "request": { "tool_id": "audio", "args": [] } }),
        )
        .unwrap_err();
    assert!(bad.as_str().unwrap().contains("settings"), "{bad}");
}

// ---- Slice D1: the Play panel's commands (launch_view, launch_start, launch_refresh) ----

/// As `TinyAdapter`, but it can say how to start the game: its program is `game.exe` in the build folder.
struct PlayableAdapter;

impl GameAdapter for PlayableAdapter {
    fn id(&self) -> &str {
        "gog-1"
    }
    fn display_name(&self) -> &str {
        "Made-up Game"
    }
    fn version(&self) -> &str {
        "0.0-test"
    }
    fn recognise(&self, _folder: &std::path::Path) -> sul_core::Result<Recognition> {
        Ok(Recognition::not_this_game(
            "a test adapter recognises nothing",
        ))
    }
    fn launch_plan(&self, _dir: &std::path::Path, _v: &str) -> sul_core::Result<LaunchPlan> {
        Ok(LaunchPlan {
            exe: "game.exe".into(),
            args: vec!["--list".into()],
            working_dir: String::new(),
            env: vec![],
            explanation: "--list asks the test program to print its tests and stop.".into(),
        })
    }
    fn mod_layout(&self) -> ModLayout {
        ModLayout {
            allowed_prefixes: vec!["data".into(), "mods".into()],
            forbidden_prefixes: vec![],
            case_insensitive: true,
        }
    }
    fn capabilities(&self) -> Vec<CapabilityEntry> {
        vec![]
    }
}

fn playable_adapters() -> AdapterRegistry {
    let mut a = AdapterRegistry::new();
    a.register(Box::new(PlayableAdapter)).unwrap();
    a
}

const LAUNCH_VIEW_KEYS: [&str; 3] = ["builds", "games", "history"];

#[test]
fn the_launch_view_has_exactly_the_fields_the_page_reads() {
    let empty = shell(vec![]).call("launch_view", json!({})).unwrap();
    assert_eq!(keys(&empty), LAUNCH_VIEW_KEYS);
    for k in LAUNCH_VIEW_KEYS {
        assert_eq!(empty[k], json!([]), "{k}");
    }

    let (shell, _install, build, _src) = mods_shell(playable_adapters());
    let view = shell.call("launch_view", json!({})).unwrap();
    assert_eq!(keys(&view), LAUNCH_VIEW_KEYS);
    let b = &view["builds"][0];
    assert_eq!(
        keys(b),
        [
            "active",
            "build_id",
            "game_id",
            "has_adapter",
            "managed",
            "slot",
            "state"
        ]
    );
    assert_eq!(b["build_id"], json!(build));
    assert_eq!(b["game_id"], json!("gog-1"));
    assert_eq!(b["state"], json!("ready"));
    assert_eq!(b["managed"], json!(true));
    assert_eq!(b["has_adapter"], json!(true));
    assert_eq!(keys(&view["games"][0]), ["game_id", "state"]);
    assert_eq!(view["games"][0]["state"], json!({ "state": "not_running" }));
    assert_eq!(view["history"], json!([]));
}

#[test]
fn with_no_adapter_a_build_is_refused_through_the_command_and_nothing_is_started() {
    // The real app registers no adapter, so this is what Play does for every real game today.
    let (shell, _install, build, _src) = mods_shell(AdapterRegistry::new());
    assert_eq!(
        shell.call("launch_view", json!({})).unwrap()["builds"][0]["has_adapter"],
        json!(false)
    );
    let answer = shell
        .call(
            "launch_start",
            json!({ "buildId": build, "captureOutput": false }),
        )
        .unwrap();
    assert_eq!(keys(&answer), ["record", "view"]);
    let record = &answer["record"];
    assert_eq!(
        keys(record),
        [
            "build_id",
            "ended_unix",
            "game_id",
            "launch_id",
            "log_file",
            "outcome",
            "started_unix",
            "warnings"
        ]
    );
    assert_eq!(record["outcome"]["kind"], json!("not_started"));
    assert_eq!(record["outcome"]["why"], json!({ "reason": "no_adapter" }));
    assert!(
        record["outcome"]["explanation"]
            .as_str()
            .unwrap()
            .contains("no adapter registered")
    );
    assert!(shell.launched.started.lock().unwrap().is_empty());
    assert_eq!(answer["view"]["history"].as_array().unwrap().len(), 1);
    assert_eq!(
        answer["view"]["games"][0]["state"]["state"],
        json!("last_not_started")
    );
}

#[test]
fn a_build_is_started_followed_and_reported_when_it_ends_through_the_commands() {
    let (shell, install, build, _src) = mods_shell(playable_adapters());
    shell.launched.scripts.lock().unwrap().push_back((1, 0));
    let started = shell
        .call(
            "launch_start",
            json!({ "buildId": build, "captureOutput": false }),
        )
        .unwrap();
    assert_eq!(started["record"]["outcome"]["kind"], json!("running"));
    assert_eq!(started["record"]["outcome"]["pid"], json!(7777));
    assert_eq!(
        started["view"]["games"][0]["state"]["state"],
        json!("running")
    );
    assert_eq!(
        started["view"]["games"][0]["state"]["build_id"],
        json!(build)
    );
    {
        let spec = shell.launched.started.lock().unwrap();
        assert_eq!(spec.len(), 1);
        assert!(spec[0].exe.ends_with("game.exe"));
        assert!(
            !spec[0].exe.starts_with(&install),
            "the managed copy's program, never the original install's"
        );
        assert_eq!(spec[0].capture_bytes, None);
    }

    // Reading does not poll, however often it is read.
    for _ in 0..3 {
        let v = shell.call("launch_view", json!({})).unwrap();
        assert_eq!(v["games"][0]["state"]["state"], json!("running"));
    }
    let first = shell.call("launch_refresh", json!({})).unwrap();
    assert_eq!(keys(&first), ["finished", "view"]);
    assert_eq!(first["finished"], json!([]));
    let second = shell.call("launch_refresh", json!({})).unwrap();
    assert_eq!(second["finished"].as_array().unwrap().len(), 1);
    let done = &second["finished"][0];
    assert_eq!(done["outcome"]["kind"], json!("exited"));
    assert_eq!(done["outcome"]["code"], json!(0));
    assert_eq!(done["outcome"]["success"], json!(true));
    assert_eq!(
        second["view"]["games"][0]["state"]["state"],
        json!("last_exited")
    );
    assert_eq!(second["view"]["history"].as_array().unwrap().len(), 1);
}

#[test]
fn a_second_start_of_a_running_game_is_refused_with_the_running_builds_name() {
    let (shell, _install, build, _src) = mods_shell(playable_adapters());
    shell.launched.scripts.lock().unwrap().push_back((100, 0));
    let args = json!({ "buildId": build, "captureOutput": false });
    shell.call("launch_start", args.clone()).unwrap();
    let again = shell.call("launch_start", args).unwrap();
    let why = &again["record"]["outcome"];
    assert_eq!(why["kind"], json!("not_started"));
    assert_eq!(
        why["why"],
        json!({ "reason": "already_running", "build_id": build })
    );
    assert_eq!(shell.launched.started.lock().unwrap().len(), 1);
}

#[test]
fn asking_for_output_sets_a_cap_on_what_the_program_may_leave_and_not_asking_sets_none() {
    let (shell, _install, build, _src) = mods_shell(playable_adapters());
    shell
        .call(
            "launch_start",
            json!({ "buildId": build, "captureOutput": true }),
        )
        .unwrap();
    assert!(
        shell.launched.started.lock().unwrap()[0]
            .capture_bytes
            .is_some()
    );
}

#[test]
fn launch_refusals_reach_the_page_as_text_and_the_arguments_must_be_camel_case() {
    let (shell, _install, build, _src) = mods_shell(playable_adapters());
    let refused = |cmd: &str, args: Value, needle: &str| {
        let e = shell.call(cmd, args).unwrap_err();
        assert!(e.as_str().unwrap().contains(needle), "{cmd}: {e}");
    };
    refused(
        "launch_start",
        json!({ "buildId": "  ", "captureOutput": false }),
        "pick a build",
    );
    // snake_case arguments are not read: the command names what is missing.
    refused(
        "launch_start",
        json!({ "build_id": build, "capture_output": false }),
        "buildId",
    );
    refused("launch_start", json!({ "buildId": build }), "captureOutput");
    assert!(shell.launched.started.lock().unwrap().is_empty());

    // An unknown build is an answer, not an error: the record says there is no such build.
    let ghost = shell
        .call(
            "launch_start",
            json!({ "buildId": "ghost-1", "captureOutput": false }),
        )
        .unwrap();
    assert_eq!(
        ghost["record"]["outcome"]["why"],
        json!({ "reason": "no_such_build" })
    );
}

// ---- Slice D2: the catalogue and the game detail (catalogue_view, game_detail, catalogue_set_categories, catalogue_set_artwork) ----

const DETAIL_KEYS: [&str; 10] = [
    "adapter",
    "artwork",
    "builds",
    "capability",
    "capability_label",
    "categories",
    "display_name",
    "game_id",
    "launch",
    "notes",
];

fn text_of(e: &Value) -> &str {
    e.as_str().unwrap_or_else(|| panic!("not text: {e}"))
}

#[test]
fn the_catalogue_view_and_the_detail_have_exactly_the_fields_the_page_reads() {
    let empty = shell(vec![]).call("catalogue_view", json!({})).unwrap();
    assert_eq!(keys(&empty), ["categories", "entries"]);
    assert_eq!(empty["categories"], json!([]));
    assert_eq!(empty["entries"], json!([]));

    let shell = shell(steam_and_gog());
    let d = shell
        .call("game_detail", json!({ "gameId": "gog-1" }))
        .unwrap();
    assert_eq!(keys(&d), DETAIL_KEYS);
    // The name is the library's, and a game with no adapter and no build says so in words.
    assert_eq!(d["game_id"], json!("gog-1"));
    assert_eq!(d["display_name"], json!("Alpha"));
    assert_eq!(d["categories"], json!([]));
    assert_eq!(d["artwork"], Value::Null);
    assert_eq!(d["adapter"], Value::Null);
    assert_eq!(d["builds"], json!([]));
    assert_eq!(d["capability"], json!("not_investigated"));
    assert_eq!(d["capability_label"], json!("Not investigated"));
    assert_eq!(d["launch"], json!({ "state": "not_running" }));
    let notes: Vec<&str> = d["notes"].as_array().unwrap().iter().map(text_of).collect();
    assert!(notes.iter().any(|n| n.contains("No adapter")), "{notes:?}");
    assert!(notes.iter().any(|n| n.contains("no build")), "{notes:?}");
}

#[test]
fn a_game_with_a_build_and_an_adapter_shows_both_in_its_detail() {
    let (shell, _install, build, _src) = mods_shell(playable_adapters());
    let d = shell
        .call("game_detail", json!({ "gameId": "gog-1" }))
        .unwrap();
    assert_eq!(keys(&d), DETAIL_KEYS);
    assert_eq!(keys(&d["adapter"]), ["id", "version"]);
    assert_eq!(d["adapter"]["id"], json!("gog-1"));
    let b = &d["builds"][0];
    assert_eq!(keys(b), ["active", "build_id", "managed", "slot", "state"]);
    assert_eq!(b["build_id"], json!(build));
    assert_eq!(b["state"], json!("ready"));
    assert_eq!(b["managed"], json!(true));
    // Importing does not make the build active; the detail says so until it is.
    assert_eq!(b["active"], json!(false));
    assert!(
        d["notes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| text_of(n).contains("None of this game's builds is active"))
    );
    shell
        .call("manager_activate_build", json!({ "buildId": build }))
        .unwrap();
    let d = shell
        .call("game_detail", json!({ "gameId": "gog-1" }))
        .unwrap();
    assert_eq!(d["builds"][0]["active"], json!(true));
}

#[test]
fn categories_set_through_the_command_come_back_with_the_detail_and_the_view() {
    let shell = shell(steam_and_gog());
    let e = shell
        .call(
            "catalogue_set_categories",
            json!({ "gameId": "gog-1", "categories": ["RPG", " co-op ", "rpg"] }),
        )
        .unwrap();
    assert_eq!(keys(&e), ["detail", "view"]);
    assert_eq!(keys(&e["detail"]), DETAIL_KEYS);
    assert_eq!(e["detail"]["categories"], json!(["RPG", "co-op"]));
    let view = &e["view"];
    assert_eq!(keys(view), ["categories", "entries"]);
    assert_eq!(keys(&view["categories"][0]), ["count", "game_ids", "name"]);
    assert_eq!(keys(&view["entries"][0]), ["categories", "game_id"]);
    assert_eq!(view["categories"][0]["name"], json!("co-op"));
    assert_eq!(view["categories"][1]["name"], json!("RPG"));
    assert_eq!(view["categories"][1]["game_ids"], json!(["gog-1"]));
    assert_eq!(&shell.call("catalogue_view", json!({})).unwrap(), view);
    // Another game takes the same category; the library's order (Helldivers 2 first) decides the order of the ids.
    let both = shell
        .call(
            "catalogue_set_categories",
            json!({ "gameId": "steam-553850", "categories": ["rpg"] }),
        )
        .unwrap();
    let rpg = both["view"]["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == json!("RPG"))
        .unwrap();
    assert_eq!(rpg["count"], json!(2));
    assert_eq!(rpg["game_ids"].as_array().unwrap().len(), 2);
    // A bad name is refused in words and nothing changes.
    let refused = shell
        .call(
            "catalogue_set_categories",
            json!({ "gameId": "gog-1", "categories": ["fine", "  "] }),
        )
        .unwrap_err();
    assert!(text_of(&refused).contains("category"), "{refused}");
    assert_eq!(
        shell.call("catalogue_view", json!({})).unwrap(),
        both["view"]
    );
}

#[test]
fn a_game_the_library_does_not_list_is_refused_by_every_command_that_names_one() {
    let shell = shell(steam_and_gog());
    for (cmd, args) in [
        ("game_detail", json!({ "gameId": "ghost" })),
        (
            "catalogue_set_categories",
            json!({ "gameId": "ghost", "categories": ["x"] }),
        ),
        (
            "catalogue_set_artwork",
            json!({ "gameId": "ghost", "path": null }),
        ),
    ] {
        let e = shell.call(cmd, args).unwrap_err();
        assert!(
            text_of(&e).contains("the library has no game"),
            "{cmd}: {e}"
        );
    }
    // No entry was made for a name nobody has seen.
    assert_eq!(
        shell.call("catalogue_view", json!({})).unwrap()["entries"],
        json!([])
    );
}

#[test]
fn a_game_the_last_scan_did_not_find_can_still_be_labelled() {
    let (shell, _install, _store) = real_game_shell_with(true, AdapterRegistry::new());
    let e = shell
        .call(
            "catalogue_set_categories",
            json!({ "gameId": "gog-2", "categories": ["Gone"] }),
        )
        .unwrap();
    assert_eq!(e["detail"]["display_name"], json!("Gone Game"));
    assert_eq!(e["detail"]["categories"], json!(["Gone"]));
}

#[test]
fn artwork_is_referenced_from_a_real_image_file_and_cleared_with_null() {
    let (shell, _install, _build, src) = mods_shell(AdapterRegistry::new());
    let base = src.parent().unwrap();
    let image = base.join("cover.png");
    std::fs::write(&image, b"only a file with a picture's name").unwrap();
    let canonical = image.canonicalize().unwrap();
    let e = shell
        .call(
            "catalogue_set_artwork",
            json!({ "gameId": "gog-1", "path": image.to_str().unwrap() }),
        )
        .unwrap();
    assert_eq!(
        e["detail"]["artwork"],
        json!(canonical.to_str().unwrap()),
        "the detail names the file the reference points at"
    );
    assert_eq!(
        shell
            .call("game_detail", json!({ "gameId": "gog-1" }))
            .unwrap()["artwork"],
        e["detail"]["artwork"]
    );
    // Not an image, not there, or blank: refused in words, the reference stays.
    let text = base.join("notes.txt");
    std::fs::write(&text, "words").unwrap();
    for bad in [
        text.to_str().unwrap(),
        "   ",
        base.join("nope.png").to_str().unwrap(),
    ] {
        let e = shell
            .call(
                "catalogue_set_artwork",
                json!({ "gameId": "gog-1", "path": bad }),
            )
            .unwrap_err();
        assert!(!text_of(&e).is_empty(), "{bad}: {e}");
    }
    assert_eq!(
        shell
            .call("game_detail", json!({ "gameId": "gog-1" }))
            .unwrap()["artwork"],
        e["detail"]["artwork"]
    );
    let cleared = shell
        .call(
            "catalogue_set_artwork",
            json!({ "gameId": "gog-1", "path": null }),
        )
        .unwrap();
    assert_eq!(cleared["detail"]["artwork"], Value::Null);
    assert!(image.is_file(), "the owner's image is never touched");
}

#[test]
fn the_catalogue_arguments_must_be_camel_case_and_the_refusals_are_text() {
    let shell = shell(steam_and_gog());
    let refused = |cmd: &str, args: Value, needle: &str| {
        let e = shell.call(cmd, args).unwrap_err();
        assert!(text_of(&e).contains(needle), "{cmd}: {e}");
    };
    refused("game_detail", json!({ "game_id": "gog-1" }), "gameId");
    refused(
        "catalogue_set_categories",
        json!({ "game_id": "gog-1", "categories": [] }),
        "gameId",
    );
    refused(
        "catalogue_set_categories",
        json!({ "gameId": "gog-1" }),
        "categories",
    );
    refused(
        "catalogue_set_artwork",
        json!({ "game_id": "gog-1", "path": null }),
        "gameId",
    );
    assert_eq!(
        shell.call("catalogue_view", json!({})).unwrap()["entries"],
        json!([])
    );
}
