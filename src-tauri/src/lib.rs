//! Tauri container. All real logic lives in `sul-core`; commands here stay thin.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use sul_core::capability::Status;
use sul_core::discord::{IpcTransport, Presence, PublishOutcome, RichPresence};
use sul_core::portability::{self, InstallMode};

/// Environment variable that overrides the detected app root (useful in development,
/// where the executable lives in `target/debug`).
const APP_ROOT_ENV: &str = "SUL_APP_ROOT";

/// Environment variable holding the Discord application (client) id. It is an identifier,
/// not a secret, but it is still read from the environment rather than baked into source
/// (see `docs/discord.md`).
const DISCORD_CLIENT_ID_ENV: &str = "SUL_DISCORD_CLIENT_ID";

/// The folder that holds the launcher: `SUL_APP_ROOT` if set, otherwise the
/// directory containing the executable.
fn app_root() -> PathBuf {
    if let Some(root) = std::env::var_os(APP_ROOT_ENV) {
        return PathBuf::from(root);
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[derive(Serialize)]
struct AppInfo {
    name: &'static str,
    version: &'static str,
    install_mode: InstallMode,
    app_root: String,
}

/// First wired command: proves the UI -> Tauri -> `sul-core` path end to end.
#[tauri::command]
fn app_info() -> AppInfo {
    let root = app_root();
    AppInfo {
        name: "Soups Unified Launcher",
        version: env!("CARGO_PKG_VERSION"),
        install_mode: portability::detect_mode(&root),
        app_root: root.display().to_string(),
    }
}

/// Holds the live Discord connection while the global Rich Presence switch is on.
#[derive(Default)]
struct DiscordState(Mutex<Option<RichPresence<IpcTransport>>>);

/// What the launcher shows while it is open. Only the asset key for the small corner image
/// is user-chosen; Discord draws that image at a size the activity payload cannot change.
fn idle_presence(small_image: Option<String>) -> Presence {
    Presence {
        details: "Browsing the launcher".to_owned(),
        small_image: small_image.unwrap_or_default(),
        small_text: "Soups Unified Launcher".to_owned(),
        ..Presence::default()
    }
}

/// Global Rich Presence switch. Returns "on" / "off", or an error message the UI shows.
///
/// Opt-in: nothing connects to Discord until this is called with `enabled = true`. Calling it
/// again while on re-publishes, which is how a changed `small_image` asset key takes effect.
#[tauri::command]
fn discord_set_enabled(
    enabled: bool,
    small_image: Option<String>,
    state: tauri::State<'_, DiscordState>,
) -> Result<String, String> {
    let mut guard = state
        .0
        .lock()
        .map_err(|_| "internal lock error".to_owned())?;

    if !enabled {
        if let Some(mut rp) = guard.take() {
            rp.set_enabled(false).map_err(|e| e.to_string())?;
        }
        return Ok("off".to_owned());
    }

    let presence = idle_presence(small_image);
    if let Some(rp) = guard.as_mut() {
        publish_idle(rp, &presence)?;
        return Ok("on".to_owned());
    }

    let client_id = std::env::var(DISCORD_CLIENT_ID_ENV).map_err(|_| {
        format!(
            "Discord application ID not set. See docs/discord.md (environment variable {DISCORD_CLIENT_ID_ENV})."
        )
    })?;
    let transport = IpcTransport::connect().map_err(|e| e.to_string())?;
    let mut rp = RichPresence::new(&client_id, transport);
    rp.connect().map_err(|e| e.to_string())?;
    rp.set_enabled(true).map_err(|e| e.to_string())?;
    publish_idle(&mut rp, &presence)?;
    *guard = Some(rp);
    Ok("on".to_owned())
}

fn publish_idle(rp: &mut RichPresence<IpcTransport>, presence: &Presence) -> Result<(), String> {
    match rp
        .publish(Status::Experimental, presence)
        .map_err(|e| e.to_string())?
    {
        PublishOutcome::Sent => Ok(()),
        other => Err(format!("presence not published: {other:?}")),
    }
}

pub fn run() {
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .manage(DiscordState::default())
        .invoke_handler(tauri::generate_handler![app_info, discord_set_enabled])
        .run(tauri::generate_context!())
        .expect("failed to run Soups Unified Launcher");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_presence_carries_only_the_chosen_small_image() {
        let p = idle_presence(Some("team27_corner".into()));
        assert_eq!(p.small_image, "team27_corner");
        assert!(p.large_image.is_empty(), "no large image is claimed");
        assert!(
            p.start_unix.is_none(),
            "no play-time timer for the idle state"
        );

        assert!(idle_presence(None).small_image.is_empty());
    }
}
