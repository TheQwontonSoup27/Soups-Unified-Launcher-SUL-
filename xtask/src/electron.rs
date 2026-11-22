//! Electron exclusion for `cargo xtask electron-audit` and `ac-regression` (SUL-LEDGER/guards.md "Electron exclusion",
//! Ch. 1: zero Electron and zero Node in `sul.exe`).
//!
//! The ledger's rules, and where each is checked:
//!
//! - No `electron`, `electron-builder` or `@electron/*` in any manifest: `Cargo.toml` dependency tables, `package.json`
//!   dependency sections, and every package named in `package-lock.json`.
//! - No `nodeIntegration`, `contextBridge`, `webPreferences`, `BrowserWindow`, `BrowserView` or `<webview>` in any file:
//!   a text scan of the repository's source and config files.
//! - No `asar` anywhere: `.asar` files and the `asar` package.
//! - No bundled Node.js runtime and no Electron sidecar: the Tauri bundle's `externalBin` and `resources`, and a Node or
//!   Electron executable tracked in the tree.
//!
//! Left out of the text scan on purpose: Markdown (the spec and decision records name the banned APIs in order to ban them)
//! and this crate (it holds the pattern list). What this is NOT: a check of the built `sul.exe`. It reads files, not the
//! binary, and the Node sidecars the spec allows in isolated containers (D-139) are outside the tree it scans.

use crate::common::{extension, file_name, finish, read_text, repository_files};
use miette::Result;
use std::collections::BTreeSet;
use std::path::Path;

/// APIs that only exist in Electron. Matched case-sensitively, except `<webview`, which is an HTML tag.
pub const API_TOKENS: &[&str] = &[
    "nodeIntegration",
    "contextBridge",
    "webPreferences",
    "BrowserWindow",
    "BrowserView",
];

/// npm packages that carry "electron" in their name but are not Electron: `electron-to-chromium` is a data table that
/// `browserslist` (a dependency of most front-end build tooling) pulls in; it has no runtime and ships nothing in the app.
pub const NOT_ELECTRON: &[&str] = &["electron-to-chromium"];

/// File extensions the text scan reads. Binary files are skipped by the reader.
const SCANNED_EXTENSIONS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "html", "htm", "json", "toml", "yml", "yaml",
    "ps1",
];

/// Whether a package name belongs to Electron or its tooling.
pub fn is_electron_package(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    if NOT_ELECTRON.contains(&name.as_str()) {
        return false;
    }
    name == "electron"
        || name == "asar"
        || name.starts_with("electron-")
        || name.starts_with("@electron/")
        || name.starts_with("@electron-forge/")
}

/// Dependency names declared anywhere in a Cargo manifest, including `package = "..."` renames.
fn cargo_dependency_names(value: &toml::Value, out: &mut Vec<String>) {
    let Some(table) = value.as_table() else {
        return;
    };
    for (key, inner) in table {
        if matches!(
            key.as_str(),
            "dependencies" | "dev-dependencies" | "build-dependencies"
        ) {
            if let Some(dependencies) = inner.as_table() {
                for (name, spec) in dependencies {
                    out.push(name.clone());
                    if let Some(real) = spec.get("package").and_then(toml::Value::as_str) {
                        out.push(real.to_owned());
                    }
                }
            }
        } else {
            cargo_dependency_names(inner, out);
        }
    }
}

fn cargo_problems(path: &str, text: &str) -> Vec<String> {
    let Ok(value) = toml::from_str::<toml::Value>(text) else {
        return vec![format!(
            "{path}: not valid TOML, so it cannot be audited for Electron"
        )];
    };
    let mut names = Vec::new();
    cargo_dependency_names(&value, &mut names);
    names
        .into_iter()
        .filter(|name| is_electron_package(name))
        .map(|name| format!("{path}: depends on \"{name}\" (Electron)"))
        .collect()
}

fn package_json_problems(path: &str, text: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return vec![format!(
            "{path}: not valid JSON, so it cannot be audited for Electron"
        )];
    };
    let mut names: Vec<String> = Vec::new();
    for section in [
        "dependencies",
        "devDependencies",
        "optionalDependencies",
        "peerDependencies",
    ] {
        if let Some(map) = value.get(section).and_then(serde_json::Value::as_object) {
            names.extend(map.keys().cloned());
        }
    }
    for section in ["bundledDependencies", "bundleDependencies"] {
        if let Some(list) = value.get(section).and_then(serde_json::Value::as_array) {
            names.extend(list.iter().filter_map(|v| v.as_str().map(str::to_owned)));
        }
    }
    names
        .into_iter()
        .filter(|name| is_electron_package(name))
        .map(|name| format!("{path}: depends on \"{name}\" (Electron)"))
        .collect()
}

