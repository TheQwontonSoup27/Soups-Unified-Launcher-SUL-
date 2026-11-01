//! Library roots (spec 1.5): the folders SUL keeps managed builds and mods in.
//!
//! * Several roots on several drives, each with an id, a label and a kind.
//! * Every root holds a small marker file naming its id, so a root that was moved, unplugged or
//!   replaced by a different drive at the same letter is noticed, and relinking only accepts a folder
//!   that carries the right marker.
//! * Ownership is explicit: roots may not overlap each other, may not sit inside a protected location
//!   (an original game install) and may not overlap a reserved location (the save-data sandbox),
//!   so a file belongs to at most one root and saves stay apart from managed binaries.
//! * Forgetting a root only edits SUL's own list. Nothing in a root is ever deleted here.
//! * Free space is read through a seam ([`DiskSpace`]) so the checks can be tested without a real disk.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::io;
use crate::fsops::{self, ProtectedRoots};
use crate::storage::validate_game_id;
use crate::{Error, Result};

pub const ROOTS_SCHEMA: u32 = 1;
/// Name of the marker file written at the top of every root.
pub const MARKER_FILE: &str = ".sul-root.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RootKind {
    /// Managed game builds.
    Builds,
    /// The mod library.
    Mods,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Root {
    pub id: String,
    pub label: String,
    pub path: PathBuf,
    pub kind: RootKind,
}

#[derive(Debug, Serialize, Deserialize)]
struct RootsFile {
    schema: u32,
    roots: Vec<Root>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Marker {
    schema: u32,
    id: String,
}

/// Free space on the disk that holds a path.
pub trait DiskSpace {
    fn free_bytes(&self, path: &Path) -> Result<u64>;
}

/// Free space as Windows reports it for the calling user (quota aware).
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemDiskSpace;

#[cfg(windows)]
impl DiskSpace for SystemDiskSpace {
    fn free_bytes(&self, path: &Path) -> Result<u64> {
        use std::os::windows::ffi::OsStrExt;

        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetDiskFreeSpaceExW(
                directory_name: *const u16,
                free_bytes_available_to_caller: *mut u64,
                total_number_of_bytes: *mut u64,
                total_number_of_free_bytes: *mut u64,
            ) -> i32;
        }

        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut free_to_caller = 0u64;
        let mut total = 0u64;
        let mut total_free = 0u64;
        // SAFETY: `wide` is a valid NUL-terminated UTF-16 string that outlives the call, and the three
        // out-pointers are valid, aligned, writable `u64` locals.
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free_to_caller,
                &mut total,
                &mut total_free,
            )
        };
        if ok == 0 {
            return Err(io(path)(std::io::Error::last_os_error()));
        }
        Ok(free_to_caller)
    }
}

/// Windows only. Off Windows this exists so the workspace compiles, and it only refuses.
#[cfg(not(windows))]
impl DiskSpace for SystemDiskSpace {
    fn free_bytes(&self, _path: &Path) -> Result<u64> {
        Err(Error::Unsupported("the free-space check needs Windows"))
    }
}

/// What SUL can say about one root right now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RootStatus {
    pub id: String,
    pub path: PathBuf,
    /// The folder exists.
    pub present: bool,
    /// The folder holds this root's marker. `false` with `present` set means "some other folder or drive is here".
    pub marker_ok: bool,
    pub free_bytes: Option<u64>,
}

#[derive(Debug)]
pub struct LibraryRoots {
    file: PathBuf,
    roots: Vec<Root>,
    reserved: Vec<PathBuf>,
}

fn canon(p: &Path) -> Result<PathBuf> {
    p.canonicalize().map_err(io(p))
}

fn overlaps(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}

