//! Crate-boundary guards for `cargo xtask ac-regression` (SUL-LEDGER/guards.md: "must not import an HTTP client", "no
//! wasapi import outside sul-audio", "must not import ReadProcessMemory").
//!
//! Each rule names one crate, one kind of boundary and the guard sentence it comes from. A rule is **armed** when that crate
//! is a workspace member and **pending** when it is not, so the table can list the whole ledger today and each rule starts
//! to bite the day its crate is added. The gate prints how many rules were armed and which crates are still waited for, so
//! a green run never reads as "every guard in the ledger holds".
//!
//! - [`Kind::HttpClient`]: the crate's Windows dependency graph (normal dependencies) holds no network crate and does not
//!   reach a declared network package (`sul-net`). The network guard already keeps network crates out of everything but
//!   `sul-net`; this adds "and does not depend on `sul-net` either" for the crates the ledger names.
//! - [`Kind::Wasapi`]: the crate has no direct normal dependency on `wasapi` (the ledger says "directly": a crate may
//!   reach WASAPI through `sul-audio`).
//! - [`Kind::ProcessMemory`]: no source file of the crate names `ReadProcessMemory` or `NtReadVirtualMemory` (the same
//!   capability through the native API; the ledger also says "or any game memory access"). Lines that start with `//` are
//!   skipped so a crate can say why it never does this.
//!
//! What this is NOT: the ledger's other guards. Those that name a module inside a crate ("sul-tts's Higgs Audio path", "ReTrace
//! paths of sul-ai-native"), a config key, a call order ("does not bypass `try_push`") or a runtime property ("runs at
//! BELOW_NORMAL") need syntax-level analysis, a shipped `config.toml` or a running process, and are not implemented. A text
//! scan can be fooled by a name built at run time.

use crate::common::{ensure_repository_root, finish, read_text, repository_files};
use crate::network;
use miette::{Context, IntoDiagnostic, Result};
use std::collections::BTreeSet;
use std::path::Path;

/// What a rule forbids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    HttpClient,
    Wasapi,
    ProcessMemory,
}

impl Kind {
    /// Every kind, in the order the gate reports them.
    pub const ALL: [Kind; 3] = [Kind::HttpClient, Kind::Wasapi, Kind::ProcessMemory];

    /// The words the gate uses for this kind in its own output.
    pub fn label(self) -> &'static str {
        match self {
            Kind::HttpClient => "HTTP client",
            Kind::Wasapi => "direct wasapi",
            Kind::ProcessMemory => "process-memory access",
        }
    }
}

/// One guard from guards.md applied to one crate.
#[derive(Debug, Clone, Copy)]
pub struct BoundaryRule {
    pub package: &'static str,
    pub kind: Kind,
    /// The ledger's own words, quoted in every problem the rule reports.
    pub guard: &'static str,
}

const fn rule(kind: Kind, package: &'static str, guard: &'static str) -> BoundaryRule {
    BoundaryRule {
        package,
        kind,
        guard,
    }
}

/// Names that give a source file access to another process's memory.
pub const PROCESS_MEMORY_TOKENS: &[&str] = &["ReadProcessMemory", "NtReadVirtualMemory"];

