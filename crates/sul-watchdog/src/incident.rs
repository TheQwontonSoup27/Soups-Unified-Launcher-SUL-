//! What the watchdog reports: an [`Incident`] for the person and the caller, and the [`ThreatRecord`] that is all the threat
//! log and the hive-mind ever see.
//!
//! The split is the privacy rule of Ch. 14 §14.13.5: "the threat log contains file hashes, detection reasons, and event
//! types, never user file contents, never user paths beyond the file's own hash". An [`Incident`] may name the guarded file
//! (`bin/helper.dll`, relative to SUL's own install, and the person needs it to see what happened) and may carry a sentence.
//! A [`ThreatRecord`] has no such field at all: its `reason` is one word from a fixed list ([`Reason`]), so there is nowhere
//! for a path to hide and the type cannot be made to carry one.

use crate::code::WatchdogCode;
use crate::guard::{Deviation, PathDeviation};
use crate::scan::{ScanFinding, ScanKind};
use serde::{Deserialize, Serialize};
use sul_trust::findings::{Certainty, Finding};

/// Why an incident was raised, in one word from a closed list. This is the only reason text the log carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    HashMismatch,
    FileMissing,
    NotAPlainFile,
    FileUnreadable,
    UnlistedFile,
    RwxRegion,
    InlineHook,
    ForeignThread,
    ImageHollowing,
}

impl Reason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HashMismatch => "hash_mismatch",
            Self::FileMissing => "file_missing",
            Self::NotAPlainFile => "not_a_plain_file",
            Self::FileUnreadable => "file_unreadable",
            Self::UnlistedFile => "unlisted_file",
            Self::RwxRegion => "rwx_region",
            Self::InlineHook => "inline_hook",
            Self::ForeignThread => "foreign_thread",
            Self::ImageHollowing => "image_hollowing",
        }
    }
}

/// What an incident is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    /// A guarded file: its path relative to SUL's own install, and the digest that identifies it.
    File {
        label: String,
        sha256: Option<String>,
    },
    /// Something in SUL's own process (an address range, `module!function`, a thread).
    Process { what: String },
}

/// One detection, for the person and the caller. Not written to the threat log as it is: see [`Incident::to_record`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Incident {
    /// The ledger code, when the detection has one (decision 0014 lists the ones that do not).
    pub code: Option<WatchdogCode>,
    /// The owning module's own code for a modified artifact, when the baseline gave one.
    pub specific_code: Option<String>,
    pub certainty: Certainty,
    pub reason: Reason,
    pub subject: Subject,
    /// A sentence for a person. May name the file; never goes into a [`ThreatRecord`].
    pub detail: String,
    pub observed_unix: u64,
}

impl Incident {
    pub fn from_deviation(d: &PathDeviation, observed_unix: u64) -> Self {
        let (code, reason, sha256) = match &d.deviation {
            Deviation::Modified { actual_sha256, .. } => (
                Some(WatchdogCode::Tamper),
                Reason::HashMismatch,
                Some(actual_sha256.clone()),
            ),
            Deviation::Missing => (Some(WatchdogCode::Tamper), Reason::FileMissing, None),
            Deviation::NotAFile => (Some(WatchdogCode::Tamper), Reason::NotAPlainFile, None),
            Deviation::Unreadable { .. } => (None, Reason::FileUnreadable, None),
            Deviation::Unlisted { sha256 } => (None, Reason::UnlistedFile, sha256.clone()),
        };
        let what = d.role.map_or("file", |r| r.label());
        Self {
            code,
            specific_code: d.artifact_code.clone(),
            certainty: d.deviation.certainty(),
            reason,
            subject: Subject::File {
                label: d.path.clone(),
                sha256,
            },
            detail: format!("{what} {} {}", d.path, d.deviation.describe()),
            observed_unix,
        }
    }

    pub fn from_scan(f: &ScanFinding, observed_unix: u64) -> Self {
        let reason = match f.kind {
            ScanKind::RwxRegion => Reason::RwxRegion,
            ScanKind::InlineHook => Reason::InlineHook,
            ScanKind::ForeignThread => Reason::ForeignThread,
            ScanKind::ImageHollowing => Reason::ImageHollowing,
        };
        Self {
            code: f.kind.code(),
            specific_code: None,
            certainty: f.certainty,
            reason,
            subject: Subject::Process {
                what: f.subject.clone(),
            },
            detail: format!("{}: {}", f.subject, f.detail),
            observed_unix,
        }
    }

