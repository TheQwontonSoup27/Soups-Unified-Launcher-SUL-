//! Network-surface guard for `cargo xtask ac-regression` (Ch. 16 §16.15.14, "Network tripwires").
//!
//! The spec's rule: every network surface is declared, and nothing else may reach a network. In this workspace that is
//! checked three ways, for the Windows target and normal dependencies only (build scripts and test-only crates are not in
//! the app):
//!
//! 1. A crate on [`NETWORK_CRATES`] (HTTP, sockets, DNS, TLS, WebSockets) may only enter a package's dependency graph
//!    through a package on [`DECLARED_NETWORK_PACKAGES`]. The graph is read with those packages pruned, so `sul.exe` may
//!    depend on `sul-net` (Ch. 1 §2.6) without every other package being allowed the same crates.
//! 2. No package outside the declared ones turns on tokio's `net` feature or a Windows networking API feature, which add
//!    networking without adding a crate.
//! 3. No source file outside the declared packages names a std, tokio or HTTP-client networking API.
//!
//! What this is NOT: a proof that nothing in the app touches a network. It does not look inside Microsoft's WebView2, the
//! Rust standard library or Steam, and the source check reads text, not syntax. It replaces the V5.1 build's
//! `check-network-crates.mjs`, whose rule was stricter (no network crate anywhere, because no network code was allowed).

use miette::{Context, IntoDiagnostic, Result, bail};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const TARGET: &str = "x86_64-pc-windows-msvc";

/// A package that may carry network-capable crates, with the declared surface that justifies it.
pub struct DeclaredPackage {
    pub package: &'static str,
    pub surfaces: &'static str,
}

/// Packages allowed to compile network crates. Add a row only when the spec declares the surface (Ch. 16 §16.15.14) and
/// the package exists. The other declared surfaces (hive-mind fetch, auto-updater, Sentry, GEV feeds, the Phase 3 list)
/// have no package yet. The Steam Web API and a Discord webhook were added to the spec's list by the owner on 2026-11-21
/// (D-149) and also live behind `sul-net`.
pub const DECLARED_NETWORK_PACKAGES: &[DeclaredPackage] = &[DeclaredPackage {
    package: "sul-net",
    surfaces: "RustXNG MCP (localhost), the GitHub search API, the Steam Web API and a Discord webhook (Ch. 1 §2.6, Ch. 16 §16.15.14, D-149)",
}];

/// Crates whose job is HTTP, sockets, DNS, TLS or WebSockets. A match is a build that can reach a network.
pub const NETWORK_CRATES: &[&str] = &[
    "reqwest",
    "hyper",
    "hyper-util",
    "hyper-tls",
    "hyper-rustls",
    "h2",
    "http-body-util",
    "tower-http",
    "mio",
    "socket2",
    "net2",
    "async-std",
    "smol",
    "async-net",
    "tokio-util-net",
    "native-tls",
    "openssl",
    "openssl-sys",
    "rustls",
    "rustls-pemfile",
    "tokio-rustls",
    "tokio-native-tls",
    "schannel",
    "ureq",
    "curl",
    "curl-sys",
    "isahc",
    "attohttpc",
    "surf",
    "minreq",
    "http-client",
    "awc",
    "actix-http",
    "tungstenite",
    "tokio-tungstenite",
    "websocket",
    "quinn",
    "quinn-proto",
    "hickory-resolver",
    "trust-dns-resolver",
    "c-ares",
    "dns-lookup",
];

/// Text that names a networking API in source outside the declared packages.
const FORBIDDEN_SOURCE_PATTERNS: &[&str] = &[
    "std::net",
    "TcpStream",
    "TcpListener",
    "UdpSocket",
    "ToSocketAddrs",
    "tokio::net",
    "reqwest::",
    "hyper::",
    "ureq::",
    "curl::",
];

/// A workspace member that ships in the app (everything but `xtask`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub name: String,
    pub has_features: bool,
}

/// How cargo is asked for a dependency tree: package, then extra arguments. Tests pass a fake.
pub type TreeSource<'a> = &'a dyn Fn(&str, &[String]) -> Result<String>;

