//! Safe file operations: atomic writes, protected roots, and isolated tree copies.
//! The original installation is registered as protected; nothing here writes into it.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::io;
use crate::journal::Journal;
use crate::safepath::resolve_lenient;
use crate::{Error, Result};

/// Locations that must never be written to or deleted from (e.g. the Steam install).
#[derive(Debug, Default, Clone)]
pub struct ProtectedRoots {
    roots: Vec<PathBuf>,
}

impl ProtectedRoots {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, p: &Path) -> Result<()> {
        self.roots.push(p.canonicalize().map_err(io(p))?);
        Ok(())
    }

    /// `Err(Protected)` if `target` (existing or not) lies inside any protected root.
    ///
    /// The target is resolved with [`resolve_lenient`], so symlinks and `..` segments, including
    /// ones that follow folders that do not exist yet, cannot be used to reach a protected root.
    pub fn check_writable(&self, target: &Path) -> Result<()> {
        let resolved = resolve_lenient(target);
        if self.roots.iter().any(|root| resolved.starts_with(root)) {
            return Err(Error::Protected(target.to_path_buf()));
        }
        Ok(())
    }
}

/// Write via a temp file in the same directory, fsync, then rename over the target.
pub fn atomic_write(protected: &ProtectedRoots, target: &Path, data: &[u8]) -> Result<()> {
    protected.check_writable(target)?;
    let dir = target
        .parent()
        .ok_or_else(|| Error::Invalid("target has no parent".into()))?;
    std::fs::create_dir_all(dir).map_err(io(dir))?;
    let tmp = dir.join(format!(
        ".{}.tmp",
        target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
    ));
    {
        let mut f = std::fs::File::create(&tmp).map_err(io(&tmp))?;
        f.write_all(data).map_err(io(&tmp))?;
        f.sync_all().map_err(io(&tmp))?;
    }
    std::fs::rename(&tmp, target).map_err(io(target))?;
    Ok(())
}

/// Keep `bytes` (the exact current content of `target`) in a new file beside it named
/// `<target file name>.<tag>`, or `.<tag>.1`, `.<tag>.2`, … when that name is taken.
///
/// The file is created with create-new, so an existing backup is never overwritten and never reused:
/// every backup is a distinct record of what the file held at that moment. Returns the backup's path.
pub fn backup_beside(
    protected: &ProtectedRoots,
    target: &Path,
    tag: &str,
    bytes: &[u8],
) -> Result<PathBuf> {
    protected.check_writable(target)?;
    let dir = target
        .parent()
        .ok_or_else(|| Error::Invalid("target has no parent".into()))?;
    let name = target
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| Error::Invalid("target has no usable file name".into()))?;
    if tag.is_empty() || tag.contains(['/', '\\', '\0']) {
        return Err(Error::Invalid(format!("unusable backup tag {tag:?}")));
    }
    for n in 0u32..10_000 {
        let candidate = if n == 0 {
            dir.join(format!("{name}.{tag}"))
        } else {
            dir.join(format!("{name}.{tag}.{n}"))
        };
        match std::fs::File::create_new(&candidate) {
            Ok(mut f) => {
                f.write_all(bytes).map_err(io(&candidate))?;
                f.sync_all().map_err(io(&candidate))?;
                return Ok(candidate);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(io(&candidate)(e)),
        }
    }
    Err(Error::Invalid(format!(
        "too many backups of {} with tag {tag}",
        target.display()
    )))
}