    /// The same incident in the words every trust check uses (`sul_trust::findings`).
    pub fn to_finding(&self) -> Finding {
        let code = self.code.map_or(self.reason.as_str(), WatchdogCode::as_str);
        let path = match &self.subject {
            Subject::File { label, .. } => label.as_str(),
            Subject::Process { what } => what.as_str(),
        };
        match self.certainty {
            Certainty::Confirmed => Finding::confirmed(code, path, self.detail.clone()),
            Certainty::Suspicious => Finding::suspicious(code, path, self.detail.clone()),
        }
    }

    /// What may be logged and shared: no label, no sentence, no process detail.
    pub fn to_record(&self) -> ThreatRecord {
        let (event, sha256) = match &self.subject {
            Subject::File { sha256, .. } => ("file_deviation", sha256.clone()),
            Subject::Process { .. } => ("process_scan", None),
        };
        ThreatRecord {
            event: event.to_owned(),
            code: self.code.map(|c| c.as_str().to_owned()),
            specific_code: self.specific_code.clone(),
            confirmed: self.certainty == Certainty::Confirmed,
            reason: self.reason,
            sha256,
            observed_unix: self.observed_unix,
        }
    }
}

/// One line of the threat log (Ch. 14 §14.13.5). Fields: what kind of event, which code, how sure, the one-word reason, the
/// digest of the file when it is about a file, and when. Nothing else, on purpose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreatRecord {
    pub event: String,
    pub code: Option<String>,
    pub specific_code: Option<String>,
    pub confirmed: bool,
    pub reason: Reason,
    pub sha256: Option<String>,
    pub observed_unix: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::baseline::Role;
    use crate::scan::ScanFinding;

    fn deviation(d: Deviation) -> PathDeviation {
        PathDeviation {
            path: "bin/helper.dll".into(),
            role: Some(Role::Binary),
            artifact_code: Some("E_HELPER_TAMPER".into()),
            deviation: d,
        }
    }

    #[test]
    fn a_modified_file_is_tamper_confirmed_and_carries_the_new_digest() {
        let inc = Incident::from_deviation(
            &deviation(Deviation::Modified {
                expected_sha256: "a".repeat(64),
                actual_sha256: "b".repeat(64),
                expected_size: 1,
                actual_size: 2,
            }),
            42,
        );
        assert_eq!(inc.code, Some(WatchdogCode::Tamper));
        assert_eq!(inc.certainty, Certainty::Confirmed);
        assert_eq!(inc.reason, Reason::HashMismatch);
        assert_eq!(inc.specific_code.as_deref(), Some("E_HELPER_TAMPER"));
        assert_eq!(
            inc.subject,
            Subject::File {
                label: "bin/helper.dll".into(),
                sha256: Some("b".repeat(64))
            }
        );
        assert!(inc.detail.starts_with("binary bin/helper.dll "));
        let f = inc.to_finding();
        assert_eq!(f.code, "E_WATCHDOG_TAMPER");
        assert_eq!(f.certainty, Certainty::Confirmed);
    }

    #[test]
    fn the_record_of_a_file_incident_carries_the_digest_found_and_nothing_else_about_the_file() {
        // The digest is the point of the log: "every quarantined file with its hash" (Ch. 14 §14.13.5), the fingerprint the
        // hive-mind can match without seeing the file. Found by a deliberate-bug check, 2026-11-26: nothing asserted that the
        // record keeps it.
        let record = |d| Incident::from_deviation(&deviation(d), 9).to_record();
        let modified = record(Deviation::Modified {
            expected_sha256: "a".repeat(64),
            actual_sha256: "b".repeat(64),
            expected_size: 1,
            actual_size: 2,
        });
        assert_eq!(modified.event, "file_deviation");
        assert_eq!(
            modified.sha256,
            Some("b".repeat(64)),
            "the digest found, not the one expected"
        );
        let planted = record(Deviation::Unlisted {
            sha256: Some("c".repeat(64)),
        });
        assert_eq!(planted.sha256, Some("c".repeat(64)));
        assert_eq!(
            record(Deviation::Unlisted { sha256: None }).sha256,
            None,
            "a link has no digest to share"
        );
        assert_eq!(record(Deviation::Missing).sha256, None);
        assert_eq!(record(Deviation::NotAFile).sha256, None);
    }

    #[test]
    fn each_deviation_maps_to_the_code_and_reason_the_spec_allows() {
        let cases = [
            (
                Deviation::Missing,
                Some(WatchdogCode::Tamper),
                Reason::FileMissing,
                Certainty::Confirmed,
            ),
            (
                Deviation::NotAFile,
                Some(WatchdogCode::Tamper),
                Reason::NotAPlainFile,
                Certainty::Confirmed,
            ),
            (
                Deviation::Unreadable {
                    kind: "PermissionDenied".into(),
                },
                None,
                Reason::FileUnreadable,
                Certainty::Suspicious,
            ),
            (
                Deviation::Unlisted {
                    sha256: Some("c".repeat(64)),
                },
                None,
                Reason::UnlistedFile,
                Certainty::Suspicious,
            ),
        ];
        for (d, code, reason, certainty) in cases {
            let inc = Incident::from_deviation(&deviation(d), 1);
            assert_eq!(
                (inc.code, inc.reason, inc.certainty),
                (code, reason, certainty)
            );
            // A finding without a ledger code is reported under its reason, never under a code nobody defined.
            if code.is_none() {
                assert_eq!(inc.to_finding().code, reason.as_str());
            }
        }
    }

    #[test]
    fn a_scan_finding_becomes_a_process_incident_with_its_own_code_or_none() {
        let mk = |kind| ScanFinding {
            kind,
            certainty: Certainty::Suspicious,
            subject: "user32.dll!SetWindowsHookExW".into(),
            detail: "starts with a relative jump".into(),
            allowed_because: None,
        };
        let hook = Incident::from_scan(&mk(ScanKind::InlineHook), 9);
        assert_eq!(hook.code, Some(WatchdogCode::HookDetected));
        assert_eq!(hook.reason, Reason::InlineHook);
        assert_eq!(hook.certainty, Certainty::Suspicious);
        assert!(matches!(hook.subject, Subject::Process { .. }));
        assert_eq!(
            Incident::from_scan(&mk(ScanKind::RwxRegion), 9).code,
            Some(WatchdogCode::RwxRegion)
        );
        assert_eq!(
            Incident::from_scan(&mk(ScanKind::ForeignThread), 9).code,
            None
        );
        assert_eq!(
            Incident::from_scan(&mk(ScanKind::ImageHollowing), 9).code,
            None
        );
    }

    /// The privacy rule, tested the way it matters: put recognisable private text into every field an incident can carry
    /// and check that none of it reaches the serialised record.
    #[test]
    fn a_threat_record_never_carries_a_path_a_sentence_or_process_detail() {
        let secret = "C:\\Users\\alice\\Documents\\taxes_2026";
        let from_file = Incident::from_deviation(
            &PathDeviation {
                path: secret.replace('\\', "/"),
                role: Some(Role::Binary),
                artifact_code: None,
                deviation: Deviation::Modified {
                    expected_sha256: "a".repeat(64),
                    actual_sha256: "b".repeat(64),
                    expected_size: 1,
                    actual_size: 2,
                },
            },
            1,
        );
        let from_scan = Incident::from_scan(
            &ScanFinding {
                kind: ScanKind::ForeignThread,
                certainty: Certainty::Suspicious,
                subject: secret.into(),
                detail: secret.into(),
                allowed_because: None,
            },
            1,
        );
        for inc in [from_file, from_scan] {
            assert!(
                inc.detail.contains("alice"),
                "the local incident keeps its detail"
            );
            let json = serde_json::to_string(&inc.to_record()).unwrap();
            assert!(
                !json.contains("alice") && !json.contains("taxes") && !json.contains("Users"),
                "{json}"
            );
            assert!(
                !json.contains('\\') && !json.contains("Documents"),
                "{json}"
            );
        }
    }

    #[test]
    fn records_round_trip_and_name_their_event() {
        let rec = Incident::from_deviation(&deviation(Deviation::Missing), 5).to_record();
        assert_eq!(rec.event, "file_deviation");
        assert_eq!(rec.code.as_deref(), Some("E_WATCHDOG_TAMPER"));
        assert!(rec.confirmed);
        let back: ThreatRecord =
            serde_json::from_str(&serde_json::to_string(&rec).unwrap()).unwrap();
        assert_eq!(back, rec);
        let scan = Incident::from_scan(
            &ScanFinding {
                kind: ScanKind::RwxRegion,
                certainty: Certainty::Suspicious,
                subject: "0x1000".into(),
                detail: "d".into(),
                allowed_because: None,
            },
            5,
        )
        .to_record();
        assert_eq!(scan.event, "process_scan");
        assert_eq!(scan.sha256, None);
        assert!(!scan.confirmed);
    }

    #[test]
    fn reasons_serialise_as_their_names() {
        for r in [
            Reason::HashMismatch,
            Reason::FileMissing,
            Reason::NotAPlainFile,
            Reason::FileUnreadable,
            Reason::UnlistedFile,
            Reason::RwxRegion,
            Reason::InlineHook,
            Reason::ForeignThread,
            Reason::ImageHollowing,
        ] {
            assert_eq!(
                serde_json::to_string(&r).unwrap(),
                format!("\"{}\"", r.as_str())
            );
        }
    }
}
