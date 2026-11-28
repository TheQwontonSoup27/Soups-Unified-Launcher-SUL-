//! The event-driven half of the integrity guard: the operating system says a file changed, and the changed file is checked
//! against the baseline at once ("filesystem hooks detect a modified binary within 1 s of write", Ch. 12 §2.8).
//!
//! Nothing runs while nothing changes: on Windows the events come from `ReadDirectoryChangesW` through the `notify` crate,
//! so an idle watchdog costs no CPU, which is why it can sit at `BELOW_NORMAL` priority and never touch a game's frame time
//! (the priority itself is set by the Windows adapter, decision 0014).
//!
//! The design has four safeguards, because a file-event stream is not a promise:
//!
//! 1. **Debounce.** A write arrives as many events. The monitor waits until they stop for [`MonitorConfig::quiet`] (or
//!    [`MonitorConfig::max_batch`] has passed), then checks each affected file once.
//! 2. **Overflow means "look at everything".** When the system's event buffer overflows, events are lost. `notify` says so
//!    (a rescan notice) and so does any watcher error; either one triggers a full check ([`Change::Rescan`]).
//! 3. **A periodic full check** ([`MonitorConfig::full_rescan_every`]) as the backstop for anything the stream never
//!    reported. It skips model files ([`Role::in_periodic_rescan`]): they are verified at session start and when they change.
//! 4. **A locked file is retried once** after the quiet period (an installer or antivirus scan holding it), and only then
//!    reported as unreadable.
//!
//! The monitor reports **changes of state**: a deviation is reported when it first appears or changes, not on every event
//! while it persists, and a file that is clean again is reported as [`Update::Restored`] so a warning can be cleared.
//!
//! Pass the monitor and the watcher the same root path (the same spelling): event paths are matched to the baseline by
//! stripping that root from them.

use crate::baseline::{Baseline, Role, Selection, Watched, key};
use crate::guard::{self, Deviation, PathDeviation};
use notify::event::{AccessKind, AccessMode};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};
use sul_core::{Error, Result};

/// One thing a [`ChangeSource`] saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// This path changed (created, written, renamed, removed).
    Path(PathBuf),
    /// Events may have been lost: check everything.
    Rescan,
}

/// Where changes come from. [`NotifySource`] is the real one; tests use a scripted one.
pub trait ChangeSource {
    /// Wait up to `timeout` for a change and return every change that is ready. An empty answer means the time ran out.
    fn wait(&mut self, timeout: Duration) -> Result<Vec<Change>>;
}

/// Changes from the operating system, through `notify`.
pub struct NotifySource {
    // Held so the watch lives as long as the source does.
    _watcher: RecommendedWatcher,
    events: Receiver<notify::Result<notify::Event>>,
}

fn watch_error(root: &Path, e: notify::Error) -> Error {
    Error::io(root, std::io::Error::other(e))
}

impl NotifySource {
    /// Watch `root` and everything under it.
    pub fn watch(root: &Path) -> Result<Self> {
        let (tx, events) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(tx).map_err(|e| watch_error(root, e))?;
        watcher
            .watch(root, RecursiveMode::Recursive)
            .map_err(|e| watch_error(root, e))?;
        Ok(Self {
            _watcher: watcher,
            events,
        })
    }
}

fn translate(result: notify::Result<notify::Event>, out: &mut Vec<Change>) {
    match result {
        // The watcher itself failed: assume events were lost.
        Err(_) => out.push(Change::Rescan),
        Ok(event) => {
            if event.need_rescan() {
                out.push(Change::Rescan);
            }
            // Reading or opening a file is not a change; closing it after a write is the end of one.
            let a_change = match event.kind {
                EventKind::Access(AccessKind::Close(AccessMode::Write)) => true,
                EventKind::Access(_) => false,
                _ => true,
            };
            if a_change {
                out.extend(event.paths.into_iter().map(Change::Path));
            }
        }
    }
}

impl ChangeSource for NotifySource {
    fn wait(&mut self, timeout: Duration) -> Result<Vec<Change>> {
        let mut out = Vec::new();
        match self.events.recv_timeout(timeout) {
            Ok(first) => translate(first, &mut out),
            Err(RecvTimeoutError::Timeout) => return Ok(out),
            Err(RecvTimeoutError::Disconnected) => {
                return Err(Error::Unsupported("the file watcher stopped"));
            }
        }
        while let Ok(next) = self.events.try_recv() {
            translate(next, &mut out);
        }
        Ok(out)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorConfig {
    /// How long events must stop before the changed files are checked.
    pub quiet: Duration,
    /// The longest a stream of events is allowed to postpone the check.
    pub max_batch: Duration,
    /// How often to check everything regardless of events. `None` turns the backstop off.
    pub full_rescan_every: Option<Duration>,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            quiet: Duration::from_millis(50),
            max_batch: Duration::from_millis(250),
            full_rescan_every: Some(Duration::from_secs(600)),
        }
    }
}

