//! Shared plumbing for the gates: which files make up the repository, how to read one, and one way to report problems.
//!
//! Every guard in this crate has the same shape: a pure function that takes text and returns a list of problems (so it can
//! be tested with made-up input), and a `check()` that gathers the real files, prints a `[xtask] <gate>: ...` line and
//! turns the list into a pass or a fail. The CI wrapper `.github/scripts/surface-failure.ps1` copies the `PROBLEM ` lines
//! into the job annotations, so that prefix is part of the contract.

use miette::{Context, IntoDiagnostic, Result};
use std::path::Path;
use std::process::Command;

/// The repository files the guards read, relative to `root`, with forward slashes: everything git tracks plus new files it
/// would not ignore. A guard therefore sees a file before it is committed, and never sees `target/` or `node_modules/`.
pub fn repository_files(root: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .output()
        .into_diagnostic()
        .context("failed to start git (the guards read the repository's file list from it)")?;
    if !output.status.success() {
        return Err(miette::miette!(
            "git ls-files failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(parse_file_list(&output.stdout))
}

/// Splits `git ls-files -z` output into sorted, de-duplicated paths with forward slashes.
pub fn parse_file_list(raw: &[u8]) -> Vec<String> {
    let mut files: Vec<String> = raw
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8_lossy(path).replace('\\', "/"))
        .collect();
    files.sort();
    files.dedup();
    files
}

/// The text of a file, or `None` for a file that is not UTF-8 (a binary) or that git lists but is gone from the disk.
pub fn read_text(root: &Path, relative: &str) -> Option<String> {
    let bytes = std::fs::read(root.join(relative)).ok()?;
    String::from_utf8(bytes).ok()
}

/// The part of a file name after its last dot, or "" when it has none.
pub fn extension(path: &str) -> &str {
    let name = file_name(path);
    name.rsplit_once('.').map_or("", |(_, ext)| ext)
}

/// The last path segment.
pub fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The guards read relative paths, so they must run from the repository root.
pub fn ensure_repository_root(root: &Path) -> Result<()> {
    if root.join("Cargo.toml").is_file() && root.join("SUL-LEDGER").is_dir() {
        return Ok(());
    }
    Err(miette::miette!(
        "run `cargo xtask` from the repository root (no Cargo.toml and SUL-LEDGER/ in {})",
        root.display()
    ))
}

/// Prints the verdict of one gate. Problems go to stderr as `PROBLEM <gate>: ...` lines.
///
/// Deliberately `Err(miette!(...))` and not `bail!`: as the last expression of a function `bail!` expands to a statement
/// with a trailing semicolon, which a newer rustc than the one in the Linux container rejects (Windows CI, 2026-11-20).
pub fn finish(gate: &str, problems: &[String], ok: impl FnOnce() -> String) -> Result<()> {
    if problems.is_empty() {
        println!("[xtask] {gate}: OK ({})", ok());
        return Ok(());
    }
    for problem in problems {
        eprintln!("PROBLEM {gate}: {problem}");
    }
    Err(miette::miette!("{gate}: {} problem(s)", problems.len()))
}

/// Runs every gate in the list even when an earlier one fails, so one run reports everything wrong. A gate that fails
/// through [`finish`] has already printed its `PROBLEM` lines; one that fails any other way (cargo or git could not run) has
/// its message printed here, so no failure is swallowed. Returns an error that names the failed gates.
pub fn run_all(gates: &[(&str, &dyn Fn() -> Result<()>)]) -> Result<()> {
    let mut failed: Vec<&str> = Vec::new();
    for (name, gate) in gates {
        if let Err(error) = gate() {
            eprintln!("[xtask] {name}: FAILED: {error}");
            failed.push(name);
        }
    }
    if failed.is_empty() {
        return Ok(());
    }
    Err(miette::miette!("failed: {}", failed.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_list_is_split_sorted_and_normalised() {
        let raw = b"b.rs\0a/c.rs\0a\\d.rs\0b.rs\0\0";
        assert_eq!(parse_file_list(raw), ["a/c.rs", "a/d.rs", "b.rs"]);
        assert!(parse_file_list(b"").is_empty());
    }

    #[test]
    fn extensions_and_names() {
        assert_eq!(extension("crates/x/src/lib.rs"), "rs");
        assert_eq!(extension("Cargo.toml"), "toml");
        assert_eq!(extension("LICENSE"), "");
        assert_eq!(extension("dir.with.dots/file"), "");
        assert_eq!(extension(".gitignore"), "gitignore");
        assert_eq!(file_name("a/b/c.txt"), "c.txt");
        assert_eq!(file_name("c.txt"), "c.txt");
    }

    #[test]
    fn a_gate_with_no_problems_passes_and_one_with_problems_fails() {
        assert!(finish("g", &[], || "fine".to_owned()).is_ok());
        let err = finish(
            "g",
            &["bad".to_owned(), "worse".to_owned()],
            || unreachable!(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("2 problem"));
    }

    #[test]
    fn run_all_runs_every_gate_and_names_the_failures() {
        use std::cell::Cell;
        let ran = Cell::new(0);
        let ok = || {
            ran.set(ran.get() + 1);
            Ok(())
        };
        let bad = || {
            ran.set(ran.get() + 1);
            Err(miette::miette!("no"))
        };
        let err =
            run_all(&[("one", &ok), ("two", &bad), ("three", &ok), ("four", &bad)]).unwrap_err();
        assert_eq!(ran.get(), 4, "a failure must not stop the later gates");
        let text = err.to_string();
        assert!(text.contains("two") && text.contains("four") && !text.contains("one"));
        assert!(run_all(&[("one", &ok)]).is_ok());
    }

    #[test]
    fn the_root_check_names_the_directory() {
        let err = ensure_repository_root(Path::new("/definitely/not/a/repo")).unwrap_err();
        assert!(err.to_string().contains("repository root"));
    }
}