fn is_declared(package: &str) -> bool {
    DECLARED_NETWORK_PACKAGES
        .iter()
        .any(|d| d.package == package)
}

/// Crate names in the output of `cargo tree -e normal,features --prefix none` (package and feature lines both start with
/// the crate name).
pub fn package_names(tree_text: &str) -> BTreeSet<String> {
    tree_text
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_owned)
        .collect()
}

/// The declared network packages that appear in a graph, so they can be pruned from it.
pub fn declared_packages_in(names: &BTreeSet<String>) -> Vec<&'static str> {
    DECLARED_NETWORK_PACKAGES
        .iter()
        .map(|d| d.package)
        .filter(|p| names.contains(*p))
        .collect()
}

/// `tokio feature "net"` -> ("tokio", "net"). `None` for a package line.
fn crate_feature(line: &str) -> Option<(&str, &str)> {
    let (krate, rest) = line.trim().split_once(" feature \"")?;
    Some((krate, rest.strip_suffix('"')?))
}

fn is_windows_networking_feature(feature: &str) -> bool {
    feature.starts_with("Win32_Networking")
        || feature.starts_with("Win32_NetworkManagement")
        || feature == "Win32_System_Com_Urlmon"
}

/// Everything wrong with one dependency tree, as readable lines. Empty means nothing was found.
pub fn find_problems(label: &str, tree_text: &str) -> Vec<String> {
    let names = package_names(tree_text);
    let mut problems = BTreeSet::new();
    for krate in NETWORK_CRATES {
        if names.contains(*krate) {
            problems.insert(format!(
                "{label}: the crate \"{krate}\" is compiled in outside a declared network package"
            ));
        }
    }
    for (krate, feature) in tree_text.lines().filter_map(crate_feature) {
        if krate == "tokio" && feature == "net" {
            problems.insert(format!("{label}: tokio's \"net\" feature is on"));
        }
        if is_windows_networking_feature(feature) {
            problems.insert(format!(
                "{label}: the Windows networking feature \"{feature}\" is on ({krate})"
            ));
        }
    }
    problems.into_iter().collect()
}

/// Checks each member (default features, and all features when it has any). Returns the problems and a summary per build.
pub fn check_members(
    members: &[Member],
    tree: TreeSource<'_>,
) -> Result<(Vec<String>, Vec<String>)> {
    let mut problems = Vec::new();
    let mut summary = Vec::new();
    for member in members {
        if is_declared(&member.name) {
            summary.push(format!(
                "{}: declared network package, not restricted",
                member.name
            ));
            continue;
        }
        let mut variants: Vec<(String, Vec<String>)> = vec![(member.name.clone(), vec![])];
        if member.has_features {
            variants.push((
                format!("{} (all features)", member.name),
                vec!["--all-features".to_owned()],
            ));
        }
        for (label, flags) in variants {
            let mut args = flags;
            args.extend(["-e", "normal,features", "--prefix", "none"].map(str::to_owned));
            let full = tree(&member.name, &args)?;
            let reached = declared_packages_in(&package_names(&full));
            let text = if reached.is_empty() {
                full
            } else {
                let mut pruned = args.clone();
                for package in &reached {
                    pruned.extend(["--prune".to_owned(), (*package).to_owned()]);
                }
                tree(&member.name, &pruned)?
            };
            problems.extend(find_problems(&label, &text));
            let via = if reached.is_empty() {
                String::new()
            } else {
                format!(", reaches declared {}", reached.join(", "))
            };
            summary.push(format!(
                "{label}: {} crates{via}",
                package_names(&text).len()
            ));
        }
    }
    Ok((problems, summary))
}

/// One problem line per forbidden API named in one source file.
pub fn scan_source(path: &str, text: &str) -> Vec<String> {
    FORBIDDEN_SOURCE_PATTERNS
        .iter()
        .filter(|pattern| text.contains(*pattern))
        .map(|pattern| format!("{path}: names the networking API `{pattern}`"))
        .collect()
}

