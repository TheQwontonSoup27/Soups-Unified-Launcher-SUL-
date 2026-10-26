//! Minimal read-only filesystem view.
//!
//! Discovery code reads manifests and probes install folders through this trait so
//! it can be tested against in-memory fixtures instead of real drives.

use std::io;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntryInfo {
    pub name: String,
    pub is_dir: bool,
}

pub trait FileSystem {
    fn is_dir(&self, path: &Path) -> bool;
    fn read_to_string(&self, path: &Path) -> io::Result<String>;
    fn list_dir(&self, path: &Path) -> io::Result<Vec<DirEntryInfo>>;
}

/// The real filesystem.
#[derive(Debug, Default, Clone, Copy)]
pub struct StdFs;

impl FileSystem for StdFs {
    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    /// Reads lossily (launcher manifests are not always clean UTF-8) and drops a leading BOM.
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        let bytes = std::fs::read(path)?;
        let text = String::from_utf8_lossy(&bytes);
        Ok(text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned())
    }

    fn list_dir(&self, path: &Path) -> io::Result<Vec<DirEntryInfo>> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(path)? {
            let Ok(entry) = entry else { continue };
            out.push(DirEntryInfo {
                name: entry.file_name().to_string_lossy().into_owned(),
                is_dir: entry.file_type().map(|t| t.is_dir()).unwrap_or(false),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
pub(crate) mod fake {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::{Path, PathBuf};

    use super::*;

    /// In-memory filesystem. Paths are compared as built with `Path::join`, so tests
    /// can use Windows-style roots (`D:\SteamLibrary`) on any host OS.
    #[derive(Debug, Default)]
    pub struct FakeFs {
        files: BTreeMap<PathBuf, String>,
        dirs: BTreeSet<PathBuf>,
    }

    impl FakeFs {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn dir(mut self, path: impl AsRef<Path>) -> Self {
            self.add_dir(path.as_ref());
            self
        }

        pub fn file(mut self, path: impl AsRef<Path>, contents: &str) -> Self {
            let path = path.as_ref();
            if let Some(parent) = path.parent() {
                self.add_dir(parent);
            }
            self.files.insert(path.to_path_buf(), contents.to_owned());
            self
        }

        fn add_dir(&mut self, path: &Path) {
            let mut cur = Some(path);
            while let Some(p) = cur {
                self.dirs.insert(p.to_path_buf());
                cur = p.parent();
            }
        }
    }

    impl FileSystem for FakeFs {
        fn is_dir(&self, path: &Path) -> bool {
            self.dirs.contains(path)
        }

        fn read_to_string(&self, path: &Path) -> io::Result<String> {
            self.files
                .get(path)
                .cloned()
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
        }

        fn list_dir(&self, path: &Path) -> io::Result<Vec<DirEntryInfo>> {
            if !self.dirs.contains(path) {
                return Err(io::Error::from(io::ErrorKind::NotFound));
            }
            let name_of = |p: &PathBuf| p.file_name().map(|n| n.to_string_lossy().into_owned());
            let mut out = Vec::new();
            for f in self.files.keys().filter(|f| f.parent() == Some(path)) {
                out.extend(name_of(f).map(|name| DirEntryInfo {
                    name,
                    is_dir: false,
                }));
            }
            for d in self
                .dirs
                .iter()
                .filter(|d| d.parent() == Some(path) && d.as_path() != path)
            {
                out.extend(name_of(d).map(|name| DirEntryInfo { name, is_dir: true }));
            }
            Ok(out)
        }
    }
}
