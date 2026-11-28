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
//! 3. **What about code that did not exist when the baseline was signed?** Hot updates, things this machine builds, generated
//!    code and hook trampolines are signed as *live code* (D-159, [`live_code`]) and the watchdog verifies them instead of
//!    flagging them or skipping them. Files go into the baseline the monitor checks ([`Watchdog::admit_file`]); memory is
//!    read and hashed by the watchdog itself on every scan ([`Watchdog::admit_region`]).
//!
//! Everything it finds is an [`incident::Incident`] for the caller and a [`incident::ThreatRecord`] for the [`threat_log`]:
//! the record has no path and no free text by construction (Ch. 14 §14.13.5). [`Watchdog`] ties these together.
//!
//! | What it gives                                                                      | Module           |
//! |------------------------------------------------------------------------------------|------------------|
//! | The seven failure codes, checked against the ledger                               | [`code`]         |
//! | The signed baseline and the rules for what is guarded                              | [`baseline`]     |
//! | Disk against baseline, one shot                                                    | [`guard`]        |
//! | The same, event-driven, with debounce, overflow handling and a backstop            | [`monitor`]      |
//! | Process snapshot analysis and the allowlist                                        | [`scan`]         |
//! | Signed live code: the records, the ledgers and every refusal                       | [`live_code`]    |
//! | Incidents, and the privacy-safe record                                             | [`incident`]     |
//! | A hash-chained append-only log and the sink trait the hive-mind will plug into     | [`threat_log`]   |
//!
//! **What is not here, and what that means.** No Windows API is called. A [`scan::ProcessProbe`] that reads a live process,
//! the `BELOW_NORMAL` priority of the watchdog's thread, and the thread itself are the next cut (decision 0014): until then the
//! process scan has only ever seen made-up snapshots, and nothing sets the watchdog's priority. Nothing decides what the
//! installed product guards (the installer's layout is not fixed), nothing signs a baseline, and no `sul-hive-mind` exists to
//! receive records. For live code, the signer is the vault (layer 4) and does not exist yet, and nothing reads a live
//! process's memory yet: a test digester and a test probe stand in. Green tests here mean the logic is right for the inputs
//! it was given, not that SUL is protected.

#![forbid(unsafe_code)]

pub mod baseline;
pub mod code;
pub mod guard;
pub mod incident;
pub mod live_code;
pub mod monitor;
pub mod scan;
pub mod threat_log;

pub use code::WatchdogCode;
pub use incident::{Incident, Reason, Subject, ThreatRecord};

use baseline::Baseline;
use live_code::{LiveCodeLedger, RegionDigester, Scope, SignedRecord};
use monitor::{ChangeSource, Monitor, Update};
use scan::{Allowlist, Expectations, ProcessProbe, ScanReport};
use std::time::Duration;
use sul_core::{Error, Result};
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

/// What presenting one live-code record did (D-159).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Admission {
    /// The serial the record was admitted under, or `None` when it was refused. A refusal is not an error of the call: it is
    /// a detection, and it is in `outcome` and in the log.
    pub serial: Option<u64>,
    pub outcome: Outcome,
}

/// What retiring live code did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Retired {
    /// Whether a record with that serial was being checked.
    pub removed: bool,
    pub outcome: Outcome,
}

/// The signed baseline and the two ledgers that sit on it.
struct LiveCode {
    /// The baseline the persistent ledger is rooted in. The monitor checks the *effective* one (this plus attested files).
    base: Baseline,
    persistent: LiveCodeLedger,
    session: LiveCodeLedger,
}

