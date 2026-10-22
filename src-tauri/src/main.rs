// UNVERIFIED: this wrapper has not been compiled yet (no webview libs in the authoring
// environment). First build happens on Windows or in CI. See docs/SETUP-WINDOWS.md.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;

#[derive(Serialize)]
struct AppInfo {
    name: &'static str,
    version: &'static str,
}

/// Placeholder bridge command. Real adapters/capabilities are exposed here later, always
/// through sul-core so path-safety and protected-root checks apply.
#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo { name: "Soups Unified Launcher", version: env!("CARGO_PKG_VERSION") }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![app_info])
        .run(tauri::generate_context!())
        .expect("error while running SUL");
}
