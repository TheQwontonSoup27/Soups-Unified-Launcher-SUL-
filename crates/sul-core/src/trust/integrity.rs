//! Integrity checks: what a verification of a build against its recorded manifest means.
//!
//! [`crate::manifest::BuildManifest::verify`] lists missing, modified and extra files. That list is facts, not
//! judgements. This module turns it into three separate piles, so a person is never told more than the evidence says:
//!
//! * **violations**: conclusive differences in a build that is supposed to stay as recorded (an original, optimized or
//!   legacy build);
//! * **changes**: the same conclusive differences in a build that is allowed to change (experimental or custom). They are
//!   facts, not violations, so an allowed experimental build is not flagged just because it differs;
//! * **suspicious**: signals that need a look and can be false positives: an unlisted program file, a name that
//!   differs only in letter case.
//!
//! It reads only the report it is given (or, through [`check_build`], reads the build directory and never writes to it).
//! It is a comparison with a recorded state. It is not cheat detection and not malware detection.

use super::{Finding, ProgramKind, program_kind};
use crate::Result;
use crate::manifest::{BuildManifest, Variant, VerifyReport};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Whether the recorded build is expected to stay exactly as recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Expectation {
    /// Original, optimized and legacy builds: any recorded file that changed or vanished is a violation.
    Unchanged,
    /// Experimental and custom builds: changes are expected, so they are reported as changes, not violations.
    MayChange,
}

impl Expectation {
    pub fn for_variant(variant: &Variant) -> Self {
        match variant {
            Variant::Original | Variant::Optimized | Variant::Legacy => Self::Unchanged,
            Variant::Experimental | Variant::Custom(_) => Self::MayChange,
        }
    }
}

/// The overall reading, strongest concern first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Conclusive differences in a build that must not change.
    ViolationsFound,
    /// No conclusive violation, but signals worth a look.
    SignalsOnly,
    /// Differences in a build that is allowed to change, and no signals.
    ChangesAllowed,
    /// Every recorded file is there and identical, and no unexpected program file was found.
    Matches,
}

impl Verdict {
    /// One sentence for a screen. None of them says "safe" or "clean".
    pub fn describe(self) -> &'static str {
        match self {
            Self::ViolationsFound => {
                "Differs from the recorded manifest in a way this build is not allowed to."
            }
            Self::SignalsOnly => {
                "Nothing conclusive, but there are signals worth a look. Signals are not proof."
            }
            Self::ChangesAllowed => {
                "Differs from the recorded manifest, in a build that is allowed to change."
            }
            Self::Matches => {
                "Matches the recorded manifest. This says nothing about whether the recorded build was trustworthy."
            }
        }
    }
}

/// Files the person has decided not to judge (for example a `logs/` folder the game writes to).
#[derive(Debug, Clone, Default)]
pub struct IntegrityOptions {
    /// Exact relative paths, or folders ending in `/`. Compared in forward-slash form, ignoring letter case.
    pub ignore: Vec<String>,
}