/// Copy one file to `dst` through a temp file in `dst`'s folder, fsync, then put it in place.
///
/// `src` is only read. An existing `dst` is replaced only when `replace` is set; callers that replace
/// something must have kept it first (see [`move_aside`]). Without `replace`, a `dst` that appears while the
/// copy is running is never overwritten (see [`publish_no_replace`]). Refuses a protected `dst`. If the copy
/// fails, the temp file is removed again.
pub fn copy_file_atomic(
    protected: &ProtectedRoots,
    src: &Path,
    dst: &Path,
    replace: bool,
) -> Result<()> {
    protected.check_writable(dst)?;
    if !replace && dst.exists() {
        return Err(Error::DestinationExists(dst.to_path_buf()));
    }
    let dir = dst
        .parent()
        .ok_or_else(|| Error::Invalid("dst has no parent".into()))?;
    std::fs::create_dir_all(dir).map_err(io(dir))?;
    let tmp = dir.join(format!(
        ".{}.sul-tmp",
        dst.file_name().and_then(|n| n.to_str()).unwrap_or("file")
    ));
    // A temp name left by an earlier failure is removed first, never reused: after a publish by hard link the
    // temp name can still point at the same bytes as `dst`, and writing through it would change `dst`.
    match std::fs::remove_file(&tmp) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(io(&tmp)(e)),
    }
    let result = (|| -> Result<()> {
        // The bytes are streamed into a new file and flushed through that same handle. Two things are
        // avoided on purpose: `std::fs::copy` also copies the source's attributes, so a read-only game file
        // would make a read-only temp file, and flushing a handle opened read-only fails on Windows with
        // "Access is denied". The copy has default attributes, whatever `src` has.
        let mut input = std::fs::File::open(src).map_err(io(src))?;
        let mut out = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(io(&tmp))?;
        std::io::copy(&mut input, &mut out).map_err(io(&tmp))?;
        out.sync_all().map_err(io(&tmp))?;
        drop(out);
        if replace {
            std::fs::rename(&tmp, dst).map_err(io(dst))
        } else {
            publish_no_replace(&tmp, dst)
        }
    })();
    if result.is_err() {
        // Best effort: a failed copy must not leave a half-written file beside the build's files.
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Put the finished temp file `tmp` at `dst`, and never replace something that is already at `dst`.
///
/// A hard link is made first: it fails when `dst` exists, in one step, so a program that creates `dst` while
/// the copy was running cannot be overwritten. Where hard links are not available (FAT and exFAT volumes, some
/// network shares) it falls back to a check followed by a rename, which leaves a window of microseconds in
/// which a file created by another program could still be replaced. That fallback is not tested.
fn publish_no_replace(tmp: &Path, dst: &Path) -> Result<()> {
    match std::fs::hard_link(tmp, dst) {
        Ok(()) => {
            // `dst` is complete. The temp name is not needed any more; if it cannot be removed now, the
            // next copy to this name removes it first.
            let _ = std::fs::remove_file(tmp);
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(Error::DestinationExists(dst.to_path_buf()))
        }
        Err(_) => {
            if std::fs::symlink_metadata(dst).is_ok() {
                return Err(Error::DestinationExists(dst.to_path_buf()));
            }
            std::fs::rename(tmp, dst).map_err(io(dst))
        }
    }
}

/// Create a folder and its parents, unless it is inside a protected root. An existing folder is fine.
pub fn create_dir(protected: &ProtectedRoots, dir: &Path) -> Result<()> {
    protected.check_writable(dir)?;
    std::fs::create_dir_all(dir).map_err(io(dir))
}

/// Rename `from` (a file or a folder) to `to`, which must not exist. Neither end may be inside a protected root.
///
/// Used to put a finished folder in its place and to move a kept file back where it came from. An existing
/// destination is an error, so a rename can be undone by renaming back. The check and the rename are two
/// steps: a program that creates `to` in the microseconds between them could have its file replaced (a file
/// rename replaces on Windows too). The callers use it for paths only SUL writes (its own recovery folders, a
/// file it has just moved away), so no such program is expected there.
pub fn rename_new(protected: &ProtectedRoots, from: &Path, to: &Path) -> Result<()> {
    protected.check_writable(from)?;
    protected.check_writable(to)?;
    if std::fs::symlink_metadata(to).is_ok() {
        return Err(Error::DestinationExists(to.to_path_buf()));
    }
    let dir = to
        .parent()
        .ok_or_else(|| Error::Invalid("destination has no parent".into()))?;
    std::fs::create_dir_all(dir).map_err(io(dir))?;
    std::fs::rename(from, to).map_err(io(from))?;
    Ok(())
}

/// Move `file` to `<recovery_dir>/<rel>` (same volume, a rename) instead of deleting or overwriting it.
///
/// Both ends must be outside protected roots, and the recovery path must not exist yet, so a file already
/// kept is not replaced (the same microsecond check-then-rename window as [`rename_new`], in a folder only
/// SUL writes). Returns where the file now is.
pub fn move_aside(
    protected: &ProtectedRoots,
    file: &Path,
    recovery_dir: &Path,
    rel: &Path,
) -> Result<PathBuf> {
    protected.check_writable(file)?;
    let to = recovery_dir.join(rel);
    protected.check_writable(&to)?;
    if to.exists() {
        return Err(Error::DestinationExists(to));
    }
    let dir = to
        .parent()
        .ok_or_else(|| Error::Invalid("recovery path has no parent".into()))?;
    std::fs::create_dir_all(dir).map_err(io(dir))?;
    std::fs::rename(file, &to).map_err(io(file))?;
    Ok(to)
}

fn copy_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst).map_err(io(dst))?;
    for entry in std::fs::read_dir(src).map_err(io(src))? {
        let entry = entry.map_err(io(src))?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let ft = entry.file_type().map_err(io(&from))?;
        if ft.is_symlink() {
            return Err(Error::UnsafePath(format!(
                "refusing to copy symlink: {}",
                from.display()
            )));
        }
        if ft.is_dir() {
            copy_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(io(&from))?;
        }
    }
    Ok(())
}

