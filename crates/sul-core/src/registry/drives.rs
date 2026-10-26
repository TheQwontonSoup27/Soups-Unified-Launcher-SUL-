//! Drive enumeration and library-folder probing.
//!
//! Games that never registered anywhere (portable copies, manual installs) usually
//! sit in a "Games"-style folder on some drive. For every fixed drive we look inside
//! a short list of conventional library folders and report each subfolder.

use std::path::PathBuf;

use super::paths::name_id;
use super::{GameSource, InstalledGame, path_key};
use crate::fsys::FileSystem;

/// Lists the drive roots to probe (for example `C:\`, `D:\`).
pub trait DriveLister {
    fn fixed_drive_roots(&self) -> Vec<PathBuf>;
}

/// A fixed list of roots: tests, or user-added scan folders.
#[derive(Debug, Default, Clone)]
pub struct StaticDrives(pub Vec<PathBuf>);

impl DriveLister for StaticDrives {
    fn fixed_drive_roots(&self) -> Vec<PathBuf> {
        self.0.clone()
    }
}

#[cfg(windows)]
pub use windows_impl::FixedDrives;

#[cfg(windows)]
mod windows_impl {
    use std::path::PathBuf;

    use super::DriveLister;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLogicalDrives() -> u32;
        fn GetDriveTypeW(root_path_name: *const u16) -> u32;
    }

    /// `GetDriveTypeW` result for a local fixed disk. Network, removable and optical
    /// drives are skipped on purpose: probing a disconnected network drive can block
    /// for a long time.
    const DRIVE_FIXED: u32 = 3;

    /// Every local fixed disk, via `GetLogicalDrives` / `GetDriveTypeW`.
    #[derive(Debug, Default, Clone, Copy)]
    pub struct FixedDrives;

    impl DriveLister for FixedDrives {
        fn fixed_drive_roots(&self) -> Vec<PathBuf> {
            // SAFETY: `GetLogicalDrives` takes no arguments and has no preconditions.
            let mask = unsafe { GetLogicalDrives() };

            (0..26u8)
                .filter(|bit| mask & (1u32 << bit) != 0)
                .filter_map(|bit| {
                    let root = format!("{}:\\", char::from(b'A' + bit));
                    let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
                    // SAFETY: `wide` is a valid NUL-terminated UTF-16 string that outlives the call.
                    let kind = unsafe { GetDriveTypeW(wide.as_ptr()) };
                    (kind == DRIVE_FIXED).then(|| PathBuf::from(root))
                })
                .collect()
        }
    }
}

/// Conventional game library folders, relative to a drive root.
const LIBRARY_DIRS: &[&[&str]] = &[
    &["Games"],
    &["GOG Games"],
    &["Epic Games"],
    &["XboxGames"],
    &["Program Files", "Epic Games"],
    &["Program Files", "EA Games"],
    &["Program Files (x86)", "EA Games"],
    &["Program Files (x86)", "Origin Games"],
    &["Program Files (x86)", "GOG Galaxy", "Games"],
    &["Program Files", "Ubisoft", "Ubisoft Game Launcher", "games"],
    &[
        "Program Files (x86)",
        "Ubisoft",
        "Ubisoft Game Launcher",
        "games",
    ],
];

/// Subfolders of those libraries that hold tooling, not games (lowercase).
const IGNORED_FOLDERS: [&str; 7] = [
    "launcher",
    "epic online services",
    "directxredist",
    "redist",
    "_commonredist",
    "common files",
    "installer",
];

pub(crate) fn probe<F: FileSystem + ?Sized>(fs: &F, roots: &[PathBuf]) -> Vec<InstalledGame> {
    let mut games = Vec::new();
    for root in roots {
        for parts in LIBRARY_DIRS {
            let mut library = root.clone();
            library.extend(parts.iter());

            let Ok(entries) = fs.list_dir(&library) else {
                continue;
            };
            for entry in entries.into_iter().filter(|e| e.is_dir) {
                let lower = entry.name.to_ascii_lowercase();
                if entry.name.starts_with(['.', '$']) || IGNORED_FOLDERS.contains(&lower.as_str()) {
                    continue;
                }
                let install_dir = library.join(&entry.name);
                games.push(InstalledGame {
                    id: name_id("lib", &entry.name, &path_key(&install_dir)),
                    name: entry.name,
                    install_dir,
                    source: GameSource::DriveScan,
                    store_id: None,
                    publisher: None,
                    steam: None,
                });
            }
        }
    }
    games
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsys::fake::FakeFs;
    use crate::storage::validate_game_id;

    fn root(letter: char) -> PathBuf {
        PathBuf::from(format!("{letter}:\\"))
    }

    #[test]
    fn finds_game_folders_on_every_drive_and_skips_tooling() {
        let (c, d) = (root('C'), root('D'));
        let fs = FakeFs::new()
            .dir(d.join("Games").join("Foo"))
            .dir(d.join("Games").join("Bar Baz"))
            .file(d.join("Games").join("readme.txt"), "not a folder")
            .dir(d.join("Games").join("$RECYCLE.BIN"))
            .dir(c.join("Program Files").join("Epic Games").join("Fortnite"))
            .dir(c.join("Program Files").join("Epic Games").join("Launcher"))
            .dir(c.join("Windows"))
            .dir(c.join("Users"));

        let mut games = probe(&fs, &[c.clone(), d.clone()]);
        games.sort_by(|a, b| a.name.cmp(&b.name));

        let names: Vec<_> = games.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["Bar Baz", "Foo", "Fortnite"]);
        assert_eq!(games[0].install_dir, d.join("Games").join("Bar Baz"));
        assert!(games.iter().all(|g| g.source == GameSource::DriveScan));
        for g in &games {
            validate_game_id(&g.id).unwrap();
        }
    }

    #[test]
    fn missing_libraries_and_roots_are_fine() {
        assert!(probe(&FakeFs::new(), &[root('Z')]).is_empty());
        assert!(probe(&FakeFs::new(), &[]).is_empty());
    }

    #[test]
    fn static_drives_returns_its_roots() {
        let drives = StaticDrives(vec![root('D')]);
        assert_eq!(drives.fixed_drive_roots(), [root('D')]);
    }
}
