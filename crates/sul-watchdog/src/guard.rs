//! The integrity guard: compare the files on disk with the baseline (Ch. 12 §2.8: "monitor binaries, sidecars, manifests
//! against trusted baselines"; Ch. 14 §14.14.1: verified "at session start, not just first load").
//!
//! This is the one-shot half. The event-driven half that calls it the moment a file changes is [`crate::monitor`].
//!
//! What a deviation means, in the words of `sul_trust::findings`: **confirmed** when the evidence settles the fact (the digest
//! differs, the file is gone, the name is now a link), **suspicious** when it only needs a person to look (a new file that
//! matches a guarded kind could be an update; a file that cannot be read could be an antivirus lock). A changed file is not
//! proof of an attack and this never says it is.
//!
//! Links are never followed: a guarded path must be a plain file and so must every folder above it, so a junction swapped in
//! for a folder cannot make a tampered file look like the original.

use crate::baseline::{Baseline, Role, Selection, Watched, key, walk_selected};
use serde::Serialize;
use std::path::Path;
use sul_core::{Error, Result, error::io, safepath};
use sul_trust::findings::Certainty;
use sul_trust::manifest::hash_file;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "deviation", rename_all = "snake_case")]
pub enum Deviation {
    /// The file is there and is not what the baseline recorded.
    Modified {
        expected_sha256: String,
        actual_sha256: String,
        expected_size: u64,
        actual_size: u64,
    },
    Missing,
    /// The name is there but it is a link, a folder or another kind of thing, not a plain file.
    NotAFile,
    /// It could not be read. `kind` is the operating system's error kind (`PermissionDenied`...), never a path.
    Unreadable {
        kind: String,
    },
    /// Not in the baseline, but it matches a rule of the selection.
    Unlisted {
        sha256: Option<String>,
    },
}

impl Deviation {
    pub fn certainty(&self) -> Certainty {
        match self {
            Self::Modified { .. } | Self::Missing | Self::NotAFile => Certainty::Confirmed,
            Self::Unreadable { .. } | Self::Unlisted { .. } => Certainty::Suspicious,
        }
    }

    /// One plain sentence for a person.
    pub fn describe(&self) -> String {
        match self {
            Self::Modified { .. } => {
                "no longer has the SHA-256 and size recorded in the baseline".into()
            }
            Self::Missing => "is missing".into(),
            Self::NotAFile => "is now a link or something other than a plain file".into(),
            Self::Unreadable { kind } => format!("could not be read ({kind})"),
            Self::Unlisted { .. } => {
                "is not in the baseline but is a kind the watchdog guards".into()
            }
        }
    }
}

/// A deviation and the file it is about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PathDeviation {
    /// Forward-slash path relative to the guarded root.
    pub path: String,
    pub role: Option<Role>,
    pub artifact_code: Option<String>,
    pub deviation: Deviation,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct GuardReport {
    /// How many baseline entries were looked at.
    pub checked: usize,
    pub deviations: Vec<PathDeviation>,
}

impl GuardReport {
    pub fn is_clean(&self) -> bool {
        self.deviations.is_empty()
    }
}

fn kind_of(error: &std::io::Error) -> String {
    format!("{:?}", error.kind())
}

/// Whether `rel` under `root` reaches its file through a link (a symlink or junction anywhere in the chain, the file itself
/// included). `Err` carries a deviation for a link or an unreadable step.
fn walk_chain(root: &Path, rel: &str) -> std::result::Result<(), Deviation> {
    let mut current = root.to_path_buf();
    let parts: Vec<&str> = rel.split('/').collect();
    for (i, part) in parts.iter().enumerate() {
        current.push(part);
        let meta = match std::fs::symlink_metadata(&current) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(Deviation::Missing),
            Err(e) => {
                return Err(Deviation::Unreadable { kind: kind_of(&e) });
            }
        };
        // Kept on purpose although the two checks below also refuse a link: `symlink_metadata` never reports a link as a
        // file or a folder, so removing this line changes no outcome (a deliberate-bug check, 2026-11-26, proved it: the
        // tests still pass without it). It says what is meant, and it keeps the answer right if the checks below change.
        if meta.file_type().is_symlink() {
            return Err(Deviation::NotAFile);
        }
        let last = i + 1 == parts.len();
        if last && !meta.is_file() {
            return Err(Deviation::NotAFile);
        }
        if !last && !meta.is_dir() {
            return Err(Deviation::NotAFile);
        }
    }
    Ok(())
}