fn read_marker(dir: &Path) -> Option<Marker> {
    let text = std::fs::read_to_string(dir.join(MARKER_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

impl LibraryRoots {
    /// Load the list from `file`. A missing file is an empty list; an unreadable, corrupt or newer-schema
    /// file is an error, never a silent reset (that would forget where the builds are).
    pub fn load(file: &Path) -> Result<Self> {
        let roots = match std::fs::read_to_string(file) {
            Ok(text) => {
                let parsed: RootsFile =
                    serde_json::from_str(&text).map_err(|e| Error::json(file, e))?;
                if parsed.schema > ROOTS_SCHEMA {
                    return Err(Error::Invalid(format!(
                        "library roots file schema {} is newer than supported {ROOTS_SCHEMA}",
                        parsed.schema
                    )));
                }
                parsed.roots
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(io(file)(e)),
        };
        Ok(Self {
            file: file.to_path_buf(),
            roots,
            reserved: Vec::new(),
        })
    }

    /// Keep roots away from `path` (the save-data sandbox, for example). Not saved; set it at start-up.
    pub fn reserve(&mut self, path: &Path) {
        self.reserved.push(path.to_path_buf());
    }

    pub fn roots(&self) -> &[Root] {
        &self.roots
    }

    pub fn get(&self, id: &str) -> Option<&Root> {
        self.roots.iter().find(|r| r.id == id)
    }

    fn save(&self, protected: &ProtectedRoots) -> Result<()> {
        let body = serde_json::to_string_pretty(&RootsFile {
            schema: ROOTS_SCHEMA,
            roots: self.roots.clone(),
        })?;
        fsops::atomic_write(protected, &self.file, body.as_bytes())
    }

    /// Checks shared by `add` and `relink`: a real absolute folder, outside protected and reserved
    /// locations, not overlapping another root. Returns the canonical path.
    fn check_location(
        &self,
        protected: &ProtectedRoots,
        path: &Path,
        ignoring: Option<&str>,
    ) -> Result<PathBuf> {
        if !path.is_absolute() {
            return Err(Error::Invalid(format!(
                "a library root must be an absolute path: {}",
                path.display()
            )));
        }
        if !path.is_dir() {
            return Err(Error::Invalid(format!(
                "{} is not an existing folder",
                path.display()
            )));
        }
        protected.check_writable(&path.join(MARKER_FILE))?;
        let here = canon(path)?;
        for r in self
            .roots
            .iter()
            .filter(|r| Some(r.id.as_str()) != ignoring)
        {
            // A root that is currently unplugged cannot be compared; it simply is not in the way.
            if let Ok(other) = r.path.canonicalize()
                && overlaps(&here, &other)
            {
                return Err(Error::Invalid(format!(
                    "{} overlaps the library root {:?} ({}); a file may belong to only one root",
                    path.display(),
                    r.id,
                    r.path.display()
                )));
            }
        }
        for keep_apart in &self.reserved {
            if let Ok(other) = keep_apart.canonicalize()
                && overlaps(&here, &other)
            {
                return Err(Error::Invalid(format!(
                    "{} overlaps {}, which is kept apart from managed binaries (save data and settings live there)",
                    path.display(),
                    keep_apart.display()
                )));
            }
        }
        Ok(here)
    }

    /// Register `path` as a root. Writes the marker file with create-new, so an existing file is never
    /// overwritten: a marker already naming this id is accepted (the list was lost), one naming another id
    /// is refused.
    pub fn add(
        &mut self,
        protected: &ProtectedRoots,
        id: &str,
        label: &str,
        path: &Path,
        kind: RootKind,
    ) -> Result<()> {
        validate_game_id(id)?;
        if label.trim().is_empty() || label.len() > 120 || label.chars().any(char::is_control) {
            return Err(Error::Invalid(
                "a root label must be 1-120 plain characters".into(),
            ));
        }
        if self.get(id).is_some() {
            return Err(Error::Invalid(format!(
                "a library root with id {id:?} already exists"
            )));
        }
        self.check_location(protected, path, None)?;

        match read_marker(path) {
            Some(m) if m.id == id => {}
            Some(m) => {
                return Err(Error::Invalid(format!(
                    "{} is already the library root {:?}; refusing to register it as {id:?}",
                    path.display(),
                    m.id
                )));
            }
            None => {
                let marker = path.join(MARKER_FILE);
                let body = serde_json::to_string_pretty(&Marker {
                    schema: ROOTS_SCHEMA,
                    id: id.to_owned(),
                })?;
                let mut f = std::fs::File::create_new(&marker).map_err(io(&marker))?;
                f.write_all(body.as_bytes()).map_err(io(&marker))?;
                f.sync_all().map_err(io(&marker))?;
            }
        }
        self.roots.push(Root {
            id: id.to_owned(),
            label: label.to_owned(),
            path: path.to_path_buf(),
            kind,
        });
        self.save(protected)
    }

    /// Point root `id` at a new folder after the old one moved. Only a folder that carries this root's
    /// marker is accepted, so relinking cannot point a root at the wrong drive.
    pub fn relink(&mut self, protected: &ProtectedRoots, id: &str, new_path: &Path) -> Result<()> {
        let idx = self
            .roots
            .iter()
            .position(|r| r.id == id)
            .ok_or_else(|| Error::Invalid(format!("no library root with id {id:?}")))?;
        self.check_location(protected, new_path, Some(id))?;
        match read_marker(new_path) {
            Some(m) if m.id == id => {}
            Some(m) => {
                return Err(Error::Invalid(format!(
                    "{} is the library root {:?}, not {id:?}",
                    new_path.display(),
                    m.id
                )));
            }
            None => {
                return Err(Error::Invalid(format!(
                    "{} has no {MARKER_FILE} for {id:?}; copy the whole root folder, marker included, before relinking",
                    new_path.display()
                )));
            }
        }
        self.roots[idx].path = new_path.to_path_buf();
        self.save(protected)
    }

    /// Remove root `id` from SUL's list. The folder and everything in it stay untouched. Refused while
    /// `in_use_by` is non-empty (ids of builds or mods still stored there).
    pub fn forget(
        &mut self,
        protected: &ProtectedRoots,
        id: &str,
        in_use_by: &[String],
    ) -> Result<()> {
        if !in_use_by.is_empty() {
            return Err(Error::Invalid(format!(
                "library root {id:?} still holds {} item(s); move or retire them first",
                in_use_by.len()
            )));
        }
        let before = self.roots.len();
        self.roots.retain(|r| r.id != id);
        if self.roots.len() == before {
            return Err(Error::Invalid(format!("no library root with id {id:?}")));
        }
        self.save(protected)
    }

    pub fn status(&self, disk: &dyn DiskSpace) -> Vec<RootStatus> {
        self.roots
            .iter()
            .map(|r| {
                let present = r.path.is_dir();
                let marker_ok = present && read_marker(&r.path).is_some_and(|m| m.id == r.id);
                RootStatus {
                    id: r.id.clone(),
                    path: r.path.clone(),
                    present,
                    marker_ok,
                    free_bytes: if present {
                        disk.free_bytes(&r.path).ok()
                    } else {
                        None
                    },
                }
            })
            .collect()
    }

    /// The folder of root `id`, only if it is present and carries its marker.
    pub fn usable_path(&self, id: &str) -> Result<&Path> {
        let r = self
            .get(id)
            .ok_or_else(|| Error::Invalid(format!("no library root with id {id:?}")))?;
        if !r.path.is_dir() {
            return Err(Error::Invalid(format!(
                "library root {id:?} is not available: {} is missing (drive unplugged or moved? relink it)",
                r.path.display()
            )));
        }
        if !read_marker(&r.path).is_some_and(|m| m.id == id) {
            return Err(Error::Invalid(format!(
                "library root {id:?} is not the folder it was: {} has no matching marker (different drive? relink it)",
                r.path.display()
            )));
        }
        Ok(&r.path)
    }

    /// Fail unless root `id` has `needed` bytes plus `margin` free.
    pub fn ensure_space(
        &self,
        disk: &dyn DiskSpace,
        id: &str,
        needed: u64,
        margin: u64,
    ) -> Result<()> {
        let path = self.usable_path(id)?;
        let free = disk.free_bytes(path)?;
        let want = needed.saturating_add(margin);
        if free < want {
            return Err(Error::InsufficientSpace {
                path: path.to_path_buf(),
                needed: want,
                free,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::cell::Cell;

    /// A disk with a fixed amount of free space.
    pub struct FakeDisk(pub Cell<u64>);

    impl FakeDisk {
        pub fn with(free: u64) -> Self {
            Self(Cell::new(free))
        }
    }

    impl DiskSpace for FakeDisk {
        fn free_bytes(&self, _path: &Path) -> Result<u64> {
            Ok(self.0.get())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeDisk;
    use super::*;

    struct Env {
        _dir: tempfile::TempDir,
        base: PathBuf,
        list: PathBuf,
        protected: ProtectedRoots,
    }

    fn env() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        for d in ["ssd", "hdd", "install"] {
            std::fs::create_dir_all(base.join(d)).unwrap();
        }
        let mut protected = ProtectedRoots::new();
        protected.add(&base.join("install")).unwrap();
        Env {
            list: base.join("state").join("roots.json"),
            base,
            protected,
            _dir: dir,
        }
    }

    fn add(e: &Env, r: &mut LibraryRoots, id: &str, dir: &str) -> Result<()> {
        r.add(&e.protected, id, id, &e.base.join(dir), RootKind::Builds)
    }

    #[test]
    fn a_root_is_registered_marked_and_remembered() {
        let e = env();
        let mut r = LibraryRoots::load(&e.list).unwrap();
        add(&e, &mut r, "ssd", "ssd").unwrap();
        add(&e, &mut r, "hdd", "hdd").unwrap();

        let marker = std::fs::read_to_string(e.base.join("ssd").join(MARKER_FILE)).unwrap();
        assert!(marker.contains("\"id\": \"ssd\""));

        let again = LibraryRoots::load(&e.list).unwrap();
        assert_eq!(again.roots().len(), 2);
        assert_eq!(again.get("hdd").unwrap().path, e.base.join("hdd"));
        assert_eq!(again.usable_path("ssd").unwrap(), e.base.join("ssd"));
    }

    #[test]
    fn a_root_inside_a_protected_install_is_refused_and_nothing_is_written() {
        let e = env();
        let mut r = LibraryRoots::load(&e.list).unwrap();
        std::fs::create_dir_all(e.base.join("install").join("sub")).unwrap();
        for dir in ["install", "install/sub"] {
            assert!(
                matches!(add(&e, &mut r, "bad", dir), Err(Error::Protected(_))),
                "{dir}"
            );
        }
        assert!(!e.base.join("install").join(MARKER_FILE).exists());
        assert!(!e.base.join("install/sub").join(MARKER_FILE).exists());
        assert!(r.roots().is_empty());
    }

    #[test]
    fn roots_cannot_overlap_each_other_or_the_save_sandbox() {
        let e = env();
        let mut r = LibraryRoots::load(&e.list).unwrap();
        std::fs::create_dir_all(e.base.join("ssd").join("inner")).unwrap();
        std::fs::create_dir_all(e.base.join("sandbox").join("saves")).unwrap();
        r.reserve(&e.base.join("sandbox"));
        add(&e, &mut r, "ssd", "ssd").unwrap();

        assert!(
            add(&e, &mut r, "inner", "ssd/inner").is_err(),
            "inside another root"
        );
        assert!(
            add(&e, &mut r, "sandbox", "sandbox").is_err(),
            "is the sandbox"
        );
        assert!(
            add(&e, &mut r, "saves", "sandbox/saves").is_err(),
            "inside the sandbox"
        );
        assert_eq!(r.roots().len(), 1);
        assert!(!e.base.join("ssd/inner").join(MARKER_FILE).exists());
    }

    #[test]
    fn ids_labels_paths_and_duplicates_are_validated() {
        let e = env();
        let mut r = LibraryRoots::load(&e.list).unwrap();
        for bad in ["", "..", "a/b", "con"] {
            assert!(add(&e, &mut r, bad, "ssd").is_err(), "{bad:?}");
        }
        assert!(
            r.add(
                &e.protected,
                "ok",
                "",
                &e.base.join("ssd"),
                RootKind::Builds
            )
            .is_err()
        );
        assert!(
            r.add(
                &e.protected,
                "ok",
                "x\ny",
                &e.base.join("ssd"),
                RootKind::Builds
            )
            .is_err()
        );
        assert!(
            r.add(
                &e.protected,
                "ok",
                "x",
                Path::new("relative"),
                RootKind::Builds
            )
            .is_err()
        );
        assert!(add(&e, &mut r, "ok", "missing").is_err(), "must exist");
        add(&e, &mut r, "ok", "ssd").unwrap();
        assert!(add(&e, &mut r, "ok", "hdd").is_err(), "duplicate id");
    }

    #[test]
    fn an_existing_marker_is_never_overwritten() {
        let e = env();
        let mut r = LibraryRoots::load(&e.list).unwrap();
        // The folder already belongs to another root: refuse.
        std::fs::write(
            e.base.join("ssd").join(MARKER_FILE),
            r#"{"schema":1,"id":"other"}"#,
        )
        .unwrap();
        assert!(add(&e, &mut r, "ssd", "ssd").is_err());
        assert!(
            std::fs::read_to_string(e.base.join("ssd").join(MARKER_FILE))
                .unwrap()
                .contains("other")
        );
        // The folder already carries this root's marker (the list was lost): accept and keep it.
        std::fs::write(
            e.base.join("hdd").join(MARKER_FILE),
            r#"{"schema":1,"id":"hdd"}"#,
        )
        .unwrap();
        add(&e, &mut r, "hdd", "hdd").unwrap();
        assert_eq!(
            std::fs::read_to_string(e.base.join("hdd").join(MARKER_FILE)).unwrap(),
            r#"{"schema":1,"id":"hdd"}"#
        );
    }

    #[test]
    fn relinking_needs_the_right_marker() {
        let e = env();
        let mut r = LibraryRoots::load(&e.list).unwrap();
        add(&e, &mut r, "ssd", "ssd").unwrap();
        add(&e, &mut r, "hdd", "hdd").unwrap();

        // Pretend the drive moved: copy the root, marker included, elsewhere.
        std::fs::create_dir_all(e.base.join("moved")).unwrap();
        std::fs::copy(
            e.base.join("ssd").join(MARKER_FILE),
            e.base.join("moved").join(MARKER_FILE),
        )
        .unwrap();
        std::fs::remove_file(e.base.join("ssd").join(MARKER_FILE)).unwrap();
        std::fs::remove_dir(e.base.join("ssd")).unwrap();

        // Wrong folders: no marker, someone else's marker.
        std::fs::create_dir_all(e.base.join("blank")).unwrap();
        assert!(
            r.relink(&e.protected, "ssd", &e.base.join("blank"))
                .is_err()
        );
        assert!(r.relink(&e.protected, "ssd", &e.base.join("hdd")).is_err());
        assert_eq!(r.get("ssd").unwrap().path, e.base.join("ssd"));

        r.relink(&e.protected, "ssd", &e.base.join("moved"))
            .unwrap();
        assert_eq!(r.usable_path("ssd").unwrap(), e.base.join("moved"));
        assert_eq!(
            LibraryRoots::load(&e.list)
                .unwrap()
                .get("ssd")
                .unwrap()
                .path,
            e.base.join("moved")
        );
        assert!(
            r.relink(&e.protected, "nope", &e.base.join("moved"))
                .is_err()
        );
    }

    #[test]
    fn a_missing_or_replaced_drive_is_reported_not_used() {
        let e = env();
        let mut r = LibraryRoots::load(&e.list).unwrap();
        add(&e, &mut r, "ssd", "ssd").unwrap();
        let disk = FakeDisk::with(1000);
        let s = &r.status(&disk)[0];
        assert!((s.present, s.marker_ok, s.free_bytes) == (true, true, Some(1000)));

        std::fs::remove_file(e.base.join("ssd").join(MARKER_FILE)).unwrap();
        let s = &r.status(&disk)[0];
        assert!(
            (s.present, s.marker_ok) == (true, false),
            "a different drive at the same path"
        );
        assert!(
            r.usable_path("ssd")
                .unwrap_err()
                .to_string()
                .contains("marker")
        );

        std::fs::remove_dir(e.base.join("ssd")).unwrap();
        let s = &r.status(&disk)[0];
        assert!((s.present, s.marker_ok, s.free_bytes) == (false, false, None));
        assert!(
            r.usable_path("ssd")
                .unwrap_err()
                .to_string()
                .contains("missing")
        );
        assert!(r.usable_path("zzz").is_err());
    }

    #[test]
    fn free_space_must_cover_the_need_and_the_margin() {
        let e = env();
        let mut r = LibraryRoots::load(&e.list).unwrap();
        add(&e, &mut r, "ssd", "ssd").unwrap();
        r.ensure_space(&FakeDisk::with(150), "ssd", 100, 50)
            .unwrap();
        let err = r
            .ensure_space(&FakeDisk::with(149), "ssd", 100, 50)
            .unwrap_err();
        assert!(
            matches!(
                err,
                Error::InsufficientSpace {
                    needed: 150,
                    free: 149,
                    ..
                }
            ),
            "{err}"
        );
        // Huge numbers do not wrap around.
        assert!(
            r.ensure_space(&FakeDisk::with(10), "ssd", u64::MAX, 50)
                .is_err()
        );
    }

    #[test]
    fn forgetting_a_root_keeps_its_files_and_refuses_while_in_use() {
        let e = env();
        let mut r = LibraryRoots::load(&e.list).unwrap();
        add(&e, &mut r, "ssd", "ssd").unwrap();
        std::fs::write(e.base.join("ssd").join("keep.bin"), "x").unwrap();

        assert!(
            r.forget(&e.protected, "ssd", &["build-1".to_string()])
                .is_err()
        );
        assert_eq!(r.roots().len(), 1);
        r.forget(&e.protected, "ssd", &[]).unwrap();
        assert!(r.roots().is_empty());
        assert!(e.base.join("ssd").join("keep.bin").is_file());
        assert!(e.base.join("ssd").join(MARKER_FILE).is_file());
        assert!(r.forget(&e.protected, "ssd", &[]).is_err());
    }

    #[test]
    fn a_corrupt_or_newer_list_is_an_error_not_an_empty_list() {
        let e = env();
        std::fs::create_dir_all(e.list.parent().unwrap()).unwrap();
        std::fs::write(&e.list, "{ not json").unwrap();
        assert!(LibraryRoots::load(&e.list).is_err());
        std::fs::write(&e.list, r#"{"schema":9,"roots":[]}"#).unwrap();
        assert!(LibraryRoots::load(&e.list).is_err());
        assert_eq!(
            std::fs::read_to_string(&e.list).unwrap(),
            r#"{"schema":9,"roots":[]}"#
        );
    }
}
