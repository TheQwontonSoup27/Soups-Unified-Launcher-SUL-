//! Tauri capability audit and WebView runtime audit (SUL-LEDGER/guards.md "Tauri capability audit" and "WebView runtime
//! audit"; two of the three CI gates, the third being `electron-audit`).
//!
//! `tauri-capability-audit` reads `tauri.conf.json` (and its platform overrides) and every file in a `capabilities/`
//! folder:
//!
//! - no `"csp": null` (a missing `csp` is refused too: Tauri applies no policy when it is absent);
//! - no `withGlobalTauri: true` in a production config (a file whose name contains `.dev.` is a development config);
//! - no `dangerousDisableAssetCspModification: true`;
//! - no wildcard window or webview in a capability (`"windows": ["*"]`);
//! - no `shell:allow-execute` granted to any window;
//! - no wildcard `fs` scope. The ledger does not define "wildcard"; this reads it as a scope that is only wildcards
//!   (`*`, `**`, `/**`), or a wildcard directly under `$HOME` or a drive root (`C:/**`). A scope under an application folder
//!   such as `$APPDATA/SUL/**` is not a wildcard in this sense.
//!
//! `webview-runtime-audit` reads `Cargo.lock` and `package-lock.json`: Tauri at least 2.11.5 and `plugin-updater` at least
//! 2.12.0 (the crate and the npm package). The ledger also says `plugin-dialog` and `plugin-http` are "pinned to fixed
//! versions" but gives no version; if either is present the audit prints a note and does not fail.
//!
//! What this is NOT: a check of the WebView2 runtime floor (156.0.4314.58). That is a property of the machine the app runs
//! on, checked through `[webview] runtime_floor` in the shipped `config.toml`, which does not exist in the repository yet.
//! It does not read the built installer either. Capability files in a format other than JSON are refused, not audited.

use crate::common::{
    ensure_repository_root, extension, file_name, finish, read_text, repository_files,
};
use miette::Result;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

/// "Tauri version floor >= 2.11.5" (guards.md, WebView runtime audit).
pub const TAURI_FLOOR: Version = Version::release(2, 11, 5);
/// "plugin-updater >= 2.12.0".
pub const UPDATER_FLOOR: Version = Version::release(2, 12, 0);
/// "WebView2 runtime floor >= 156.0.4314.58 (Nov 2026)". Not checkable from the repository; named in the gate's output.
pub const WEBVIEW2_FLOOR: &str = "156.0.4314.58";

/// Crates with a version floor.
const CRATE_FLOORS: &[(&str, Version)] = &[
    ("tauri", TAURI_FLOOR),
    ("tauri-plugin-updater", UPDATER_FLOOR),
];
/// npm packages with a version floor.
const NPM_FLOORS: &[(&str, Version)] = &[("@tauri-apps/plugin-updater", UPDATER_FLOOR)];
/// Plugins the ledger says must be pinned but gives no version for: reported, not failed.
const UNPINNED_NOTE_CRATES: &[&str] = &["tauri-plugin-dialog", "tauri-plugin-http"];
const UNPINNED_NOTE_NPM: &[&str] = &["@tauri-apps/plugin-dialog", "@tauri-apps/plugin-http"];

/// `major.minor.patch`, with a pre-release counting as older than the release it leads up to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    major: u64,
    minor: u64,
    patch: u64,
    /// `false` for `2.11.5-rc.1`, so it sorts below `2.11.5`.
    stable: bool,
}