/// Every crate-scoped dependency or import guard in guards.md. Edit this table only to follow the ledger.
pub const RULES: &[BoundaryRule] = &[
    // "must not import an HTTP client"
    rule(
        Kind::HttpClient,
        "sul-wire",
        "sul-wire does not import an HTTP client",
    ),
    rule(
        Kind::HttpClient,
        "sul-metrics",
        "sul-metrics must not import an HTTP client",
    ),
    rule(
        Kind::HttpClient,
        "sul-crash",
        "sul-crash must not import an HTTP client",
    ),
    rule(
        Kind::HttpClient,
        "sul-vault-core",
        "sul-vault-core must not import any HTTP client",
    ),
    rule(
        Kind::HttpClient,
        "sul-agent",
        "sul-agent does not import an HTTP client",
    ),
    rule(
        Kind::HttpClient,
        "sul-audio",
        "sul-audio must not import an HTTP client",
    ),
    rule(
        Kind::HttpClient,
        "sul-spec-draft",
        "sul-spec-draft must not import an HTTP client",
    ),
    rule(
        Kind::HttpClient,
        "sul-music-engine",
        "sul-music-engine must not import an HTTP client",
    ),
    rule(
        Kind::HttpClient,
        "sul-broadcast-audio",
        "sul-broadcast-audio must not import an HTTP client",
    ),
    rule(
        Kind::HttpClient,
        "sul-voicestudio",
        "sul-voicestudio must not import an HTTP client",
    ),
    rule(
        Kind::HttpClient,
        "sul-csi",
        "sul-csi must not import an HTTP client (local USB/LAN only)",
    ),
    rule(
        Kind::HttpClient,
        "sul-bci",
        "sul-bci must not import an HTTP client (USB only)",
    ),
    // "must not import wasapi" (directly)
    rule(
        Kind::Wasapi,
        "sul-dsp",
        "No wasapi import in sul-dsp/sul-voice outside sul-audio",
    ),
    rule(
        Kind::Wasapi,
        "sul-voice",
        "No wasapi import in sul-dsp/sul-voice outside sul-audio",
    ),
    rule(Kind::Wasapi, "sul-net", "sul-net must not import wasapi"),
    rule(
        Kind::Wasapi,
        "sul-agent",
        "No wasapi import in sul-net/sul-agent outside sul-audio",
    ),
    rule(
        Kind::Wasapi,
        "sul-audio-governor",
        "sul-audio-governor does not import wasapi directly (uses sul-audio)",
    ),
    rule(
        Kind::Wasapi,
        "sul-voice-fx",
        "sul-voice-fx must not import wasapi directly (uses sul-audio's WASAPI layer)",
    ),
    rule(Kind::Wasapi, "sul-gb", "sul-gb must not import wasapi"),
    rule(
        Kind::Wasapi,
        "sul-kv-compress",
        "sul-kv-compress must not import wasapi",
    ),
    rule(
        Kind::Wasapi,
        "sul-toolprm",
        "sul-toolprm must not import wasapi",
    ),
    // "must not import ReadProcessMemory"
    rule(
        Kind::ProcessMemory,
        "sul-fg",
        "sul-fg does not import ReadProcessMemory",
    ),
    rule(
        Kind::ProcessMemory,
        "sul-broadcast-video",
        "sul-broadcast-video must not import ReadProcessMemory",
    ),
    rule(
        Kind::ProcessMemory,
        "sul-csi",
        "sul-csi must not import ReadProcessMemory or any game memory access",
    ),
    rule(
        Kind::ProcessMemory,
        "sul-bci",
        "sul-bci must not import ReadProcessMemory",
    ),
    rule(
        Kind::ProcessMemory,
        "sul-gev",
        "sul-gev must not import ReadProcessMemory",
    ),
    rule(
        Kind::ProcessMemory,
        "sul-gb",
        "sul-gb must not import ReadProcessMemory or any game memory access",
    ),
    rule(
        Kind::ProcessMemory,
        "sul-music",
        "sul-music must not import ReadProcessMemory",
    ),
    rule(
        Kind::ProcessMemory,
        "sul-toolprm",
        "sul-toolprm must not import ReadProcessMemory",
    ),
];

/// A workspace member as the rules see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    /// Relative to the repository root, forward slashes, no trailing slash.
    pub directory: String,
    /// Normal (shipped) direct dependencies, by crate name, optional ones included.
    pub direct: BTreeSet<String>,
}

/// Names of every crate in a package's Windows normal-dependency graph.
pub type GraphSource<'a> = &'a dyn Fn(&str) -> Result<BTreeSet<String>>;
/// `(path, text)` of each Rust source file of a package.
pub type SourceReader<'a> = &'a dyn Fn(&Package) -> Vec<(String, String)>;

