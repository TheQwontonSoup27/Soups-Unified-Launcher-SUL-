//! Public-API contract of `sul_core::steam`, as seen from outside the crate (the Tauri container
//! and the UI depend on exactly these items).
//!
//! The manifest and library parsing, de-duplication and the read-only guarantees are tested next to
//! the code in `registry::steam_library` and `registry::vdf`, against `C:\...` fixtures that run on
//! any host. They are not repeated here.

use std::fs;
use std::path::Path;

#[cfg(not(feature = "steam"))]
use sul_core::Error;
#[cfg(not(feature = "steam"))]
use sul_core::steam::SteamSession;
use sul_core::steam::{STEAM_SILENT_ARG, SteamAppInfo, is_inside_steam_library, launch_url};

#[test]
fn launch_url_uses_the_steam_protocol_only() {
    assert_eq!(launch_url(553850), "steam://rungameid/553850");
}

#[test]
fn the_silent_flag_is_steams_own_tray_only_switch() {
    assert_eq!(STEAM_SILENT_ARG, "-silent");
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
fn without_the_steam_feature_the_session_refuses_instead_of_pretending() {
    assert!(matches!(
        SteamSession::init(480),
        Err(Error::Unsupported(_))
    ));
}