/// The monitor and the threat log together.
pub struct Watchdog<S: ChangeSource, K: ThreatSink> {
    monitor: Monitor<S>,
    sink: K,
    clock: fn() -> u64,
    live: Option<LiveCode>,
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
            live: None,
        }
    }

    /// Turn on live-code attestation (D-159): new code made at run time is signed as live code and verified instead of being
    /// flagged. The signed baseline is the one the monitor holds now, so call this before anything else changes it. Both
    /// ledgers must be new (records are presented through [`admit_file`](Self::admit_file) and
    /// [`admit_region`](Self::admit_region), so that the monitor learns of each), the first persistent and the second
    /// session-scoped.
    pub fn with_live_code(
        mut self,
        persistent: LiveCodeLedger,
        session: LiveCodeLedger,
    ) -> Result<Self> {
        if self.live.is_some() {
            return Err(Error::Invalid("live code is already set up".into()));
        }
        if persistent.scope() != Scope::Persistent || session.scope() != Scope::Session {
            return Err(Error::Invalid(
                "the persistent and the session live-code ledgers were given the wrong way round"
                    .into(),
            ));
        }
        if !(persistent.files().is_empty()
            && persistent.regions().is_empty()
            && session.files().is_empty()
            && session.regions().is_empty())
        {
            return Err(Error::Invalid(
                "a live-code ledger must start empty: present its records through the watchdog"
                    .into(),
            ));
        }
        self.live = Some(LiveCode {
            base: self.monitor.baseline().clone(),
            persistent,
            session,
        });
        Ok(self)
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

    /// A file record whose time is up stops vouching for its file: the monitor is told, and the file is then judged by the
    /// baseline alone (D-159). Memory records that expired go quietly: the code they covered is the caller's to unload.
    fn expire_live(&mut self) -> Result<Vec<Update>> {
        let now = (self.clock)();
        let Some(live) = self.live.as_mut() else {
            return Ok(Vec::new());
        };
        live.session.prune_expired(now);
        if live.persistent.prune_expired(now) == 0 {
            return Ok(Vec::new());
        }
        let effective = live.persistent.effective_baseline(&live.base, now)?;
        self.monitor.rebase(effective)
    }

    /// The full check at the start of a session: every guarded file, models included.
    pub fn session_start(&mut self) -> Result<Outcome> {
        let mut updates = self.expire_live()?;
        updates.extend(self.monitor.session_start()?);
        Ok(self.outcome(updates))
    }

    /// Wait up to `timeout` for a change and check what changed (see [`Monitor::pump`]). Call it in a loop from the
    /// watchdog's own thread.
    pub fn pump(&mut self, timeout: Duration) -> Result<Outcome> {
        let mut updates = self.expire_live()?;
        updates.extend(self.monitor.pump(timeout)?);
        Ok(self.outcome(updates))
    }

    fn refused(&mut self, refusal: live_code::Refusal) -> Admission {
        let incidents = vec![Incident::from_refusal(refusal, (self.clock)())];
        let log_error = self.log(&incidents);
        Admission {
            serial: None,
            outcome: Outcome {
                incidents,
                restored: Vec::new(),
                log_error,
            },
        }
    }

    fn live_mut(&mut self) -> Result<&mut LiveCode> {
        self.live
            .as_mut()
            .ok_or(Error::Unsupported("live-code attestation is not set up"))
    }

    /// Present the signed record for a **file** (a hot update, or something this machine built). It is checked, and if it
    /// passes the monitor is told the file is coming (see [`Monitor::rebase`]): the signer signs, the watchdog is told, and
    /// only then is the file written, so the write is never a surprise. A refusal is a detection (`E_WATCHDOG_LIVE_CODE_REFUSED`),
    /// returned and logged; nothing is registered. After a restart the caller presents its stored records again, in order,
    /// each of which is checked afresh.
    pub fn admit_file(&mut self, signed: &SignedRecord) -> Result<Admission> {
        let now = (self.clock)();
        let live = self.live_mut()?;
        // Work on a copy: the ledger is replaced only once the monitor has taken the new baseline.
        let mut trial = live.persistent.clone();
        let serial = match trial.admit_file(signed, &live.base, now) {
            Ok(serial) => serial,
            Err(refusal) => return Ok(self.refused(refusal)),
        };
        let effective = trial.effective_baseline(&live.base, now)?;
        live.persistent = trial;
        let updates = self.monitor.rebase(effective)?;
        Ok(Admission {
            serial: Some(serial),
            outcome: self.outcome(updates),
        })
    }

    /// Present the signed record for code in **this process's memory** (generated code, a hook trampoline) at `base`. The
    /// watchdog reads the bytes there itself through `digester`; from now on every [`scan_process`](Self::scan_process)
    /// verifies them against the signed digest instead of skipping them.
    pub fn admit_region(
        &mut self,
        signed: &SignedRecord,
        base: u64,
        digester: &mut dyn RegionDigester,
    ) -> Result<Admission> {
        let now = (self.clock)();
        let live = self.live_mut()?;
        match live.session.admit_region(signed, base, now, digester) {
            Ok(serial) => Ok(Admission {
                serial: Some(serial),
                outcome: Outcome::default(),
            }),
            Err(refusal) => Ok(self.refused(refusal)),
        }
    }

    /// The code under `serial` is gone (unloaded, freed, deleted): stop vouching for it. A file goes back to being judged by
    /// the baseline alone. The serial stays used.
    pub fn retire(&mut self, serial: u64) -> Result<Retired> {
        let now = (self.clock)();
        let live = self.live_mut()?;
        if live.session.retire(serial) {
            return Ok(Retired {
                removed: true,
                outcome: Outcome::default(),
            });
        }
        if !live.persistent.retire(serial) {
            return Ok(Retired::default());
        }
        let effective = live.persistent.effective_baseline(&live.base, now)?;
        let updates = self.monitor.rebase(effective)?;
        Ok(Retired {
            removed: true,
            outcome: self.outcome(updates),
        })
    }

    /// The ledger of attested files and the ledger of attested memory, if live code is set up.
    pub fn live_code(&self) -> Option<(&LiveCodeLedger, &LiveCodeLedger)> {
        self.live.as_ref().map(|l| (&l.persistent, &l.session))
    }

    /// Look at SUL's own process through `probe`, log what is suspicious and return both the report (which includes what
    /// the allowlist let through, for the person to see) and the new incidents.
    pub fn scan_process(
        &mut self,
        probe: &mut dyn ProcessProbe,
        expect: &Expectations,
        allow: &Allowlist,
    ) -> Result<(ScanReport, Outcome)> {
        let now = (self.clock)();
        // Memory records that ran out are not verified any more (and were not skipped before: they are gone from the list).
        let merged;
        let expect = match self.live.as_mut() {
            Some(live) => {
                live.session.prune_expired(now);
                if live.session.regions().is_empty() {
                    expect
                } else {
                    let mut all = expect.clone();
                    all.attested.extend(live.session.regions().iter().cloned());
                    merged = all;
                    &merged
                }
            }
            None => expect,
        };
        let report = scan::scan(probe, expect, allow)?;
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
    use std::path::{Path, PathBuf};
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
        fn snapshot(&mut self, _digest_ranges: &[(u64, u64)]) -> Result<Snapshot> {
            Ok(self.0.clone())
        }
    }

    struct BlindProbe;

    impl ProcessProbe for BlindProbe {
        fn snapshot(&mut self, _digest_ranges: &[(u64, u64)]) -> Result<Snapshot> {
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

    /// Live-code attestation through the facade (D-159).
    mod live {
        use super::*;
        use crate::live_code::{
            Content, LIVE_SCHEMA, LiveCodeRecord, LiveCodeSigner, Purpose, Refusal, RegionDigester,
            SignedRecord, TrustRoots, TrustedProducers,
        };
        use crate::scan::LiveDigest;
        use ed25519_dalek::{Signer, SigningKey};
        use std::cell::Cell;
        use sul_trust::findings::Certainty;

        thread_local! {
            static CLOCK: Cell<u64> = const { Cell::new(NOW) };
        }

        fn clock() -> u64 {
            CLOCK.with(Cell::get)
        }

        fn set_clock(to: u64) {
            CLOCK.with(|c| c.set(to));
        }

        struct Key(SigningKey);

        impl LiveCodeSigner for Key {
            fn sign(&mut self, raw: &[u8]) -> Result<[u8; 64]> {
                Ok(self.0.sign(raw).to_bytes())
            }
        }

        fn release() -> SigningKey {
            SigningKey::from_bytes(&[1u8; 32])
        }
        fn device() -> SigningKey {
            SigningKey::from_bytes(&[2u8; 32])
        }
        fn session_key() -> SigningKey {
            SigningKey::from_bytes(&[3u8; 32])
        }

        fn roots() -> TrustRoots {
            TrustRoots {
                release: Some(release().verifying_key().to_bytes()),
                device: Some(device().verifying_key().to_bytes()),
                session: Some(session_key().verifying_key().to_bytes()),
            }
        }

        const BASELINE_BYTES: &[u8] = b"the signed baseline, as stored";
        const NONCE: [u8; 32] = [9u8; 32];

        fn selection() -> Selection {
            Selection::new()
                .rule(Pattern::Exact("sul.exe".into()), Role::TrustPrimitive)
                .rule(Pattern::Extension("dll".into()), Role::Binary)
        }

        fn ledgers(baseline: &Baseline) -> (LiveCodeLedger, LiveCodeLedger) {
            let producers = TrustedProducers::from_baseline(baseline);
            (
                LiveCodeLedger::persistent(BASELINE_BYTES, roots(), producers.clone()).unwrap(),
                LiveCodeLedger::session(NONCE, roots(), producers).unwrap(),
            )
        }

        /// A watchdog on `root` for `baseline` with live code set up. `script` is what the file watcher reports.
        fn dog_on(
            root: &Path,
            baseline: &Baseline,
            script: Vec<Vec<Change>>,
        ) -> Watchdog<Script, MemorySink> {
            let (persistent, session) = ledgers(baseline);
            let monitor = Monitor::new(
                root,
                baseline.clone(),
                selection(),
                Script(script.into()),
                quick(),
            );
            Watchdog::with_clock(monitor, MemorySink::default(), clock)
                .with_live_code(persistent, session)
                .unwrap()
        }

        /// An install, its signed baseline, and a watchdog. The script is made from the install's root.
        fn rig(
            script: impl FnOnce(&Path) -> Vec<Vec<Change>>,
        ) -> (tempfile::TempDir, Baseline, Watchdog<Script, MemorySink>) {
            set_clock(NOW);
            let dir = install();
            let baseline = Baseline::compute(dir.path(), "t", &selection(), 1).unwrap();
            let script = script(dir.path());
            let dog = dog_on(dir.path(), &baseline, script);
            (dir, baseline, dog)
        }

        /// What one pump needs: the change, then a quiet moment.
        fn burst(path: PathBuf) -> Vec<Vec<Change>> {
            vec![vec![Change::Path(path)], vec![]]
        }

        fn sha(bytes: &[u8]) -> String {
            hex::encode(sha2::Sha256::digest(bytes))
        }

        fn root_of(dog: &Watchdog<Script, MemorySink>, persistent: bool) -> String {
            let (p, s) = dog.live_code().unwrap();
            if persistent {
                p.root_hex()
            } else {
                s.root_hex()
            }
        }

        #[allow(clippy::too_many_arguments)]
        fn file_record(
            root: String,
            serial: u64,
            purpose: Purpose,
            path: &str,
            bytes: &[u8],
            role: Role,
            expires_unix: Option<u64>,
        ) -> LiveCodeRecord {
            LiveCodeRecord {
                schema_version: LIVE_SCHEMA,
                root,
                serial,
                purpose,
                producer: "sul.exe".into(),
                created_unix: NOW,
                expires_unix,
                content: Content::File {
                    path: path.into(),
                    sha256: sha(bytes),
                    size: bytes.len() as u64,
                    role,
                },
            }
        }

        fn signed(record: &LiveCodeRecord, by: SigningKey) -> SignedRecord {
            SignedRecord::new(record, &mut Key(by)).unwrap()
        }

        fn region_record(root: String, serial: u64, digest: [u8; 32], size: u64) -> LiveCodeRecord {
            LiveCodeRecord {
                schema_version: LIVE_SCHEMA,
                root,
                serial,
                purpose: Purpose::HookTrampoline,
                producer: "sul.exe".into(),
                created_unix: NOW,
                expires_unix: None,
                content: Content::Region {
                    size,
                    blake3: hex::encode(digest),
                },
            }
        }

        struct Memory {
            base: u64,
            len: u64,
            digest: [u8; 32],
        }

        impl RegionDigester for Memory {
            fn blake3(&mut self, base: u64, len: u64) -> Result<[u8; 32]> {
                if (base, len) == (self.base, self.len) {
                    Ok(self.digest)
                } else {
                    Err(Error::Invalid("unmapped".into()))
                }
            }
        }

        /// A probe that says what it was asked to digest.
        struct RangeProbe {
            snapshot: Snapshot,
            asked: Vec<(u64, u64)>,
        }

        impl ProcessProbe for RangeProbe {
            fn snapshot(&mut self, digest_ranges: &[(u64, u64)]) -> Result<Snapshot> {
                self.asked = digest_ranges.to_vec();
                Ok(self.snapshot.clone())
            }
        }

        fn rwx_at(base: u64, size: u64) -> Region {
            Region {
                base,
                size,
                protection: Protection::READ_WRITE_EXECUTE,
                kind: RegionKind::Private,
                module: None,
            }
        }

        fn baseline_digest(dog: &Watchdog<Script, MemorySink>, path: &str) -> String {
            dog.monitor()
                .baseline()
                .entries
                .iter()
                .find(|e| e.path == path)
                .unwrap_or_else(|| panic!("{path} is not in the baseline"))
                .sha256
                .clone()
        }

        #[test]
        fn a_signed_hot_update_is_announced_first_so_the_write_is_no_surprise_and_a_wrong_write_is()
        {
            let (dir, _, mut dog) = rig(|root| {
                let helper = root.join("bin/helper.dll");
                let mut script = Vec::new();
                for _ in 0..4 {
                    script.extend(burst(helper.clone()));
                }
                script
            });
            let root = root_of(&dog, true);
            let record = file_record(
                root,
                1,
                Purpose::HotUpdate,
                "bin/helper.dll",
                b"a library v2",
                Role::Binary,
                None,
            );
            let admission = dog.admit_file(&signed(&record, release())).unwrap();
            assert_eq!(admission.serial, Some(1));
            assert_eq!(
                admission.outcome,
                Outcome::default(),
                "announced, not yet written"
            );
            assert_eq!(
                baseline_digest(&dog, "bin/helper.dll"),
                sha(b"a library v2")
            );

            // An event while the old file is still there, then the update lands: nothing to report either time.
            assert_eq!(
                dog.pump(Duration::from_millis(1)).unwrap(),
                Outcome::default()
            );
            fs::write(dir.path().join("bin/helper.dll"), b"a library v2").unwrap();
            assert_eq!(
                dog.pump(Duration::from_millis(1)).unwrap(),
                Outcome::default()
            );

            // From here on the new content is the one that counts.
            fs::write(dir.path().join("bin/helper.dll"), b"injected").unwrap();
            let outcome = dog.pump(Duration::from_millis(1)).unwrap();
            assert_eq!(outcome.incidents.len(), 1, "{outcome:?}");
            assert_eq!(outcome.incidents[0].code, Some(WatchdogCode::Tamper));
            assert_eq!(
                dog.into_sink().records.len(),
                1,
                "the accepted update left no threat record"
            );
        }

        #[test]
        fn a_device_signed_local_build_adds_a_file_but_cannot_rewrite_the_program() {
            let (dir, _, mut dog) = rig(|root| burst(root.join("mods/adapter.dll")));
            let root = root_of(&dog, true);
            let add = file_record(
                root.clone(),
                1,
                Purpose::LocalBuild,
                "mods/adapter.dll",
                b"built here",
                Role::Sidecar,
                None,
            );
            let admission = dog.admit_file(&signed(&add, device())).unwrap();
            assert_eq!(admission.serial, Some(1));
            assert_eq!(admission.outcome, Outcome::default());
            fs::create_dir_all(dir.path().join("mods")).unwrap();
            fs::write(dir.path().join("mods/adapter.dll"), b"built here").unwrap();
            assert_eq!(
                dog.pump(Duration::from_millis(1)).unwrap(),
                Outcome::default()
            );

            // The same key asking to replace a file the release-signed baseline lists is refused and written to the log.
            let before = baseline_digest(&dog, "sul.exe");
            let rewrite = file_record(
                root,
                2,
                Purpose::LocalBuild,
                "sul.exe",
                b"a different program",
                Role::Other,
                None,
            );
            let admission = dog.admit_file(&signed(&rewrite, device())).unwrap();
            assert_eq!(admission.serial, None);
            assert_eq!(admission.outcome.incidents.len(), 1);
            assert_eq!(
                admission.outcome.incidents[0].code,
                Some(WatchdogCode::LiveCodeRefused)
            );
            assert_eq!(
                admission.outcome.incidents[0].refusal,
                Some(Refusal::CannotReplace)
            );
            assert_eq!(
                baseline_digest(&dog, "sul.exe"),
                before,
                "nothing was changed"
            );
            let records = dog.into_sink().records;
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].event, "live_code_refusal");
            assert_eq!(records[0].why, Some(Refusal::CannotReplace));
        }

        #[test]
        fn a_refused_record_registers_nothing_and_is_a_confirmed_detection_when_the_key_was_wrong()
        {
            let (_dir, _, mut dog) = rig(|_| vec![]);
            let root = root_of(&dog, true);
            let record = file_record(
                root,
                1,
                Purpose::HotUpdate,
                "bin/helper.dll",
                b"evil",
                Role::Binary,
                None,
            );

            // Signed with the session key: the key for generated code cannot stand in for the release key.
            let wrong_key = dog.admit_file(&signed(&record, session_key())).unwrap();
            assert_eq!(
                wrong_key.outcome.incidents[0].refusal,
                Some(Refusal::WrongAuthority)
            );
            assert!(wrong_key.outcome.incidents[0].certainty == Certainty::Confirmed);

            // The text changed after it was signed.
            let mut forged = signed(&record, release());
            let original = forged.raw.clone();
            forged.raw = forged.raw.replace("bin/helper.dll", "bin/helpex.dll");
            assert_ne!(
                forged.raw, original,
                "the test must really change the signed text"
            );
            let forged = dog.admit_file(&forged).unwrap();
            assert_eq!(
                forged.outcome.incidents[0].refusal,
                Some(Refusal::BadSignature)
            );

            // Not the signature of any trusted key at all.
            let stranger = dog
                .admit_file(&signed(&record, SigningKey::from_bytes(&[77u8; 32])))
                .unwrap();
            assert_eq!(stranger.serial, None);

            let (persistent, _) = dog.live_code().unwrap();
            assert!(persistent.files().is_empty());
            assert_eq!(baseline_digest(&dog, "bin/helper.dll"), sha(b"a library"));
            assert_eq!(dog.into_sink().records.len(), 3, "each refusal is logged");
        }

        #[test]
        fn a_record_cannot_be_presented_twice_and_after_a_restart_the_stored_records_are_checked_again()
         {
            let (dir, baseline, mut dog) = rig(|_| vec![]);
            let root = root_of(&dog, true);
            let record = signed(
                &file_record(
                    root,
                    1,
                    Purpose::HotUpdate,
                    "bin/helper.dll",
                    b"a library v2",
                    Role::Binary,
                    None,
                ),
                release(),
            );
            assert_eq!(dog.admit_file(&record).unwrap().serial, Some(1));
            let again = dog.admit_file(&record).unwrap();
            assert_eq!(
                again.outcome.incidents[0].refusal,
                Some(Refusal::OutOfOrder)
            );
            assert!(
                again.outcome.incidents[0].certainty != Certainty::Confirmed,
                "a restart does this too"
            );

            // The update landed, the process restarted, and the caller stores and replays what it was given.
            fs::write(dir.path().join("bin/helper.dll"), b"a library v2").unwrap();
            let mut restarted = dog_on(dir.path(), &baseline, vec![]);
            let replay = restarted.admit_file(&record).unwrap();
            assert_eq!(replay.serial, Some(1));
            assert_eq!(replay.outcome, Outcome::default());
            assert_eq!(restarted.session_start().unwrap(), Outcome::default());
            // Without the replay the same disk is a deviation from the signed baseline: that is the point of it.
            let mut forgetful = dog_on(dir.path(), &baseline, vec![]);
            assert_eq!(forgetful.session_start().unwrap().incidents.len(), 1);
        }

        #[test]
        fn signed_generated_code_is_verified_every_scan_not_skipped_and_a_change_loses_its_cover() {
            let (_dir, _, mut dog) = rig(|_| vec![]);
            let root = root_of(&dog, false);
            let digest = [0x42; 32];
            let record = signed(&region_record(root, 1, digest, 4096), session_key());
            let mut memory = Memory {
                base: 0x10000,
                len: 4096,
                digest,
            };
            let admission = dog.admit_region(&record, 0x10000, &mut memory).unwrap();
            assert_eq!(admission.serial, Some(1));
            assert_eq!(admission.outcome, Outcome::default());

            let snapshot = |digest: [u8; 32]| Snapshot {
                regions: vec![rwx_at(0x10000, 4096)],
                live: vec![LiveDigest {
                    base: 0x10000,
                    size: 4096,
                    blake3: Some(digest),
                }],
                ..Snapshot::default()
            };
            let mut probe = RangeProbe {
                snapshot: snapshot(digest),
                asked: vec![],
            };
            let (report, outcome) = dog
                .scan_process(&mut probe, &Expectations::default(), &Allowlist::new())
                .unwrap();
            assert_eq!(
                probe.asked,
                vec![(0x10000, 4096)],
                "the probe is asked to hash the registered range"
            );
            assert!(report.is_clean(), "{report:?}");
            assert_eq!((report.live_examined, report.live_verified), (1, 1));
            assert_eq!(outcome, Outcome::default());

            // One byte of the code changed: it is no longer cover for the RWX region, and the change is a detection.
            let mut probe = RangeProbe {
                snapshot: snapshot([0x43; 32]),
                asked: vec![],
            };
            let (report, outcome) = dog
                .scan_process(&mut probe, &Expectations::default(), &Allowlist::new())
                .unwrap();
            assert_eq!(report.live_verified, 0);
            let codes: Vec<_> = outcome.incidents.iter().map(|i| i.code).collect();
            assert_eq!(
                codes,
                vec![
                    Some(WatchdogCode::RwxRegion),
                    Some(WatchdogCode::LiveCodeModified)
                ]
            );
        }

        #[test]
        fn memory_that_does_not_match_its_record_is_refused_and_unsigned_code_is_still_a_finding() {
            let (_dir, _, mut dog) = rig(|_| vec![]);
            let root = root_of(&dog, false);
            let record = signed(&region_record(root, 1, [0x42; 32], 4096), session_key());
            let mut other = Memory {
                base: 0x10000,
                len: 4096,
                digest: [0x99; 32],
            };
            let admission = dog.admit_region(&record, 0x10000, &mut other).unwrap();
            assert_eq!(admission.serial, None);
            assert_eq!(
                admission.outcome.incidents[0].refusal,
                Some(Refusal::ContentMismatch)
            );
            assert!(admission.outcome.incidents[0].certainty == Certainty::Confirmed);

            // Nothing was registered, so the same executable memory is reported as the foreign code it is.
            let mut probe = RangeProbe {
                snapshot: Snapshot {
                    regions: vec![rwx_at(0x10000, 4096)],
                    ..Snapshot::default()
                },
                asked: vec![],
            };
            let (report, outcome) = dog
                .scan_process(&mut probe, &Expectations::default(), &Allowlist::new())
                .unwrap();
            assert!(probe.asked.is_empty());
            assert_eq!(report.findings.len(), 1);
            assert_eq!(outcome.incidents[0].code, Some(WatchdogCode::RwxRegion));
        }

        #[test]
        fn a_record_that_runs_out_stops_vouching_and_the_file_is_then_judged_by_the_baseline() {
            let (dir, _, mut dog) = rig(|_| vec![vec![], vec![]]);
            let root = root_of(&dog, true);
            let record = file_record(
                root,
                1,
                Purpose::LocalBuild,
                "mods/adapter.dll",
                b"built here",
                Role::Sidecar,
                Some(NOW + 100),
            );
            dog.admit_file(&signed(&record, device())).unwrap();
            fs::create_dir_all(dir.path().join("mods")).unwrap();
            fs::write(dir.path().join("mods/adapter.dll"), b"built here").unwrap();
            assert_eq!(
                dog.pump(Duration::from_millis(1)).unwrap(),
                Outcome::default()
            );

            set_clock(NOW + 101);
            let outcome = dog.pump(Duration::from_millis(1)).unwrap();
            assert_eq!(outcome.incidents.len(), 1, "{outcome:?}");
            assert!(
                outcome.incidents[0].detail.contains("mods/adapter.dll"),
                "{}",
                outcome.incidents[0].detail
            );
            assert!(dog.live_code().unwrap().0.files().is_empty());

            // Memory records lapse the same way: the trampoline no longer covers its region.
            let sess = root_of(&dog, false);
            let mut region = region_record(sess, 1, [0x42; 32], 4096);
            region.created_unix = NOW + 101;
            region.expires_unix = Some(NOW + 200);
            let mut memory = Memory {
                base: 0x10000,
                len: 4096,
                digest: [0x42; 32],
            };
            assert_eq!(
                dog.admit_region(&signed(&region, session_key()), 0x10000, &mut memory)
                    .unwrap()
                    .serial,
                Some(1)
            );
            set_clock(NOW + 201);
            let mut probe = RangeProbe {
                snapshot: Snapshot {
                    regions: vec![rwx_at(0x10000, 4096)],
                    ..Snapshot::default()
                },
                asked: vec![],
            };
            let (_, outcome) = dog
                .scan_process(&mut probe, &Expectations::default(), &Allowlist::new())
                .unwrap();
            assert_eq!(outcome.incidents.len(), 1);
            assert!(probe.asked.is_empty());
        }

        #[test]
        fn retiring_live_code_stops_it_being_vouched_for_and_a_serial_is_never_reused() {
            let (dir, _, mut dog) = rig(|_| vec![]);
            let files = root_of(&dog, true);
            let record = file_record(
                files.clone(),
                1,
                Purpose::LocalBuild,
                "mods/adapter.dll",
                b"built here",
                Role::Sidecar,
                None,
            );
            dog.admit_file(&signed(&record, device())).unwrap();
            fs::create_dir_all(dir.path().join("mods")).unwrap();
            fs::write(dir.path().join("mods/adapter.dll"), b"built here").unwrap();

            let retired = dog.retire(1).unwrap();
            assert!(retired.removed);
            assert_eq!(
                retired.outcome.incidents.len(),
                1,
                "the file is no longer attested: {retired:?}"
            );
            assert_eq!(dog.retire(1).unwrap(), Retired::default(), "already gone");
            assert_eq!(dog.retire(99).unwrap(), Retired::default());

            // The retired record cannot be presented again.
            let again = dog.admit_file(&signed(&record, device())).unwrap();
            assert_eq!(
                again.outcome.incidents[0].refusal,
                Some(Refusal::OutOfOrder)
            );

            let sess = root_of(&dog, false);
            let region = signed(&region_record(sess, 5, [1; 32], 64), session_key());
            let mut memory = Memory {
                base: 0x2000,
                len: 64,
                digest: [1; 32],
            };
            assert_eq!(
                dog.admit_region(&region, 0x2000, &mut memory)
                    .unwrap()
                    .serial,
                Some(5)
            );
            assert!(dog.retire(5).unwrap().removed);
            let replay = dog.admit_region(&region, 0x2000, &mut memory).unwrap();
            assert_eq!(
                replay.outcome.incidents[0].refusal,
                Some(Refusal::OutOfOrder)
            );
        }

        #[test]
        fn live_code_has_to_be_set_up_once_with_the_right_ledgers() {
            set_clock(NOW);
            let dir = install();
            let baseline = Baseline::compute(dir.path(), "t", &selection(), 1).unwrap();
            let monitor = || {
                Monitor::new(
                    dir.path(),
                    baseline.clone(),
                    selection(),
                    Script(VecDeque::new()),
                    quick(),
                )
            };

            // Without it, presenting a record is an error of the call, not a detection.
            let mut plain = Watchdog::with_clock(monitor(), MemorySink::default(), clock);
            let (p, s) = ledgers(&baseline);
            let record = signed(
                &file_record(
                    p.root_hex(),
                    1,
                    Purpose::HotUpdate,
                    "bin/helper.dll",
                    b"x",
                    Role::Binary,
                    None,
                ),
                release(),
            );
            assert!(plain.admit_file(&record).is_err());
            assert!(
                plain
                    .admit_region(
                        &record,
                        1,
                        &mut Memory {
                            base: 1,
                            len: 1,
                            digest: [0; 32]
                        }
                    )
                    .is_err()
            );
            assert!(plain.retire(1).is_err());
            assert!(plain.live_code().is_none());

            // The two ledgers the wrong way round.
            let (p, s2) = (p.clone(), s.clone());
            let swapped =
                Watchdog::with_clock(monitor(), MemorySink::default(), clock).with_live_code(s, p);
            assert!(swapped.is_err());

            // A ledger that already holds records: they would never have reached the monitor.
            let mut used = p2(&baseline);
            let signed_record = signed(
                &file_record(
                    used.root_hex(),
                    1,
                    Purpose::HotUpdate,
                    "bin/helper.dll",
                    b"x",
                    Role::Binary,
                    None,
                ),
                release(),
            );
            used.admit_file(&signed_record, &baseline, NOW).unwrap();
            let loaded = Watchdog::with_clock(monitor(), MemorySink::default(), clock)
                .with_live_code(used, s2.clone());
            assert!(loaded.is_err());

            // Twice.
            let once = Watchdog::with_clock(monitor(), MemorySink::default(), clock)
                .with_live_code(p2(&baseline), s2.clone())
                .unwrap();
            assert!(once.with_live_code(p2(&baseline), s2).is_err());
        }

        fn p2(baseline: &Baseline) -> LiveCodeLedger {
            ledgers(baseline).0
        }
    }
}
