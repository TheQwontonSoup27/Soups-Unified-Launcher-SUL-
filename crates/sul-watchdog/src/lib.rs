//! `sul-watchdog`: the integrity watchdog (Ch. 12 §2.8, Ch. 14 §14.13 and §14.14). R0 layer 3, cut 3 (D-157, decision 0014).
//!
//! It answers two questions about SUL itself, and only reports; it does not act. Acting on a report (quarantining a file,
//! turning a feature off, telling the person) belongs to the caller, because only the caller knows whether a file is in use,
//! which feature a hook touches and where the person is looking.
//!
//! 1. **Are the files SUL trusts still the files that were installed?** A [`baseline`] records a SHA-256 for each guarded
//!    file and can be signed (Ed25519, checked before anything is read from it). The [`guard`] compares disk with it, never
//!    following a link. The [`monitor`] does that the moment the operating system reports a change, and again as a backstop.
//! 2. **Is something foreign inside SUL's own process?** The [`scan`] module analyses a snapshot of the process for injected
//!    RWX memory, API hooks, foreign threads and hollowed images, with a user-visible allowlist for antivirus and overlays.
//!
//! Everything it finds is an [`incident::Incident`] for the caller and a [`incident::ThreatRecord`] for the [`threat_log`]:
//! the record has no path and no free text by construction (Ch. 14 §14.13.5). [`Watchdog`] ties these together.
//!
//! | What it gives                                                                      | Module           |
//! |------------------------------------------------------------------------------------|------------------|
//! | The three failure codes, checked against the ledger                                | [`code`]         |
//! | The signed baseline and the rules for what is guarded                              | [`baseline`]     |
//! | Disk against baseline, one shot                                                    | [`guard`]        |
//! | The same, event-driven, with debounce, overflow handling and a backstop            | [`monitor`]      |
//! | Process snapshot analysis and the allowlist                                        | [`scan`]         |
//! | Incidents, and the privacy-safe record                                             | [`incident`]     |
//! | A hash-chained append-only log and the sink trait the hive-mind will plug into     | [`threat_log`]   |
//!
//! **What is not here, and what that means.** No Windows API is called. A [`scan::ProcessProbe`] that reads a live process,
//! the `BELOW_NORMAL` priority of the watchdog's thread, and the thread itself are the next cut (decision 0014): until then the
//! process scan has only ever seen made-up snapshots, and nothing sets the watchdog's priority. Nothing decides what the
//! installed product guards (the installer's layout is not fixed), nothing signs a baseline, and no `sul-hive-mind` exists to
//! receive records. Green tests here mean the logic is right for the inputs it was given, not that SUL is protected.

#![forbid(unsafe_code)]

pub mod baseline;
pub mod code;
pub mod guard;
pub mod incident;
pub mod monitor;
pub mod scan;
pub mod threat_log;

pub use code::WatchdogCode;
pub use incident::{Incident, Reason, Subject, ThreatRecord};

use monitor::{ChangeSource, Monitor, Update};
use scan::{Allowlist, Expectations, ProcessProbe, ScanReport};
use std::time::Duration;
use sul_core::Result;
use sul_trust::manifest::now_unix;
use threat_log::ThreatSink;

/// What one call to the watchdog found.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// New incidents, already handed to the sink.
    pub incidents: Vec<Incident>,
    /// Guarded files that were deviating and match the baseline again (paths relative to the root).
    pub restored: Vec<String>,
    /// Set when the sink could not take a record. The incidents are still returned: a detection reaches the caller even
    /// when the log cannot be written.
    pub log_error: Option<String>,
}

/// The monitor and the threat log together.
pub struct Watchdog<S: ChangeSource, K: ThreatSink> {
    monitor: Monitor<S>,
    sink: K,
    clock: fn() -> u64,
}

impl<S: ChangeSource, K: ThreatSink> Watchdog<S, K> {
    pub fn new(monitor: Monitor<S>, sink: K) -> Self {
        Self::with_clock(monitor, sink, now_unix)
    }

    /// As [`new`](Self::new) with a clock of the caller's choosing (tests).
    pub fn with_clock(monitor: Monitor<S>, sink: K, clock: fn() -> u64) -> Self {
        Self {
            monitor,
            sink,
            clock,
        }
    }