/// Copy `src` to a brand-new `dst` as an isolated build. `src` is only ever read.
///
/// Copies into `<dst>.partial` first, then renames, journaling begin/commit so an interrupted
/// copy is detectable. Refuses overlapping paths and existing destinations.
pub fn isolated_copy(
    protected: &ProtectedRoots,
    journal: &Journal,
    op_id: &str,
    src: &Path,
    dst: &Path,
) -> Result<()> {
    protected.check_writable(dst)?;
    if dst.exists() {
        return Err(Error::DestinationExists(dst.to_path_buf()));
    }
    let src_c = src.canonicalize().map_err(io(src))?;
    let parent = dst
        .parent()
        .ok_or_else(|| Error::Invalid("dst has no parent".into()))?;
    std::fs::create_dir_all(parent).map_err(io(parent))?;
    let dst_c = parent.canonicalize().map_err(io(parent))?.join(
        dst.file_name()
            .ok_or_else(|| Error::Invalid("dst has no name".into()))?,
    );
    if dst_c.starts_with(&src_c) || src_c.starts_with(&dst_c) {
        return Err(Error::Invalid("source and destination overlap".into()));
    }
    let partial = PathBuf::from(format!("{}.partial", dst.display()));
    if partial.exists() {
        // Leftover from an interrupted run: needs explicit recovery, never silent reuse.
        return Err(Error::DestinationExists(partial));
    }
    journal.begin(
        op_id,
        "isolated_copy",
        &format!("{} -> {}", src.display(), dst.display()),
    )?;
    let result = copy_recursive(&src_c, &partial)
        .and_then(|_| std::fs::rename(&partial, dst).map_err(io(dst)));
    match result {
        Ok(()) => journal.commit(op_id),
        Err(e) => {
            let _ = journal.abort(op_id, &e.to_string());
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_dirs_after_missing_folders_cannot_reach_a_protected_root() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        let original = base.join("original_install");
        std::fs::create_dir_all(&original).unwrap();
        std::fs::create_dir_all(base.join("elsewhere")).unwrap();

        let mut protected = ProtectedRoots::new();
        protected.add(&original).unwrap();

        // elsewhere/missing/../../original_install/x  ==  original_install/x
        let sneaky = base
            .join("elsewhere")
            .join("missing")
            .join("..")
            .join("..")
            .join("original_install")
            .join("x");
        assert!(matches!(
            protected.check_writable(&sneaky),
            Err(Error::Protected(_))
        ));
        assert!(matches!(
            atomic_write(&protected, &sneaky, b"nope"),
            Err(Error::Protected(_))
        ));
        assert!(!original.join("x").exists());

        // A normal sibling location is fine.
        protected
            .check_writable(&base.join("elsewhere").join("ok.txt"))
            .unwrap();
    }

    #[test]
    fn a_copy_of_a_read_only_file_is_a_normal_file_and_the_source_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        let src = base.join("game.exe");
        std::fs::write(&src, b"original bytes").unwrap();
        let mut perms = std::fs::metadata(&src).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&src, perms).unwrap();

        let protected = ProtectedRoots::new();
        let dst = base.join("build").join("game.exe");
        copy_file_atomic(&protected, &src, &dst, false).unwrap();

        assert_eq!(std::fs::read(&dst).unwrap(), b"original bytes");
        assert!(
            !std::fs::metadata(&dst).unwrap().permissions().readonly(),
            "the copy must not inherit the read-only flag"
        );
        assert!(std::fs::metadata(&src).unwrap().permissions().readonly());
        assert!(!dst.parent().unwrap().join(".game.exe.sul-tmp").exists());

        // A second copy onto the same name is refused unless the caller says it kept the old file.
        assert!(matches!(
            copy_file_atomic(&protected, &src, &dst, false),
            Err(Error::DestinationExists(_))
        ));

        // Tidy: a read-only file in a temp dir cannot always be removed on Windows.
        let mut perms = std::fs::metadata(&src).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        std::fs::set_permissions(&src, perms).unwrap();
    }
    #[test]
    fn a_finished_copy_is_never_put_over_a_file_that_is_already_there() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        let tmp = base.join(".ui.txt.sul-tmp");
        let dst = base.join("ui.txt");

        // Another program created `ui.txt` while the copy was running: its file stays, the copy is refused.
        std::fs::write(&tmp, b"MOD BYTES").unwrap();
        std::fs::write(&dst, b"THEIRS").unwrap();
        assert!(matches!(
            publish_no_replace(&tmp, &dst),
            Err(Error::DestinationExists(_))
        ));
        assert_eq!(std::fs::read(&dst).unwrap(), b"THEIRS");

        // With nothing there, the copy is put in place and the temp name is gone.
        std::fs::remove_file(&dst).unwrap();
        publish_no_replace(&tmp, &dst).unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), b"MOD BYTES");
        assert!(!tmp.exists());
    }

    #[test]
    fn a_failed_copy_leaves_no_temp_file_and_a_stale_temp_name_is_never_written_through() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        let protected = ProtectedRoots::new();
        let src = base.join("src.bin");
        std::fs::write(&src, b"new bytes").unwrap();

        // The copy cannot be put in place (the destination is a folder with something in it).
        let dst = base.join("out").join("game.exe");
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::write(dst.join("inside.txt"), b"x").unwrap();
        assert!(copy_file_atomic(&protected, &src, &dst, true).is_err());
        assert!(!base.join("out").join(".game.exe.sul-tmp").exists());
        assert!(dst.join("inside.txt").exists());

        // A temp name left by an earlier failure that shares its bytes with another file is replaced, so
        // writing the new bytes cannot change that other file.
        let other = base.join("other.dat");
        std::fs::write(&other, b"KEEP").unwrap();
        let dst2 = base.join("out2").join("a.bin");
        std::fs::create_dir_all(dst2.parent().unwrap()).unwrap();
        std::fs::hard_link(&other, dst2.parent().unwrap().join(".a.bin.sul-tmp")).unwrap();
        copy_file_atomic(&protected, &src, &dst2, false).unwrap();
        assert_eq!(std::fs::read(&dst2).unwrap(), b"new bytes");
        assert_eq!(std::fs::read(&other).unwrap(), b"KEEP");
    }
}
