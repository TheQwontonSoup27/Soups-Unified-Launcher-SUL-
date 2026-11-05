//! The commands, called the way the webview calls them: through Tauri's IPC, with JSON arguments.
//!
//! This is the other half of `ui/tests/dom-wire.test.tsx`. That test checks what the page sends; this
//! one checks that the shell accepts exactly that, answers with the shapes the page reads, and that
//! a refused request reaches the page as plain text. The mock runtime opens no window.

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

struct Shell {
    webview: tauri::WebviewWindow<MockRuntime>,
    opened: Opened,
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
    let file = dir.path().join("library.json");
    if !games.is_empty() {
        LibraryService::open(&file)
            .unwrap()
            .scan(&Found(games))
            .unwrap();
    }
    let opened = Opened::default();
    let steam = SteamManager::with_parts(Box::new(SteamClosed), Box::new(opened.clone()));
    let app = mock_builder()
        .manage(DiscordState::default())
        .manage(AppState::with_parts(&file, steam, Box::new(Plenty)))
        .invoke_handler(handlers())
        .build(mock_context(noop_assets()))
        .expect("the mock app builds");
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    Shell {
        webview,
        opened,
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
    (shell_in(dir, vec![]), install, store)
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
