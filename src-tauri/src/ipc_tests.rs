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

use crate::{DiscordState, commands::AppState, handlers};

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
    let dir = tempfile::tempdir().unwrap();
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
        .manage(AppState::with_steam(&file, steam))
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