/// A change in what is wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Update {
    /// A deviation appeared or became a different deviation.
    Deviation(PathDeviation),
    /// A file that was deviating matches the baseline again (the path, relative to the root).
    Restored { path: String },
}

/// How long a signed file may take to arrive after it was announced (D-159) before its absence is reported as tampering.
pub const ANNOUNCE_TTL: Duration = Duration::from_secs(600);

/// A file the baseline was told to expect, announced **before** it was written: the updater signs the new code, tells the
/// watchdog (see [`Monitor::rebase`]) and then writes it, so the write is never a surprise. Until the file has arrived
/// as signed, its absence (new file) or the old content still in place (replacement) is not a deviation.
#[derive(Debug, Clone)]
struct Announced {
    /// The digest the baseline had for this path before the announcement, if it had one (a replacement, not a new file).
    old_sha256: Option<String>,
    since: Instant,
}

pub struct Monitor<S: ChangeSource> {
    root: PathBuf,
    baseline: Baseline,
    selection: Selection,
    source: S,
    config: MonitorConfig,
    /// Deviations currently reported and not yet restored: path key -> (deviation, role, path as written).
    open: BTreeMap<String, (Deviation, Option<Role>, String)>,
    last_full: Instant,
    announced: BTreeMap<String, Announced>,
    announce_ttl: Duration,
}

