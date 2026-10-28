//! Registry scanner (spec section 1): discovers installed games across all drives.
//!
//! "Across all drives" works through four independent sources, merged and
//! de-duplicated by install folder:
//!
//! | Source              | Finds                                            | Module          |
//! |---------------------|--------------------------------------------------|-----------------|
//! | Steam libraries     | every library in `libraryfolders.vdf`, any drive | `steam_library` |
//! | GOG Galaxy keys     | GOG installs, wherever the path points           | `gog`           |
//! | Uninstall keys      | registered installers, wherever the path points  | `uninstall`     |
//! | Drive probing       | games in "Games"-style folders on each fixed drive | `drives`      |
//!
//! Everything reads through two seams, [`RegistryAccess`] and [`crate::fsys::FileSystem`],
//! so it runs against in-memory fakes in tests. The only code that touches Windows
//! APIs is [`WindowsRegistry`] and [`FixedDrives`], both thin adapters.

mod access;
mod drives;
mod gog;
pub mod paths;
mod steam_library;
mod uninstall;
mod vdf;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use access::{Hive, KeyPath, RawKey, RegValue, RegistryAccess, View};
pub use drives::{DriveLister, StaticDrives};

#[cfg(windows)]
pub use access::WindowsRegistry;
#[cfg(windows)]
pub use drives::FixedDrives;

use crate::fsys::FileSystem;
use crate::steam::SteamAppInfo;
use crate::{Error, Result};

/// Every Steam library folder (any drive) that the registry and `libraryfolders.vdf` lead to,
/// each listed once. Feed the result to [`crate::steam::is_inside_steam_library`] to keep a
/// managed build outside Steam's update reach.
pub fn steam_library_roots<R, F>(registry: &R, fs: &F) -> Result<Vec<PathBuf>>
where
    R: RegistryAccess + ?Sized,
    F: FileSystem + ?Sized,
{
    steam_library::library_roots(registry, fs)
}

/// Where a game entry was discovered or how it is run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameSource {
    Steam,
    Gog,
    /// A generic installer entry in the Uninstall registry keys.
    Registry,
    /// Found only by looking inside a conventional library folder on a drive.
    DriveScan,
    Emulated,
    Manual,
}

impl GameSource {
    /// Lower wins when two sources report the same install folder.
    fn precedence(self) -> u8 {
        match self {
            Self::Steam => 0,
            Self::Gog => 1,
            Self::Registry => 2,
            Self::DriveScan => 3,
            Self::Emulated => 4,
            Self::Manual => 5,
        }
    }

