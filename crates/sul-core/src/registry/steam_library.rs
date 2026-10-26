//! Steam games across every library folder, on any drive.
//!
//! Steam records its install folder in the registry and lists every library folder
//! (typically one per drive) in `<steam>/steamapps/libraryfolders.vdf`. Each library
//! holds one `appmanifest_<appid>.acf` per installed game, which is how titles on
//! secondary drives are found.

use std::collections::BTreeSet;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::access::{Hive, KeyPath, RegistryAccess, View};
use super::paths::{clean_dir, store_id};
use super::vdf::{self, Value};
use super::{GameSource, InstalledGame, expand_env, path_key};
use crate::Result;
use crate::fsys::FileSystem;
use crate::steam::{SteamAppInfo, default_steam_dirs};

/// "Steamworks Common Redistributables" is a runtime package, not a game.
const REDISTRIBUTABLE_APP_ID: &str = "228980";
/// `StateFlags` bit meaning "fully installed".
const STATE_FULLY_INSTALLED: u32 = 4;

/// (hive, view, key, value) pairs that hold the Steam install folder.
const STEAM_ROOT_VALUES: [(Hive, View, &str, &str); 2] = [
    (
        Hive::CurrentUser,
        View::Registry64,
        r"SOFTWARE\Valve\Steam",
        "SteamPath",
    ),
    (
        Hive::LocalMachine,
        View::Registry32,
        r"SOFTWARE\Valve\Steam",
        "InstallPath",
    ),
];

fn steam_roots<R: RegistryAccess + ?Sized>(registry: &R) -> Result<Vec<PathBuf>> {
    let mut roots = Vec::new();
    for (hive, view, key_path, value) in STEAM_ROOT_VALUES {
        let Some(key) = registry.key(&KeyPath::new(hive, view, key_path))? else {
            continue;
        };
        if let Some(dir) = key
            .string(value)
            .map(expand_env)
            .and_then(|path| clean_dir(&path))
        {
            roots.push(PathBuf::from(dir));
        }
    }
    Ok(roots)
}

/// The Steam install folders to start from: the ones the registry names, or, when none of
/// those exists (a missing or stale registry entry), the standard install locations.
fn start_folders<R, F>(registry: &R, fs: &F) -> Result<Vec<PathBuf>>
where
    R: RegistryAccess + ?Sized,
    F: FileSystem + ?Sized,
{
    let from_registry = steam_roots(registry)?;
    if from_registry.iter().any(|root| fs.is_dir(root)) {
        return Ok(from_registry);
    }
    Ok(default_steam_dirs())
}

/// Library folder paths listed in a `libraryfolders.vdf` document.
///
/// Handles both layouts Steam has used: `"1" "D:\\Lib"` and `"1" { "path" "D:\\Lib" ... }`.
/// Entries that are not absolute Windows paths are ignored.
pub(super) fn library_paths(text: &str) -> Vec<String> {
    let Some(root) = vdf::parse(text) else {
        return Vec::new();
    };
    let Some(folders) = root.get("libraryfolders") else {
        return Vec::new();
    };

    folders
        .entries()
        .iter()
        .filter(|(key, _)| !key.is_empty() && key.bytes().all(|b| b.is_ascii_digit()))
        .filter_map(|(_, value)| match value {
            Value::Str(path) => Some(path.as_str()),
            Value::Obj(_) => value.get("path").and_then(Value::as_str),
        })
        .filter_map(clean_dir)
        .collect()
}

fn push_unique(found: &mut Vec<PathBuf>, seen: &mut BTreeSet<String>, path: PathBuf) {
    if seen.insert(path_key(&path)) {
        found.push(path);
    }
}

/// Every library folder reachable from the Steam install roots, each listed once.
fn libraries<F: FileSystem + ?Sized>(fs: &F, roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut seen = BTreeSet::new();

    for root in roots {
        if !fs.is_dir(root) {
            tracing::debug!(root = %root.display(), "Steam folder from the registry does not exist");
            continue;
        }
        push_unique(&mut found, &mut seen, root.clone());

        let list = root.join("steamapps").join("libraryfolders.vdf");
        match fs.read_to_string(&list) {
            Ok(text) => {
                for dir in library_paths(&text) {
                    push_unique(&mut found, &mut seen, PathBuf::from(dir));
                }
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => {
                tracing::warn!(path = %list.display(), error = %e, "cannot read libraryfolders.vdf");
            }
        }
    }
    found
}

fn is_manifest_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.len() > "appmanifest_.acf".len()
        && lower.starts_with("appmanifest_")
        && lower.ends_with(".acf")
}

