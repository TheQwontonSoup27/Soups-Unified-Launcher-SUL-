//! Path safety: all managed writes must resolve inside an explicit root.

use std::path::{Component, Path, PathBuf};

use crate::error::io;
use crate::{Error, Result};

/// Validate a manifest-style relative path (forward or back slashes accepted).
/// Rejects absolute paths, drive prefixes, `..`, empty paths and NUL bytes.
pub fn validate_relative(rel: &str) -> Result<PathBuf> {
    if rel.is_empty() || rel.contains('\0') {
        return Err(Error::UnsafePath(format!("empty or NUL path: {rel:?}")));
    }
    let normalized = rel.replace('\\', "/");
    if normalized.starts_with('/') || normalized.as_bytes().get(1) == Some(&b':') {
        return Err(Error::UnsafePath(format!("absolute path: {rel}")));
    }
    let mut out = PathBuf::new();
    for part in normalized.split('/') {
        match part {
            "" | "." => continue,
            ".." => return Err(Error::UnsafePath(format!("parent traversal: {rel}"))),
            p => out.push(p),
        }
    }
    if out.as_os_str().is_empty() {
        return Err(Error::UnsafePath(format!("empty path: {rel}")));
    }
    for c in out.components() {
        if !matches!(c, Component::Normal(_)) {
            return Err(Error::UnsafePath(format!("unexpected component: {rel}")));
        }
    }
    Ok(out)
}

/// Join `rel` under `root`, ensuring no existing symlink in the chain escapes `root`.
pub fn join_within(root: &Path, rel: &str) -> Result<PathBuf> {
    let rel = validate_relative(rel)?;
    let root_canon = root.canonicalize().map_err(io(root))?;
    let full = root_canon.join(&rel);
    // Canonicalize the deepest existing ancestor and make sure it is still inside root.
    let mut probe = full.as_path();
    loop {
        if probe.exists() {
            let canon = probe.canonicalize().map_err(io(probe))?;
            if !canon.starts_with(&root_canon) {
                return Err(Error::UnsafePath(format!(
                    "{} escapes root via symlink",
                    full.display()
                )));
            }
            break;
        }
        match probe.parent() {
            Some(p) => probe = p,
            None => break,
        }
    }
    Ok(full)
}

/// Resolve `path` as far as the filesystem allows, even if its tail does not exist yet.
///
/// The longest existing prefix is canonicalised (symlinks and `..` inside it are resolved by
/// the OS); the remaining components are appended with `.` and `..` applied lexically, which
/// is how Windows treats them before the filesystem is consulted. Use this to decide whether
/// a *future* write location lies inside or outside a directory of interest.
pub(crate) fn resolve_lenient(path: &Path) -> PathBuf {
    let parts: Vec<Component> = path.components().collect();
    for split in (1..=parts.len()).rev() {
        let prefix: PathBuf = parts[..split].iter().collect();
        let Ok(mut resolved) = prefix.canonicalize() else {
            continue;
        };
        for part in &parts[split..] {
            match part {
                Component::ParentDir => {
                    resolved.pop();
                }
                Component::CurDir => {}
                other => resolved.push(other.as_os_str()),
            }
        }
        return resolved;
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_lenient_applies_parent_dirs_after_missing_components() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("a")).unwrap();

        // `missing` does not exist, so the OS cannot resolve the `..`; we do it lexically.
        assert_eq!(
            resolve_lenient(
                &root
                    .join("a")
                    .join("missing")
                    .join("..")
                    .join("..")
                    .join("b")
            ),
            root.join("b")
        );
        // Existing prefix and trailing missing parts are kept as written.
        assert_eq!(
            resolve_lenient(&root.join("a").join("x").join("y")),
            root.join("a").join("x").join("y")
        );
        // `..` through an existing directory is resolved by the OS.
        assert_eq!(
            resolve_lenient(&root.join("a").join("..").join("z")),
            root.join("z")
        );
    }
}