    /// Scans never create or remove these; the user does.
    pub fn is_user_managed(self) -> bool {
        matches!(self, Self::Emulated | Self::Manual)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledGame {
    /// Stable id; satisfies [`crate::storage::validate_game_id`].
    pub id: String,
    pub name: String,
    pub install_dir: PathBuf,
    pub source: GameSource,
    /// The launcher's own identifier (Steam app id, GOG game id).
    #[serde(default)]
    pub store_id: Option<String>,
    #[serde(default)]
    pub publisher: Option<String>,
    /// Build id and update policy from the Steam manifest. Steam games only; recorded, never edited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steam: Option<SteamAppInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScanOptions {
    /// Also return installed programs with no sign of being a game, so a UI can
    /// offer them. Updates, redistributables and launchers stay excluded.
    pub include_unclassified: bool,
    /// Keep entries whose install folder no longer exists (stale registry entries,
    /// disconnected drives).
    pub keep_missing: bool,
}

pub trait GameScanner {
    fn scan(&self) -> Result<Vec<InstalledGame>>;
}

/// Case-, separator- and repeated-separator-insensitive key for comparing install folders.
pub(crate) fn path_key(path: &Path) -> String {
    let key = paths::normalize_key(&path.to_string_lossy());
    let (unc, rest) = match key.strip_prefix("\\\\") {
        Some(rest) => ("\\\\", rest),
        None => ("", key.as_str()),
    };

    let mut out = String::from(unc);
    let mut previous_was_separator = false;
    for c in rest.chars() {
        let is_separator = c == '\\';
        if !(is_separator && previous_was_separator) {
            out.push(c);
        }
        previous_was_separator = is_separator;
    }
    out
}

/// Expand `%NAME%` references in a registry string (REG_EXPAND_SZ is returned unexpanded).
pub(crate) fn expand_env(value: &str) -> String {
    paths::expand_env_vars(value, |name| std::env::var(name).ok())
}

/// One game per install folder and per id; the highest-precedence source wins.
fn dedupe(mut games: Vec<InstalledGame>) -> Vec<InstalledGame> {
    games.sort_by_key(|g| g.source.precedence()); // stable: keeps discovery order within a source

    let mut seen_dirs = BTreeSet::new();
    let mut seen_ids = BTreeSet::new();
    let mut kept: Vec<InstalledGame> = Vec::new();

    for game in games {
        let dir_key = path_key(&game.install_dir);
        if seen_dirs.contains(&dir_key) || seen_ids.contains(&game.id) {
            // Keep the winner but borrow the publisher from a duplicate if it lacks one.
            if let Some(winner) = kept
                .iter_mut()
                .find(|w| path_key(&w.install_dir) == dir_key)
            {
                if winner.publisher.is_none() {
                    winner.publisher = game.publisher;
                }
            }
            continue;
        }
        seen_dirs.insert(dir_key);
        seen_ids.insert(game.id.clone());
        kept.push(game);
    }
    kept
}

/// Collect a source's result; a failing source is logged and must not hide the others.
fn absorb(
    found: &mut Vec<InstalledGame>,
    first_error: &mut Option<Error>,
    source: &str,
    result: Result<Vec<InstalledGame>>,
) {
    match result {
        Ok(games) => found.extend(games),
        Err(error) => {
            tracing::warn!(source, %error, "game discovery source failed");
            first_error.get_or_insert(error);
        }
    }
}

/// Discovers installed games from the registry, Steam libraries and drives.
pub struct RegistryScanner<R, F, D> {
    registry: R,
    fs: F,
    drives: D,
    options: ScanOptions,
}

impl<R, F, D> RegistryScanner<R, F, D> {
    pub fn new(registry: R, fs: F, drives: D, options: ScanOptions) -> Self {
        Self {
            registry,
            fs,
            drives,
            options,
        }
    }
}

impl<R, F, D> GameScanner for RegistryScanner<R, F, D>
where
    R: RegistryAccess,
    F: FileSystem,
    D: DriveLister,
{
    fn scan(&self) -> Result<Vec<InstalledGame>> {
        let mut found = Vec::new();
        let mut first_error = None;

        absorb(
            &mut found,
            &mut first_error,
            "steam",
            steam_library::discover(&self.registry, &self.fs),
        );
        absorb(
            &mut found,
            &mut first_error,
            "gog",
            gog::discover(&self.registry),
        );
        absorb(
            &mut found,
            &mut first_error,
            "uninstall",
            uninstall::discover(&self.registry, self.options.include_unclassified),
        );
        let roots = self.drives.fixed_drive_roots();
        found.extend(drives::probe(&self.fs, &roots));

        // Every source failed and nothing was found: surface the error instead of
        // reporting a misleading empty library.
        if found.is_empty() {
            if let Some(error) = first_error {
                return Err(error);
            }
        }

        let mut games = dedupe(found);
        if !self.options.keep_missing {
            games.retain(|game| self.fs.is_dir(&game.install_dir));
        }
        games.sort_by_cached_key(|g| (g.name.to_lowercase(), g.id.clone()));
        Ok(games)
    }
}

/// A scanner wired to the live system. Windows only.
#[cfg(windows)]
pub fn system_scanner(options: ScanOptions) -> Result<Box<dyn GameScanner>> {
    Ok(Box::new(RegistryScanner::new(
        WindowsRegistry,
        crate::fsys::StdFs,
        FixedDrives,
        options,
    )))
}

/// A scanner wired to the live system. Windows only.
#[cfg(not(windows))]
pub fn system_scanner(_options: ScanOptions) -> Result<Box<dyn GameScanner>> {
    Err(Error::Unsupported("registry scanning requires Windows"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsys::fake::FakeFs;
    use crate::storage::validate_game_id;
    use access::{MemoryRegistry, s};

    const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";

    fn steam_root() -> PathBuf {
        PathBuf::from(r"c:\program files (x86)\steam")
    }

    fn acf(app_id: &str, name: &str) -> String {
        format!(
            "\"AppState\"\n{{\n\t\"appid\" \"{app_id}\"\n\t\"name\" \"{name}\"\n\t\"StateFlags\" \"4\"\n\t\"installdir\" \"{name}\"\n}}\n"
        )
    }

    /// A machine with Steam (two libraries), GOG, registered installers, an
    /// unregistered game on D:, and a stale registry entry.
    fn machine() -> (MemoryRegistry, FakeFs, StaticDrives) {
        let mut reg = MemoryRegistry::default();
        let lm = Hive::LocalMachine;
        let add = |reg: &mut MemoryRegistry, view, key: &str, values: &[(&str, RegValue)]| {
            reg.add(lm, view, &format!(r"{UNINSTALL}\{key}"), values);
        };

        reg.add(
            Hive::CurrentUser,
            View::Registry64,
            r"SOFTWARE\Valve\Steam",
            &[("SteamPath", s("c:/program files (x86)/steam"))],
        );
        // Same folder the Steam manifest reports: must collapse into one entry.
        add(
            &mut reg,
            View::Registry32,
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
        // Also found by drive probing: the registry entry wins.
        add(
            &mut reg,
            View::Registry64,
            "FooGame",
            &[
                ("DisplayName", s("Foo Game")),
                ("InstallLocation", s(r"D:\Games\Foo")),
                ("Publisher", s("Foo Studio")),
            ],
        );
        add(
            &mut reg,
            View::Registry64,
            "GoneGame",
            &[
                ("DisplayName", s("Gone Game")),
                ("InstallLocation", s(r"D:\Games\Gone")),
            ],
        );
        add(
            &mut reg,
            View::Registry64,
            "SomeEditor",
            &[
                ("DisplayName", s("Some Editor")),
                ("InstallLocation", s(r"C:\Program Files\SomeEditor")),
            ],
        );
        reg.add(
            lm,
            View::Registry32,
            r"SOFTWARE\GOG.com\Games\1207658924",
            &[
                ("gameID", s("1207658924")),
                ("gameName", s("The Witcher 3")),
                ("path", s(r"E:\GOG Games\The Witcher 3")),
            ],
        );

        let lib2 = PathBuf::from(r"D:\SteamLibrary");
        let drive_d = PathBuf::from(r"D:\");
        let drive_c = PathBuf::from(r"C:\");
        let apps1 = steam_root().join("steamapps");
        let apps2 = lib2.join("steamapps");
        let vdf = "\"libraryfolders\"\n{\n\t\"0\" { \"path\" \"c:\\\\program files (x86)\\\\steam\" }\n\t\"1\" { \"path\" \"D:\\\\SteamLibrary\" }\n}";

        let fs = FakeFs::new()
            .file(apps1.join("libraryfolders.vdf"), vdf)
            .file(
                apps1.join("appmanifest_220.acf"),
                &acf("220", "Half-Life 2"),
            )
            .file(
                apps2.join("appmanifest_359320.acf"),
                &acf("359320", "Elite Dangerous"),
            )
            .dir(apps1.join("common").join("Half-Life 2"))
            .dir(apps2.join("common").join("Elite Dangerous"))
            // Registry-supplied folders exist as plain strings...
            .dir(PathBuf::from(r"D:\Games\Foo"))
            .dir(PathBuf::from(r"E:\GOG Games\The Witcher 3"))
            .dir(PathBuf::from(r"C:\Program Files\SomeEditor"))
            // ...and the drive-probe view of D:\Games built with join.
            .dir(drive_d.join("Games").join("Foo"))
            .dir(drive_d.join("Games").join("Bar"))
            .dir(drive_c.join("Windows"));

        (reg, fs, StaticDrives(vec![drive_c, drive_d]))
    }

    fn scan_with(options: ScanOptions) -> Vec<InstalledGame> {
        let (reg, fs, drives) = machine();
        RegistryScanner::new(reg, fs, drives, options)
            .scan()
            .unwrap()
    }

    fn names(games: &[InstalledGame]) -> Vec<&str> {
        games.iter().map(|g| g.name.as_str()).collect()
    }

    #[test]
    fn default_scan_merges_all_sources() {
        let games = scan_with(ScanOptions::default());
        assert_eq!(
            names(&games),
            [
                "Bar",
                "Elite Dangerous",
                "Foo Game",
                "Half-Life 2",
                "The Witcher 3"
            ]
        );

        let source_of = |name: &str| games.iter().find(|g| g.name == name).unwrap().source;
        assert_eq!(source_of("Bar"), GameSource::DriveScan);
        assert_eq!(source_of("Elite Dangerous"), GameSource::Steam);
        assert_eq!(
            source_of("Foo Game"),
            GameSource::Registry,
            "registry beats drive probe"
        );
        assert_eq!(
            source_of("Half-Life 2"),
            GameSource::Steam,
            "steam beats uninstall key"
        );
        assert_eq!(source_of("The Witcher 3"), GameSource::Gog);
    }

    #[test]
    fn duplicates_keep_the_winner_and_borrow_the_publisher() {
        let games = scan_with(ScanOptions::default());
        let hl2 = games.iter().find(|g| g.name == "Half-Life 2").unwrap();
        assert_eq!(
            hl2.publisher.as_deref(),
            Some("Valve"),
            "steam manifest has none; uninstall key does"
        );
        assert_eq!(hl2.id, "steam-220");
        assert_eq!(games.iter().filter(|g| g.name == "Half-Life 2").count(), 1);
    }

    #[test]
    fn stale_entries_are_dropped_unless_kept() {
        assert!(!names(&scan_with(ScanOptions::default())).contains(&"Gone Game"));

        let kept = scan_with(ScanOptions {
            keep_missing: true,
            ..Default::default()
        });
        assert!(names(&kept).contains(&"Gone Game"));
    }

    #[test]
    fn unclassified_software_is_opt_in() {
        assert!(!names(&scan_with(ScanOptions::default())).contains(&"Some Editor"));

        let all = scan_with(ScanOptions {
            include_unclassified: true,
            ..Default::default()
        });
        assert!(names(&all).contains(&"Some Editor"));
    }

    #[test]
    fn ids_are_valid_unique_and_output_is_deterministic() {
        let options = ScanOptions {
            include_unclassified: true,
            keep_missing: true,
        };
        let games = scan_with(options);

        let mut ids: Vec<_> = games.iter().map(|g| g.id.as_str()).collect();
        for id in &ids {
            validate_game_id(id).unwrap();
        }
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), games.len());

        assert_eq!(games, scan_with(options));
    }

    struct BrokenRegistry;

    impl RegistryAccess for BrokenRegistry {
        fn subkeys(&self, path: &KeyPath) -> Result<Vec<RawKey>> {
            self.key(path).map(|_| Vec::new())
        }

        fn key(&self, path: &KeyPath) -> Result<Option<RawKey>> {
            Err(Error::Registry {
                key: path.path.clone(),
                source: std::io::Error::other("boom"),
            })
        }
    }

    #[test]
    fn a_failing_registry_does_not_hide_drive_results() {
        let (_, fs, drives) = machine();
        let games = RegistryScanner::new(BrokenRegistry, fs, drives, ScanOptions::default())
            .scan()
            .unwrap();
        assert_eq!(names(&games), ["Bar", "Foo"]);
    }

    #[test]
    fn total_failure_is_an_error_not_an_empty_library() {
        let scanner = RegistryScanner::new(
            BrokenRegistry,
            FakeFs::new(),
            StaticDrives::default(),
            ScanOptions::default(),
        );
        assert!(matches!(scanner.scan(), Err(Error::Registry { .. })));
    }

    #[test]
    fn path_keys_ignore_case_separators_and_repeats() {
        let a = path_key(Path::new(r"D:\/Games//Foo/"));
        let b = path_key(Path::new(r"d:\games\FOO"));
        assert_eq!(a, b);
        assert_eq!(path_key(Path::new(r"\\NAS\Share\X")), r"\\nas\share\x");
    }

    #[cfg(not(windows))]
    #[test]
    fn system_scanner_is_windows_only() {
        assert!(matches!(
            system_scanner(ScanOptions::default()),
            Err(Error::Unsupported(_))
        ));
    }
}

#[cfg(test)]
mod helper_tests {
    use super::*;
    use crate::fsys::fake::FakeFs;
    use access::{MemoryRegistry, s};

    #[test]
    fn only_the_sources_the_user_adds_by_hand_are_left_alone_by_scans() {
        assert!(GameSource::Emulated.is_user_managed());
        assert!(GameSource::Manual.is_user_managed());
        for scanned in [
            GameSource::Steam,
            GameSource::Gog,
            GameSource::Registry,
            GameSource::DriveScan,
        ] {
            assert!(!scanned.is_user_managed(), "{scanned:?}");
        }
    }

    #[test]
    fn steam_library_roots_lists_every_library_once_and_nothing_for_a_machine_without_steam() {
        let mut registry = MemoryRegistry::default();
        registry.add(
            Hive::CurrentUser,
            View::Registry64,
            r"SOFTWARE\Valve\Steam",
            &[("SteamPath", s("C:/Steam"))],
        );
        let main = PathBuf::from(r"C:\Steam");
        let fs = FakeFs::new().file(
            main.join("steamapps").join("libraryfolders.vdf"),
            r#""libraryfolders" { "0" { "path" "C:\\Steam" } "1" { "path" "D:\\SteamLibrary" } }"#,
        );
        assert_eq!(
            steam_library_roots(&registry, &fs).unwrap(),
            [main, PathBuf::from(r"D:\SteamLibrary")]
        );

        let none = steam_library_roots(&MemoryRegistry::default(), &FakeFs::new()).unwrap();
        assert!(none.is_empty(), "{none:?}");
    }

    #[test]
    fn registry_strings_are_expanded_from_the_environment_and_unknown_names_are_left_alone() {
        if let Ok(path) = std::env::var("PATH") {
            assert_eq!(expand_env("%PATH%"), path);
            assert_eq!(expand_env("x%PATH%y"), format!("x{path}y"));
        }
        assert_eq!(
            expand_env("%SUL_SURELY_NOT_A_VARIABLE%\\Games"),
            "%SUL_SURELY_NOT_A_VARIABLE%\\Games"
        );
        assert_eq!(expand_env("no variables here"), "no variables here");
        assert_eq!(expand_env("100%"), "100%");
    }
}