impl Version {
    pub const fn release(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
            stable: true,
        }
    }

    /// `2.11.6`, `2.11.5-rc.1`, `2.11.6+build`. `None` for anything that is not three numbers.
    pub fn parse(text: &str) -> Option<Self> {
        let without_build = text.trim().split('+').next()?;
        let (core, pre) = match without_build.split_once('-') {
            Some((core, _)) => (core, true),
            None => (without_build, false),
        };
        let mut parts = core.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self {
            major,
            minor,
            patch,
            stable: !pre,
        })
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if self.stable {
            Ok(())
        } else {
            write!(f, "-pre")
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// tauri-capability-audit
// ---------------------------------------------------------------------------------------------------------------------

/// `tauri.conf.json` and the platform or custom overrides (`tauri.windows.conf.json`).
pub fn is_tauri_conf(path: &str) -> bool {
    let name = file_name(path);
    name == "tauri.conf.json" || (name.starts_with("tauri.") && name.ends_with(".conf.json"))
}

/// A file inside a folder named `capabilities`.
pub fn is_capability_file(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    parts.len() >= 2 && parts[..parts.len() - 1].contains(&"capabilities")
}

/// A scope that grants a whole drive, the whole home folder, or everything (see the module comment).
pub fn is_wildcard_scope(path: &str) -> bool {
    let normalised = path.trim().replace('\\', "/");
    if !normalised.ends_with('*') {
        return false;
    }
    let root = normalised.trim_end_matches(['*', '/']);
    let drive =
        root.len() == 2 && root.as_bytes()[1] == b':' && root.as_bytes()[0].is_ascii_alphabetic();
    root.is_empty() || root.eq_ignore_ascii_case("$HOME") || drive
}

fn conf_problems(path: &str, text: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return vec![format!("{path}: not valid JSON, so it cannot be audited")];
    };
    let mut problems = Vec::new();
    let name = file_name(path);
    let main_file = name == "tauri.conf.json";
    let development = name.contains(".dev.");

    match value.pointer("/app/security/csp") {
        None if main_file => problems.push(format!(
            "{path}: app.security.csp is missing (Tauri applies no Content-Security-Policy)"
        )),
        Some(Value::Null) => problems.push(format!("{path}: app.security.csp is null")),
        Some(Value::String(csp)) if csp.trim().is_empty() => {
            problems.push(format!("{path}: app.security.csp is empty"));
        }
        _ => {}
    }
    if !development && value.pointer("/app/withGlobalTauri") == Some(&Value::Bool(true)) {
        problems.push(format!(
            "{path}: app.withGlobalTauri is true in a production config"
        ));
    }
    if value.pointer("/app/security/dangerousDisableAssetCspModification")
        == Some(&Value::Bool(true))
    {
        problems.push(format!(
            "{path}: app.security.dangerousDisableAssetCspModification is true"
        ));
    }
    problems
}

fn string_entries(value: Option<&Value>) -> impl Iterator<Item = &str> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

fn capability_problems(path: &str, text: &str) -> Vec<String> {
    if !extension(path).eq_ignore_ascii_case("json") {
        return vec![format!(
            "{path}: a capability file that is not JSON cannot be audited"
        )];
    }
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return vec![format!("{path}: not valid JSON, so it cannot be audited")];
    };
    let mut problems = Vec::new();
    for key in ["windows", "webviews"] {
        for entry in string_entries(value.get(key)) {
            if entry.contains('*') {
                problems.push(format!("{path}: {key} holds the wildcard \"{entry}\""));
            }
        }
    }
    for permission in value
        .get("permissions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let (identifier, object) = match permission {
            Value::String(identifier) => (identifier.as_str(), None),
            Value::Object(map) => (
                map.get("identifier").and_then(Value::as_str).unwrap_or(""),
                Some(map),
            ),
            _ => continue,
        };
        if identifier == "shell:allow-execute" {
            problems.push(format!("{path}: grants shell:allow-execute"));
        }
        if !(identifier.starts_with("fs:") || identifier.starts_with("fs-")) {
            continue;
        }
        let Some(allow) = object
            .and_then(|map| map.get("allow"))
            .and_then(Value::as_array)
        else {
            continue;
        };
        for scope in allow {
            let scope_path = match scope {
                Value::String(s) => Some(s.as_str()),
                Value::Object(map) => map.get("path").and_then(Value::as_str),
                _ => None,
            };
            if let Some(scope_path) = scope_path.filter(|p| is_wildcard_scope(p)) {
                problems.push(format!(
                    "{path}: {identifier} allows the wildcard scope \"{scope_path}\""
                ));
            }
        }
    }
    problems
}

