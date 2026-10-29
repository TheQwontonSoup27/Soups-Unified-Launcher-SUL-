//! The Windows-only adapters, run against the real machine. They only compile and run on Windows
//! (the CI job uses `windows-latest`); elsewhere this file is empty.
//!
//! Everything here is read-only: it reads registry keys that exist on every Windows install, lists
//! the drives, and scans. Nothing asserts what is installed on the machine, because that differs
//! from one machine to the next; the checks are on shape, safety and consistency.
#![cfg(windows)]

use std::collections::BTreeSet;
use std::path::PathBuf;

use sul_core::discord::IpcTransport;
use sul_core::registry::{
    DriveLister, FixedDrives, Hive, KeyPath, RegistryAccess, ScanOptions, View, WindowsRegistry,
    paths, system_scanner,
};
use sul_core::service::LibraryService;
use sul_core::steam::{ClientProbe, RegistryProbe, SteamManager};
use sul_core::storage::validate_game_id;

fn lm(view: View, path: &str) -> KeyPath {
    KeyPath::new(Hive::LocalMachine, view, path)
}

const CURRENT_VERSION: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";

#[test]
fn the_registry_adapter_reads_strings_and_numbers_from_a_key_every_windows_has() {
    let key = WindowsRegistry
        .key(&lm(View::Registry64, CURRENT_VERSION))
        .unwrap()
        .expect("the CurrentVersion key exists on every Windows install");
    assert_eq!(key.name, "CurrentVersion");
    assert!(
        key.string("ProductName")
            .is_some_and(|name| !name.is_empty()),
        "ProductName is a string value"
    );
    assert!(
        key.string("CurrentBuildNumber")
            .is_some_and(|build| build.parse::<u32>().is_ok()),
        "the build number is text holding a number"
    );
    assert!(
        key.dword("CurrentMajorVersionNumber")
            .is_some_and(|major| major >= 10),
        "a DWORD value reads as a number, and this is Windows 10 or later"
    );
    // The right type for each: a string is not a number and the reverse.
    assert_eq!(key.dword("ProductName"), None);
    assert_eq!(key.string("CurrentMajorVersionNumber"), None);
    // Value names are not case sensitive.
    assert_eq!(key.string("productname"), key.string("ProductName"));
}

#[test]
fn expandable_strings_are_read_as_text_and_left_unexpanded() {
    let key = WindowsRegistry
        .key(&lm(
            View::Registry64,
            r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
        ))
        .unwrap()
        .expect("the machine environment key exists");
    let path = key.string("Path").expect("Path is a REG_EXPAND_SZ value");
    assert!(path.to_ascii_lowercase().contains("system32"), "{path}");
}

#[test]
fn a_missing_key_is_nothing_not_an_error() {
    let missing = lm(View::Registry64, r"SOFTWARE\SulTestSurelyMissing\Nope");
    assert!(WindowsRegistry.key(&missing).unwrap().is_none());
    assert!(WindowsRegistry.subkeys(&missing).unwrap().is_empty());
}

#[test]
fn subkeys_lists_the_direct_children_with_their_names() {
    let children = WindowsRegistry
        .subkeys(&lm(View::Registry64, r"SOFTWARE\Microsoft"))
        .unwrap();
    let names: BTreeSet<String> = children
        .iter()
        .map(|k| k.name.to_ascii_lowercase())
        .collect();
    assert!(names.contains("windows"), "{} children", names.len());
    assert!(names.contains("windows nt"));
    assert!(
        names.iter().all(|n| !n.contains('\\')),
        "direct children only, no nested paths"
    );
}

#[test]
fn both_registry_views_and_the_user_hive_can_be_read() {
    let uninstall = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
    for view in [View::Registry64, View::Registry32] {
        WindowsRegistry.subkeys(&lm(view, uninstall)).unwrap();
    }
    WindowsRegistry
        .key(&KeyPath::new(
            Hive::CurrentUser,
            View::Registry64,
            "Environment",
        ))
        .unwrap();
}

