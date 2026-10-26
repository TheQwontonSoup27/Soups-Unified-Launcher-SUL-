//! Steam integration (spec section 1).
//!
//! Two independent halves live here:
//!
//! * **Library helpers** (always built, read-only): the `steam://` launch URL, the
//!   default install folders, and the guard that keeps a managed build out of Steam's
//!   update reach. Discovery of installed games is in [`crate::registry`].
//! * **Headless session** ([`SteamSession`], behind the `steam` feature).
//!
//! What `steamworks-rs` can and cannot do matters for the session's design:
//!
//! * It attaches to a Steam client that is **already running** on the machine;
//!   it does not log in by itself and does not store credentials.
//! * The client can run without a visible window if it is started with
//!   [`STEAM_SILENT_ARG`] (tray only). That is how "no Steam window on the
//!   desktop" is achieved with this crate.
//! * Initialising with an app id makes Steam treat that app as running.
//!
//! Persistent credential handling therefore needs a separate mechanism and is
//! deliberately not part of this module. Launching a game never needs a Steam
//! password: Steam stays logged in and SUL asks it to start the game with
//! [`launch_url`]. Nothing here writes into Steam's folders.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[cfg(not(feature = "steam"))]
use crate::Error;
use crate::Result;
use crate::safepath::resolve_lenient;

/// Command-line flag that starts the Steam client minimised to the tray.
pub const STEAM_SILENT_ARG: &str = "-silent";

/// What an `appmanifest_<id>.acf` says about the installed build.
///
/// Both fields are informational: SUL records them (the build id is the natural baseline
/// for "which version is this?") and never edits them. Editing `AutoUpdateBehavior` would
/// modify the original installation, and Steam can reset it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SteamAppInfo {
    /// `buildid`: changes with every published Steam build.
    #[serde(default)]
    pub build_id: Option<String>,
    /// `AutoUpdateBehavior`: 0 = always update, 1 = only on launch, 2 = high priority.
    #[serde(default)]
    pub auto_update_behavior: Option<u32>,
}

/// URL that asks the logged-in Steam client to start a game. No credentials involved.
pub fn launch_url(app_id: u32) -> String {
    format!("steam://rungameid/{app_id}")
}

/// Likely Steam install folders on Windows, used when the registry does not say where Steam is.
///
/// Candidates only: callers must check that they exist.
pub fn default_steam_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for var in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(base) = std::env::var_os(var).filter(|v| !v.is_empty()) {
            dirs.push(PathBuf::from(base).join("Steam"));
        }
    }
    dirs.push(PathBuf::from(r"C:\Program Files (x86)\Steam"));
    dirs.dedup();
    dirs
}

/// True if `candidate` lies inside the `steamapps` folder of any Steam library.
///
/// A managed legacy build must **not** live there: Steam updates and verifies the games
/// inside its libraries. Paths are canonicalised where they exist, so symlinks and `..`
/// cannot hide a location inside a library.
pub fn is_inside_steam_library(candidate: &Path, library_roots: &[PathBuf]) -> bool {
    let candidate = resolve_lenient(candidate);
    library_roots
        .iter()
        .any(|root| candidate.starts_with(resolve_lenient(&root.join("steamapps"))))
}

/// Handle to a connected Steam client.
pub struct SteamSession {
    #[cfg(feature = "steam")]
    client: steamworks::Client,
}

impl SteamSession {
    /// Connect to the running Steam client as the given app id.
    #[cfg(feature = "steam")]
    pub fn init(app_id: u32) -> Result<Self> {
        let client =
            steamworks::Client::init_app(app_id).map_err(|e| crate::Error::Steam(e.to_string()))?;
        Ok(Self { client })
    }

    /// Without the `steam` feature there is nothing to connect to.
    #[cfg(not(feature = "steam"))]
    pub fn init(_app_id: u32) -> Result<Self> {
        Err(Error::Unsupported("built without the `steam` feature"))
    }

    /// Display name of the signed-in Steam user.
    #[cfg(feature = "steam")]
    pub fn persona_name(&self) -> String {
        self.client.friends().name()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[cfg(not(feature = "steam"))]
    #[test]
    fn init_requires_the_steam_feature() {
        assert!(matches!(
            SteamSession::init(480),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn launch_url_uses_the_steam_protocol_only() {
        assert_eq!(launch_url(553850), "steam://rungameid/553850");
    }

    #[test]
    fn default_dirs_always_offer_the_standard_location() {
        let dirs = default_steam_dirs();
        assert!(dirs.contains(&PathBuf::from(r"C:\Program Files (x86)\Steam")));
        // Compared as text: on non-Windows hosts a backslash path is a single component.
        assert!(dirs.iter().all(|d| d.to_string_lossy().ends_with("Steam")));
    }

    #[test]
    fn managed_builds_must_stay_outside_steam_libraries() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join("steamapps").join("common").join("game")).unwrap();
        fs::create_dir_all(root.join("SUL-builds").join("legacy")).unwrap();
        let libraries = vec![root.clone()];

        let inside = root.join("steamapps").join("common").join("game");
        assert!(is_inside_steam_library(&inside, &libraries));
        // A not-yet-created folder under steamapps is still inside.
        assert!(is_inside_steam_library(
            &inside.join("new_build"),
            &libraries
        ));
        // `..` cannot be used to sneak back in.
        let sneaky = root
            .join("SUL-builds")
            .join("..")
            .join("steamapps")
            .join("x");
        assert!(is_inside_steam_library(&sneaky, &libraries));

        assert!(!is_inside_steam_library(
            &root.join("SUL-builds").join("legacy"),
            &libraries
        ));
        assert!(!is_inside_steam_library(
            Path::new("/definitely/elsewhere"),
            &libraries
        ));
        assert!(!is_inside_steam_library(&inside, &[]));
    }
}