/// Compare one guarded file with its baseline entry. `None` means it matches.
pub fn check_entry(root: &Path, entry: &Watched) -> Option<Deviation> {
    if let Err(deviation) = walk_chain(root, &entry.path) {
        return Some(deviation);
    }
    let path = root.join(&entry.path);
    match hash_file(&path) {
        Ok((sha256, size)) => {
            if sha256 == entry.sha256 && size == entry.size {
                None
            } else {
                Some(Deviation::Modified {
                    expected_sha256: entry.sha256.clone(),
                    actual_sha256: sha256,
                    expected_size: entry.size,
                    actual_size: size,
                })
            }
        }
        Err(Error::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            Some(Deviation::Missing)
        }
        Err(Error::Io { source, .. }) => Some(Deviation::Unreadable {
            kind: kind_of(&source),
        }),
        Err(_) => Some(Deviation::Unreadable {
            kind: "Other".into(),
        }),
    }
}

fn path_deviation(entry: &Watched, deviation: Deviation) -> PathDeviation {
    PathDeviation {
        path: entry.path.clone(),
        role: Some(entry.role),
        artifact_code: entry.artifact_code.clone(),
        deviation,
    }
}

fn unlisted(rel: &str, full: &Path, selection: &Selection) -> PathDeviation {
    let rule = selection.matching(rel);
    // A link is reported as a link, not followed to hash whatever it points at.
    let plain = std::fs::symlink_metadata(full).is_ok_and(|m| m.is_file());
    PathDeviation {
        path: rel.to_owned(),
        role: rule.map(|r| r.role),
        artifact_code: rule.and_then(|r| r.artifact_code.clone()),
        deviation: Deviation::Unlisted {
            sha256: if plain {
                hash_file(full).ok().map(|(h, _)| h)
            } else {
                None
            },
        },
    }
}

/// Check every guarded file whose role `include` accepts, and look for unlisted files of a guarded kind. A root that cannot
/// be read at all is an error (nothing was checked), not an empty report.
pub fn verify_all(
    root: &Path,
    baseline: &Baseline,
    selection: &Selection,
    include: impl Fn(Role) -> bool,
) -> Result<GuardReport> {
    std::fs::metadata(root).map_err(io(root))?;
    let mut report = GuardReport::default();
    for entry in baseline.entries.iter().filter(|e| include(e.role)) {
        report.checked += 1;
        if let Some(deviation) = check_entry(root, entry) {
            report.deviations.push(path_deviation(entry, deviation));
        }
    }
    for (_, (rel, full)) in walk_selected(root, selection)? {
        if baseline.get(&rel).is_some() {
            continue;
        }
        let role = selection.matching(&rel).map(|r| r.role);
        if role.is_some_and(&include) {
            report.deviations.push(unlisted(&rel, &full, selection));
        }
    }
    report.deviations.sort_by_key(|d| key(&d.path));
    Ok(report)
}