/// Everything wrong with one file. `text` is `None` for a binary or missing file, which these checks do not read.
pub fn capability_file_problems(path: &str, text: Option<&str>) -> Vec<String> {
    let Some(text) = text else {
        return Vec::new();
    };
    let mut problems = Vec::new();
    if is_tauri_conf(path) {
        problems.extend(conf_problems(path, text));
    }
    if is_capability_file(path) {
        problems.extend(capability_problems(path, text));
    }
    problems
}

/// The `tauri-capability-audit` gate.
pub fn check_capabilities() -> Result<()> {
    let root = Path::new(".");
    ensure_repository_root(root)?;
    let mut problems = Vec::new();
    let mut audited = 0;
    for path in repository_files(root)? {
        if !(is_tauri_conf(&path) || is_capability_file(&path)) {
            continue;
        }
        audited += 1;
        problems.extend(capability_file_problems(
            &path,
            read_text(root, &path).as_deref(),
        ));
    }
    if audited == 0 {
        problems.push(
            "no tauri.conf.json or capability file found: nothing was audited (run from the repository root)"
                .to_owned(),
        );
    }
    finish("tauri-capability-audit", &problems, || {
        format!(
            "{audited} config and capability files audited: csp, withGlobalTauri, asset-CSP override, wildcard windows, shell execute, wildcard fs scopes"
        )
    })
}

// ---------------------------------------------------------------------------------------------------------------------
// webview-runtime-audit
// ---------------------------------------------------------------------------------------------------------------------

type Locked = BTreeMap<String, Vec<String>>;

/// `name -> versions` from a `Cargo.lock`.
pub fn cargo_lock_packages(text: &str) -> std::result::Result<Locked, String> {
    let value: toml::Value = toml::from_str(text).map_err(|e| format!("not valid TOML: {e}"))?;
    let mut packages = Locked::new();
    for package in value
        .get("package")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = package.get("name").and_then(toml::Value::as_str);
        let version = package.get("version").and_then(toml::Value::as_str);
        if let (Some(name), Some(version)) = (name, version) {
            packages
                .entry(name.to_owned())
                .or_default()
                .push(version.to_owned());
        }
    }
    Ok(packages)
}

/// `name -> versions` from a `package-lock.json` (lockfile v2/v3 `packages` map).
pub fn npm_lock_packages(text: &str) -> std::result::Result<Locked, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| format!("not valid JSON: {e}"))?;
    let mut packages = Locked::new();
    if let Some(map) = value.get("packages").and_then(Value::as_object) {
        for (key, entry) in map {
            let name = key.rsplit_once("node_modules/").map(|(_, name)| name);
            let version = entry.get("version").and_then(Value::as_str);
            if let (Some(name), Some(version)) = (name, version) {
                packages
                    .entry(name.to_owned())
                    .or_default()
                    .push(version.to_owned());
            }
        }
    }
    Ok(packages)
}

fn floor_problems(
    source: &str,
    packages: &Locked,
    floors: &[(&str, Version)],
    required: &[&str],
) -> Vec<String> {
    let mut problems = Vec::new();
    for (name, floor) in floors {
        let Some(versions) = packages.get(*name) else {
            if required.contains(name) {
                problems.push(format!(
                    "{source}: no \"{name}\" package, so its floor of {floor} cannot be checked"
                ));
            }
            continue;
        };
        for text in versions {
            match Version::parse(text) {
                Some(version) if version >= *floor => {}
                Some(version) => problems.push(format!(
                    "{source}: \"{name}\" {version} is below the floor {floor}"
                )),
                None => problems.push(format!(
                    "{source}: \"{name}\" has the version \"{text}\", which is not major.minor.patch"
                )),
            }
        }
    }
    problems
}