#[test]
fn the_drive_list_has_the_system_drive_and_only_well_formed_roots() {
    let roots = FixedDrives.fixed_drive_roots();
    assert!(!roots.is_empty());
    let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    assert!(
        roots.iter().any(|r| r
            .to_string_lossy()
            .eq_ignore_ascii_case(&format!("{system}\\"))),
        "{system} not in {roots:?}"
    );
    for root in &roots {
        let text = root.to_string_lossy();
        let bytes = text.as_bytes();
        assert!(
            bytes.len() == 3 && bytes[0].is_ascii_uppercase() && &bytes[1..] == b":\\",
            "not a drive root: {text}"
        );
        assert!(root.is_dir(), "{text}");
    }
    let unique: BTreeSet<&PathBuf> = roots.iter().collect();
    assert_eq!(unique.len(), roots.len(), "each drive once");
    assert!(
        roots.windows(2).all(|w| w[0] < w[1]),
        "in drive-letter order"
    );
}

#[test]
fn the_steam_running_check_answers_without_touching_anything_and_agrees_with_itself() {
    // Whether Steam runs on the machine is not known here; the check must simply answer, and the same way twice.
    let probe = RegistryProbe(WindowsRegistry);
    let first = probe.client_running();
    assert_eq!(first, probe.client_running());
    assert_eq!(first, SteamManager::system().client_running());
}

fn scan(options: ScanOptions) -> Vec<sul_core::registry::InstalledGame> {
    system_scanner(options).unwrap().scan().unwrap()
}

#[test]
fn a_scan_of_the_real_machine_returns_clean_unique_entries_that_exist() {
    let games = scan(ScanOptions::default());
    let mut ids = BTreeSet::new();
    let mut dirs = BTreeSet::new();
    for game in &games {
        assert!(validate_game_id(&game.id).is_ok(), "bad id {:?}", game.id);
        assert!(!game.name.trim().is_empty(), "{}", game.id);
        assert!(
            game.install_dir.is_dir(),
            "{} -> {:?} is not a folder",
            game.id,
            game.install_dir
        );
        assert!(ids.insert(game.id.clone()), "id {} appears twice", game.id);
        assert!(
            dirs.insert(folder_key(&game.install_dir)),
            "{:?} appears twice",
            game.install_dir
        );
    }
}

/// The comparison the scanner itself uses to decide that two entries are one folder: case, `/`
/// against `\` and repeated separators do not matter.
fn folder_key(path: &std::path::Path) -> String {
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

#[test]
fn scanning_twice_gives_the_same_list_in_the_same_order() {
    let ids = |games: Vec<sul_core::registry::InstalledGame>| {
        games.into_iter().map(|g| g.id).collect::<Vec<_>>()
    };
    assert_eq!(
        ids(scan(ScanOptions::default())),
        ids(scan(ScanOptions::default()))
    );
}

#[test]
fn keeping_missing_folders_only_ever_adds_entries() {
    let kept: BTreeSet<String> = scan(ScanOptions::default())
        .into_iter()
        .map(|g| g.id)
        .collect();
    let with_missing: BTreeSet<String> = scan(ScanOptions {
        keep_missing: true,
        ..ScanOptions::default()
    })
    .into_iter()
    .map(|g| g.id)
    .collect();
    assert!(
        kept.is_subset(&with_missing),
        "{:?}",
        kept.difference(&with_missing).collect::<Vec<_>>()
    );
}

#[test]
fn a_real_scan_is_saved_and_survives_reopening_the_library() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("library.json");
    let first = LibraryService::open(&file).unwrap();
    let scanned = first.scan_system().unwrap();
    let listed: Vec<String> = first.games().into_iter().map(|g| g.id).collect();
    assert_eq!(
        listed,
        scanned
            .games
            .iter()
            .map(|g| g.id.clone())
            .collect::<Vec<_>>()
    );

    let reopened = LibraryService::open(&file).unwrap();
    let after: Vec<String> = reopened.games().into_iter().map(|g| g.id).collect();
    assert_eq!(
        after, listed,
        "what was found is what comes back after a restart"
    );
    assert!(reopened.games().iter().all(|g| g.present));
}

#[test]
fn looking_for_discord_without_it_running_is_an_error_not_a_panic() {
    // Whether Discord runs on the machine is not known here. If it does, the pipe opens (and is closed
    // again without a word); if it does not, the answer is an error that says so.
    match IpcTransport::connect() {
        Ok(_) => {}
        Err(error) => assert!(error.to_string().contains("Discord"), "{error}"),
    }
}
