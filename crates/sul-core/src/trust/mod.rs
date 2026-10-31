//! Security & Trust building blocks: integrity checks, package scanning, quarantine and webhook-address checking.
//!
//! These are library code with tests. Nothing in the app calls them yet (there is no mod manager to hand them a package),
//! and none of them is an antivirus: they look at names, sizes, file types and hashes, never at what code does.
//!
//! Two words are used on purpose, and kept apart everywhere:
//! * **Confirmed**: the evidence settles the fact (a hash differs from the recorded one; a path climbs out of its folder).
//!   It says nothing about intent: a changed file is not proof of a cheat, and a bad path is not proof of malice.
//! * **Suspicious**: a signal that needs a person to look (a program file in a package, a file name that imitates a
//!   document). Signals have false positives (many legitimate mods ship a DLL) and are never reported as confirmed.
//!
//! "No findings" is never reported as "safe" ([`NOT_A_GUARANTEE`]).

use serde::Serialize;

pub mod integrity;
pub mod package;
pub mod quarantine;
pub mod webhook;

/// How sure a finding is. See the module documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Certainty {
    Confirmed,
    Suspicious,
}

/// One thing a check found. `code` is a stable machine name; `detail` is the sentence a person reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub certainty: Certainty,
    pub code: &'static str,
    pub path: String,
    pub detail: String,
}

impl Finding {
    pub(crate) fn confirmed(code: &'static str, path: &str, detail: impl Into<String>) -> Self {
        Self {
            certainty: Certainty::Confirmed,
            code,
            path: path.to_owned(),
            detail: detail.into(),
        }
    }

    pub(crate) fn suspicious(code: &'static str, path: &str, detail: impl Into<String>) -> Self {
        Self {
            certainty: Certainty::Suspicious,
            code,
            path: path.to_owned(),
            detail: detail.into(),
        }
    }
}

/// Shown wherever a check ends with nothing to report.
pub const NOT_A_GUARANTEE: &str = "No findings is not a guarantee of safety: these checks look at names, sizes, file types and hashes, not at what code does.";

/// What kind of program-like file a name is, judged from its extension only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramKind {
    /// Compiled code: exe, dll, sys, scr, com, msi, cpl.
    NativeCode,
    /// Scripts and registry files that run or change settings when opened: bat, cmd, ps1, vbs, js, hta, reg and similar.
    Script,
    /// A shortcut that can point anywhere: lnk.
    Shortcut,
    /// A Java archive.
    Java,
}

/// The last extension of a forward- or back-slash path's file name, in lower case.
pub(crate) fn extension(path: &str) -> Option<String> {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() || ext.is_empty() {
        return None;
    }
    Some(ext.to_ascii_lowercase())
}

/// The kind of program-like file `path` looks like, or `None` for anything else (documents, pictures, data).
pub fn program_kind(path: &str) -> Option<ProgramKind> {
    Some(match extension(path)?.as_str() {
        "exe" | "dll" | "sys" | "scr" | "com" | "msi" | "cpl" | "pif" => ProgramKind::NativeCode,
        "bat" | "cmd" | "ps1" | "psm1" | "vbs" | "vbe" | "js" | "jse" | "wsf" | "wsh" | "hta"
        | "reg" | "msc" => ProgramKind::Script,
        "lnk" => ProgramKind::Shortcut,
        "jar" => ProgramKind::Java,
        _ => return None,
    })
}

/// Test helper: make a folder junction (a folder link Windows lets anyone create; a symlink needs a privilege).
#[cfg(all(test, windows))]
pub(crate) fn make_junction(link: &std::path::Path, target: &std::path::Path) {
    use std::os::windows::process::CommandExt;
    let made = std::process::Command::new("cmd")
        .raw_arg(format!(
            r#"/C mklink /J "{}" "{}""#,
            link.display(),
            target.display()
        ))
        .output()
        .unwrap();
    assert!(
        made.status.success(),
        "mklink failed: {}",
        String::from_utf8_lossy(&made.stdout)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_are_read_from_the_file_name_only() {
        assert_eq!(extension("a/b/c.TXT").as_deref(), Some("txt"));
        assert_eq!(extension("a\\b\\c.tar.gz").as_deref(), Some("gz"));
        assert_eq!(
            extension("dir.exe/readme"),
            None,
            "a dot in a folder name is not an extension"
        );
        assert_eq!(
            extension(".gitignore"),
            None,
            "a leading dot is a hidden name, not an extension"
        );
        assert_eq!(extension("noext"), None);
        assert_eq!(extension("trailing."), None);
        assert_eq!(extension(""), None);
    }

    #[test]
    fn program_kinds_cover_the_usual_suspects_and_nothing_else() {
        for (name, kind) in [
            ("Game.EXE", ProgramKind::NativeCode),
            ("plugin.dll", ProgramKind::NativeCode),
            ("driver.sys", ProgramKind::NativeCode),
            ("setup.msi", ProgramKind::NativeCode),
            ("run.bat", ProgramKind::Script),
            ("run.CMD", ProgramKind::Script),
            ("x.ps1", ProgramKind::Script),
            ("x.vbs", ProgramKind::Script),
            ("x.js", ProgramKind::Script),
            ("x.reg", ProgramKind::Script),
            ("link.lnk", ProgramKind::Shortcut),
            ("tool.jar", ProgramKind::Java),
        ] {
            assert_eq!(program_kind(name), Some(kind), "{name}");
        }
        for name in [
            "readme.txt",
            "tex.dds",
            "a.png",
            "data.json",
            "model.fbx",
            "noext",
            "exe",
            "dll.txt",
            ".exe",
        ] {
            assert_eq!(program_kind(name), None, "{name}");
        }
    }

    #[test]
    fn the_disclaimer_never_calls_anything_safe() {
        assert!(NOT_A_GUARANTEE.contains("not a guarantee"));
    }
}
