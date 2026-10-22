//! Path safety: all managed writes must resolve inside an explicit root.

use crate::{error::io, Error, Result};
use std::path::{Component, Path, PathBuf};

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