/// Every package a lockfile names, direct or transitive (lockfile v2/v3 `packages` keys and v1 nested `dependencies`).
fn lockfile_names(value: &serde_json::Value) -> BTreeSet<String> {
    fn walk(dependencies: &serde_json::Map<String, serde_json::Value>, out: &mut BTreeSet<String>) {
        for (name, entry) in dependencies {
            out.insert(name.clone());
            if let Some(inner) = entry
                .get("dependencies")
                .and_then(serde_json::Value::as_object)
            {
                walk(inner, out);
            }
        }
    }
    let mut names = BTreeSet::new();
    if let Some(packages) = value.get("packages").and_then(serde_json::Value::as_object) {
        for key in packages.keys() {
            let name = key
                .rsplit_once("node_modules/")
                .map_or(key.as_str(), |(_, n)| n);
            if !name.is_empty() {
                names.insert(name.to_owned());
            }
        }
    }
    if let Some(dependencies) = value
        .get("dependencies")
        .and_then(serde_json::Value::as_object)
    {
        walk(dependencies, &mut names);
    }
    names
}

fn lockfile_problems(path: &str, text: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return vec![format!(
            "{path}: not valid JSON, so it cannot be audited for Electron"
        )];
    };
    lockfile_names(&value)
        .into_iter()
        .filter(|name| is_electron_package(name))
        .map(|name| format!("{path}: the lockfile contains \"{name}\" (Electron)"))
        .collect()
}

/// Looks like a Node.js runtime shipped as a file: `node`, `node.exe`, `nodejs`, `npm`, `npx`, `node-v22.1.0-win-x64`.
fn looks_like_node_runtime(entry: &str) -> bool {
    let name = entry.rsplit(['/', '\\']).next().unwrap_or(entry);
    let name = name.to_ascii_lowercase();
    let name = name.strip_suffix(".exe").unwrap_or(&name);
    matches!(name, "node" | "nodejs" | "npm" | "npx") || name.starts_with("node-v")
}

/// Strings a Tauri bundle ships next to the app: `bundle.externalBin` and `bundle.resources` (a list, or a map whose keys
/// and values are paths).
fn tauri_bundle_entries(value: &serde_json::Value) -> Vec<String> {
    let mut entries = Vec::new();
    let Some(bundle) = value.get("bundle") else {
        return entries;
    };
    if let Some(list) = bundle
        .get("externalBin")
        .and_then(serde_json::Value::as_array)
    {
        entries.extend(list.iter().filter_map(|v| v.as_str().map(str::to_owned)));
    }
    match bundle.get("resources") {
        Some(serde_json::Value::Array(list)) => {
            entries.extend(list.iter().filter_map(|v| v.as_str().map(str::to_owned)));
        }
        Some(serde_json::Value::Object(map)) => {
            for (from, to) in map {
                entries.push(from.clone());
                if let Some(to) = to.as_str() {
                    entries.push(to.to_owned());
                }
            }
        }
        _ => {}
    }
    entries
}

fn tauri_conf_problems(path: &str, text: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return vec![format!(
            "{path}: not valid JSON, so its bundle cannot be audited for a Node or Electron sidecar"
        )];
    };
    let mut problems = Vec::new();
    for entry in tauri_bundle_entries(&value) {
        if looks_like_node_runtime(&entry) {
            problems.push(format!(
                "{path}: the bundle ships \"{entry}\", a Node.js runtime"
            ));
        }
        if entry.to_ascii_lowercase().contains("electron") {
            problems.push(format!(
                "{path}: the bundle ships \"{entry}\", an Electron sidecar"
            ));
        }
    }
    problems
}

/// File names that mean Electron or a bundled Node runtime wherever they sit in the tree.
pub fn path_problems(path: &str) -> Vec<String> {
    let name = file_name(path).to_ascii_lowercase();
    let mut problems = Vec::new();
    if extension(path).eq_ignore_ascii_case("asar") {
        problems.push(format!("{path}: an asar archive (Electron)"));
    }
    if matches!(
        name.as_str(),
        "node.exe"
            | "electron.exe"
            | "electron-builder.yml"
            | "electron-builder.json"
            | "electron-builder.json5"
            | "forge.config.js"
    ) {
        problems.push(format!("{path}: a Node.js or Electron file is tracked"));
    }
    problems
}

/// Whether the API-name text scan reads this path.
pub fn is_token_scanned(path: &str) -> bool {
    if path.starts_with("xtask/") || path.starts_with("docs/") {
        return false;
    }
    if matches!(file_name(path), "package-lock.json" | "Cargo.lock") {
        return false;
    }
    SCANNED_EXTENSIONS
        .iter()
        .any(|ext| extension(path).eq_ignore_ascii_case(ext))
}

