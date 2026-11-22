//! The workspace manifest guard (Ch. 1 §4: Edition 2024, MSRV 1.85, no Linux-only `io-uring` in a Windows workspace).
//!
//! It used to live in `main.rs` as a function that stopped at the first problem and said nothing when it passed. It has the
//! same shape as every other guard now: a pure function over the manifest text, and a `check()` that prints a verdict.

use crate::common::{ensure_repository_root, finish, read_text};
use miette::Result;
use std::path::Path;

pub const EDITION: &str = "2024";
pub const MSRV: &str = "1.85";

/// Everything wrong with the text of the workspace `Cargo.toml`.
pub fn manifest_problems(raw: &str) -> Vec<String> {
    let Ok(doc) = toml::from_str::<toml::Value>(raw) else {
        return vec!["Cargo.toml: not valid TOML".to_owned()];
    };
    let mut problems = Vec::new();
    match doc.get("workspace").and_then(|w| w.get("package")) {
        None => problems.push(
            "Cargo.toml has no [workspace.package]: the edition and rust-version cannot be checked"
                .to_owned(),
        ),
        Some(package) => {
            let edition = package
                .get("edition")
                .and_then(toml::Value::as_str)
                .unwrap_or("");
            if edition != EDITION {
                problems.push(format!(
                    "workspace.package.edition must be {EDITION} (found {edition:?})"
                ));
            }
            let msrv = package
                .get("rust-version")
                .and_then(toml::Value::as_str)
                .unwrap_or("");
            if msrv != MSRV {
                problems.push(format!(
                    "workspace.package.rust-version must be {MSRV} (found {msrv:?})"
                ));
            }
        }
    }
    if raw.contains("io-uring") {
        problems.push(
            "the Linux-only compio `io-uring` feature must not appear in the Windows workspace Cargo.toml"
                .to_owned(),
        );
    }
    problems
}

/// `workspace.package.version` from the text of the workspace `Cargo.toml`.
pub fn workspace_version(raw: &str) -> Option<String> {
    let doc: toml::Value = toml::from_str(raw).ok()?;
    doc.get("workspace")?
        .get("package")?
        .get("version")?
        .as_str()
        .map(str::to_owned)
}

/// The `workspace-manifest` gate.
pub fn check() -> Result<()> {
    let root = Path::new(".");
    ensure_repository_root(root)?;
    let problems = match read_text(root, "Cargo.toml") {
        Some(raw) => manifest_problems(&raw),
        None => vec!["Cargo.toml could not be read".to_owned()],
    };
    finish("workspace-manifest", &problems, || {
        format!("edition {EDITION}, rust-version {MSRV}, no io-uring")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "[workspace]\nmembers = []\n[workspace.package]\nversion = \"0.1.0\"\nedition = \"2024\"\nrust-version = \"1.85\"\n";

    #[test]
    fn a_conforming_manifest_has_no_problems() {
        assert!(manifest_problems(GOOD).is_empty());
    }

    #[test]
    fn each_rule_is_checked_on_its_own() {
        let old_edition = GOOD.replace("2024", "2021");
        let problems = manifest_problems(&old_edition);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("edition"));

        let new_msrv = GOOD.replace("1.85", "1.90");
        let problems = manifest_problems(&new_msrv);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("rust-version"));

        let linux = format!(
            "{GOOD}\n[workspace.dependencies]\ncompio = {{ version = \"1\", features = [\"io-uring\"] }}\n"
        );
        let problems = manifest_problems(&linux);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("io-uring"));
    }

    #[test]
    fn every_problem_is_reported_not_only_the_first() {
        let bad = "[workspace.package]\nedition = \"2021\"\nrust-version = \"1.70\"\n# io-uring\n";
        assert_eq!(manifest_problems(bad).len(), 3);
    }

    #[test]
    fn a_missing_package_table_or_broken_toml_is_a_problem_not_a_pass() {
        assert_eq!(manifest_problems("[workspace]\nmembers = []\n").len(), 1);
        assert_eq!(manifest_problems("= not toml").len(), 1);
    }

    #[test]
    fn the_workspace_version_is_read() {
        assert_eq!(workspace_version(GOOD).as_deref(), Some("0.1.0"));
        assert_eq!(workspace_version("[workspace]\n"), None);
        assert_eq!(workspace_version("= not toml"), None);
    }
}
