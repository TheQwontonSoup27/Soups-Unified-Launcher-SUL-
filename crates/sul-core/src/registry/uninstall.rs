//! Games found through the Windows "Uninstall" registry keys.
//!
//! That list holds every installed program, not just games, so entries are filtered
//! in two steps:
//!
//! 1. **Noise out** (always): system components, updates and redistributables,
//!    known launchers, entries with no usable install folder.
//! 2. **Game signals in** (default): the entry is a `Steam App <id>` key, or its
//!    install path passes through a well-known game folder (`steamapps`,
//!    `GOG Games`, `Games`, ...). With `include_unclassified`, entries that pass
//!    step 1 but show no signal are returned too, so a UI can offer them.

use std::path::PathBuf;

use super::access::{Hive, KeyPath, RawKey, RegistryAccess, View};
use super::paths::{
    clean_dir, name_id, normalize_key, parse_command_path, store_id, windows_parent,
};
use super::{GameSource, InstalledGame, expand_env};
use crate::Result;

const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";

const LOCATIONS: [(Hive, View); 4] = [
    (Hive::LocalMachine, View::Registry64),
    (Hive::LocalMachine, View::Registry32),
    (Hive::CurrentUser, View::Registry64),
    (Hive::CurrentUser, View::Registry32),
];

/// Path components that mean "a game library folder" (compared case-insensitively).
const GAME_DIR_MARKERS: [&str; 7] = [
    "steamapps",
    "gog games",
    "epic games",
    "xboxgames",
    "origin games",
    "ea games",
    "games",
];

/// Launchers and stores that live inside game-looking folders but are not games.
const LAUNCHER_NAMES: [&str; 10] = [
    "steam",
    "epic games launcher",
    "epic online services",
    "gog galaxy",
    "battle.net",
    "ubisoft connect",
    "ea app",
    "ea desktop",
    "origin",
    "riot client",
];

/// Substrings (lowercase) of display names that are never games.
const NOISE_NAME_PARTS: [&str; 7] = [
    "redistributable",
    "visual c++",
    "microsoft .net",
    "directx",
    "vulkan runtime",
    "security update",
    "hotfix",
];

const NOISE_RELEASE_TYPES: [&str; 5] = [
    "update",
    "hotfix",
    "security update",
    "update rollup",
    "service pack",
];

struct Candidate {
    key_name: String,
    name: String,
    dir: String,
    publisher: Option<String>,
}

fn is_noise_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let is_kb = lower
        .strip_prefix("kb")
        .is_some_and(|rest| rest.bytes().next().is_some_and(|b| b.is_ascii_digit()));
    is_kb
        || LAUNCHER_NAMES.contains(&lower.as_str())
        || NOISE_NAME_PARTS.iter().any(|part| lower.contains(part))
}

/// Windows system folders are never an install location worth reporting.
fn is_windows_dir(dir: &str) -> bool {
    let key = normalize_key(dir);
    key.contains("\\windows\\") || key.ends_with("\\windows")
}

fn install_dir(key: &RawKey) -> Option<String> {
    let declared = key
        .string("InstallLocation")
        .map(expand_env)
        .and_then(|value| clean_dir(&value));
    if declared.is_some() {
        return declared;
    }

    // No InstallLocation: fall back to the folder of the icon or uninstaller.
    ["DisplayIcon", "UninstallString"]
        .into_iter()
        .find_map(|name| {
            let command = expand_env(key.string(name)?);
            let dir = windows_parent(&parse_command_path(&command)?)?;
            (!is_windows_dir(&dir)).then_some(dir)
        })
}

fn candidate(key: &RawKey) -> Option<Candidate> {
    let name = key.string("DisplayName")?;

    if key.dword("SystemComponent") == Some(1)
        || key.string("ParentKeyName").is_some()
        || key.string("ParentDisplayName").is_some()
        || is_noise_name(name)
    {
        return None;
    }
    if let Some(release_type) = key.string("ReleaseType") {
        if NOISE_RELEASE_TYPES.contains(&release_type.to_ascii_lowercase().as_str()) {
            return None;
        }
    }

    Some(Candidate {
        key_name: key.name.clone(),
        name: name.to_owned(),
        dir: install_dir(key)?,
        publisher: key.string("Publisher").map(str::to_owned),
    })
}