    fn log(&mut self, incidents: &[Incident]) -> Option<String> {
        let mut first_error = None;
        for incident in incidents {
            if let Err(e) = self.sink.record(&incident.to_record()) {
                first_error.get_or_insert_with(|| e.to_string());
            }
        }
        first_error
    }

    fn outcome(&mut self, updates: Vec<Update>) -> Outcome {
        let now = (self.clock)();
        let mut outcome = Outcome::default();
        for update in updates {
            match update {
                Update::Deviation(d) => outcome.incidents.push(Incident::from_deviation(&d, now)),
                Update::Restored { path } => outcome.restored.push(path),
            }
        }
        outcome.log_error = self.log(&outcome.incidents);
        outcome
    }

    /// The full check at the start of a session: every guarded file, models included.
    pub fn session_start(&mut self) -> Result<Outcome> {
        let updates = self.monitor.session_start()?;
        Ok(self.outcome(updates))
    }

    /// Wait up to `timeout` for a change and check what changed (see [`Monitor::pump`]). Call it in a loop from the
    /// watchdog's own thread.
    pub fn pump(&mut self, timeout: Duration) -> Result<Outcome> {
        let updates = self.monitor.pump(timeout)?;
        Ok(self.outcome(updates))
    }

    /// Look at SUL's own process through `probe`, log what is suspicious and return both the report (which includes what
    /// the allowlist let through, for the person to see) and the new incidents.
    pub fn scan_process(
        &mut self,
        probe: &mut dyn ProcessProbe,
        expect: &Expectations,
        allow: &Allowlist,
    ) -> Result<(ScanReport, Outcome)> {
        let report = scan::scan(probe, expect, allow)?;
        let now = (self.clock)();
        let incidents: Vec<Incident> = report
            .findings
            .iter()
            .map(|f| Incident::from_scan(f, now))
            .collect();
        let log_error = self.log(&incidents);
        Ok((
            report,
            Outcome {
                incidents,
                restored: Vec::new(),
                log_error,
            },
        ))
    }

    pub fn monitor(&self) -> &Monitor<S> {
        &self.monitor
    }