/// `installdir` must be a single folder name; anything that could climb out of
/// `steamapps/common` is rejected.
fn is_plain_folder_name(name: &str) -> bool {
    !name.is_empty() && !name.contains("..") && !name.contains(['/', '\\', ':'])
}

/// Build a game from one `appmanifest_*.acf` document found in `steamapps`.
fn game_from_manifest<F: FileSystem + ?Sized>(
    fs: &F,
    text: &str,
    steamapps: &Path,
) -> Option<InstalledGame> {
    let root = vdf::parse(text)?;
    let state = root.get("AppState")?;

    let app_id = state.get("appid")?.as_str()?.trim();
    if app_id.is_empty() || app_id.len() > 10 || !app_id.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let name = state.get("name")?.as_str()?.trim();
    let folder = state.get("installdir")?.as_str()?.trim();

    if name.is_empty() || !is_plain_folder_name(folder) {
        return None;
    }
    if app_id == REDISTRIBUTABLE_APP_ID || name.starts_with("Steamworks Common Redistributables") {
        return None;
    }

    let flags: u32 = state
        .get("StateFlags")
        .and_then(Value::as_str)
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);

    let install_dir = steamapps.join("common").join(folder);
    // A manifest without a folder is either a download that never started or an
    // installed game whose drive is offline. Only the latter is worth listing.
    if flags & STATE_FULLY_INSTALLED == 0 && !fs.is_dir(&install_dir) {
        return None;
    }

    let info = SteamAppInfo {
        build_id: state
            .get("buildid")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
        auto_update_behavior: state
            .get("AutoUpdateBehavior")
            .and_then(Value::as_str)
            .and_then(|s| s.trim().parse().ok()),
    };

    Some(InstalledGame {
        id: store_id("steam", app_id),
        name: name.to_owned(),
        install_dir,
        source: GameSource::Steam,
        store_id: Some(app_id.to_owned()),
        publisher: None,
        steam: (info != SteamAppInfo::default()).then_some(info),
    })
}

/// Every Steam library folder that can be reached, each listed once (see
/// [`super::steam_library_roots`]).
pub(super) fn library_roots<R, F>(registry: &R, fs: &F) -> Result<Vec<PathBuf>>
where
    R: RegistryAccess + ?Sized,
    F: FileSystem + ?Sized,
{
    let roots = start_folders(registry, fs)?;
    Ok(libraries(fs, &roots))
}

/// Find Steam games in every library folder on every drive.
///
/// A missing Steam install is not an error (no Steam, no games); only registry
/// failures are reported. Nothing is ever written: manifests are only read.
pub(crate) fn discover<R, F>(registry: &R, fs: &F) -> Result<Vec<InstalledGame>>
where
    R: RegistryAccess + ?Sized,
    F: FileSystem + ?Sized,
{
    let mut games = Vec::new();
    for library in library_roots(registry, fs)? {
        games.extend(games_in_library(fs, &library));
    }
    Ok(games)
}