/// Check one path. `None` when it is clean, or when it is neither in the baseline nor a guarded kind, or when `rel` is not a
/// safe relative path (an event for something outside the root is not this guard's business).
pub fn verify_one(
    root: &Path,
    baseline: &Baseline,
    selection: &Selection,
    rel: &str,
) -> Option<PathDeviation> {
    if safepath::validate_relative(rel).is_err() {
        return None;
    }
    if let Some(entry) = baseline.get(rel) {
        return check_entry(root, entry).map(|d| path_deviation(entry, d));
    }
    selection.matching(rel)?;
    let full = root.join(rel);
    let meta = std::fs::symlink_metadata(&full).ok()?;
    if meta.is_dir() {
        return None;
    }
    Some(unlisted(rel, &full, selection))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::baseline::Pattern;
    use std::fs;

    struct Fixture {
        dir: tempfile::TempDir,
        baseline: Baseline,
        selection: Selection,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::create_dir_all(root.join("models")).unwrap();
        fs::write(root.join("sul.exe"), b"the program").unwrap();
        fs::write(root.join("bin/helper.dll"), b"a library").unwrap();
        fs::write(root.join("models/jarvis.gguf"), b"weights").unwrap();
        let selection = Selection::new()
            .rule(Pattern::Prefix("models/".into()), Role::ModelFile)
            .rule_with_code(
                Pattern::Exact("sul.exe".into()),
                Role::TrustPrimitive,
                "E_SUL_TAMPER",
            )
            .rule(Pattern::Extension("dll".into()), Role::Binary);
        let baseline = Baseline::compute(root, "t", &selection, 1).unwrap();
        Fixture {
            dir,
            baseline,
            selection,
        }
    }

    fn all(_: Role) -> bool {
        true
    }

    #[test]
    fn an_untouched_tree_is_clean_and_every_entry_was_checked() {
        let f = fixture();
        let report = verify_all(f.dir.path(), &f.baseline, &f.selection, all).unwrap();
        assert!(report.is_clean(), "{report:?}");
        assert_eq!(report.checked, 3);
    }

    #[test]
    fn a_changed_file_is_modified_with_both_digests_and_its_artifact_code() {
        let f = fixture();
        fs::write(f.dir.path().join("sul.exe"), b"the program!").unwrap();
        let report = verify_all(f.dir.path(), &f.baseline, &f.selection, all).unwrap();
        assert_eq!(report.deviations.len(), 1);
        let d = &report.deviations[0];
        assert_eq!(d.path, "sul.exe");
        assert_eq!(d.role, Some(Role::TrustPrimitive));
        assert_eq!(d.artifact_code.as_deref(), Some("E_SUL_TAMPER"));
        match &d.deviation {
            Deviation::Modified {
                expected_sha256,
                actual_sha256,
                expected_size,
                actual_size,
            } => {
                assert_eq!(expected_sha256, &f.baseline.get("sul.exe").unwrap().sha256);
                assert_ne!(expected_sha256, actual_sha256);
                assert_eq!((*expected_size, *actual_size), (11, 12));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(d.deviation.certainty(), Certainty::Confirmed);
    }

    #[test]
    fn a_change_that_keeps_the_size_is_still_caught() {
        let f = fixture();
        fs::write(f.dir.path().join("sul.exe"), b"the Program").unwrap();
        let report = verify_all(f.dir.path(), &f.baseline, &f.selection, all).unwrap();
        assert!(matches!(
            report.deviations[0].deviation,
            Deviation::Modified {
                expected_size: 11,
                actual_size: 11,
                ..
            }
        ));
    }

    #[test]
    fn a_missing_file_and_a_file_replaced_by_a_folder_are_confirmed() {
        let f = fixture();
        fs::remove_file(f.dir.path().join("bin/helper.dll")).unwrap();
        fs::remove_file(f.dir.path().join("sul.exe")).unwrap();
        fs::create_dir(f.dir.path().join("sul.exe")).unwrap();
        let report = verify_all(f.dir.path(), &f.baseline, &f.selection, all).unwrap();
        let by_path = |p: &str| {
            report
                .deviations
                .iter()
                .find(|d| d.path == p)
                .unwrap()
                .deviation
                .clone()
        };
        assert_eq!(by_path("bin/helper.dll"), Deviation::Missing);
        assert_eq!(by_path("sul.exe"), Deviation::NotAFile);
        assert_eq!(Deviation::Missing.certainty(), Certainty::Confirmed);
    }

    #[test]
    fn a_new_file_of_a_guarded_kind_is_unlisted_and_other_new_files_are_ignored() {
        let f = fixture();
        fs::write(f.dir.path().join("bin/planted.dll"), b"evil").unwrap();
        fs::write(f.dir.path().join("bin/readme.txt"), b"hello").unwrap();
        let report = verify_all(f.dir.path(), &f.baseline, &f.selection, all).unwrap();
        assert_eq!(report.deviations.len(), 1, "{report:?}");
        let d = &report.deviations[0];
        assert_eq!(d.path, "bin/planted.dll");
        assert_eq!(d.role, Some(Role::Binary));
        match &d.deviation {
            Deviation::Unlisted { sha256: Some(h) } => assert_eq!(h.len(), 64),
            other => panic!("{other:?}"),
        }
        assert_eq!(d.deviation.certainty(), Certainty::Suspicious);
    }

    #[test]
    fn the_role_filter_skips_model_files_in_a_periodic_pass() {
        let f = fixture();
        fs::write(f.dir.path().join("models/jarvis.gguf"), b"swapped").unwrap();
        fs::write(f.dir.path().join("models/new.gguf"), b"new").unwrap();
        let periodic = verify_all(
            f.dir.path(),
            &f.baseline,
            &f.selection,
            Role::in_periodic_rescan,
        )
        .unwrap();
        assert!(periodic.is_clean(), "{periodic:?}");
        assert_eq!(periodic.checked, 2);
        let full = verify_all(f.dir.path(), &f.baseline, &f.selection, all).unwrap();
        assert_eq!(full.deviations.len(), 2, "{full:?}");
        assert_eq!(full.checked, 3);
    }

    #[test]
    fn a_missing_root_is_an_error_not_a_clean_report() {
        let f = fixture();
        let gone = f.dir.path().join("nowhere");
        assert!(verify_all(&gone, &f.baseline, &f.selection, all).is_err());
    }

    #[test]
    fn verify_one_agrees_with_verify_all_and_ignores_what_it_should() {
        let f = fixture();
        let root = f.dir.path();
        assert_eq!(verify_one(root, &f.baseline, &f.selection, "sul.exe"), None);
        fs::write(root.join("sul.exe"), b"changed").unwrap();
        let d = verify_one(root, &f.baseline, &f.selection, "sul.exe").unwrap();
        assert!(matches!(d.deviation, Deviation::Modified { .. }));
        // Not guarded and not in the baseline.
        fs::write(root.join("notes.txt"), b"x").unwrap();
        assert_eq!(
            verify_one(root, &f.baseline, &f.selection, "notes.txt"),
            None
        );
        // A folder that matches a rule's name is not a file.
        fs::create_dir(root.join("bin/dir.dll")).unwrap();
        assert_eq!(
            verify_one(root, &f.baseline, &f.selection, "bin/dir.dll"),
            None
        );
        // Unsafe paths are not this guard's business.
        assert_eq!(
            verify_one(root, &f.baseline, &f.selection, "../sul.exe"),
            None
        );
        assert_eq!(verify_one(root, &f.baseline, &f.selection, ""), None);
        // New guarded file.
        fs::write(root.join("bin/new.dll"), b"x").unwrap();
        assert!(matches!(
            verify_one(root, &f.baseline, &f.selection, "bin/new.dll")
                .unwrap()
                .deviation,
            Deviation::Unlisted { .. }
        ));
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_never_followed_even_when_it_points_at_identical_content() {
        let f = fixture();
        let root = f.dir.path();
        // A file replaced by a symlink to a byte-identical copy elsewhere.
        let copy = tempfile::NamedTempFile::new().unwrap();
        fs::write(copy.path(), b"the program").unwrap();
        fs::remove_file(root.join("sul.exe")).unwrap();
        std::os::unix::fs::symlink(copy.path(), root.join("sul.exe")).unwrap();
        let report = verify_all(root, &f.baseline, &f.selection, all).unwrap();
        assert_eq!(report.deviations.len(), 1, "{report:?}");
        assert_eq!(report.deviations[0].deviation, Deviation::NotAFile);

        // A folder replaced by a symlink to a folder holding the same bytes.
        let elsewhere = tempfile::tempdir().unwrap();
        fs::write(elsewhere.path().join("helper.dll"), b"a library").unwrap();
        fs::remove_file(root.join("sul.exe")).unwrap();
        fs::write(root.join("sul.exe"), b"the program").unwrap();
        fs::remove_dir_all(root.join("bin")).unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), root.join("bin")).unwrap();
        let report = verify_all(root, &f.baseline, &f.selection, all).unwrap();
        let d = report
            .deviations
            .iter()
            .find(|d| d.path == "bin/helper.dll")
            .unwrap();
        assert_eq!(d.deviation, Deviation::NotAFile, "{report:?}");
    }

    #[cfg(unix)]
    #[test]
    fn a_new_link_of_a_guarded_kind_is_unlisted_with_no_digest_and_is_never_hashed() {
        // Found by a deliberate-bug check, 2026-11-26. A planted link must be reported as a link: following it to hash
        // "what it points at" would describe somebody else's file as if it were SUL's.
        let f = fixture();
        let root = f.dir.path();
        let outside = tempfile::NamedTempFile::new().unwrap();
        fs::write(outside.path(), b"whatever the link points at").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("bin/planted.dll")).unwrap();
        fs::write(root.join("bin/plain.dll"), b"a plain new file").unwrap();

        let report = verify_all(root, &f.baseline, &f.selection, all).unwrap();
        let find = |name: &str| {
            report
                .deviations
                .iter()
                .find(|d| d.path == name)
                .unwrap_or_else(|| panic!("{name} not reported: {report:?}"))
        };
        assert_eq!(
            find("bin/planted.dll").deviation,
            Deviation::Unlisted { sha256: None },
            "a link is not followed"
        );
        // The control: a plain new file does get its digest, so the `None` above is the link's doing.
        assert!(matches!(
            &find("bin/plain.dll").deviation,
            Deviation::Unlisted { sha256: Some(h) } if h.len() == 64
        ));
    }

    #[test]
    fn a_folder_that_has_become_a_plain_file_is_not_a_file_and_confirmed() {
        // The baseline says a guarded file lives inside a folder; here a plain file stands where that folder was. That is
        // not "could not be read" (which could be an antivirus lock): the layout no longer matches, so it is settled.
        // Found 2026-11-26: this test used to expect `Unreadable { NotADirectory }`, the answer before `walk_chain` checked
        // every folder above the file; the module documents the stricter rule.
        let f = fixture();
        let mut entry = f.baseline.get("sul.exe").unwrap().clone();
        entry.path = "sul.exe/inside".into();
        let found = check_entry(f.dir.path(), &entry);
        assert_eq!(found, Some(Deviation::NotAFile));
        assert_eq!(found.unwrap().certainty(), Certainty::Confirmed);
    }

    #[test]
    fn a_file_that_cannot_be_read_is_suspicious_not_confirmed() {
        assert_eq!(
            Deviation::Unreadable {
                kind: "PermissionDenied".into()
            }
            .certainty(),
            Certainty::Suspicious
        );
    }

    #[test]
    fn deviations_are_described_without_a_path() {
        for d in [
            Deviation::Missing,
            Deviation::NotAFile,
            Deviation::Unreadable {
                kind: "PermissionDenied".into(),
            },
            Deviation::Unlisted { sha256: None },
            Deviation::Modified {
                expected_sha256: "a".into(),
                actual_sha256: "b".into(),
                expected_size: 1,
                actual_size: 2,
            },
        ] {
            let text = d.describe();
            assert!(
                !text.is_empty() && !text.contains('/') && !text.contains('\\'),
                "{text}"
            );
        }
    }
}