fn in_declared_package(path: &Path) -> bool {
    path.components()
        .any(|c| c.as_os_str().to_str().is_some_and(is_declared))
}

fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)
        .into_diagnostic()
        .with_context(|| format!("failed to read {}", dir.display()))?
    {
        let path = entry.into_diagnostic()?.path();
        if path.is_dir() {
            collect_rust_files(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// Production Rust sources outside the declared packages: `crates/*/src` and `src-tauri/src`.
pub fn source_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut roots = vec![root.join("src-tauri").join("src")];
    let crates = root.join("crates");
    if crates.is_dir() {
        for entry in std::fs::read_dir(&crates).into_diagnostic()? {
            let src = entry.into_diagnostic()?.path().join("src");
            if src.is_dir() {
                roots.push(src);
            }
        }
    }
    let mut files = Vec::new();
    for dir in roots.into_iter().filter(|d| d.is_dir()) {
        collect_rust_files(&dir, &mut files)?;
    }
    files.retain(|f| !in_declared_package(f));
    files.sort();
    Ok(files)
}

fn run_cargo(args: &[String]) -> Result<String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let out = Command::new(cargo)
        .args(args)
        .output()
        .into_diagnostic()
        .context("failed to start cargo")?;
    if !out.status.success() {
        bail!(
            "cargo {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn cargo_tree(package: &str, extra: &[String]) -> Result<String> {
    let mut args: Vec<String> = ["tree", "-p", package, "--target", TARGET]
        .map(str::to_owned)
        .into();
    args.extend_from_slice(extra);
    run_cargo(&args)
}

/// Workspace members that ship in the app, read from `cargo metadata`.
pub fn workspace_members() -> Result<Vec<Member>> {
    let raw = run_cargo(&["metadata", "--no-deps", "--format-version", "1"].map(str::to_owned))?;
    let doc: serde_json::Value = serde_json::from_str(&raw)
        .into_diagnostic()
        .context("cargo metadata did not return JSON")?;
    let packages = doc["packages"]
        .as_array()
        .ok_or_else(|| miette::miette!("cargo metadata has no `packages` list"))?;
    let mut members: Vec<Member> = packages
        .iter()
        .filter_map(|p| {
            let name = p["name"].as_str()?;
            (name != "xtask").then(|| Member {
                name: name.to_owned(),
                has_features: p["features"].as_object().is_some_and(|f| !f.is_empty()),
            })
        })
        .collect();
    members.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(members)
}

/// Runs the whole guard from the repository root.
pub fn check() -> Result<()> {
    let members = workspace_members()?;
    let (mut problems, summary) = check_members(&members, &cargo_tree)?;

    let files = source_files(Path::new("."))?;
    if files.is_empty() {
        bail!(
            "no Rust sources found under crates/*/src or src-tauri/src: run from the repository root"
        );
    }
    for file in &files {
        let text = std::fs::read_to_string(file)
            .into_diagnostic()
            .with_context(|| format!("failed to read {}", file.display()))?;
        let shown = file.strip_prefix(".").unwrap_or(file).display().to_string();
        problems.extend(scan_source(&shown, &text));
    }

    println!(
        "[xtask] network-surfaces: Windows target {TARGET}, normal dependencies. {}. {} source files scanned.",
        summary.join("; "),
        files.len()
    );
    if problems.is_empty() {
        let declared: Vec<String> = DECLARED_NETWORK_PACKAGES
            .iter()
            .map(|d| format!("{} for {}", d.package, d.surfaces))
            .collect();
        println!(
            "[xtask] network-surfaces: OK (network crates allowed only through: {})",
            declared.join("; ")
        );
        return Ok(());
    }
    for problem in &problems {
        eprintln!("PROBLEM {problem}");
    }
    // Not `bail!` here: it expands to `return ...;`, and a newer rustc than the one in the Linux container rejects that
    // trailing semicolon when the macro is the last expression of a function (Windows CI, 2026-11-20).
    Err(miette::miette!(
        "{} network-surface problem(s)",
        problems.len()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const WINDOWS_TREE: &str = "sul-tauri v0.1.0 (C:\\repo\\src-tauri)\r\ntauri v2.11.6\r\ntokio v1.54.1\r\nhttp v1.5.0\r\nurl v2.5.8\r\nwindows v0.61.3\r\nwry v0.55.1";
    const ANDROID_TREE: &str = "sul-tauri v0.1.0\ntauri v2.11.6\ntokio v1.54.1\nreqwest v0.13.5\nhyper v1.12.0\nhyper-util v0.1.21\nmio v1.2.4\nsocket2 v0.6.5";

    fn member(name: &str, has_features: bool) -> Member {
        Member {
            name: name.into(),
            has_features,
        }
    }

    #[test]
    fn deny_list_covers_the_common_crates_with_no_duplicates() {
        for c in [
            "reqwest",
            "hyper",
            "mio",
            "socket2",
            "ureq",
            "curl",
            "native-tls",
            "rustls",
            "openssl",
            "tungstenite",
            "h2",
            "quinn",
        ] {
            assert!(NETWORK_CRATES.contains(&c), "{c}");
        }
        assert_eq!(NETWORK_CRATES.len(), 42);
        let unique: BTreeSet<_> = NETWORK_CRATES.iter().collect();
        assert_eq!(unique.len(), NETWORK_CRATES.len());
    }

    #[test]
    fn sul_net_is_the_only_declared_package() {
        let declared: Vec<_> = DECLARED_NETWORK_PACKAGES
            .iter()
            .map(|d| d.package)
            .collect();
        assert_eq!(declared, ["sul-net"]);
    }

    #[test]
    fn package_names_read_either_line_ending_and_feature_lines() {
        let names: Vec<_> = package_names(WINDOWS_TREE).into_iter().collect();
        assert_eq!(
            names,
            [
                "http",
                "sul-tauri",
                "tauri",
                "tokio",
                "url",
                "windows",
                "wry"
            ]
        );
        assert!(package_names("").is_empty());
        let with_features = package_names("  tokio v1.0.0 (*)\n\ntokio feature \"rt\"\n");
        assert_eq!(with_features.into_iter().collect::<Vec<_>>(), ["tokio"]);
    }

    #[test]
    fn a_windows_graph_is_clean_and_a_mobile_graph_is_not() {
        assert!(find_problems("sul-tauri", WINDOWS_TREE).is_empty());
        let problems = find_problems("sul-tauri", ANDROID_TREE);
        for krate in ["reqwest", "hyper", "hyper-util", "mio", "socket2"] {
            assert!(
                problems.iter().any(|p| p.contains(&format!("\"{krate}\""))),
                "{krate}: {problems:?}"
            );
        }
        assert_eq!(problems.len(), 5);
    }

    #[test]
    fn tokio_net_and_windows_networking_features_are_found() {
        let clean = "tokio feature \"rt\"\ntokio feature \"sync\"\nwindows feature \"Win32_UI\"\nwindows feature \"Win32_System_Registry\"";
        assert!(find_problems("x", clean).is_empty());
        let net = find_problems("x", "tokio feature \"net\"");
        assert_eq!(net, ["x: tokio's \"net\" feature is on"]);
        for feature in [
            "Win32_Networking_WinHttp",
            "Win32_Networking_WinSock",
            "Win32_NetworkManagement_Dns",
            "Win32_System_Com_Urlmon",
        ] {
            let p = find_problems("x", &format!("windows-sys feature \"{feature}\""));
            assert_eq!(p.len(), 1, "{feature}");
            assert!(p[0].contains(feature) && p[0].contains("windows-sys"));
        }
        // A different crate's `net` feature is not tokio's.
        assert!(find_problems("x", "tokio-stream feature \"net\"").is_empty());
    }

    #[test]
    fn repeated_lines_give_one_problem() {
        let text = "tokio feature \"net\"\ntokio feature \"net\"\nmio v1.0\nmio v1.0";
        assert_eq!(find_problems("x", text).len(), 2);
    }

    #[test]
    fn declared_packages_are_found_in_a_graph() {
        assert_eq!(
            declared_packages_in(&package_names("sul-tauri v1\nsul-net v1\nreqwest v1")),
            ["sul-net"]
        );
        assert!(declared_packages_in(&package_names(WINDOWS_TREE)).is_empty());
    }

    #[test]
    fn a_member_that_reaches_sul_net_is_checked_with_it_pruned() {
        let calls = RefCell::new(Vec::new());
        let fake = |pkg: &str, args: &[String]| -> Result<String> {
            calls.borrow_mut().push((pkg.to_owned(), args.to_vec()));
            if args.iter().any(|a| a == "--prune") {
                Ok(WINDOWS_TREE.to_owned())
            } else {
                Ok(format!(
                    "{WINDOWS_TREE}\r\nsul-net v0.1.0\r\nreqwest v0.13.5\r\nhyper v1.12.0"
                ))
            }
        };
        let (problems, summary) = check_members(&[member("sul-tauri", false)], &fake).unwrap();
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(summary.len(), 1);
        assert!(
            summary[0].contains("reaches declared sul-net"),
            "{summary:?}"
        );
        let calls = calls.borrow();
        assert_eq!(calls.len(), 2);
        assert!(calls[1].1.windows(2).any(|w| w == ["--prune", "sul-net"]));
    }

    #[test]
    fn a_network_crate_outside_sul_net_is_a_problem_even_when_sul_net_is_present() {
        let fake = |_: &str, args: &[String]| -> Result<String> {
            // After pruning, `reqwest` is still there: something other than sul-net pulls it in.
            let pruned = if args.iter().any(|a| a == "--prune") {
                "\r\nreqwest v0.13.5"
            } else {
                "\r\nsul-net v0.1.0"
            };
            Ok(format!("{WINDOWS_TREE}{pruned}"))
        };
        let (problems, _) = check_members(&[member("sul-core", false)], &fake).unwrap();
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("\"reqwest\"") && problems[0].starts_with("sul-core:"));
    }

    #[test]
    fn a_declared_package_is_not_restricted_and_not_run() {
        let fake = |_: &str, _: &[String]| -> Result<String> { panic!("must not be asked for") };
        let (problems, summary) = check_members(&[member("sul-net", true)], &fake).unwrap();
        assert!(problems.is_empty());
        assert!(summary[0].contains("declared network package"));
    }

    #[test]
    fn a_member_with_features_is_checked_again_with_all_of_them() {
        let seen = RefCell::new(Vec::new());
        let fake = |_: &str, args: &[String]| -> Result<String> {
            seen.borrow_mut()
                .push(args.iter().any(|a| a == "--all-features"));
            Ok(WINDOWS_TREE.to_owned())
        };
        let (_, summary) = check_members(&[member("sul-tauri", true)], &fake).unwrap();
        assert_eq!(*seen.borrow(), [false, true]);
        assert!(summary[1].starts_with("sul-tauri (all features):"));
    }

    #[test]
    fn a_failing_cargo_call_is_an_error_not_a_pass() {
        let fake =
            |_: &str, _: &[String]| -> Result<String> { Err(miette::miette!("no such package")) };
        assert!(check_members(&[member("sul-core", false)], &fake).is_err());
    }

    #[test]
    fn source_scan_finds_networking_apis() {
        assert!(scan_source("a.rs", "use std::fs;\nuse tokio::sync::Mutex;\n").is_empty());
        let found = scan_source("b.rs", "use std::net::TcpStream;");
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|f| f.starts_with("b.rs:")));
        assert_eq!(
            scan_source("c.rs", "let c = reqwest::Client::new();").len(),
            1
        );
    }

    #[test]
    fn declared_package_sources_are_exempt() {
        assert!(in_declared_package(Path::new("crates/sul-net/src/lib.rs")));
        assert!(!in_declared_package(Path::new(
            "crates/sul-core/src/lib.rs"
        )));
        assert!(!in_declared_package(Path::new("src-tauri/src/lib.rs")));
    }
}