/// Electron-only API names in one file's text.
pub fn source_problems(path: &str, text: &str) -> Vec<String> {
    let mut problems: Vec<String> = API_TOKENS
        .iter()
        .filter(|token| text.contains(*token))
        .map(|token| format!("{path}: names the Electron API `{token}`"))
        .collect();
    if text.to_ascii_lowercase().contains("<webview") {
        problems.push(format!("{path}: uses the Electron `<webview>` tag"));
    }
    problems
}

/// Everything wrong with one file, whatever its kind. `text` is `None` for a binary or missing file.
pub fn file_problems(path: &str, text: Option<&str>) -> Vec<String> {
    let mut problems = path_problems(path);
    let Some(text) = text else {
        return problems;
    };
    match file_name(path) {
        "Cargo.toml" => problems.extend(cargo_problems(path, text)),
        "package.json" => problems.extend(package_json_problems(path, text)),
        "package-lock.json" => problems.extend(lockfile_problems(path, text)),
        name if name == "tauri.conf.json"
            || (name.starts_with("tauri.") && name.ends_with(".conf.json")) =>
        {
            problems.extend(tauri_conf_problems(path, text));
        }
        _ => {}
    }
    if is_token_scanned(path) {
        problems.extend(source_problems(path, text));
    }
    problems
}

/// Runs the audit over the repository at `root`. Returns the problems and how many files were read.
pub fn problems_in(root: &Path) -> Result<(Vec<String>, usize)> {
    let files = repository_files(root)?;
    let mut problems = Vec::new();
    let mut read = 0;
    for path in &files {
        let text = read_text(root, path);
        read += usize::from(text.is_some());
        problems.extend(file_problems(path, text.as_deref()));
    }
    Ok((problems, read))
}

