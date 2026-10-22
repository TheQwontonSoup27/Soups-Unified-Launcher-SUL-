//! Safe file operations: atomic writes, protected roots, and isolated tree copies.
//! The original installation is registered as protected; nothing here writes into it.

use crate::{error::io, journal::Journal, Error, Result};
use std::io::Write;
use std::path::{Path, PathBuf};

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
    /// Err(Protected) if `target` (existing or not) lies inside any protected root.
    pub fn check_writable(&self, target: &Path) -> Result<()> {
        let mut probe = target;
        let canon = loop {
            if probe.exists() {
                let c = probe.canonicalize().map_err(io(probe))?;
                break c.join(target.strip_prefix(probe).unwrap_or(Path::new("")));
            }
            match probe.parent() {
                Some(p) if !p.as_os_str().is_empty() => probe = p,
                _ => break target.to_path_buf(),
            }
        };
        for r in &self.roots {
            if canon.starts_with(r) {
                return Err(Error::Protected(target.to_path_buf()));
            }
        }
        Ok(())
    }
}

/// Write via a temp file in the same directory, fsync, then rename over the target.
pub fn atomic_write(protected: &ProtectedRoots, target: &Path, data: &[u8]) -> Result<()> {
    protected.check_writable(target)?;
    let dir = target.parent().ok_or_else(|| Error::Invalid("target has no parent".into()))?;
    std::fs::create_dir_all(dir).map_err(io(dir))?;
    let tmp = dir.join(format!(".{}.tmp", target.file_name().and_then(|n| n.to_str()).unwrap_or("file")));
    {
        let mut f = std::fs::File::create(&tmp).map_err(io(&tmp))?;
        f.write_all(data).map_err(io(&tmp))?;
        f.sync_all().map_err(io(&tmp))?;
    }
    std::fs::rename(&tmp, target).map_err(io(target))?;
    Ok(())
}

fn copy_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst).map_err(io(dst))?;
    for entry in std::fs::read_dir(src).map_err(io(src))? {
        let entry = entry.map_err(io(src))?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let ft = entry.file_type().map_err(io(&from))?;
        if ft.is_symlink() {
            return Err(Error::UnsafePath(format!("refusing to copy symlink: {}", from.display())));
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
/// Copies into `<dst>.partial` first, then renames, journaling begin/commit so an interrupted
/// copy is detectable. Refuses overlapping paths and existing destinations.
pub fn isolated_copy(protected: &ProtectedRoots, journal: &Journal, op_id: &str, src: &Path, dst: &Path) -> Result<()> {
    protected.check_writable(dst)?;
    if dst.exists() {
        return Err(Error::DestinationExists(dst.to_path_buf()));
    }
    let src_c = src.canonicalize().map_err(io(src))?;
    let parent = dst.parent().ok_or_else(|| Error::Invalid("dst has no parent".into()))?;
    std::fs::create_dir_all(parent).map_err(io(parent))?;
    let dst_c = parent.canonicalize().map_err(io(parent))?
        .join(dst.file_name().ok_or_else(|| Error::Invalid("dst has no name".into()))?);
    if dst_c.starts_with(&src_c) || src_c.starts_with(&dst_c) {
        return Err(Error::Invalid("source and destination overlap".into()));
    }
    let partial = PathBuf::from(format!("{}.partial", dst.display()));
    if partial.exists() {
        return Err(Error::DestinationExists(partial)); // leftover from an interrupted run: needs explicit recovery
    }
    journal.begin(op_id, "isolated_copy", &format!("{} -> {}", src.display(), dst.display()))?;
    let result = copy_recursive(&src_c, &partial).and_then(|_| std::fs::rename(&partial, dst).map_err(io(dst)));
    match result {
        Ok(()) => journal.commit(op_id),
        Err(e) => {
            let _ = journal.abort(op_id, &e.to_string());
            Err(e)
        }
    }
}