/// The outcome of running the table.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Evaluation {
    pub problems: Vec<String>,
    /// Rules whose crate is a workspace member.
    pub armed: usize,
    /// The kinds of rule that have at least one armed rule. A kind missing here was not checked at all.
    pub armed_kinds: BTreeSet<Kind>,
    /// Crates named by a rule that are not workspace members yet.
    pub pending: Vec<&'static str>,
}

fn code_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .filter(|line| !line.trim_start().starts_with("//"))
}

fn rule_problems(
    rule: &BoundaryRule,
    package: &Package,
    graph: GraphSource<'_>,
    sources: SourceReader<'_>,
) -> Result<Vec<String>> {
    let cite = |what: String| format!("{}: {what} (guards.md: {})", rule.package, rule.guard);
    let mut problems = Vec::new();
    match rule.kind {
        Kind::HttpClient => {
            let names = graph(&package.name)?;
            let declared = network::DECLARED_NETWORK_PACKAGES.iter().map(|d| d.package);
            for krate in network::NETWORK_CRATES.iter().copied().chain(declared) {
                if names.contains(krate) {
                    problems.push(cite(format!(
                        "its Windows dependency graph reaches \"{krate}\""
                    )));
                }
            }
        }
        Kind::Wasapi => {
            if package.direct.contains("wasapi") {
                problems.push(cite("it depends on the \"wasapi\" crate".to_owned()));
            }
        }
        Kind::ProcessMemory => {
            for (path, text) in sources(package) {
                for token in PROCESS_MEMORY_TOKENS {
                    if code_lines(&text).any(|line| line.contains(token)) {
                        problems.push(cite(format!("{path} names `{token}`")));
                    }
                }
            }
        }
    }
    Ok(problems)
}

/// Runs every rule whose crate exists and lists the crates that do not.
pub fn evaluate(
    rules: &[BoundaryRule],
    packages: &[Package],
    graph: GraphSource<'_>,
    sources: SourceReader<'_>,
) -> Result<Evaluation> {
    let mut evaluation = Evaluation::default();
    let mut pending = BTreeSet::new();
    for rule in rules {
        let Some(package) = packages.iter().find(|p| p.name == rule.package) else {
            pending.insert(rule.package);
            continue;
        };
        evaluation.armed += 1;
        evaluation.armed_kinds.insert(rule.kind);
        evaluation
            .problems
            .extend(rule_problems(rule, package, graph, sources)?);
    }
    evaluation.pending = pending.into_iter().collect();
    Ok(evaluation)
}

fn normalise(path: &str) -> String {
    path.replace('\\', "/")
}

fn relative_directory(root: &str, manifest: &str) -> String {
    let directory = manifest.strip_suffix("/Cargo.toml").unwrap_or(manifest);
    let relative = directory.strip_prefix(root).unwrap_or(directory);
    relative.trim_start_matches('/').to_owned()
}

/// Workspace members from the text of `cargo metadata --no-deps --format-version 1`.
pub fn packages_from_metadata(raw: &str) -> Result<Vec<Package>> {
    let doc: serde_json::Value = serde_json::from_str(raw)
        .into_diagnostic()
        .context("cargo metadata did not return JSON")?;
    let root = normalise(doc["workspace_root"].as_str().unwrap_or(""));
    let listed = doc["packages"]
        .as_array()
        .ok_or_else(|| miette::miette!("cargo metadata has no `packages` list"))?;
    let mut packages: Vec<Package> = listed
        .iter()
        .filter_map(|p| {
            let name = p["name"].as_str()?.to_owned();
            let manifest = normalise(p["manifest_path"].as_str().unwrap_or(""));
            let direct = p["dependencies"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|d| d["kind"].is_null())
                .filter_map(|d| d["name"].as_str().map(str::to_owned))
                .collect();
            Some(Package {
                name,
                directory: relative_directory(&root, &manifest),
                direct,
            })
        })
        .collect();
    packages.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(packages)
}