/// Problems and notes for the two lockfiles. A missing lockfile is a problem for `Cargo.lock` (the shell depends on Tauri)
/// and a note for `package-lock.json`.
pub fn runtime_findings(
    cargo_lock: Option<&str>,
    npm_lock: Option<&str>,
) -> (Vec<String>, Vec<String>) {
    let mut problems = Vec::new();
    let mut notes = Vec::new();
    match cargo_lock.map(cargo_lock_packages) {
        None => {
            problems.push("Cargo.lock is missing: the Tauri version cannot be checked".to_owned())
        }
        Some(Err(reason)) => problems.push(format!("Cargo.lock: {reason}")),
        Some(Ok(packages)) => {
            problems.extend(floor_problems(
                "Cargo.lock",
                &packages,
                CRATE_FLOORS,
                &["tauri"],
            ));
            for name in UNPINNED_NOTE_CRATES
                .iter()
                .filter(|n| packages.contains_key(**n))
            {
                notes.push(format!(
                    "Cargo.lock holds \"{name}\"; the ledger says to pin it to a fixed version but names none, so no floor was checked"
                ));
            }
        }
    }
    match npm_lock.map(npm_lock_packages) {
        None => {
            notes.push("package-lock.json is missing: the npm plugins were not checked".to_owned())
        }
        Some(Err(reason)) => problems.push(format!("package-lock.json: {reason}")),
        Some(Ok(packages)) => {
            problems.extend(floor_problems(
                "package-lock.json",
                &packages,
                NPM_FLOORS,
                &[],
            ));
            for name in UNPINNED_NOTE_NPM
                .iter()
                .filter(|n| packages.contains_key(**n))
            {
                notes.push(format!(
                    "package-lock.json holds \"{name}\"; the ledger says to pin it to a fixed version but names none, so no floor was checked"
                ));
            }
        }
    }
    (problems, notes)
}