fn rel_path(root: &Path, full: &Path) -> Option<String> {
    let rel = full.strip_prefix(root).ok()?;
    Some(
        rel.components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

impl<S: ChangeSource> Monitor<S> {
    pub fn new(
        root: &Path,
        baseline: Baseline,
        selection: Selection,
        source: S,
        config: MonitorConfig,
    ) -> Self {
        Self {
            root: root.to_path_buf(),
            baseline,
            selection,
            source,
            config,
            open: BTreeMap::new(),
            last_full: Instant::now(),
            announced: BTreeMap::new(),
            announce_ttl: ANNOUNCE_TTL,
        }
    }

    /// As [`new`](Self::new) with another announcement window than [`ANNOUNCE_TTL`] (tests; a slow disk).
    pub fn with_announce_ttl(mut self, ttl: Duration) -> Self {
        self.announce_ttl = ttl;
        self
    }

    /// Adopt a new effective baseline (the signed one plus attested live code, see [`crate::live_code`]) and check **only**
    /// what it changed: a path that is new, whose digest or size differs, or that is no longer listed. Every added or
    /// replaced path is announced: until the file is seen matching the new entry, a missing file (new path) or the old
    /// content (replacement) is waited for, not reported; any **other** state, and anything still not right when
    /// [`ANNOUNCE_TTL`] is up, is reported as usual. A deviation that was open and is now explained is restored.
    pub fn rebase(&mut self, new: Baseline) -> Result<Vec<Update>> {
        let old: BTreeMap<String, Watched> = self
            .baseline
            .entries
            .iter()
            .map(|e| (key(&e.path), e.clone()))
            .collect();
        let mut look_at: Vec<(String, String)> = Vec::new();
        for entry in &new.entries {
            let k = key(&entry.path);
            let before = old.get(&k);
            if before.is_none_or(|b| b.sha256 != entry.sha256 || b.size != entry.size) {
                self.announced.insert(
                    k.clone(),
                    Announced {
                        old_sha256: before.map(|b| b.sha256.clone()),
                        since: Instant::now(),
                    },
                );
                look_at.push((k, entry.path.clone()));
            }
        }
        for (k, before) in &old {
            if !new.entries.iter().any(|e| key(&e.path) == *k) {
                self.announced.remove(k);
                look_at.push((k.clone(), before.path.clone()));
            }
        }
        self.baseline = new;
        let mut out = Vec::new();
        for (k, rel) in look_at {
            let current = self.check_retrying(&rel);
            self.record(k, current, &mut out);
        }
        Ok(out)
    }

    /// What an announcement says about what was just found at `path_key`: `None` means "nothing to report".
    fn after_announcement(
        &mut self,
        path_key: &str,
        current: Option<PathDeviation>,
    ) -> Option<PathDeviation> {
        let (old_sha256, expired) = match self.announced.get(path_key) {
            Some(a) => (a.old_sha256.clone(), a.since.elapsed() >= self.announce_ttl),
            None => return current,
        };
        if expired {
            self.announced.remove(path_key);
            return current;
        }
        match current {
            // The file is there and is what was signed: the announcement has done its job.
            None => {
                self.announced.remove(path_key);
                None
            }
            Some(d) => {
                let waiting = match &d.deviation {
                    Deviation::Missing => old_sha256.is_none(),
                    Deviation::Modified { actual_sha256, .. } => {
                        old_sha256.as_deref() == Some(actual_sha256.as_str())
                    }
                    _ => false,
                };
                if waiting { None } else { Some(d) }
            }
        }
    }

    /// The baseline the monitor is checking against right now (after any [`rebase`](Self::rebase)).
    pub fn baseline(&self) -> &Baseline {
        &self.baseline
    }

    /// Deviations that are open now (reported and not restored), in path order.
    pub fn open_deviations(&self) -> Vec<(&str, &Deviation)> {
        self.open
            .values()
            .map(|(d, _, path)| (path.as_str(), d))
            .collect()
    }

    /// The full check every session starts with: every guarded file, every role.
    pub fn session_start(&mut self) -> Result<Vec<Update>> {
        self.full_pass(&|_| true)
    }

    /// Wait up to `timeout` for the system to report a change; check what changed; and, if the backstop is due, check
    /// everything. Returns only changes of state. An empty answer is the normal one.
    pub fn pump(&mut self, timeout: Duration) -> Result<Vec<Update>> {
        let mut changes = self.source.wait(timeout)?;
        let mut out = Vec::new();
        if !changes.is_empty() {
            let started = Instant::now();
            while started.elapsed() < self.config.max_batch {
                let more = self.source.wait(self.config.quiet)?;
                if more.is_empty() {
                    break;
                }
                changes.extend(more);
            }
            out.extend(self.handle(changes)?);
        }
        let due = self
            .config
            .full_rescan_every
            .is_some_and(|every| self.last_full.elapsed() >= every);
        if due {
            out.extend(self.full_pass(&Role::in_periodic_rescan)?);
        }
        Ok(out)
    }

    fn record(&mut self, path_key: String, current: Option<PathDeviation>, out: &mut Vec<Update>) {
        let current = self.after_announcement(&path_key, current);
        match current {
            Some(d) => {
                let changed = self
                    .open
                    .get(&path_key)
                    .is_none_or(|(previous, _, _)| *previous != d.deviation);
                self.open
                    .insert(path_key, (d.deviation.clone(), d.role, d.path.clone()));
                if changed {
                    out.push(Update::Deviation(d));
                }
            }
            None => {
                if let Some((_, _, path)) = self.open.remove(&path_key) {
                    out.push(Update::Restored { path });
                }
            }
        }
    }

    fn full_pass(&mut self, include: &dyn Fn(Role) -> bool) -> Result<Vec<Update>> {
        let report = guard::verify_all(&self.root, &self.baseline, &self.selection, include)?;
        let mut out = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for d in report.deviations {
            let k = key(&d.path);
            seen.insert(k.clone());
            self.record(k, Some(d), &mut out);
        }
        // Whatever was open, was looked at in this pass and is clean now, is restored. A file this pass did not look at (a
        // model file in a periodic pass) is left as it was.
        let stale: Vec<String> = self
            .open
            .iter()
            .filter(|(k, (_, role, _))| !seen.contains(*k) && role.is_none_or(include))
            .map(|(k, _)| k.clone())
            .collect();
        for k in stale {
            self.record(k, None, &mut out);
        }
        self.last_full = Instant::now();
        Ok(out)
    }

    fn handle(&mut self, changes: Vec<Change>) -> Result<Vec<Update>> {
        if changes.iter().any(|c| matches!(c, Change::Rescan)) {
            return self.full_pass(&|_| true);
        }
        let mut affected: BTreeMap<String, String> = BTreeMap::new();
        for change in changes {
            let Change::Path(full) = change else { continue };
            let Some(rel) = rel_path(&self.root, &full) else {
                continue;
            };
            if rel.is_empty() {
                // The root itself changed (renamed, removed): nothing below it can be trusted to be where it was.
                return self.full_pass(&|_| true);
            }
            let k = key(&rel);
            let under = format!("{k}/");
            for entry in &self.baseline.entries {
                let entry_key = key(&entry.path);
                if entry_key == k || entry_key.starts_with(&under) {
                    affected.insert(entry_key, entry.path.clone());
                }
            }
            affected.entry(k).or_insert(rel);
        }
        let mut out = Vec::new();
        for (k, rel) in affected {
            let current = self.check_retrying(&rel);
            self.record(k, current, &mut out);
        }
        Ok(out)
    }

    /// Check one path; if it could not be read, wait out the quiet period once and look again (a lock is usually brief).
    fn check_retrying(&self, rel: &str) -> Option<PathDeviation> {
        retry_once_if_unreadable(self.config.quiet, || {
            guard::verify_one(&self.root, &self.baseline, &self.selection, rel)
        })
    }
}

/// Run `check`; if the answer is "could not be read", wait `pause` and run it once more, and that second answer stands (a
/// file that is still locked is reported as unreadable, never looped on). Any other answer, clean or not, is final at once.
/// Pulled out of [`Monitor::check_retrying`] so the policy can be tested without an operating system that holds a lock.
fn retry_once_if_unreadable(
    pause: Duration,
    mut check: impl FnMut() -> Option<PathDeviation>,
) -> Option<PathDeviation> {
    let first = check();
    let locked = first
        .as_ref()
        .is_some_and(|d| matches!(d.deviation, Deviation::Unreadable { .. }));
    if locked {
        std::thread::sleep(pause);
        return check();
    }
    first
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::baseline::Pattern;
    use std::collections::VecDeque;
    use std::fs;

    /// A source that hands out scripted batches, one per call, then reports a timeout.
    struct Script(VecDeque<Vec<Change>>);

    impl ChangeSource for Script {
        fn wait(&mut self, _timeout: Duration) -> Result<Vec<Change>> {
            Ok(self.0.pop_front().unwrap_or_default())
        }
    }

    struct Fixture {
        dir: tempfile::TempDir,
    }

    impl Fixture {
        fn root(&self) -> &Path {
            self.dir.path()
        }
        fn monitor(&self, script: Vec<Vec<Change>>, config: MonitorConfig) -> Monitor<Script> {
            let selection = selection();
            let baseline = Baseline::compute(self.root(), "t", &selection, 1).unwrap();
            Monitor::new(
                self.root(),
                baseline,
                selection,
                Script(script.into()),
                config,
            )
        }
        fn change(&self, rel: &str) -> Change {
            Change::Path(self.root().join(rel))
        }
    }

    fn selection() -> Selection {
        Selection::new()
            .rule(Pattern::Prefix("models/".into()), Role::ModelFile)
            .rule(Pattern::Exact("sul.exe".into()), Role::TrustPrimitive)
            .rule(Pattern::Extension("dll".into()), Role::Binary)
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::create_dir_all(root.join("models")).unwrap();
        fs::write(root.join("sul.exe"), b"the program").unwrap();
        fs::write(root.join("bin/helper.dll"), b"a library").unwrap();
        fs::write(root.join("models/jarvis.gguf"), b"weights").unwrap();
        Fixture { dir }
    }

    fn quick() -> MonitorConfig {
        MonitorConfig {
            quiet: Duration::from_millis(1),
            max_batch: Duration::from_millis(20),
            full_rescan_every: None,
        }
    }

    fn digest(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(bytes))
    }

    /// `m`'s baseline with `path` added or replaced by an entry for `content`.
    fn with_entry<S: ChangeSource>(m: &Monitor<S>, path: &str, content: &[u8]) -> Baseline {
        let mut b = m.baseline.clone();
        b.entries.retain(|e| e.path != path);
        b.entries.push(Watched {
            path: path.into(),
            sha256: digest(content),
            size: content.len() as u64,
            role: Role::Binary,
            artifact_code: None,
        });
        b.entries.sort_by_key(|e| key(&e.path));
        b.validate().unwrap();
        b
    }

    fn pump_for<S: ChangeSource>(m: &mut Monitor<S>) -> Vec<Update> {
        m.pump(Duration::from_millis(1)).unwrap()
    }

    #[test]
    fn a_new_signed_file_is_waited_for_not_reported_missing_and_then_checked_when_it_arrives() {
        let f = fixture();
        let new = f.change("bin/mod.dll");
        let mut m = f.monitor(
            vec![
                vec![new.clone()],
                vec![],
                vec![new.clone()],
                vec![],
                vec![new],
                vec![],
            ],
            quick(),
        );
        let b = with_entry(&m, "bin/mod.dll", b"new mod");
        assert!(
            m.rebase(b).unwrap().is_empty(),
            "announced, not yet written: not missing"
        );
        assert!(m.open_deviations().is_empty());

        // It arrives as signed: nothing to report, and the announcement is used up.
        fs::write(f.root().join("bin/mod.dll"), b"new mod").unwrap();
        assert!(pump_for(&mut m).is_empty());
        assert!(m.announced.is_empty());

        // From now on it is guarded like any other file: gone is Missing.
        fs::remove_file(f.root().join("bin/mod.dll")).unwrap();
        let updates = pump_for(&mut m);
        assert!(
            matches!(&updates[..], [Update::Deviation(d)] if d.deviation == Deviation::Missing),
            "{updates:?}"
        );
    }

    #[test]
    fn a_new_file_that_arrives_with_the_wrong_content_is_reported_at_once() {
        let f = fixture();
        let new = f.change("bin/mod.dll");
        let mut m = f.monitor(vec![vec![new], vec![]], quick());
        let b = with_entry(&m, "bin/mod.dll", b"new mod");
        m.rebase(b).unwrap();
        fs::write(f.root().join("bin/mod.dll"), b"something else").unwrap();
        let updates = pump_for(&mut m);
        assert!(
            matches!(&updates[..], [Update::Deviation(d)] if matches!(d.deviation, Deviation::Modified { .. }) && d.deviation.certainty() == sul_trust::findings::Certainty::Confirmed),
            "{updates:?}"
        );
    }

    #[test]
    fn a_replacement_waits_for_the_new_content_and_a_later_return_to_the_old_one_is_caught() {
        let f = fixture();
        let helper = f.change("bin/helper.dll");
        let mut m = f.monitor(
            vec![
                vec![helper.clone()],
                vec![],
                vec![helper.clone()],
                vec![],
                vec![helper.clone()],
                vec![],
                vec![helper],
                vec![],
            ],
            quick(),
        );
        let b = with_entry(&m, "bin/helper.dll", b"a library v2");
        // The old file is still in place: that is the state before the update, not tampering.
        assert!(m.rebase(b).unwrap().is_empty());
        assert!(
            pump_for(&mut m).is_empty(),
            "an event while the old content is still there"
        );

        fs::write(f.root().join("bin/helper.dll"), b"a library v2").unwrap();
        assert!(pump_for(&mut m).is_empty());
        assert!(m.announced.is_empty());

        // Putting the old, signed-but-superseded version back is a rollback, and it is reported.
        fs::write(f.root().join("bin/helper.dll"), b"a library").unwrap();
        let updates = pump_for(&mut m);
        assert!(
            matches!(&updates[..], [Update::Deviation(d)] if matches!(d.deviation, Deviation::Modified { .. })),
            "{updates:?}"
        );

        // A third content during the window is reported too.
        let mut m2 = f.monitor(vec![vec![f.change("bin/helper.dll")], vec![]], quick());
        fs::write(f.root().join("bin/helper.dll"), b"a library").unwrap();
        let b2 = with_entry(&m2, "bin/helper.dll", b"a library v2");
        m2.rebase(b2).unwrap();
        fs::write(f.root().join("bin/helper.dll"), b"injected").unwrap();
        let updates = pump_for(&mut m2);
        assert!(
            matches!(&updates[..], [Update::Deviation(d)] if matches!(d.deviation, Deviation::Modified { .. })),
            "{updates:?}"
        );
    }

    #[test]
    fn a_replaced_file_that_disappears_while_its_update_is_awaited_is_reported_missing() {
        let f = fixture();
        let mut m = f.monitor(vec![vec![f.change("bin/helper.dll")], vec![]], quick());
        let b = with_entry(&m, "bin/helper.dll", b"a library v2");
        assert!(m.rebase(b).unwrap().is_empty());
        // The old file may still be there, but it may not simply vanish: an update does not delete first.
        fs::remove_file(f.root().join("bin/helper.dll")).unwrap();
        let updates = pump_for(&mut m);
        assert!(
            matches!(&updates[..], [Update::Deviation(d)] if d.deviation == Deviation::Missing),
            "{updates:?}"
        );
    }

    #[test]
    fn an_announcement_that_never_arrives_is_reported_once_its_time_is_up() {
        let f = fixture();
        let mut m = f.monitor(vec![], quick()).with_announce_ttl(Duration::ZERO);
        let b = with_entry(&m, "bin/mod.dll", b"new mod");
        let updates = m.rebase(b).unwrap();
        assert!(
            matches!(&updates[..], [Update::Deviation(d)] if d.deviation == Deviation::Missing),
            "{updates:?}"
        );
        // And a replacement whose new file never came: the old content is reported.
        let mut m2 = f.monitor(vec![], quick()).with_announce_ttl(Duration::ZERO);
        let b2 = with_entry(&m2, "bin/helper.dll", b"a library v2");
        let updates = m2.rebase(b2).unwrap();
        assert!(
            matches!(&updates[..], [Update::Deviation(d)] if matches!(d.deviation, Deviation::Modified { .. })),
            "{updates:?}"
        );
    }

    #[test]
    fn a_rebase_that_explains_an_open_deviation_restores_it() {
        let f = fixture();
        let mut m = f.monitor(vec![], quick());
        fs::write(f.root().join("sul.exe"), b"the program, patched").unwrap();
        assert_eq!(m.session_start().unwrap().len(), 1);
        let b = with_entry(&m, "sul.exe", b"the program, patched");
        let updates = m.rebase(b).unwrap();
        assert_eq!(
            updates,
            vec![Update::Restored {
                path: "sul.exe".into()
            }]
        );
        assert!(m.open_deviations().is_empty());
    }

    #[test]
    fn dropping_an_entry_makes_the_file_unlisted_and_only_changed_paths_are_read() {
        let f = fixture();
        let mut m = f.monitor(vec![], quick());
        // A model file that changed with no event: a rebase must not go and hash it.
        fs::write(f.root().join("models/jarvis.gguf"), b"changed weights").unwrap();
        let mut b = m.baseline.clone();
        b.entries.retain(|e| e.path != "bin/helper.dll");
        let updates = m.rebase(b).unwrap();
        assert!(
            matches!(&updates[..], [Update::Deviation(d)] if d.path == "bin/helper.dll" && matches!(d.deviation, Deviation::Unlisted { .. })),
            "{updates:?}"
        );
        assert_eq!(
            m.open_deviations().len(),
            1,
            "the model change was not looked at"
        );
    }

    #[test]
    fn session_start_reports_nothing_for_a_clean_tree_and_everything_for_a_dirty_one() {
        let f = fixture();
        let mut m = f.monitor(vec![], quick());
        assert!(m.session_start().unwrap().is_empty());
        fs::write(f.root().join("sul.exe"), b"changed").unwrap();
        fs::write(f.root().join("models/jarvis.gguf"), b"changed too").unwrap();
        let updates = m.session_start().unwrap();
        assert_eq!(updates.len(), 2, "{updates:?}");
        assert_eq!(m.open_deviations().len(), 2);
    }

    #[test]
    fn an_event_for_a_modified_file_is_reported_once_and_restoring_it_is_reported() {
        let f = fixture();
        let sul = f.change("sul.exe");
        // Four pumps, each one event and then the quiet that ends the batch.
        let mut m = f.monitor(
            vec![
                vec![sul.clone()],
                vec![],
                vec![sul.clone()],
                vec![],
                vec![sul.clone()],
                vec![],
                vec![sul],
                vec![],
            ],
            quick(),
        );
        // No change yet: an event for an untouched file is not a deviation.
        assert!(m.pump(Duration::from_millis(1)).unwrap().is_empty());

        fs::write(f.root().join("sul.exe"), b"changed").unwrap();
        let updates = m.pump(Duration::from_millis(1)).unwrap();
        assert_eq!(updates.len(), 1);
        match &updates[0] {
            Update::Deviation(d) => {
                assert_eq!(d.path, "sul.exe");
                assert!(matches!(d.deviation, Deviation::Modified { .. }));
            }
            other => panic!("{other:?}"),
        }

        // The same deviation again (more events while it persists) is not reported again.
        assert!(m.pump(Duration::from_millis(1)).unwrap().is_empty());
        assert_eq!(m.open_deviations().len(), 1);

        // Put the original bytes back: the warning can be cleared.
        fs::write(f.root().join("sul.exe"), b"the program").unwrap();
        let updates = m.pump(Duration::from_millis(1)).unwrap();
        assert_eq!(
            updates,
            vec![Update::Restored {
                path: "sul.exe".into()
            }]
        );
        assert!(m.open_deviations().is_empty());
    }

    #[test]
    fn a_deviation_that_changes_kind_is_reported_again() {
        let f = fixture();
        let sul = f.change("sul.exe");
        let mut m = f.monitor(vec![vec![sul.clone()], vec![], vec![sul], vec![]], quick());
        fs::write(f.root().join("sul.exe"), b"changed").unwrap();
        assert_eq!(m.pump(Duration::from_millis(1)).unwrap().len(), 1);
        fs::remove_file(f.root().join("sul.exe")).unwrap();
        let updates = m.pump(Duration::from_millis(1)).unwrap();
        assert!(
            matches!(&updates[..], [Update::Deviation(d)] if d.deviation == Deviation::Missing),
            "{updates:?}"
        );
    }

    #[test]
    fn many_events_for_one_write_cost_one_check_and_one_report() {
        let f = fixture();
        let sul = f.change("sul.exe");
        let burst = vec![sul.clone(), sul.clone(), sul.clone()];
        let mut m = f.monitor(vec![burst, vec![sul.clone(), sul], vec![]], quick());
        fs::write(f.root().join("sul.exe"), b"changed").unwrap();
        assert_eq!(m.pump(Duration::from_millis(1)).unwrap().len(), 1);
    }

    #[test]
    fn a_new_guarded_file_is_unlisted_and_an_unguarded_one_is_ignored() {
        let f = fixture();
        let script = vec![
            vec![f.change("bin/planted.dll"), f.change("bin/readme.txt")],
            vec![],
        ];
        // The baseline is taken here; what is planted afterwards is what the monitor must notice.
        let mut m = f.monitor(script, quick());
        fs::write(f.root().join("bin/planted.dll"), b"x").unwrap();
        fs::write(f.root().join("bin/readme.txt"), b"x").unwrap();
        let updates = m.pump(Duration::from_millis(1)).unwrap();
        assert_eq!(updates.len(), 1, "{updates:?}");
        assert!(matches!(&updates[0], Update::Deviation(d) if d.path == "bin/planted.dll"));
    }

    #[test]
    fn a_rescan_notice_checks_everything_including_files_with_no_event() {
        let f = fixture();
        let mut m = f.monitor(vec![vec![Change::Rescan], vec![]], quick());
        fs::write(f.root().join("bin/helper.dll"), b"changed").unwrap();
        fs::write(f.root().join("models/jarvis.gguf"), b"changed").unwrap();
        let updates = m.pump(Duration::from_millis(1)).unwrap();
        assert_eq!(
            updates.len(),
            2,
            "overflow must not lose a change: {updates:?}"
        );
    }

    #[test]
    fn an_event_for_a_folder_checks_every_guarded_file_below_it() {
        let f = fixture();
        let mut m = f.monitor(vec![vec![f.change("bin")], vec![]], quick());
        fs::write(f.root().join("bin/helper.dll"), b"changed").unwrap();
        let updates = m.pump(Duration::from_millis(1)).unwrap();
        assert!(
            matches!(&updates[..], [Update::Deviation(d)] if d.path == "bin/helper.dll"),
            "{updates:?}"
        );
    }

    #[test]
    fn an_event_on_the_root_itself_or_outside_it() {
        let f = fixture();
        let outside = Change::Path(PathBuf::from("/definitely/not/under/the/root/sul.exe"));
        // Both baselines are taken before the file changes, so the change is a real deviation that only a check would find.
        let mut ignoring = f.monitor(vec![vec![outside], vec![]], quick());
        let mut checking = f.monitor(
            vec![vec![Change::Path(f.root().to_path_buf())], vec![]],
            quick(),
        );
        fs::write(f.root().join("sul.exe"), b"changed").unwrap();
        assert!(
            ignoring.pump(Duration::from_millis(1)).unwrap().is_empty(),
            "an outside path is ignored, so the real change goes unseen until something checks"
        );
        assert_eq!(
            checking.pump(Duration::from_millis(1)).unwrap().len(),
            1,
            "the root changing means check everything"
        );
    }

    #[test]
    fn the_periodic_pass_skips_model_files_but_the_session_start_does_not() {
        let f = fixture();
        let config = MonitorConfig {
            full_rescan_every: Some(Duration::ZERO),
            ..quick()
        };
        let mut m = f.monitor(vec![], config);
        fs::write(f.root().join("models/jarvis.gguf"), b"changed").unwrap();
        fs::write(f.root().join("bin/helper.dll"), b"changed").unwrap();
        let updates = m.pump(Duration::from_millis(1)).unwrap();
        assert_eq!(updates.len(), 1, "only the dll: {updates:?}");
        assert!(matches!(&updates[0], Update::Deviation(d) if d.path == "bin/helper.dll"));
        // The model deviation is found by the full session check.
        let updates = m.session_start().unwrap();
        assert_eq!(updates.len(), 1);
        assert!(matches!(&updates[0], Update::Deviation(d) if d.path == "models/jarvis.gguf"));
    }

    #[test]
    fn a_periodic_pass_does_not_restore_a_model_deviation_it_did_not_look_at() {
        let f = fixture();
        let config = MonitorConfig {
            full_rescan_every: Some(Duration::ZERO),
            ..quick()
        };
        let mut m = f.monitor(vec![], config);
        fs::write(f.root().join("models/jarvis.gguf"), b"changed").unwrap();
        assert_eq!(m.session_start().unwrap().len(), 1);
        // The periodic pass runs (due immediately) and must leave the open model deviation alone.
        assert!(m.pump(Duration::from_millis(1)).unwrap().is_empty());
        assert_eq!(m.open_deviations().len(), 1);
    }

    #[test]
    fn a_source_that_fails_stops_the_pump_with_an_error() {
        struct Broken;
        impl ChangeSource for Broken {
            fn wait(&mut self, _: Duration) -> Result<Vec<Change>> {
                Err(Error::Unsupported("the file watcher stopped"))
            }
        }
        let f = fixture();
        let selection = selection();
        let baseline = Baseline::compute(f.root(), "t", &selection, 1).unwrap();
        let mut m = Monitor::new(f.root(), baseline, selection, Broken, quick());
        assert!(m.pump(Duration::from_millis(1)).is_err());
    }

    // ---- policy pieces that no end-to-end outcome can tell apart (found by deliberate-bug checks, 2026-11-26) ---------

    fn event(kind: EventKind, path: &str) -> notify::Result<notify::Event> {
        Ok(notify::Event::new(kind).add_path(PathBuf::from(path)))
    }

    #[test]
    fn reading_or_opening_a_file_is_not_a_change_but_closing_it_after_a_write_is() {
        // A read that counted as a change would still report nothing (the check finds the file clean); it would only
        // re-hash the file on every read. Only a direct look at what the watcher's events become can tell.
        use notify::event::{CreateKind, ModifyKind, RemoveKind};
        let mut out = Vec::new();
        for kind in [
            EventKind::Access(AccessKind::Open(AccessMode::Any)),
            EventKind::Access(AccessKind::Close(AccessMode::Read)),
            EventKind::Access(AccessKind::Read),
        ] {
            translate(event(kind, "/r/sul.exe"), &mut out);
        }
        assert!(out.is_empty(), "reads are not changes: {out:?}");
        for kind in [
            EventKind::Access(AccessKind::Close(AccessMode::Write)),
            EventKind::Modify(ModifyKind::Any),
            EventKind::Create(CreateKind::Any),
            EventKind::Remove(RemoveKind::Any),
        ] {
            translate(event(kind, "/r/sul.exe"), &mut out);
        }
        assert_eq!(out.len(), 4, "{out:?}");
        assert!(out.iter().all(|c| matches!(c, Change::Path(_))));
    }

    #[test]
    fn an_overflow_flag_and_a_watcher_error_each_ask_for_a_full_check() {
        let overflow = notify::Event::new(EventKind::Other).set_flag(notify::event::Flag::Rescan);
        let mut out = Vec::new();
        translate(Ok(overflow), &mut out);
        assert_eq!(
            out,
            vec![Change::Rescan],
            "the system said events were lost"
        );

        let mut out = Vec::new();
        translate(
            Err(notify::Error::generic("the event buffer overflowed")),
            &mut out,
        );
        assert_eq!(
            out,
            vec![Change::Rescan],
            "a failing watcher means lost events"
        );

        // An ordinary event asks for no full check.
        let mut out = Vec::new();
        translate(event(EventKind::Any, "/r/a"), &mut out);
        assert!(!out.contains(&Change::Rescan), "{out:?}");
    }

    fn pd(deviation: Deviation) -> PathDeviation {
        PathDeviation {
            path: "sul.exe".into(),
            role: None,
            artifact_code: None,
            deviation,
        }
    }

    fn unreadable() -> Deviation {
        Deviation::Unreadable {
            kind: "PermissionDenied".into(),
        }
    }

    #[test]
    fn a_locked_file_is_looked_at_again_once_and_only_once() {
        // Locked, then free and clean: the lock was brief and there is nothing to report.
        let mut looks = 0;
        let found = retry_once_if_unreadable(Duration::ZERO, || {
            looks += 1;
            (looks == 1).then(|| pd(unreadable()))
        });
        assert_eq!((found, looks), (None, 2));

        // Locked, then modified: the second look is the answer.
        let mut looks = 0;
        let modified = Deviation::Modified {
            expected_sha256: "a".into(),
            actual_sha256: "b".into(),
            expected_size: 1,
            actual_size: 2,
        };
        let found = retry_once_if_unreadable(Duration::ZERO, || {
            looks += 1;
            Some(pd(if looks == 1 {
                unreadable()
            } else {
                modified.clone()
            }))
        });
        assert_eq!((found, looks), (Some(pd(modified.clone())), 2));

        // Still locked on the second look: reported as unreadable, after two looks and no more.
        let mut looks = 0;
        let found = retry_once_if_unreadable(Duration::ZERO, || {
            looks += 1;
            Some(pd(unreadable()))
        });
        assert_eq!((found, looks), (Some(pd(unreadable())), 2));

        // Anything that is not "unreadable" is final at the first look: clean, missing, modified.
        for first in [None, Some(pd(Deviation::Missing)), Some(pd(modified))] {
            let mut looks = 0;
            let found = retry_once_if_unreadable(Duration::ZERO, || {
                looks += 1;
                first.clone()
            });
            assert_eq!((found, looks), (first, 1));
        }
    }

    // ---- the real operating-system source ------------------------------------------------------------------------

    #[test]
    fn the_real_watcher_reports_a_write_to_a_guarded_file_within_a_second() {
        let f = fixture();
        let selection = selection();
        let baseline = Baseline::compute(f.root(), "t", &selection, 1).unwrap();
        let source = NotifySource::watch(f.root()).unwrap();
        let mut m = Monitor::new(
            f.root(),
            baseline,
            selection,
            source,
            MonitorConfig::default(),
        );

        let started = Instant::now();
        fs::write(f.root().join("bin/helper.dll"), b"tampered").unwrap();
        let mut found = None;
        while started.elapsed() < Duration::from_secs(1) {
            let updates = m.pump(Duration::from_millis(100)).unwrap();
            if let Some(Update::Deviation(d)) = updates.into_iter().next() {
                found = Some((d, started.elapsed()));
                break;
            }
        }
        let (d, took) = found.expect("no deviation reported within one second of the write");
        assert_eq!(d.path, "bin/helper.dll");
        assert!(matches!(d.deviation, Deviation::Modified { .. }));
        assert!(took < Duration::from_secs(1), "{took:?}");
    }

    #[test]
    fn the_real_watcher_stays_quiet_when_a_guarded_file_is_only_read() {
        let f = fixture();
        let selection = selection();
        let baseline = Baseline::compute(f.root(), "t", &selection, 1).unwrap();
        let source = NotifySource::watch(f.root()).unwrap();
        let mut m = Monitor::new(
            f.root(),
            baseline,
            selection,
            source,
            MonitorConfig::default(),
        );
        let _ = fs::read(f.root().join("sul.exe")).unwrap();
        let _ = fs::read(f.root().join("bin/helper.dll")).unwrap();
        assert!(m.pump(Duration::from_millis(300)).unwrap().is_empty());
    }

    #[test]
    fn watching_a_folder_that_does_not_exist_is_an_error() {
        let f = fixture();
        assert!(NotifySource::watch(&f.root().join("nowhere")).is_err());
    }
}