/// `"Steam App 220"` -> `Some("220")`.
fn steam_app_id(key_name: &str) -> Option<&str> {
    let (head, id) = key_name.split_at_checked("Steam App ".len())?;
    (head.eq_ignore_ascii_case("Steam App ")
        && !id.is_empty()
        && id.bytes().all(|b| b.is_ascii_digit()))
    .then_some(id)
}

fn has_game_signal(key_name: &str, dir: &str) -> bool {
    steam_app_id(key_name).is_some()
        || dir.split(['\\', '/']).any(|part| {
            GAME_DIR_MARKERS
                .iter()
                .any(|m| part.eq_ignore_ascii_case(m))
        })
}

fn into_game(c: Candidate) -> InstalledGame {
    match steam_app_id(&c.key_name) {
        Some(app_id) => InstalledGame {
            id: store_id("steam", app_id),
            name: c.name,
            install_dir: PathBuf::from(c.dir),
            source: GameSource::Steam,
            store_id: Some(app_id.to_owned()),
            publisher: c.publisher,
            steam: None,
        },
        None => InstalledGame {
            id: name_id("reg", &c.name, &c.key_name),
            name: c.name,
            install_dir: PathBuf::from(c.dir),
            source: GameSource::Registry,
            store_id: None,
            publisher: c.publisher,
            steam: None,
        },
    }
}