/// The `electron-audit` gate.
pub fn check() -> Result<()> {
    let root = Path::new(".");
    crate::common::ensure_repository_root(root)?;
    let (problems, read) = problems_in(root)?;
    finish("electron-audit", &problems, || {
        format!(
            "{read} files read: Cargo and npm manifests, the npm lockfile, the Tauri bundle, and source files for Electron APIs; no Electron, asar or Node runtime"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn electron_packages_are_recognised_and_lookalikes_are_not() {
        for name in [
            "electron",
            "Electron",
            "electron-builder",
            "electron-updater",
            "@electron/asar",
            "@electron/get",
            "@electron-forge/cli",
            "asar",
        ] {
            assert!(is_electron_package(name), "{name} should be flagged");
        }
        for name in [
            "electron-to-chromium",
            "electronic-things",
            "react",
            "@tauri-apps/api",
            "tauri",
            "nodemon",
        ] {
            assert!(!is_electron_package(name), "{name} should not be flagged");
        }
    }

    #[test]
    fn cargo_manifests_are_checked_in_every_dependency_table() {
        let clean = "[package]\nname = \"a\"\n[dependencies]\nserde = \"1\"\n";
        assert!(cargo_problems("Cargo.toml", clean).is_empty());
        for manifest in [
            "[dependencies]\nelectron = \"1\"\n",
            "[dev-dependencies]\nelectron-builder = \"1\"\n",
            "[build-dependencies.electron]\nversion = \"1\"\n",
            "[workspace.dependencies]\nelectron = \"1\"\n",
            "[target.'cfg(windows)'.dependencies]\nelectron = \"1\"\n",
            "[dependencies]\nshiny = { package = \"electron\", version = \"1\" }\n",
        ] {
            assert_eq!(
                cargo_problems("Cargo.toml", manifest).len(),
                1,
                "{manifest}"
            );
        }
        assert_eq!(cargo_problems("Cargo.toml", "= not toml").len(), 1);
    }

    #[test]
    fn package_json_sections_are_checked() {
        let clean = r#"{"dependencies":{"react":"19"},"devDependencies":{"vite":"6"}}"#;
        assert!(package_json_problems("package.json", clean).is_empty());
        for json in [
            r#"{"dependencies":{"electron":"30"}}"#,
            r#"{"devDependencies":{"electron-builder":"24"}}"#,
            r#"{"optionalDependencies":{"@electron/asar":"3"}}"#,
            r#"{"peerDependencies":{"@electron-forge/cli":"7"}}"#,
            r#"{"bundledDependencies":["asar"]}"#,
        ] {
            assert_eq!(
                package_json_problems("package.json", json).len(),
                1,
                "{json}"
            );
        }
        assert_eq!(package_json_problems("package.json", "{").len(), 1);
    }

    #[test]
    fn lockfiles_are_checked_for_transitive_packages() {
        let v3 = r#"{"packages":{"":{"name":"x"},"node_modules/react":{},"node_modules/a/node_modules/@electron/get":{},"node_modules/electron-to-chromium":{}}}"#;
        let problems = lockfile_problems("package-lock.json", v3);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("@electron/get"));

        let v1 = r#"{"dependencies":{"a":{"dependencies":{"electron":{"version":"1"}}}}}"#;
        assert_eq!(lockfile_problems("package-lock.json", v1).len(), 1);

        let clean = r#"{"packages":{"":{},"node_modules/react":{}}}"#;
        assert!(lockfile_problems("package-lock.json", clean).is_empty());
    }

    #[test]
    fn electron_api_names_are_found_in_source() {
        for token in API_TOKENS {
            let text = format!("const x = {{ {token}: true }};");
            assert_eq!(source_problems("ui/a.ts", &text).len(), 1, "{token}");
        }
        assert_eq!(source_problems("ui/a.html", "<WebView src=x>").len(), 1);
        assert_eq!(source_problems("ui/a.html", "<webview>").len(), 1);
        assert!(
            source_problems("ui/a.ts", "import { getCurrentWebviewWindow } from 'x'").is_empty()
        );
        assert!(source_problems("a.rs", "fn browser_window_size() {}").is_empty());
    }

    #[test]
    fn markdown_docs_and_the_guard_itself_are_not_scanned() {
        assert!(!is_token_scanned("SUL-ARCHITECTURE.md"));
        assert!(!is_token_scanned("docs/decisions/0001-stack.md"));
        assert!(!is_token_scanned("docs/anything.json"));
        assert!(!is_token_scanned("xtask/src/electron.rs"));
        assert!(!is_token_scanned("package-lock.json"));
        assert!(is_token_scanned("ui/src/main.tsx"));
        assert!(is_token_scanned("crates/sul-core/src/lib.rs"));
        assert!(is_token_scanned("src-tauri/tauri.conf.json"));
        assert!(is_token_scanned(".github/workflows/ci.yml"));
    }

    #[test]
    fn asar_archives_and_node_executables_are_refused_anywhere() {
        for path in [
            "resources/app.asar",
            "bin/node.exe",
            "tools/Electron.exe",
            "electron-builder.yml",
            "forge.config.js",
        ] {
            assert_eq!(path_problems(path).len(), 1, "{path}");
        }
        assert!(path_problems("crates/a/src/lib.rs").is_empty());
        assert!(path_problems("docs/node-notes.md").is_empty());
    }

    #[test]
    fn a_tauri_bundle_may_not_ship_node_or_electron() {
        let clean = r#"{"bundle":{"active":true,"icon":["icons/32x32.png"],"externalBin":["binaries/sul-helper"],"resources":["assets/*"]}}"#;
        assert!(tauri_conf_problems("tauri.conf.json", clean).is_empty());
        assert!(tauri_conf_problems("tauri.conf.json", r#"{"app":{}}"#).is_empty());

        let node = r#"{"bundle":{"externalBin":["binaries/node"]}}"#;
        assert_eq!(tauri_conf_problems("tauri.conf.json", node).len(), 1);
        let archive = r#"{"bundle":{"resources":{"vendor/node-v22.1.0-win-x64.zip":"node/"}}}"#;
        assert_eq!(tauri_conf_problems("tauri.conf.json", archive).len(), 1);
        let electron = r#"{"bundle":{"externalBin":["sidecars/electron-host"]}}"#;
        assert_eq!(tauri_conf_problems("tauri.conf.json", electron).len(), 1);
        assert_eq!(tauri_conf_problems("tauri.conf.json", "{").len(), 1);
    }

    #[test]
    fn each_kind_of_file_is_routed_to_its_check() {
        let bad_cargo = "[dependencies]\nelectron = \"1\"\n";
        assert_eq!(
            file_problems("crates/a/Cargo.toml", Some(bad_cargo)).len(),
            1
        );
        assert_eq!(
            file_problems(
                "ui/package.json",
                Some(r#"{"dependencies":{"electron":"1"}}"#)
            )
            .len(),
            1
        );
        assert_eq!(
            file_problems(
                "src-tauri/tauri.conf.json",
                Some(r#"{"bundle":{"externalBin":["node"]}}"#)
            )
            .len(),
            1
        );
        assert_eq!(
            file_problems("ui/a.ts", Some("new BrowserWindow()")).len(),
            1
        );
        // A binary (no text) is judged by its name alone.
        assert_eq!(file_problems("bin/node.exe", None).len(), 1);
        assert!(file_problems("bin/tool.dll", None).is_empty());
    }
}