impl IntegrityOptions {
    fn ignores(&self, path: &str) -> bool {
        let p = path.to_lowercase();
        self.ignore.iter().any(|rule| {
            let r = rule.replace('\\', "/").to_lowercase();
            if r.ends_with('/') {
                p.starts_with(&r)
            } else {
                p == r
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IntegrityAssessment {
    pub expectation: Expectation,
    pub verdict: Verdict,
    /// Conclusive, and against the rules of this build.
    pub violations: Vec<Finding>,
    /// Conclusive, and allowed in this build.
    pub changes: Vec<Finding>,
    /// Signals. Never conclusive.
    pub suspicious: Vec<Finding>,
    /// Paths the person asked not to judge. Listed so that nothing is hidden.
    pub ignored: Vec<String>,
    /// Unlisted files that are not program files (logs, caches, saves). Not flagged, and not hidden.
    pub unlisted_other: Vec<String>,
}

fn program_note(path: &str) -> &'static str {
    match program_kind(path) {
        Some(ProgramKind::NativeCode) => " It is a program file.",
        Some(ProgramKind::Script) => " It is a script file.",
        Some(ProgramKind::Shortcut) => " It is a shortcut.",
        Some(ProgramKind::Java) => " It is a Java archive.",
        None => "",
    }
}

/// Classify a verification report. Pure: no file is read.
pub fn assess(
    manifest: &BuildManifest,
    report: &VerifyReport,
    options: &IntegrityOptions,
) -> IntegrityAssessment {
    let expectation = Expectation::for_variant(&manifest.variant);
    let mut out = IntegrityAssessment {
        expectation,
        verdict: Verdict::Matches,
        violations: vec![],
        changes: vec![],
        suspicious: vec![],
        ignored: vec![],
        unlisted_other: vec![],
    };

    let mut missing: Vec<&String> = vec![];
    let mut extra: Vec<&String> = vec![];
    for p in &report.missing {
        if options.ignores(p) {
            out.ignored.push(p.clone())
        } else {
            missing.push(p)
        }
    }
    for p in &report.modified {
        if options.ignores(p) {
            out.ignored.push(p.clone());
            continue;
        }
        let detail = format!(
            "The file's content differs from the recorded hash.{}",
            program_note(p)
        );
        let finding = Finding::confirmed("content_differs", p, detail);
        match expectation {
            Expectation::Unchanged => out.violations.push(finding),
            Expectation::MayChange => out.changes.push(finding),
        }
    }
    for p in &report.extra {
        if options.ignores(p) {
            out.ignored.push(p.clone())
        } else {
            extra.push(p)
        }
    }

    // A name that only changed letter case shows up as one missing and one extra file. On Windows both names are the
    // same file, so calling that a missing file would be a false alarm; say what is known and what was not compared.
    // Linear in the size of the report: a build can have a hundred thousand files, so no pairwise search.
    let mut extra_by_lowercase: HashMap<String, Vec<&String>> = HashMap::new();
    for e in &extra {
        extra_by_lowercase
            .entry(e.to_lowercase())
            .or_default()
            .push(e);
    }
    let mut case_paired: HashSet<&String> = HashSet::new();
    for m in &missing {
        let found = extra_by_lowercase
            .get_mut(&m.to_lowercase())
            .and_then(|candidates| candidates.pop());
        if let Some(e) = found {
            out.suspicious.push(Finding::suspicious(
                "name_case_differs",
                e,
                format!("Recorded as \"{m}\", found as \"{e}\": the same name in different letter case. The file under the new name was not compared."),
            ));
            case_paired.insert(m);
            case_paired.insert(e);
        }
    }
    for m in missing.iter().filter(|m| !case_paired.contains(*m)) {
        let finding = Finding::confirmed("file_missing", m, "A recorded file is not there.");
        match expectation {
            Expectation::Unchanged => out.violations.push(finding),
            Expectation::MayChange => out.changes.push(finding),
        }
    }
    for e in extra.iter().filter(|e| !case_paired.contains(*e)) {
        if program_kind(e).is_some() {
            out.suspicious.push(Finding::suspicious(
                "unlisted_program_file",
                e,
                format!("A file that is not in the manifest is there.{} Games and mods sometimes add these legitimately.", program_note(e)),
            ));
        } else {
            out.unlisted_other.push((*e).clone());
        }
    }

    out.verdict = if !out.violations.is_empty() {
        Verdict::ViolationsFound
    } else if !out.suspicious.is_empty() {
        Verdict::SignalsOnly
    } else if !out.changes.is_empty() {
        Verdict::ChangesAllowed
    } else {
        Verdict::Matches
    };
    out
}

/// Verify `root` against `manifest` (read-only) and classify the result.
pub fn check_build(
    manifest: &BuildManifest,
    root: &Path,
    options: &IntegrityOptions,
) -> Result<IntegrityAssessment> {
    let report = manifest.verify(root)?;
    Ok(assess(manifest, &report, options))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{FileEntry, State};

    fn manifest(variant: Variant) -> BuildManifest {
        BuildManifest {
            schema_version: 1,
            game_id: "g".into(),
            display_name: "G".into(),
            variant,
            base_version: "1".into(),
            source: "test".into(),
            files: vec![FileEntry {
                path: "a.bin".into(),
                sha256: "00".into(),
                size: 1,
            }],
            created_unix: 0,
            state: State::Ready,
            notes: String::new(),
        }
    }

    fn report(missing: &[&str], modified: &[&str], extra: &[&str]) -> VerifyReport {
        let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect();
        VerifyReport {
            missing: v(missing),
            modified: v(modified),
            extra: v(extra),
        }
    }

    fn codes(f: &[Finding]) -> Vec<&'static str> {
        f.iter().map(|x| x.code).collect()
    }

    #[test]
    fn expectation_follows_the_variant() {
        for v in [Variant::Original, Variant::Optimized, Variant::Legacy] {
            assert_eq!(Expectation::for_variant(&v), Expectation::Unchanged);
        }
        for v in [Variant::Experimental, Variant::Custom("mine".into())] {
            assert_eq!(Expectation::for_variant(&v), Expectation::MayChange);
        }
    }

    #[test]
    fn a_clean_report_matches_and_says_so_without_calling_it_safe() {
        let a = assess(
            &manifest(Variant::Original),
            &report(&[], &[], &[]),
            &IntegrityOptions::default(),
        );
        assert_eq!(a.verdict, Verdict::Matches);
        assert!(a.violations.is_empty() && a.changes.is_empty() && a.suspicious.is_empty());
        let text = a.verdict.describe();
        assert!(text.contains("Matches the recorded manifest"));
        assert!(!text.to_lowercase().contains("safe") && !text.to_lowercase().contains("clean"));
    }

    #[test]
    fn in_a_build_that_must_not_change_modified_and_missing_files_are_confirmed_violations() {
        let a = assess(
            &manifest(Variant::Original),
            &report(&["gone.bin"], &["a.bin"], &[]),
            &IntegrityOptions::default(),
        );
        assert_eq!(a.verdict, Verdict::ViolationsFound);
        assert_eq!(codes(&a.violations), ["content_differs", "file_missing"]);
        assert!(
            a.violations
                .iter()
                .all(|f| f.certainty == super::super::Certainty::Confirmed)
        );
        assert!(a.changes.is_empty() && a.suspicious.is_empty());
    }

    #[test]
    fn an_experimental_build_that_differs_is_not_flagged_for_it() {
        for v in [Variant::Experimental, Variant::Custom("x".into())] {
            let a = assess(
                &manifest(v),
                &report(&["gone.bin"], &["a.bin"], &[]),
                &IntegrityOptions::default(),
            );
            assert_eq!(a.verdict, Verdict::ChangesAllowed);
            assert!(
                a.violations.is_empty(),
                "an allowed change is not a violation"
            );
            assert_eq!(codes(&a.changes), ["content_differs", "file_missing"]);
        }
    }

    #[test]
    fn an_unlisted_program_file_is_only_a_signal_in_every_kind_of_build() {
        for v in [Variant::Original, Variant::Experimental] {
            let a = assess(
                &manifest(v),
                &report(&[], &[], &["mods/plugin.dll", "run.bat"]),
                &IntegrityOptions::default(),
            );
            assert_eq!(a.verdict, Verdict::SignalsOnly);
            assert_eq!(
                codes(&a.suspicious),
                ["unlisted_program_file", "unlisted_program_file"]
            );
            assert!(
                a.suspicious
                    .iter()
                    .all(|f| f.certainty == super::super::Certainty::Suspicious)
            );
            assert!(a.violations.is_empty(), "a signal is never a violation");
            assert!(
                a.suspicious[0].detail.contains("legitimately"),
                "says it can be a false alarm"
            );
        }
    }

    #[test]
    fn unlisted_ordinary_files_are_listed_but_do_not_raise_anything() {
        let a = assess(
            &manifest(Variant::Original),
            &report(&[], &[], &["logs/run.txt", "cache/x.dat", "saves/1.sav"]),
            &IntegrityOptions::default(),
        );
        assert_eq!(a.verdict, Verdict::Matches);
        assert_eq!(
            a.unlisted_other,
            ["logs/run.txt", "cache/x.dat", "saves/1.sav"]
        );
    }

    #[test]
    fn a_name_that_only_changed_case_is_a_signal_not_a_missing_file() {
        let a = assess(
            &manifest(Variant::Original),
            &report(&["Data/A.bin"], &[], &["data/a.bin"]),
            &IntegrityOptions::default(),
        );
        assert_eq!(a.verdict, Verdict::SignalsOnly);
        assert_eq!(codes(&a.suspicious), ["name_case_differs"]);
        assert!(a.violations.is_empty(), "no false 'missing file' alarm");
        assert!(a.unlisted_other.is_empty() && a.suspicious[0].detail.contains("was not compared"));
    }

    #[test]
    fn a_case_pair_is_matched_once_and_the_rest_are_still_reported() {
        let a = assess(
            &manifest(Variant::Original),
            &report(&["A.bin", "B.bin"], &[], &["a.bin", "other.dll"]),
            &IntegrityOptions::default(),
        );
        assert_eq!(
            codes(&a.suspicious),
            ["name_case_differs", "unlisted_program_file"]
        );
        assert_eq!(codes(&a.violations), ["file_missing"]);
        assert_eq!(a.violations[0].path, "B.bin");
        assert_eq!(a.verdict, Verdict::ViolationsFound);
    }

    #[test]
    fn a_report_with_tens_of_thousands_of_differences_is_classified_quickly_and_correctly() {
        // Case pairing must not be a pairwise search: a real build can have a hundred thousand files.
        let n = 40_000;
        let missing: Vec<String> = (0..n).map(|i| format!("data/file{i}.bin")).collect();
        let extra: Vec<String> = (0..n).map(|i| format!("DATA/FILE{i}.BIN")).collect();
        let r = VerifyReport {
            missing: missing.clone(),
            modified: vec![],
            extra,
        };
        let started = std::time::Instant::now();
        let a = assess(
            &manifest(Variant::Original),
            &r,
            &IntegrityOptions::default(),
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "took {:?}",
            started.elapsed()
        );
        assert_eq!(a.suspicious.len(), n, "every pair is matched exactly once");
        assert!(a.violations.is_empty() && a.unlisted_other.is_empty());
        assert_eq!(a.verdict, Verdict::SignalsOnly);
        // A pair is used once: a second extra with the same lowercase name is not matched to the same missing file.
        let r2 = VerifyReport {
            missing: vec!["a/B.bin".into()],
            modified: vec![],
            extra: vec!["A/b.bin".into(), "a/b.BIN".into()],
        };
        let a2 = assess(
            &manifest(Variant::Original),
            &r2,
            &IntegrityOptions::default(),
        );
        assert_eq!(codes(&a2.suspicious), ["name_case_differs"]);
        assert_eq!(
            a2.unlisted_other.len(),
            1,
            "the second name is listed, not swallowed"
        );
    }

    #[test]
    fn violations_outrank_signals_and_signals_outrank_allowed_changes() {
        let both = assess(
            &manifest(Variant::Original),
            &report(&[], &["a.bin"], &["x.exe"]),
            &IntegrityOptions::default(),
        );
        assert_eq!(both.verdict, Verdict::ViolationsFound);
        let sig = assess(
            &manifest(Variant::Experimental),
            &report(&[], &["a.bin"], &["x.exe"]),
            &IntegrityOptions::default(),
        );
        assert_eq!(sig.verdict, Verdict::SignalsOnly);
        assert_eq!(sig.changes.len(), 1, "the change is still reported");
    }

    #[test]
    fn the_program_kind_is_named_in_the_detail() {
        let a = assess(
            &manifest(Variant::Original),
            &report(&[], &["game.exe", "data.bin"], &["x.ps1"]),
            &IntegrityOptions::default(),
        );
        assert!(a.violations[0].detail.contains("It is a program file."));
        assert!(!a.violations[1].detail.contains("program"));
        assert!(a.suspicious[0].detail.contains("It is a script file."));
    }

    #[test]
    fn ignored_paths_are_not_judged_but_are_listed() {
        let options = IntegrityOptions {
            ignore: vec!["Logs/".into(), "config\\user.ini".into()],
        };
        let a = assess(
            &manifest(Variant::Original),
            &report(
                &["logs/old.txt"],
                &["CONFIG/user.ini"],
                &["logs/new.dll", "keep.dll"],
            ),
            &options,
        );
        assert_eq!(
            a.ignored,
            ["logs/old.txt", "CONFIG/user.ini", "logs/new.dll"]
        );
        assert_eq!(codes(&a.suspicious), ["unlisted_program_file"]);
        assert_eq!(a.suspicious[0].path, "keep.dll");
        assert!(a.violations.is_empty());
    }

    #[test]
    fn an_ignore_rule_is_exact_unless_it_ends_in_a_slash() {
        let o = IntegrityOptions {
            ignore: vec!["logs".into(), "saves/".into()],
        };
        assert!(o.ignores("logs"));
        assert!(
            !o.ignores("logs/a.txt"),
            "a name without a trailing slash matches only that path"
        );
        assert!(!o.ignores("logsx"));
        assert!(o.ignores("saves/slot1/a.sav") && o.ignores("SAVES/B.SAV"));
        assert!(!o.ignores("savesx/a"));
    }

    #[test]
    fn the_verdict_wording_never_says_safe_or_clean() {
        for v in [
            Verdict::ViolationsFound,
            Verdict::SignalsOnly,
            Verdict::ChangesAllowed,
            Verdict::Matches,
        ] {
            let t = v.describe().to_lowercase();
            assert!(
                !t.contains("safe") && !t.contains("clean") && !t.contains("cheat"),
                "{t}"
            );
        }
    }

    // ---- with real files and the real manifest code ----

    fn build(dir: &Path, files: &[(&str, &[u8])]) {
        for (name, data) in files {
            let p = dir.join(name);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, data).unwrap();
        }
    }

    #[test]
    fn a_real_build_is_matched_then_tampered_with_and_the_difference_is_confirmed() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("build");
        build(
            &root,
            &[
                ("game.exe", b"MZ original"),
                ("data/a.bin", b"aaaa"),
                ("data/b.bin", b"bbbb"),
            ],
        );
        let m = BuildManifest::scan(&root, "g", "G", Variant::Original, "1", "test").unwrap();

        let clean = check_build(&m, &root, &IntegrityOptions::default()).unwrap();
        assert_eq!(clean.verdict, Verdict::Matches);

        std::fs::write(root.join("data/a.bin"), b"AAAA").unwrap(); // same size, different content
        std::fs::remove_file(root.join("data/b.bin")).unwrap();
        build(
            &root,
            &[("mods/hook.dll", b"MZ hook"), ("logs/run.txt", b"log")],
        );

        let a = check_build(&m, &root, &IntegrityOptions::default()).unwrap();
        assert_eq!(a.verdict, Verdict::ViolationsFound);
        assert_eq!(
            a.violations
                .iter()
                .map(|f| (f.code, f.path.as_str()))
                .collect::<Vec<_>>(),
            [
                ("content_differs", "data/a.bin"),
                ("file_missing", "data/b.bin")
            ]
        );
        assert_eq!(
            a.suspicious
                .iter()
                .map(|f| (f.code, f.path.as_str()))
                .collect::<Vec<_>>(),
            [("unlisted_program_file", "mods/hook.dll")]
        );
        assert_eq!(a.unlisted_other, ["logs/run.txt"]);
    }

    #[test]
    fn checking_a_build_never_changes_it() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("build");
        build(&root, &[("a.bin", b"one"), ("sub/b.bin", b"two")]);
        let m = BuildManifest::scan(&root, "g", "G", Variant::Original, "1", "t").unwrap();
        build(&root, &[("extra.dll", b"x")]);
        let before = BuildManifest::scan(&root, "g", "G", Variant::Original, "1", "t")
            .unwrap()
            .files;
        let _ = check_build(&m, &root, &IntegrityOptions::default()).unwrap();
        let after = BuildManifest::scan(&root, "g", "G", Variant::Original, "1", "t")
            .unwrap()
            .files;
        assert_eq!(before, after);
    }

    /// Runs on Windows only (CI): it needs a real folder junction.
    #[cfg(windows)]
    #[test]
    fn a_link_in_the_build_is_an_error_not_a_pass() {
        // `verify` refuses links; that must reach the caller instead of being read as a clean result.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("build");
        build(&root, &[("a.bin", b"one")]);
        let m = BuildManifest::scan(&root, "g", "G", Variant::Original, "1", "t").unwrap();
        super::super::make_junction(&root.join("link"), tmp.path());
        assert!(check_build(&m, &root, &IntegrityOptions::default()).is_err());
    }
}
