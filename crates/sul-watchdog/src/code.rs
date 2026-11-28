//! The watchdog's failure codes, as `SUL-LEDGER/failures.md` and Ch. 12 §2.8 give them.
//!
//! | Code                            | Ledger letter | Trigger                                                  | Recovery the spec names                              |
//! |---------------------------------|---------------|----------------------------------------------------------|------------------------------------------------------|
//! | `E_WATCHDOG_TAMPER`             | E             | Trust primitive file modified                            | Mode D quarantine lifecycle; hive threat log         |
//! | `E_WATCHDOG_HOOK_DETECTED`      | none          | API hook in SUL's address space                          | Log, notify, optional auto-disable flip-model        |
//! | `E_WATCHDOG_RWX_REGION`         | none          | RWX region not mapped by SUL                             | Log, notify                                          |
//! | `E_WATCHDOG_FOREIGN_THREAD`     | none          | Thread starts outside every known-good module            | Log, notify                                          |
//! | `E_WATCHDOG_IMAGE_HOLLOWING`    | none          | Loaded image's code replaced rather than patched         | Log, notify                                          |
//! | `E_WATCHDOG_LIVE_CODE_MODIFIED` | none          | Signed live code no longer matches its record            | Log, notify; the caller unloads the owning feature   |
//! | `E_WATCHDOG_LIVE_CODE_REFUSED`  | none          | Live-code record failed a check                          | Log, notify; nothing is registered                   |
//!
//! The letter is the one in the code's own row of the ledger (D-152: code reports the row's letter, never one inferred from a
//! definition). The codes with no letter are detections, not failures: nothing is retried or restarted, so they have no
//! [`FailureMode`].
//!
//! The first three codes are the spec's own. The spec listed four process detections (RWX regions, API hooks, foreign
//! threads, DLL hollowing) but named three codes; the owner asked for whatever is best for security (2026-11-27), so the
//! fourth and the one the spec lacked get theirs (D-158), and the two live-code detections (D-159) have theirs from the
//! start: a detection with no stable code cannot be searched for in the hive-mind log or shown the same way twice.
//!
//! A test reads `SUL-LEDGER/failures.md` and fails if any row for these codes disagrees with this file.

use serde::{Deserialize, Serialize};
use sul_ipc_types::FailureMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchdogCode {
    /// A file the watchdog guards no longer matches the signed baseline.
    Tamper,
    /// A function in SUL's own address space starts with different bytes than the file it was loaded from.
    HookDetected,
    /// Memory in SUL's own process that is readable, writable and executable and that SUL did not make.
    RwxRegion,
    /// A thread in SUL's own process that starts outside every module that was loaded at the known-good point.
    ForeignThread,
    /// A loaded image whose code differs from its file in so many bytes that it was replaced, not patched.
    ImageHollowing,
    /// Live code SUL signed no longer matches what was signed (or cannot be read where it was registered).
    LiveCodeModified,
    /// A live-code record was presented and failed a check: signature, authority, order, producer or content.
    LiveCodeRefused,
}

impl WatchdogCode {
    pub const ALL: [WatchdogCode; 7] = [
        Self::Tamper,
        Self::HookDetected,
        Self::RwxRegion,
        Self::ForeignThread,
        Self::ImageHollowing,
        Self::LiveCodeModified,
        Self::LiveCodeRefused,
    ];