/// The `webview-runtime-audit` gate.
pub fn check_runtime() -> Result<()> {
    let root = Path::new(".");
    ensure_repository_root(root)?;
    let cargo_lock = read_text(root, "Cargo.lock");
    let npm_lock = read_text(root, "package-lock.json");
    let (problems, notes) = runtime_findings(cargo_lock.as_deref(), npm_lock.as_deref());
    for note in &notes {
        println!("[xtask] webview-runtime-audit: note: {note}");
    }
    finish("webview-runtime-audit", &problems, || {
        format!(
            "Tauri >= {TAURI_FLOOR} and plugin-updater >= {UPDATER_FLOOR} in the lockfiles. NOT checked: the WebView2 runtime floor {WEBVIEW2_FLOOR} (needs [webview] runtime_floor in a shipped config.toml, which the repository does not have yet)"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD_CONF: &str = r#"{"app":{"security":{"csp":"default-src 'self'"}}}"#;

    #[test]
    fn versions_parse_and_order() {
        let v = |s: &str| Version::parse(s).unwrap();
        assert!(v("2.11.6") > v("2.11.5"));
        assert!(v("2.11.5") >= TAURI_FLOOR);
        assert!(v("2.12.0") > v("2.11.99"));
        assert!(v("3.0.0") > v("2.99.99"));
        assert!(
            v("2.11.5-rc.1") < v("2.11.5"),
            "a pre-release is below its release"
        );
        assert_eq!(v("2.11.5+build.7"), v("2.11.5"));
        for bad in ["", "2", "2.11", "2.11.x", "2.11.5.1", "v2.11.5"] {
            assert!(Version::parse(bad).is_none(), "{bad:?}");
        }
        assert_eq!(v("2.11.5").to_string(), "2.11.5");
    }

    #[test]
    fn a_missing_or_null_csp_is_refused() {
        assert!(conf_problems("tauri.conf.json", GOOD_CONF).is_empty());
        let null = r#"{"app":{"security":{"csp":null}}}"#;
        assert_eq!(conf_problems("tauri.conf.json", null).len(), 1);
        assert_eq!(conf_problems("tauri.conf.json", r#"{"app":{}}"#).len(), 1);
        let empty = r#"{"app":{"security":{"csp":"  "}}}"#;
        assert_eq!(conf_problems("tauri.conf.json", empty).len(), 1);
        // An override file need not repeat the policy, but may not null it.
        assert!(conf_problems("tauri.windows.conf.json", r#"{"app":{}}"#).is_empty());
        assert_eq!(conf_problems("tauri.windows.conf.json", null).len(), 1);
    }

    #[test]
    fn the_global_tauri_object_and_asset_csp_override_are_refused() {
        let global = r#"{"app":{"withGlobalTauri":true,"security":{"csp":"x"}}}"#;
        assert_eq!(conf_problems("tauri.conf.json", global).len(), 1);
        // A development config may use it.
        assert!(conf_problems("tauri.dev.conf.json", global).is_empty());
        let off = r#"{"app":{"withGlobalTauri":false,"security":{"csp":"x"}}}"#;
        assert!(conf_problems("tauri.conf.json", off).is_empty());
        let asset =
            r#"{"app":{"security":{"csp":"x","dangerousDisableAssetCspModification":true}}}"#;
        assert_eq!(conf_problems("tauri.conf.json", asset).len(), 1);
        assert_eq!(conf_problems("tauri.conf.json", "{").len(), 1);
    }

    #[test]
    fn wildcard_windows_and_webviews_are_refused() {
        let clean = r#"{"windows":["main"],"permissions":["core:default"]}"#;
        assert!(capability_problems("capabilities/default.json", clean).is_empty());
        assert_eq!(
            capability_problems("capabilities/a.json", r#"{"windows":["*"]}"#).len(),
            1
        );
        assert_eq!(
            capability_problems("capabilities/a.json", r#"{"windows":["main-*"]}"#).len(),
            1
        );
        assert_eq!(
            capability_problems("capabilities/a.json", r#"{"webviews":["*"]}"#).len(),
            1
        );
    }

    #[test]
    fn shell_execute_is_refused_as_a_string_or_an_object() {
        let string = r#"{"windows":["main"],"permissions":["shell:allow-execute"]}"#;
        assert_eq!(capability_problems("capabilities/a.json", string).len(), 1);
        let object = r#"{"windows":["main"],"permissions":[{"identifier":"shell:allow-execute","allow":[{"cmd":"x"}]}]}"#;
        assert_eq!(capability_problems("capabilities/a.json", object).len(), 1);
        let other = r#"{"windows":["main"],"permissions":["shell:allow-open"]}"#;
        assert!(capability_problems("capabilities/a.json", other).is_empty());
    }

    #[test]
    fn wildcard_fs_scopes_are_refused_but_application_folders_are_not() {
        for scope in [
            "*", "**", "/**", "**/*", "$HOME/**", "$home/*", "C:/**", "d:\\**",
        ] {
            assert!(is_wildcard_scope(scope), "{scope} should be a wildcard");
        }
        for scope in [
            "$APPDATA/SUL/**",
            "$APPDATA/SUL/config.toml",
            "$HOME/Documents/SUL",
            "",
        ] {
            assert!(
                !is_wildcard_scope(scope),
                "{scope:?} should not be a wildcard"
            );
        }
        let wide = r#"{"windows":["main"],"permissions":[{"identifier":"fs:allow-read-file","allow":[{"path":"$HOME/**"}]}]}"#;
        assert_eq!(capability_problems("capabilities/a.json", wide).len(), 1);
        let bare =
            r#"{"windows":["main"],"permissions":[{"identifier":"fs:scope","allow":["**"]}]}"#;
        assert_eq!(capability_problems("capabilities/a.json", bare).len(), 1);
        let narrow = r#"{"windows":["main"],"permissions":[{"identifier":"fs:scope","allow":[{"path":"$APPDATA/SUL/**"}]}]}"#;
        assert!(capability_problems("capabilities/a.json", narrow).is_empty());
        // A wildcard in a deny list only narrows access.
        let deny =
            r#"{"windows":["main"],"permissions":[{"identifier":"fs:scope","deny":["**"]}]}"#;
        assert!(capability_problems("capabilities/a.json", deny).is_empty());
    }

    #[test]
    fn a_capability_file_that_is_not_json_is_refused_not_skipped() {
        assert_eq!(capability_problems("capabilities/a.toml", "x = 1").len(), 1);
        assert_eq!(capability_problems("capabilities/a.json", "{").len(), 1);
    }

    #[test]
    fn files_are_routed_to_the_right_check() {
        assert!(is_tauri_conf("src-tauri/tauri.conf.json"));
        assert!(is_tauri_conf("src-tauri/tauri.windows.conf.json"));
        assert!(!is_tauri_conf("src-tauri/Cargo.toml"));
        assert!(is_capability_file("src-tauri/capabilities/default.json"));
        assert!(!is_capability_file("capabilities"));
        assert!(!is_capability_file(
            "src-tauri/gen/schemas/capabilities.json"
        ));
        assert_eq!(
            capability_file_problems("src-tauri/tauri.conf.json", Some(r#"{"app":{}}"#)).len(),
            1
        );
        assert!(capability_file_problems("src-tauri/tauri.conf.json", None).is_empty());
        assert!(capability_file_problems("ui/src/main.tsx", Some("*")).is_empty());
    }

    fn cargo_lock(entries: &[(&str, &str)]) -> String {
        entries
            .iter()
            .map(|(n, v)| format!("[[package]]\nname = \"{n}\"\nversion = \"{v}\"\n"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_tauri_floor_is_enforced_on_every_locked_version() {
        let ok = cargo_lock(&[("tauri", "2.11.6"), ("serde", "1.0.0")]);
        assert!(runtime_findings(Some(&ok), None).0.is_empty());
        let exactly = cargo_lock(&[("tauri", "2.11.5")]);
        assert!(runtime_findings(Some(&exactly), None).0.is_empty());
        let old = cargo_lock(&[("tauri", "2.11.4")]);
        assert_eq!(runtime_findings(Some(&old), None).0.len(), 1);
        let two = cargo_lock(&[("tauri", "2.11.6"), ("tauri", "2.9.0")]);
        assert_eq!(runtime_findings(Some(&two), None).0.len(), 1);
        let pre = cargo_lock(&[("tauri", "2.11.5-rc.1")]);
        assert_eq!(runtime_findings(Some(&pre), None).0.len(), 1);
    }

    #[test]
    fn a_missing_tauri_or_lockfile_is_a_problem_not_a_pass() {
        let none = cargo_lock(&[("serde", "1.0.0")]);
        assert_eq!(runtime_findings(Some(&none), None).0.len(), 1);
        assert_eq!(runtime_findings(None, None).0.len(), 1);
        assert_eq!(runtime_findings(Some("= not toml"), None).0.len(), 1);
    }

    #[test]
    fn the_updater_floor_applies_to_the_crate_and_the_npm_package() {
        let crate_old = cargo_lock(&[("tauri", "2.11.6"), ("tauri-plugin-updater", "2.11.0")]);
        assert_eq!(runtime_findings(Some(&crate_old), None).0.len(), 1);
        let crate_ok = cargo_lock(&[("tauri", "2.11.6"), ("tauri-plugin-updater", "2.12.0")]);
        assert!(runtime_findings(Some(&crate_ok), None).0.is_empty());

        let base = cargo_lock(&[("tauri", "2.11.6")]);
        let npm_old =
            r#"{"packages":{"":{},"node_modules/@tauri-apps/plugin-updater":{"version":"2.9.0"}}}"#;
        assert_eq!(runtime_findings(Some(&base), Some(npm_old)).0.len(), 1);
        let npm_ok = r#"{"packages":{"":{},"node_modules/@tauri-apps/plugin-updater":{"version":"2.12.1"}}}"#;
        assert!(runtime_findings(Some(&base), Some(npm_ok)).0.is_empty());
        assert_eq!(runtime_findings(Some(&base), Some("{")).0.len(), 1);
    }

    #[test]
    fn dialog_and_http_plugins_are_noted_not_failed() {
        let lock = cargo_lock(&[("tauri", "2.11.6"), ("tauri-plugin-dialog", "2.0.0")]);
        let npm = r#"{"packages":{"node_modules/@tauri-apps/plugin-http":{"version":"2.0.0"}}}"#;
        let (problems, notes) = runtime_findings(Some(&lock), Some(npm));
        assert!(problems.is_empty());
        assert_eq!(notes.len(), 2);
        assert!(notes.iter().all(|n| n.contains("names none")));
    }
}