pub(crate) fn discover<R: RegistryAccess + ?Sized>(
    registry: &R,
    include_unclassified: bool,
) -> Result<Vec<InstalledGame>> {
    let mut games = Vec::new();
    for (hive, view) in LOCATIONS {
        for key in registry.subkeys(&KeyPath::new(hive, view, UNINSTALL))? {
            let Some(candidate) = candidate(&key) else {
                continue;
            };
            if include_unclassified || has_game_signal(&candidate.key_name, &candidate.dir) {
                games.push(into_game(candidate));
            }
        }
    }
    Ok(games)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::access::{MemoryRegistry, RegValue, s};
    use crate::storage::validate_game_id;

    const HKLM: Hive = Hive::LocalMachine;

    fn add(reg: &mut MemoryRegistry, view: View, key: &str, values: &[(&str, RegValue)]) {
        reg.add(HKLM, view, &format!(r"{UNINSTALL}\{key}"), values);
    }

    fn sample() -> MemoryRegistry {
        let mut reg = MemoryRegistry::default();
        let v64 = View::Registry64;
        add(
            &mut reg,
            v64,
            "Steam App 220",
            &[
                ("DisplayName", s("Half-Life 2")),
                (
                    "InstallLocation",
                    s(r"C:\Program Files (x86)\Steam\steamapps\common\Half-Life 2"),
                ),
                ("Publisher", s("Valve")),
            ],
        );
        add(
            &mut reg,
            View::Registry32,
            "FooGame_is1",
            &[
                ("DisplayName", s("Foo Game")),
                ("InstallLocation", s(r#""D:\Games\Foo\""#)),
                ("Publisher", s("Foo Studio")),
            ],
        );
        // Install location missing: derived from the icon path, with %VAR% expansion unused.
        add(
            &mut reg,
            v64,
            "BarGame",
            &[
                ("DisplayName", s("Bar Game")),
                ("DisplayIcon", s(r#""E:\Games\Bar\bar.exe",0"#)),
            ],
        );
        // Real software with no game signal.
        add(
            &mut reg,
            v64,
            "SomeEditor",
            &[
                ("DisplayName", s("Some Editor")),
                ("InstallLocation", s(r"C:\Program Files\SomeEditor")),
            ],
        );
        // Noise.
        add(
            &mut reg,
            v64,
            "vcredist",
            &[
                (
                    "DisplayName",
                    s("Microsoft Visual C++ 2022 Redistributable (x64)"),
                ),
                ("InstallLocation", s(r"D:\Games\vc")),
            ],
        );
        add(
            &mut reg,
            v64,
            "Hidden",
            &[
                ("DisplayName", s("Hidden Component")),
                ("InstallLocation", s(r"D:\Games\hidden")),
                ("SystemComponent", RegValue::Dword(1)),
            ],
        );
        add(
            &mut reg,
            v64,
            "Patch",
            &[
                ("DisplayName", s("Foo Game Patch")),
                ("InstallLocation", s(r"D:\Games\Foo")),
                ("ParentKeyName", s("FooGame_is1")),
            ],
        );
        add(
            &mut reg,
            v64,
            "EpicLauncher",
            &[
                ("DisplayName", s("Epic Games Launcher")),
                (
                    "InstallLocation",
                    s(r"C:\Program Files (x86)\Epic Games\Launcher"),
                ),
            ],
        );
        add(
            &mut reg,
            v64,
            "MsiThing",
            &[
                ("DisplayName", s("Msi Thing")),
                ("UninstallString", s("MsiExec.exe /X{ABC}")),
            ],
        );
        add(
            &mut reg,
            v64,
            "InWindows",
            &[
                ("DisplayName", s("Installer Cache Entry")),
                ("DisplayIcon", s(r"C:\Windows\Installer\{ABC}\icon.exe")),
            ],
        );
        add(
            &mut reg,
            v64,
            "NoName",
            &[("InstallLocation", s(r"D:\Games\Anon"))],
        );
        reg
    }

    #[test]
    fn default_scan_returns_only_game_like_entries() {
        let mut games = discover(&sample(), false).unwrap();
        games.sort_by(|a, b| a.name.cmp(&b.name));
        let names: Vec<_> = games.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["Bar Game", "Foo Game", "Half-Life 2"]);

        let hl2 = &games[2];
        assert_eq!(hl2.id, "steam-220");
        assert_eq!(hl2.source, GameSource::Steam);
        assert_eq!(hl2.store_id.as_deref(), Some("220"));

        let foo = &games[1];
        assert_eq!(foo.source, GameSource::Registry);
        assert_eq!(foo.install_dir, PathBuf::from(r"D:\Games\Foo"));
        assert_eq!(foo.publisher.as_deref(), Some("Foo Studio"));

        assert_eq!(games[0].install_dir, PathBuf::from(r"E:\Games\Bar"));
    }

    #[test]
    fn include_unclassified_adds_plain_software_but_never_noise() {
        let games = discover(&sample(), true).unwrap();
        let mut names: Vec<_> = games.iter().map(|g| g.name.as_str()).collect();
        names.sort();
        assert_eq!(
            names,
            ["Bar Game", "Foo Game", "Half-Life 2", "Some Editor"]
        );
    }

    #[test]
    fn ids_are_valid_and_unique() {
        let games = discover(&sample(), true).unwrap();
        let mut ids: Vec<_> = games.iter().map(|g| g.id.clone()).collect();
        for id in &ids {
            validate_game_id(id).unwrap();
        }
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), games.len());
    }

    #[test]
    fn name_filters() {
        for noise in [
            "KB5001234",
            "Steam",
            "Epic Games Launcher",
            "DirectX Runtime",
            "Vulkan Runtime 1.3",
        ] {
            assert!(is_noise_name(noise), "{noise}");
        }
        for game in [
            "Kingdom Come: Deliverance",
            "Euro Truck Driver",
            "Origin of Rome",
            "Steamworld Dig",
        ] {
            assert!(!is_noise_name(game), "{game}");
        }
    }

    #[test]
    fn steam_key_names() {
        assert_eq!(steam_app_id("Steam App 220"), Some("220"));
        assert_eq!(steam_app_id("steam app 7"), Some("7"));
        assert_eq!(steam_app_id("Steam App "), None);
        assert_eq!(steam_app_id("Steam App x1"), None);
        assert_eq!(steam_app_id("Steam"), None);
    }

    #[test]
    fn game_folder_markers_match_whole_components_only() {
        assert!(has_game_signal("k", r"D:\Games\Foo"));
        assert!(has_game_signal(
            "k",
            r"C:\Program Files\Epic Games\Fortnite"
        ));
        assert!(has_game_signal("k", "D:/SteamLibrary/steamapps/common/X"));
        assert!(!has_game_signal(
            "k",
            r"C:\Program Files\Microsoft Gamesmith"
        ));
        assert!(!has_game_signal("k", r"C:\Users\me\Saved Games\X"));
    }
}