    /// The code as written in the ledger and in logs.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tamper => "E_WATCHDOG_TAMPER",
            Self::HookDetected => "E_WATCHDOG_HOOK_DETECTED",
            Self::RwxRegion => "E_WATCHDOG_RWX_REGION",
            Self::ForeignThread => "E_WATCHDOG_FOREIGN_THREAD",
            Self::ImageHollowing => "E_WATCHDOG_IMAGE_HOLLOWING",
            Self::LiveCodeModified => "E_WATCHDOG_LIVE_CODE_MODIFIED",
            Self::LiveCodeRefused => "E_WATCHDOG_LIVE_CODE_REFUSED",
        }
    }

    /// The ledger's trigger column.
    pub const fn trigger(self) -> &'static str {
        match self {
            Self::Tamper => "Trust primitive file modified",
            Self::HookDetected => "API hook in SUL's address space",
            Self::RwxRegion => "RWX region not mapped by SUL",
            Self::ForeignThread => {
                "Thread in SUL's process starts outside every module loaded at the known-good point"
            }
            Self::ImageHollowing => "Loaded image's code replaced rather than patched",
            Self::LiveCodeModified => "Signed live code no longer matches its record",
            Self::LiveCodeRefused => {
                "Live-code record failed a signature, authority, order, producer or content check"
            }
        }
    }

    /// What the spec says should happen. The watchdog only detects and reports (decision 0014); acting on this is the
    /// caller's job, so this is the sentence a caller or a person reads, not something the crate does.
    pub const fn recovery(self) -> &'static str {
        match self {
            Self::Tamper => "Mode D quarantine lifecycle; hive threat log",
            Self::HookDetected => "Log, notify, optional auto-disable flip-model",
            Self::RwxRegion | Self::ForeignThread | Self::ImageHollowing => "Log, notify",
            Self::LiveCodeModified => {
                "Log, notify; the caller unloads the feature that owns the code"
            }
            Self::LiveCodeRefused => "Log, notify; nothing is registered",
        }
    }

    /// The failure mode the ledger gives this code's row, if it gives one.
    pub const fn failure_mode(self) -> Option<FailureMode> {
        match self {
            Self::Tamper => Some(FailureMode::ModeE),
            Self::HookDetected
            | Self::RwxRegion
            | Self::ForeignThread
            | Self::ImageHollowing
            | Self::LiveCodeModified
            | Self::LiveCodeRefused => None,
        }
    }

    pub fn from_code(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn codes_round_trip_and_are_distinct() {
        for code in WatchdogCode::ALL {
            assert_eq!(WatchdogCode::from_code(code.as_str()), Some(code));
            assert!(code.as_str().starts_with("E_WATCHDOG_"));
        }
        assert_eq!(WatchdogCode::from_code("E_WATCHDOG_UNKNOWN"), None);
        assert_eq!(WatchdogCode::from_code("e_watchdog_tamper"), None);
        let mut names: Vec<_> = WatchdogCode::ALL.iter().map(|c| c.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), WatchdogCode::ALL.len());
        assert_eq!(names.len(), 7);
    }

    /// The ledger is the source of truth (README, D-152). Every row it has for a watchdog code must agree with this file
    /// on the letter and on the trigger; a code with no row at all is a failure too.
    #[test]
    fn the_ledger_rows_agree_with_the_codes() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../SUL-LEDGER/failures.md");
        let text = std::fs::read_to_string(&path).expect("SUL-LEDGER/failures.md");
        // The ledger escapes underscores for Markdown (`E\_WATCHDOG\_TAMPER`) and separates columns with tabs.
        let plain = text.replace("\r\n", "\n").replace('\\', "");
        for code in WatchdogCode::ALL {
            let mut rows = 0;
            for line in plain.lines() {
                let mut cells = line.split('\t');
                if cells.next() != Some(code.as_str()) {
                    continue;
                }
                rows += 1;
                let letter = cells.next().expect("a letter column").trim();
                let expected = match code.failure_mode() {
                    Some(FailureMode::ModeE) => "E",
                    None => "—",
                    Some(other) => panic!("unexpected mode {other:?}"),
                };
                assert_eq!(letter, expected, "{} letter in: {line}", code.as_str());
                let trigger = cells.next().expect("a trigger column").trim();
                assert_eq!(trigger, code.trigger(), "{} trigger", code.as_str());
            }
            assert!(rows >= 1, "{} has no row in failures.md", code.as_str());
        }
    }

    /// The other direction: a watchdog code that is in the ledger and not in this file would never be raised. The ledger
    /// is searched for every `E_WATCHDOG_*` it names.
    #[test]
    fn every_watchdog_code_in_the_ledger_is_known_here() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../SUL-LEDGER/failures.md");
        let plain = std::fs::read_to_string(&path)
            .expect("SUL-LEDGER/failures.md")
            .replace('\\', "");
        let mut seen = std::collections::BTreeSet::new();
        for line in plain.lines() {
            if let Some(first) = line.split('\t').next() {
                if first.starts_with("E_WATCHDOG_") && line.contains('\t') {
                    seen.insert(first.to_owned());
                }
            }
        }
        assert!(!seen.is_empty());
        for name in &seen {
            assert!(
                WatchdogCode::from_code(name).is_some(),
                "{name} is in the ledger but not in WatchdogCode"
            );
        }
        assert_eq!(seen.len(), WatchdogCode::ALL.len());
    }
}