/// The games installed in one library folder. A manifest that cannot be read or parsed is
/// logged and skipped; it never fails the scan.
fn games_in_library<F: FileSystem + ?Sized>(fs: &F, library: &Path) -> Vec<InstalledGame> {
    let steamapps = library.join("steamapps");
    let entries = match fs.list_dir(&steamapps) {
        Ok(entries) => entries,
        Err(e) => {
            // A library on a disconnected drive is expected; anything else is worth a note.
            if e.kind() != ErrorKind::NotFound {
                tracing::warn!(path = %steamapps.display(), error = %e, "cannot list Steam library");
            }
            return Vec::new();
        }
    };

    let mut games = Vec::new();
    for entry in entries
        .into_iter()
        .filter(|e| !e.is_dir && is_manifest_name(&e.name))
    {
        let path = steamapps.join(&entry.name);
        match fs.read_to_string(&path) {
            Ok(text) => match game_from_manifest(fs, &text, &steamapps) {
                Some(game) => games.push(game),
                None => {
                    tracing::debug!(path = %path.display(), "skipping app manifest that is not an installed game");
                }
            },
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "cannot read app manifest");
            }
        }
    }
    games
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    use crate::fsys::fake::FakeFs;
    use crate::registry::access::{MemoryRegistry, RawKey, s};

    fn manifest(app_id: &str, name: &str, install_dir: &str, flags: u32) -> String {
        format!(
            "\"AppState\"\n{{\n\t\"appid\"\t\t\"{app_id}\"\n\t\"name\"\t\t\"{name}\"\n\
             \t\"installdir\"\t\t\"{install_dir}\"\n\t\"StateFlags\"\t\t\"{flags}\"\n}}\n"
        )
    }

    fn registry_with_steam_path(path: &str) -> MemoryRegistry {
        let mut reg = MemoryRegistry::default();
        reg.add(
            Hive::CurrentUser,
            View::Registry64,
            r"SOFTWARE\Valve\Steam",
            &[("SteamPath", s(path))],
        );
        reg
    }

    fn ids(games: &[InstalledGame]) -> Vec<&str> {
        games.iter().map(|g| g.id.as_str()).collect()
    }

    #[test]
    fn reads_both_libraryfolders_layouts() {
        let new_style = r#""libraryfolders"
        {
            "0" { "path" "C:\\Program Files (x86)\\Steam" "label" "" "apps" { "228980" "1" } }
            "1" { "path" "D:\\SteamLibrary" }
            "contentstatsid" "123"
        }"#;
        assert_eq!(
            library_paths(new_style),
            [r"C:\Program Files (x86)\Steam", r"D:\SteamLibrary"]
        );

        let old_style = r#""LibraryFolders" { "TimeNextStatsReport" "99" "1" "E:\\Games\\Steam" }"#;
        assert_eq!(library_paths(old_style), [r"E:\Games\Steam"]);

        assert!(library_paths("not vdf at all {").is_empty());
        assert!(library_paths(r#""libraryfolders" { "1" "relative\\path" }"#).is_empty());
    }

    #[test]
    fn finds_games_in_every_library_on_every_drive() {
        let main = PathBuf::from(r"C:\Steam");
        let second = PathBuf::from(r"D:\SteamLibrary");
        let (apps1, apps2) = (main.join("steamapps"), second.join("steamapps"));

        let fs = FakeFs::new()
            .file(
                apps1.join("libraryfolders.vdf"),
                r#""libraryfolders" { "0" { "path" "C:\\Steam" } "1" { "path" "D:\\SteamLibrary" } }"#,
            )
            .file(apps1.join("appmanifest_100.acf"), &manifest("100", "Game One", "Game One", 4))
            .file(apps2.join("appmanifest_200.acf"), &manifest("200", "Game Two", "GameTwo", 4))
            .file(
                apps1.join("appmanifest_228980.acf"),
                &manifest("228980", "Steamworks Common Redistributables", "Steamworks Shared", 4),
            )
            // Download that never produced a folder.
            .file(apps1.join("appmanifest_300.acf"), &manifest("300", "Queued", "Queued", 1026))
            // Marked installed but the folder is gone (drive offline): kept, the caller decides.
            .file(apps2.join("appmanifest_400.acf"), &manifest("400", "Offline", "Gone", 4))
            .file(apps1.join("notes.txt"), "ignored")
            .dir(apps1.join("common").join("Game One"))
            .dir(apps2.join("common").join("GameTwo"));

        // Forward slashes, as Steam writes them.
        let registry = registry_with_steam_path("C:/Steam");
        let mut games = discover(&registry, &fs).unwrap();
        games.sort_by(|a, b| a.id.cmp(&b.id));

        assert_eq!(ids(&games), ["steam-100", "steam-200", "steam-400"]);
        assert_eq!(games[1].name, "Game Two");
        assert_eq!(games[1].install_dir, apps2.join("common").join("GameTwo"));
        assert_eq!(games[1].store_id.as_deref(), Some("200"));
        assert!(games.iter().all(|g| g.source == GameSource::Steam));
    }

    #[test]
    fn falls_back_to_the_machine_wide_install_path() {
        let root = PathBuf::from(r"C:\Program Files (x86)\Steam");
        let apps = root.join("steamapps");
        let fs = FakeFs::new()
            .file(
                apps.join("appmanifest_7.acf"),
                &manifest("7", "Seven", "Seven", 4),
            )
            .dir(apps.join("common").join("Seven"));

        let mut reg = MemoryRegistry::default();
        reg.add(
            Hive::LocalMachine,
            View::Registry32,
            r"SOFTWARE\Valve\Steam",
            &[("InstallPath", s(r"C:\Program Files (x86)\Steam"))],
        );
        assert_eq!(ids(&discover(&reg, &fs).unwrap()), ["steam-7"]);
    }

    #[test]
    fn no_steam_means_no_games_and_no_error() {
        assert!(
            discover(&MemoryRegistry::default(), &FakeFs::new())
                .unwrap()
                .is_empty()
        );

        // Registry points at a folder that does not exist.
        let reg = registry_with_steam_path(r"Z:\Gone");
        assert!(discover(&reg, &FakeFs::new()).unwrap().is_empty());
    }

    #[test]
    fn registry_failures_are_reported() {
        struct Broken;
        impl RegistryAccess for Broken {
            fn subkeys(&self, _: &KeyPath) -> Result<Vec<RawKey>> {
                Ok(Vec::new())
            }
            fn key(&self, path: &KeyPath) -> Result<Option<RawKey>> {
                Err(Error::Registry {
                    key: path.path.clone(),
                    source: std::io::Error::other("boom"),
                })
            }
        }
        assert!(matches!(
            discover(&Broken, &FakeFs::new()),
            Err(Error::Registry { .. })
        ));
    }

    #[test]
    fn manifests_cannot_point_outside_the_library() {
        let apps = PathBuf::from(r"C:\Steam\steamapps");
        let fs = FakeFs::new();
        for dir in ["..", "../outside", "a/b", r"a\b", "C:evil", ""] {
            assert!(
                game_from_manifest(&fs, &manifest("1", "Evil", dir, 4), &apps).is_none(),
                "{dir:?} should be rejected"
            );
        }
        assert!(game_from_manifest(&fs, &manifest("12x", "BadId", "ok", 4), &apps).is_none());
        assert!(game_from_manifest(&fs, "garbage {", &apps).is_none());
    }

    #[test]
    fn manifests_missing_required_fields_or_with_the_wrong_root_are_skipped() {
        let apps = PathBuf::from(r"C:\Steam\steamapps");
        let fs = FakeFs::new();
        // No installdir, no appid, no name.
        for text in [
            "\"AppState\"\n{\n\"appid\" \"1\"\n\"name\" \"x\"\n}\n",
            "\"AppState\"\n{\n\"name\" \"x\"\n\"installdir\" \"x\"\n}\n",
            "\"AppState\"\n{\n\"appid\" \"1\"\n\"installdir\" \"x\"\n}\n",
            // Valid KeyValues, but not an app manifest.
            "\"Other\"\n{\n}\n",
        ] {
            assert!(game_from_manifest(&fs, text, &apps).is_none(), "{text:?}");
        }
    }

    #[test]
    fn records_the_build_id_and_update_policy_without_changing_them() {
        let text = "\"AppState\"\n{\n\"appid\" \"553850\"\n\"name\" \"HELLDIVERS 2\"\n\
                    \"StateFlags\" \"4\"\n\"installdir\" \"Helldivers 2\"\n\
                    \"buildid\" \"12345678\"\n\"AutoUpdateBehavior\" \"1\"\n}\n";
        let apps = PathBuf::from(r"C:\Steam\steamapps");
        let game = game_from_manifest(&FakeFs::new(), text, &apps).unwrap();
        assert_eq!(
            game.steam,
            Some(SteamAppInfo {
                build_id: Some("12345678".into()),
                auto_update_behavior: Some(1),
            })
        );

        // Absent or junk fields stay absent rather than becoming a guess.
        let plain =
            game_from_manifest(&FakeFs::new(), &manifest("1", "Plain", "Plain", 4), &apps).unwrap();
        assert_eq!(plain.steam, None);
        let junk = text
            .replace("\"1\"\n}", "\"soon\"\n}")
            .replace("12345678", "  ");
        let game = game_from_manifest(&FakeFs::new(), &junk, &apps).unwrap();
        assert_eq!(game.steam, None);
    }

    #[test]
    fn falls_back_to_the_standard_install_folder_when_the_registry_is_silent() {
        let root = PathBuf::from(r"C:\Program Files (x86)\Steam");
        let apps = root.join("steamapps");
        let fs = FakeFs::new()
            .file(
                apps.join("appmanifest_9.acf"),
                &manifest("9", "Nine", "Nine", 4),
            )
            .dir(apps.join("common").join("Nine"));

        // No registry entry at all.
        assert_eq!(
            ids(&discover(&MemoryRegistry::default(), &fs).unwrap()),
            ["steam-9"]
        );
        // A stale entry pointing at a folder that is gone.
        let stale = registry_with_steam_path(r"Z:\Gone");
        assert_eq!(ids(&discover(&stale, &fs).unwrap()), ["steam-9"]);
    }

    #[test]
    fn a_working_registry_entry_wins_over_the_default_folder() {
        let default_root = PathBuf::from(r"C:\Program Files (x86)\Steam");
        let custom = PathBuf::from(r"E:\Steam");
        let fs = FakeFs::new()
            .file(
                default_root.join("steamapps").join("appmanifest_1.acf"),
                &manifest("1", "Old Install", "Old", 4),
            )
            .file(
                custom.join("steamapps").join("appmanifest_2.acf"),
                &manifest("2", "Current Install", "Current", 4),
            )
            .dir(custom.join("steamapps").join("common").join("Current"));

        let registry = registry_with_steam_path("E:/Steam");
        assert_eq!(ids(&discover(&registry, &fs).unwrap()), ["steam-2"]);
    }

    #[test]
    fn library_roots_list_each_library_once() {
        let main = PathBuf::from(r"C:\Steam");
        let fs = FakeFs::new().file(
            main.join("steamapps").join("libraryfolders.vdf"),
            r#""libraryfolders" { "0" { "path" "C:\\Steam" } "1" { "path" "d:\\steamlibrary" } "2" { "path" "D:\\SteamLibrary" } }"#,
        );
        let roots = library_roots(&registry_with_steam_path("C:/Steam"), &fs).unwrap();
        assert_eq!(
            roots,
            [
                PathBuf::from(r"C:\Steam"),
                PathBuf::from(r"d:\steamlibrary")
            ]
        );
    }

    #[test]
    fn scanning_a_real_library_only_reads_it() {
        use std::fs;

        let dir = tempfile::tempdir().unwrap();
        let library = dir.path().to_path_buf();
        let apps = library.join("steamapps");
        fs::create_dir_all(apps.join("common").join("Helldivers 2")).unwrap();
        let good = manifest("553850", "HELLDIVERS 2", "Helldivers 2", 4);
        fs::write(apps.join("appmanifest_553850.acf"), &good).unwrap();
        fs::write(apps.join("appmanifest_2.acf"), "not vdf at all {{{").unwrap();

        let snapshot = |dir: &Path| {
            let mut names: Vec<_> = fs::read_dir(dir)
                .unwrap()
                .map(|e| e.unwrap().file_name())
                .collect();
            names.sort();
            names
        };
        let before = snapshot(&apps);

        let games = games_in_library(&crate::fsys::StdFs, &library);

        assert_eq!(
            ids(&games),
            ["steam-553850"],
            "the broken manifest is skipped"
        );
        assert_eq!(
            games[0].install_dir,
            apps.join("common").join("Helldivers 2")
        );
        assert_eq!(snapshot(&apps), before, "no file created or removed");
        assert_eq!(
            fs::read_to_string(apps.join("appmanifest_553850.acf")).unwrap(),
            good,
            "manifest content unchanged"
        );
    }

    #[test]
    fn manifest_file_names() {
        assert!(is_manifest_name("appmanifest_220.acf"));
        assert!(is_manifest_name("AppManifest_220.ACF"));
        assert!(!is_manifest_name("appmanifest_.acf"));
        assert!(!is_manifest_name("libraryfolders.vdf"));
        assert!(!is_manifest_name("appmanifest_220.acf.bak"));
    }
}
