//! Public-API contract of `sul_core::steam`, as seen from outside the crate (the Tauri container
//! and the UI depend on exactly these items).
//!
//! The manifest and library parsing, de-duplication and the read-only guarantees are tested next to
//! the code in `registry::steam_library` and `registry::vdf`, against `C:\...` fixtures that run on
//! any host. The manager's launch logic is unit-tested in `steam.rs` itself.

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[cfg(not(feature = "steam"))]
use sul_core::Error;
use sul_core::steam::{
    ClientProbe, LaunchRoute, STEAM_SILENT_ARG, SteamAppInfo, SteamManager, UrlOpener,
    is_inside_steam_library, launch_url,
};

/// Records every URL it is asked to open; clones share one list.
#[derive(Clone, Default)]
struct Opened(Arc<Mutex<Vec<String>>>);

impl UrlOpener for Opened {
    fn open(&self, url: &str) -> sul_core::Result<()> {
        self.0.lock().unwrap().push(url.to_owned());
        Ok(())
    }
}

struct Probe(bool);

impl ClientProbe for Probe {
    fn client_running(&self) -> bool {
        self.0
    }
}

#[test]
fn launch_url_uses_the_steam_protocol_only() {
    assert_eq!(launch_url(553850), "steam://run/553850");
}

#[test]
fn the_silent_flag_is_steams_own_tray_only_switch() {
    assert_eq!(STEAM_SILENT_ARG, "-silent");
}

#[test]
fn a_host_can_drive_the_manager_through_its_public_seams() {
    let opened = Opened::default();
    let steam = SteamManager::with_parts(Box::new(Probe(false)), Box::new(opened.clone()));

    // The exact signature the Tauri command relies on.
    let result: Result<(), String> = steam.launch_steam_game(553850);
    assert_eq!(result, Ok(()));
    assert_eq!(steam.launch(553850).unwrap(), LaunchRoute::StartedSteam);
    assert_eq!(
        *opened.0.lock().unwrap(),
        ["steam://run/553850", "steam://run/553850"]
    );

    assert!(steam.launch_steam_game(0).is_err());
}

#[test]
fn managed_builds_must_stay_outside_steam_libraries() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().to_path_buf();
    fs::create_dir_all(root.join("steamapps").join("common").join("game")).unwrap();
    fs::create_dir_all(root.join("SUL-builds").join("legacy")).unwrap();
    let libs = vec![root.clone()];

    let inside = root.join("steamapps").join("common").join("game");
    assert!(is_inside_steam_library(&inside, &libs));
    assert!(!is_inside_steam_library(
        &root.join("SUL-builds").join("legacy"),
        &libs
    ));
    assert!(!is_inside_steam_library(
        Path::new("/definitely/elsewhere"),
        &libs
    ));
    assert!(!is_inside_steam_library(&inside, &[]));
}

#[test]
fn app_info_serialises_with_the_field_names_the_ui_reads() {
    let info = SteamAppInfo {
        build_id: Some("12345678".into()),
        auto_update_behavior: Some(1),
    };
    let json = serde_json::to_value(&info).unwrap();
    assert_eq!(json["build_id"], "12345678");
    assert_eq!(json["auto_update_behavior"], 1);

    // Missing fields stay missing: a game's record from before they existed still loads.
    let old: SteamAppInfo = serde_json::from_str("{}").unwrap();
    assert_eq!(old, SteamAppInfo::default());
    assert_eq!(old.build_id, None);
}

#[cfg(not(feature = "steam"))]
#[test]
fn without_the_steam_feature_connecting_refuses_instead_of_pretending() {
    let steam = SteamManager::system();
    assert!(matches!(steam.connect(), Err(Error::Unsupported(_))));
    assert!(!steam.is_connected());
}