    pub fn into_sink(self) -> K {
        self.sink
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::baseline::{Baseline, Pattern, Role, Selection};
    use crate::monitor::{Change, MonitorConfig};
    use crate::scan::{Prologue, Protection, Region, RegionKind, Snapshot};
    use crate::threat_log::MemorySink;
    use sha2::Digest;
    use std::collections::VecDeque;
    use std::fs;
    use std::path::Path;
    use sul_core::Error;

    const NOW: u64 = 1_700_000_123;

    fn fixed_clock() -> u64 {
        NOW
    }

    /// Hands out scripted batches of changes, one per call, then reports a timeout.
    struct Script(VecDeque<Vec<Change>>);

    impl ChangeSource for Script {
        fn wait(&mut self, _: Duration) -> Result<Vec<Change>> {
            Ok(self.0.pop_front().unwrap_or_default())
        }
    }

    /// A sink whose log can never be written (a full disk, a locked file).
    struct BrokenLog;

    impl ThreatSink for BrokenLog {
        fn record(&mut self, _: &ThreatRecord) -> Result<()> {
            Err(Error::Unsupported("the log cannot be written"))
        }
    }

    struct Probe(Snapshot);

    impl ProcessProbe for Probe {
        fn snapshot(&mut self) -> Result<Snapshot> {
            Ok(self.0.clone())
        }
    }

    struct BlindProbe;

    impl ProcessProbe for BlindProbe {
        fn snapshot(&mut self) -> Result<Snapshot> {
            Err(Error::Unsupported("the process cannot be read"))
        }
    }

    fn install() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("bin")).unwrap();
        fs::write(dir.path().join("sul.exe"), b"the program").unwrap();
        fs::write(dir.path().join("bin/helper.dll"), b"a library").unwrap();
        dir
    }

    fn quick() -> MonitorConfig {
        MonitorConfig {
            quiet: Duration::from_millis(1),
            max_batch: Duration::from_millis(20),
            full_rescan_every: None,
        }
    }

    fn watchdog<K: ThreatSink>(
        root: &Path,
        script: Vec<Vec<Change>>,
        sink: K,
    ) -> Watchdog<Script, K> {
        let selection = Selection::new()
            .rule(Pattern::Exact("sul.exe".into()), Role::TrustPrimitive)
            .rule(Pattern::Extension("dll".into()), Role::Binary);
        let baseline = Baseline::compute(root, "t", &selection, 1).unwrap();
        let monitor = Monitor::new(root, baseline, selection, Script(script.into()), quick());
        Watchdog::with_clock(monitor, sink, fixed_clock)
    }

    fn rwx_snapshot() -> Snapshot {
        Snapshot {
            regions: vec![Region {
                base: 0x10000,
                size: 0x2000,
                protection: Protection::READ_WRITE_EXECUTE,
                kind: RegionKind::Private,
                module: None,
            }],
            ..Snapshot::default()
        }
    }

    #[test]
    fn a_clean_install_gives_no_incident_and_logs_nothing() {
        let dir = install();
        let mut dog = watchdog(dir.path(), vec![], MemorySink::default());
        assert_eq!(dog.session_start().unwrap(), Outcome::default());
        assert!(dog.into_sink().records.is_empty());
    }

    #[test]
    fn a_modified_file_is_an_incident_for_the_caller_and_a_path_free_record_for_the_log() {
        let dir = install();
        let mut dog = watchdog(dir.path(), vec![], MemorySink::default());
        fs::write(dir.path().join("sul.exe"), b"changed").unwrap();

        let outcome = dog.session_start().unwrap();
        assert_eq!(outcome.log_error, None);
        assert!(outcome.restored.is_empty());
        assert_eq!(outcome.incidents.len(), 1, "{outcome:?}");
        let incident = &outcome.incidents[0];
        assert_eq!(incident.code, Some(WatchdogCode::Tamper));
        assert_eq!(incident.reason, Reason::HashMismatch);
        assert_eq!(incident.observed_unix, NOW, "the injected clock stamps it");
        assert!(incident.detail.contains("sul.exe"), "{}", incident.detail);

        let records = dog.into_sink().records;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0], incident.to_record());
        assert_eq!(records[0].reason, Reason::HashMismatch);
        assert_eq!(records[0].observed_unix, NOW);
        assert_eq!(
            records[0].sha256.as_deref(),
            Some(hex::encode(sha2::Sha256::digest(b"changed")).as_str()),
            "the log keeps the digest of the file as found"
        );
        let json = serde_json::to_string(&records[0]).unwrap();
        assert!(
            !json.contains("sul.exe") && !json.contains("helper"),
            "the log must not carry a path: {json}"
        );
    }

    #[test]
    fn a_watchdog_made_with_new_reads_the_real_clock() {
        let dir = install();
        let selection =
            Selection::new().rule(Pattern::Exact("sul.exe".into()), Role::TrustPrimitive);
        let baseline = Baseline::compute(dir.path(), "t", &selection, 1).unwrap();
        let monitor = Monitor::new(
            dir.path(),
            baseline,
            selection,
            Script(VecDeque::new()),
            quick(),
        );
        let mut dog = Watchdog::new(monitor, MemorySink::default());
        fs::write(dir.path().join("sul.exe"), b"changed").unwrap();
        let outcome = dog.session_start().unwrap();
        // Any real clock is long past 2023-11-14 (the fixed test value above); 0 would mean the clock was never read.
        assert!(outcome.incidents[0].observed_unix > NOW, "{outcome:?}");
    }

    #[test]
    fn a_file_put_back_is_reported_to_the_caller_as_restored_and_is_not_a_threat_record() {
        let dir = install();
        let sul = Change::Path(dir.path().join("sul.exe"));
        let script = vec![vec![sul.clone()], vec![], vec![sul], vec![]];
        let mut dog = watchdog(dir.path(), script, MemorySink::default());

        fs::write(dir.path().join("sul.exe"), b"changed").unwrap();
        let first = dog.pump(Duration::from_millis(1)).unwrap();
        assert_eq!(first.incidents.len(), 1, "{first:?}");

        fs::write(dir.path().join("sul.exe"), b"the program").unwrap();
        let second = dog.pump(Duration::from_millis(1)).unwrap();
        assert!(second.incidents.is_empty(), "{second:?}");
        assert_eq!(second.restored, vec!["sul.exe".to_owned()]);
        assert_eq!(
            dog.into_sink().records.len(),
            1,
            "only the tamper is logged; a restore is not a detection"
        );
    }

    #[test]
    fn a_detection_still_reaches_the_caller_when_the_log_cannot_be_written() {
        let dir = install();
        let mut dog = watchdog(dir.path(), vec![], BrokenLog);
        fs::write(dir.path().join("sul.exe"), b"changed").unwrap();
        fs::write(dir.path().join("bin/helper.dll"), b"changed too").unwrap();

        let outcome = dog.session_start().unwrap();
        assert_eq!(outcome.incidents.len(), 2, "{outcome:?}");
        let why = outcome
            .log_error
            .expect("the failed write must be reported");
        assert!(why.contains("the log cannot be written"), "{why}");
    }

    #[test]
    fn a_process_scan_logs_what_it_finds_and_shows_what_the_allowlist_let_through() {
        let mut snapshot = rwx_snapshot();
        snapshot.prologues.push(Prologue {
            module: "kernel32.dll".into(),
            function: "CreateFileW".into(),
            in_memory: vec![0xE9, 1, 2, 3, 4],
            on_disk: vec![0x48, 0x89, 0x5C, 0x24, 0x08],
            target_module: Some("av_hook.dll".into()),
        });
        let mut allow = Allowlist::new();
        allow.allow("av_hook.dll", "my antivirus", 5);

        let dir = install();
        let mut dog = watchdog(dir.path(), vec![], MemorySink::default());
        let (report, outcome) = dog
            .scan_process(&mut Probe(snapshot), &Expectations::default(), &allow)
            .unwrap();

        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(
            report.allowed.len(),
            1,
            "the person is shown what was let through"
        );
        assert_eq!(outcome.incidents.len(), 1, "{outcome:?}");
        assert_eq!(outcome.incidents[0].code, Some(WatchdogCode::RwxRegion));
        assert_eq!(outcome.incidents[0].observed_unix, NOW);
        assert_eq!(outcome.log_error, None);

        let records = dog.into_sink().records;
        assert_eq!(
            records.len(),
            1,
            "an allowed finding is not a threat record"
        );
        assert_eq!(records[0].event, "process_scan");
        assert_eq!(records[0].reason, Reason::RwxRegion);
        assert_eq!(records[0].sha256, None);
        let json = serde_json::to_string(&records[0]).unwrap();
        assert!(!json.contains("0x10000"), "no address in the log: {json}");
    }

    #[test]
    fn a_clean_process_gives_an_empty_outcome_and_a_failing_probe_is_an_error_that_logs_nothing() {
        let dir = install();
        let mut dog = watchdog(dir.path(), vec![], MemorySink::default());
        let (report, outcome) = dog
            .scan_process(
                &mut Probe(Snapshot::default()),
                &Expectations::default(),
                &Allowlist::new(),
            )
            .unwrap();
        assert!(report.is_clean());
        assert_eq!(outcome, Outcome::default());

        let failed = dog.scan_process(&mut BlindProbe, &Expectations::default(), &Allowlist::new());
        assert!(failed.is_err());
        assert!(dog.into_sink().records.is_empty());
    }

    #[test]
    fn the_failed_log_is_reported_for_a_scan_too() {
        let dir = install();
        let mut dog = watchdog(dir.path(), vec![], BrokenLog);
        let (_, outcome) = dog
            .scan_process(
                &mut Probe(rwx_snapshot()),
                &Expectations::default(),
                &Allowlist::new(),
            )
            .unwrap();
        assert_eq!(outcome.incidents.len(), 1);
        assert!(outcome.log_error.is_some());
    }

    #[test]
    fn the_monitor_can_be_looked_at_through_the_watchdog() {
        let dir = install();
        let mut dog = watchdog(dir.path(), vec![], MemorySink::default());
        fs::write(dir.path().join("sul.exe"), b"changed").unwrap();
        dog.session_start().unwrap();
        assert_eq!(dog.monitor().open_deviations().len(), 1);
    }
}