/// The `crate-boundaries` guard, run inside `ac-regression`.
pub fn check() -> Result<()> {
    let root = Path::new(".");
    ensure_repository_root(root)?;
    let raw =
        network::run_cargo(&["metadata", "--no-deps", "--format-version", "1"].map(str::to_owned))?;
    let packages = packages_from_metadata(&raw)?;
    let files = repository_files(root)?;

    let graph = |name: &str| -> Result<BTreeSet<String>> {
        let args = ["-e", "normal", "--prefix", "none"].map(str::to_owned);
        Ok(network::package_names(&network::cargo_tree(name, &args)?))
    };
    let sources = |package: &Package| -> Vec<(String, String)> {
        let prefix = format!("{}/", package.directory);
        files
            .iter()
            .filter(|f| f.starts_with(&prefix) && f.ends_with(".rs"))
            .filter_map(|f| read_text(root, f).map(|text| (f.clone(), text)))
            .collect()
    };
    let evaluation = evaluate(RULES, &packages, &graph, &sources)?;

    let waiting = if evaluation.pending.is_empty() {
        "none".to_owned()
    } else {
        evaluation.pending.join(", ")
    };
    println!(
        "[xtask] crate-boundaries: {} of {} rules armed; the rest wait for crates that are not in the workspace yet: {waiting}",
        evaluation.armed,
        RULES.len()
    );
    finish("crate-boundaries", &evaluation.problems, || {
        coverage_summary(&evaluation)
    })
}

