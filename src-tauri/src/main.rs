// The desktop wrapper. All real logic lives in sul-core; commands here stay thin.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use std::sync::Mutex;
use sul_core::capability::Status;
use sul_core::discord::{IpcTransport, Presence, PublishOutcome, RichPresence};

#[derive(Serialize)]
struct AppInfo {
    name: &'static str,
    version: &'static str,
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo { name: "Soups Unified Launcher", version: env!("CARGO_PKG_VERSION") }
}

/// Holds the live Discord connection while the global Rich Presence toggle is on.
struct DiscordState(Mutex<Option<RichPresence<IpcTransport>>>);

/// Global Rich Presence switch. Returns "on" / "off", or an error message the UI shows to the user.
/// The Discord application ID comes from the SUL_DISCORD_CLIENT_ID environment variable (docs/discord.md).
#[tauri::command]
fn discord_set_enabled(enabled: bool, state: tauri::State<'_, DiscordState>) -> Result<String, String> {
    let mut guard = state.0.lock().map_err(|_| "internal lock error".to_string())?;
    if !enabled {
        if let Some(mut rp) = guard.take() {
            rp.set_enabled(false).map_err(|e| e.to_string())?;
        }
        return Ok("off".to_string());
    }
    if guard.is_some() {
        return Ok("on".to_string());
    }
    let client_id = std::env::var("SUL_DISCORD_CLIENT_ID").map_err(|_| {
        "Discord application ID not set. See docs/discord.md (environment variable SUL_DISCORD_CLIENT_ID).".to_string()
    })?;
    let transport = IpcTransport::connect().map_err(|e| e.to_string())?;
    let mut rp = RichPresence::new(&client_id, transport);
    rp.connect().map_err(|e| e.to_string())?;
    rp.set_enabled(true).map_err(|e| e.to_string())?;
    let idle = Presence { details: "Browsing the launcher".to_string(), ..Default::default() };
    match rp.publish(Status::Experimental, &idle).map_err(|e| e.to_string())? {
        PublishOutcome::Sent => {}
        other => return Err(format!("presence not published: {other:?}")),
    }
    *guard = Some(rp);
    Ok("on".to_string())
}

fn main() {
    tauri::Builder::default()
        .manage(DiscordState(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![app_info, discord_set_enabled])
        .run(tauri::generate_context!())
        .expect("error while running SUL");
}