/// What a green run says it looked at: the armed kinds by name, and the kinds that checked nothing, so "process-memory
/// access" is never named as held while no crate it guards exists.
pub fn coverage_summary(evaluation: &Evaluation) -> String {
    let held: Vec<&str> = Kind::ALL
        .iter()
        .filter(|kind| evaluation.armed_kinds.contains(kind))
        .map(|kind| kind.label())
        .collect();
    let unchecked: Vec<&str> = Kind::ALL
        .iter()
        .filter(|kind| !evaluation.armed_kinds.contains(kind))
        .map(|kind| kind.label())
        .collect();
    let held_text = if held.is_empty() {
        "no rule".to_owned()
    } else {
        held.join(", ")
    };
    let mut text = format!("{} armed rules hold ({held_text})", evaluation.armed);
    if !unchecked.is_empty() {
        text.push_str(&format!(
            ". NOT checked, no crate for it exists yet: {}",
            unchecked.join(", ")
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(name: &str, directory: &str, direct: &[&str]) -> Package {
        Package {
            name: name.to_owned(),
            directory: directory.to_owned(),
            direct: direct.iter().map(|d| (*d).to_owned()).collect(),
        }
    }

    fn graph_of(names: &'static [&'static str]) -> impl Fn(&str) -> Result<BTreeSet<String>> {
        move |_| Ok(names.iter().map(|n| (*n).to_owned()).collect())
    }

    fn no_sources(_: &Package) -> Vec<(String, String)> {
        Vec::new()
    }

    #[test]
    fn every_rule_quotes_a_guard_that_names_its_crate() {
        for rule in RULES {
            assert!(
                rule.guard.contains(rule.package),
                "{}: the guard text \"{}\" does not name the crate",
                rule.package,
                rule.guard
            );
            assert!(rule.package.starts_with("sul-"), "{}", rule.package);
        }
    }

    #[test]
    fn no_rule_is_listed_twice() {
        let mut seen = BTreeSet::new();
        for rule in RULES {
            let key = (rule.package, format!("{:?}", rule.kind));
            assert!(
                seen.insert(key),
                "{} {:?} is listed twice",
                rule.package,
                rule.kind
            );
        }
    }

    #[test]
    fn a_rule_for_a_crate_that_does_not_exist_is_pending_not_passed() {
        let rules = [rule(
            Kind::Wasapi,
            "sul-gb",
            "sul-gb must not import wasapi",
        )];
        let evaluation = evaluate(&rules, &[], &graph_of(&[]), &no_sources).unwrap();
        assert_eq!(evaluation.armed, 0);
        assert_eq!(evaluation.pending, ["sul-gb"]);
        assert!(evaluation.problems.is_empty());
    }

    #[test]
    fn a_green_run_names_only_the_kinds_it_armed() {
        let rules = [
            rule(Kind::Wasapi, "sul-net", "sul-net must not import wasapi"),
            rule(
                Kind::ProcessMemory,
                "sul-fg",
                "sul-fg does not import ReadProcessMemory",
            ),
        ];
        let net = [package("sul-net", "crates/sul-net", &[])];
        let evaluation = evaluate(&rules, &net, &graph_of(&[]), &no_sources).unwrap();
        assert_eq!(evaluation.armed_kinds, BTreeSet::from([Kind::Wasapi]));
        let text = coverage_summary(&evaluation);
        assert!(
            text.starts_with("1 armed rules hold (direct wasapi)"),
            "{text}"
        );
        assert!(
            text.contains(
                "NOT checked, no crate for it exists yet: HTTP client, process-memory access"
            ),
            "{text}"
        );
    }

    #[test]
    fn nothing_armed_says_so_and_all_armed_says_no_gap() {
        let none = evaluate(RULES, &[], &graph_of(&[]), &no_sources).unwrap();
        let text = coverage_summary(&none);
        assert!(text.starts_with("0 armed rules hold (no rule)"), "{text}");
        assert!(
            text.contains("HTTP client, direct wasapi, process-memory access"),
            "{text}"
        );

        let all = Evaluation {
            armed: 3,
            armed_kinds: BTreeSet::from(Kind::ALL),
            ..Evaluation::default()
        };
        let text = coverage_summary(&all);
        assert_eq!(
            text,
            "3 armed rules hold (HTTP client, direct wasapi, process-memory access)"
        );
    }

    #[test]
    fn the_rule_starts_to_bite_when_the_crate_appears() {
        let rules = [rule(
            Kind::Wasapi,
            "sul-gb",
            "sul-gb must not import wasapi",
        )];
        let clean = [package("sul-gb", "crates/sul-gb", &["serde"])];
        let evaluation = evaluate(&rules, &clean, &graph_of(&[]), &no_sources).unwrap();
        assert_eq!(evaluation.armed, 1);
        assert!(evaluation.pending.is_empty() && evaluation.problems.is_empty());

        let dirty = [package("sul-gb", "crates/sul-gb", &["serde", "wasapi"])];
        let evaluation = evaluate(&rules, &dirty, &graph_of(&[]), &no_sources).unwrap();
        assert_eq!(evaluation.problems.len(), 1);
        assert!(evaluation.problems[0].contains("sul-gb must not import wasapi"));
    }

    #[test]
    fn an_http_client_anywhere_in_the_graph_is_found_even_through_another_crate() {
        let rules = [rule(
            Kind::HttpClient,
            "sul-wire",
            "sul-wire does not import an HTTP client",
        )];
        let wire = [package("sul-wire", "crates/sul-wire", &[])];
        let clean = evaluate(&rules, &wire, &graph_of(&["sul-wire", "rkyv"]), &no_sources).unwrap();
        assert!(clean.problems.is_empty());

        let transitive = evaluate(
            &rules,
            &wire,
            &graph_of(&["sul-wire", "helper", "reqwest"]),
            &no_sources,
        )
        .unwrap();
        assert_eq!(transitive.problems.len(), 1);
        assert!(transitive.problems[0].contains("reqwest"));
    }

    #[test]
    fn depending_on_the_declared_network_package_counts_as_importing_a_client() {
        let rules = [rule(
            Kind::HttpClient,
            "sul-metrics",
            "sul-metrics must not import an HTTP client",
        )];
        let metrics = [package("sul-metrics", "crates/sul-metrics", &["sul-net"])];
        let evaluation = evaluate(
            &rules,
            &metrics,
            &graph_of(&["sul-metrics", "sul-net"]),
            &no_sources,
        )
        .unwrap();
        assert_eq!(evaluation.problems.len(), 1);
        assert!(evaluation.problems[0].contains("sul-net"));
    }

    #[test]
    fn a_graph_that_cannot_be_read_is_an_error_not_a_pass() {
        let rules = [rule(
            Kind::HttpClient,
            "sul-wire",
            "sul-wire does not import an HTTP client",
        )];
        let wire = [package("sul-wire", "crates/sul-wire", &[])];
        let failing =
            |_: &str| -> Result<BTreeSet<String>> { Err(miette::miette!("cargo failed")) };
        assert!(evaluate(&rules, &wire, &failing, &no_sources).is_err());
    }

    #[test]
    fn process_memory_names_are_found_in_code_but_not_in_comments() {
        let rules = [rule(
            Kind::ProcessMemory,
            "sul-fg",
            "sul-fg does not import ReadProcessMemory",
        )];
        let fg = [package("sul-fg", "crates/sul-fg", &[])];
        let reading = |text: &'static str| {
            move |_: &Package| vec![("crates/sul-fg/src/lib.rs".to_owned(), text.to_owned())]
        };

        let bad = reading("use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;");
        assert_eq!(
            evaluate(&rules, &fg, &graph_of(&[]), &bad)
                .unwrap()
                .problems
                .len(),
            1
        );
        let native = reading("extern \"system\" { fn NtReadVirtualMemory(); }");
        assert_eq!(
            evaluate(&rules, &fg, &graph_of(&[]), &native)
                .unwrap()
                .problems
                .len(),
            1
        );
        let comment = reading(
            "// this crate never calls ReadProcessMemory\n/// nor NtReadVirtualMemory\nfn ok() {}",
        );
        assert!(
            evaluate(&rules, &fg, &graph_of(&[]), &comment)
                .unwrap()
                .problems
                .is_empty()
        );
        let trailing = reading("let f = ReadProcessMemory; // not allowed");
        assert_eq!(
            evaluate(&rules, &fg, &graph_of(&[]), &trailing)
                .unwrap()
                .problems
                .len(),
            1
        );
    }

    #[test]
    fn metadata_gives_directories_and_only_normal_dependencies() {
        let raw = r#"{
            "workspace_root": "/repo",
            "packages": [
                {"name": "sul-net", "manifest_path": "/repo/crates/sul-net/Cargo.toml", "dependencies": [
                    {"name": "serde", "kind": null},
                    {"name": "wasapi", "kind": "dev"},
                    {"name": "cc", "kind": "build"},
                    {"name": "windows", "kind": null, "target": "cfg(windows)"}
                ]},
                {"name": "a", "manifest_path": "C:\\repo\\a\\Cargo.toml", "dependencies": []}
            ]
        }"#;
        let packages = packages_from_metadata(raw).unwrap();
        assert_eq!(packages.len(), 2);
        let net = packages.iter().find(|p| p.name == "sul-net").unwrap();
        assert_eq!(net.directory, "crates/sul-net");
        assert!(net.direct.contains("serde") && net.direct.contains("windows"));
        assert!(
            !net.direct.contains("wasapi"),
            "a dev-dependency is not shipped"
        );
        assert!(
            !net.direct.contains("cc"),
            "a build-dependency is not shipped"
        );
        assert!(packages_from_metadata("not json").is_err());
        assert!(packages_from_metadata("{}").is_err());
    }

    #[test]
    fn the_root_is_stripped_from_a_manifest_path() {
        assert_eq!(
            relative_directory("/repo", "/repo/crates/x/Cargo.toml"),
            "crates/x"
        );
        assert_eq!(
            relative_directory("/repo", "/repo/xtask/Cargo.toml"),
            "xtask"
        );
        assert_eq!(relative_directory("", "crates/x/Cargo.toml"), "crates/x");
    }
}
