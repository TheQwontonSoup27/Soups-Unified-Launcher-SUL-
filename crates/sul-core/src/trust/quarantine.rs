//! Quarantine: set a file aside where nothing will use it, verified and reversible.
//!
//! What it does: copy the file into the quarantine folder, check the copy hash for hash, record where it came from and why,
//! and only then remove the original. Every step is written to a recovery journal first. A quarantined file can be put back
//! byte for byte, and is never overwritten on the way.
//!
//! What it will not do:
//! * touch anything inside a protected root (the original game installation): nothing is moved out of one or restored into one;
//! * follow or move links, folders or anything but a regular file;
//! * delete anything that is not its own unfinished copy. There is no purge: a permanent delete needs its own design
//!   (preview, confirmation, log). The only file this module removes after a restore is the quarantined copy, and only after the
//!   restored file has been checked to be identical to it.
//!
//! Limits: a restored file has new timestamps and default permissions (the copy does not carry them), and a file that another
//! program has open and locked cannot be removed, in which case the quarantine is undone and an error is returned.

use crate::fsops::{ProtectedRoots, atomic_write};
use crate::journal::Journal;
use crate::manifest::hash_file;
use crate::{Error, Result, error::io};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEMA_VERSION: u32 = 1;
const MAX_REASON: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuarantineState {
    /// The copy is in quarantine and the original is gone.
    Held,
    /// The file has been put back; the quarantined copy has been removed. The record stays as a trail.
    Restored,
}

/// Everything known about one quarantined file. Stored beside the file as `record.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuarantineRecord {
    pub schema_version: u32,
    pub id: String,
    /// Where the file was, as text. A path that is not valid text is refused rather than guessed at.
    pub original_path: String,
    pub size: u64,
    pub sha256: String,
    pub reason: String,
    pub quarantined_unix: u64,
    pub state: QuarantineState,
    pub restored_unix: Option<u64>,
}

/// What is in quarantine. A folder whose record cannot be read is listed apart, so one bad record hides nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Listing {
    pub records: Vec<QuarantineRecord>,
    pub unreadable: Vec<String>,
}

/// What [`Quarantine::recover`] did with operations that were interrupted.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Recovery {
    /// Operations that could be finished or undone with no data at risk. Each says what was done.
    pub resolved: Vec<String>,
    /// Operations left as they were because deciding would risk data. A person has to look.
    pub needs_attention: Vec<String>,
}

pub struct Quarantine {
    root: PathBuf,
    protected: ProtectedRoots,
    journal: Journal,
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `q-<13 digits>-<12 hex>-<4+ digits>`. Checked on every use, so an id can never name a path outside the quarantine folder.
fn valid_id(id: &str) -> bool {
    let mut parts = id.split('-');
    let (Some(q), Some(ms), Some(hex), Some(n), None) = (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) else {
        return false;
    };
    q == "q"
        && ms.len() == 13
        && ms.bytes().all(|b| b.is_ascii_digit())
        && hex.len() == 12
        && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        && n.len() >= 4
        && n.bytes().all(|b| b.is_ascii_digit())
}

fn check_id(id: &str) -> Result<()> {
    if valid_id(id) {
        Ok(())
    } else {
        Err(Error::Invalid(format!("{id:?} is not a quarantine id")))
    }
}

fn check_reason(reason: &str) -> Result<String> {
    let r = reason.trim();
    if r.is_empty() || r.chars().count() > MAX_REASON || r.chars().any(char::is_control) {
        return Err(Error::Invalid(format!(
            "a reason is required: 1 to {MAX_REASON} characters, no control characters"
        )));
    }
    Ok(r.to_owned())
}

/// Copy `from` to a new file `to` (never over an existing one), flushed to disk.
fn copy_new(from: &Path, to: &Path) -> Result<()> {
    let mut src = std::fs::File::open(from).map_err(io(from))?;
    let mut dst = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)
        .map_err(io(to))?;
    std::io::copy(&mut src, &mut dst).map_err(io(to))?;
    dst.flush().map_err(io(to))?;
    dst.sync_all().map_err(io(to))?;
    Ok(())
}

fn require_regular_file(path: &Path) -> Result<()> {
    if !path.is_absolute() {
        return Err(Error::Invalid(format!(
            "{} is not an absolute path",
            path.display()
        )));
    }
    let meta = std::fs::symlink_metadata(path).map_err(io(path))?;
    if !meta.file_type().is_file() {
        return Err(Error::Invalid(format!(
            "{} is not a regular file (links and folders are not quarantined)",
            path.display()
        )));
    }
    Ok(())
}

impl Quarantine {
    /// Open (creating if needed) a quarantine folder. The folder itself must not be inside a protected root.
    pub fn open(root: &Path, protected: ProtectedRoots) -> Result<Self> {
        protected.check_writable(root)?;
        std::fs::create_dir_all(root.join("items")).map_err(io(root))?;
        let root = root.canonicalize().map_err(io(root))?;
        let journal = Journal::open(&root.join("journal.jsonl"))?;
        Ok(Self {
            root,
            protected,
            journal,
        })
    }

    fn item_dir(&self, id: &str) -> PathBuf {
        self.root.join("items").join(id)
    }
    fn payload(&self, id: &str) -> PathBuf {
        self.item_dir(id).join("payload")
    }
    fn record_path(&self, id: &str) -> PathBuf {
        self.item_dir(id).join("record.json")
    }

    fn write_record(&self, record: &QuarantineRecord) -> Result<()> {
        atomic_write(
            &ProtectedRoots::new(),
            &self.record_path(&record.id),
            serde_json::to_string_pretty(record)?.as_bytes(),
        )
    }

    fn read_record(&self, id: &str) -> Result<QuarantineRecord> {
        check_id(id)?;
        let path = self.record_path(id);
        let text = std::fs::read_to_string(&path).map_err(io(&path))?;
        let record: QuarantineRecord = serde_json::from_str(&text)?;
        if record.id != id || record.schema_version != SCHEMA_VERSION {
            return Err(Error::Invalid(format!(
                "the record for {id} is not one this version understands"
            )));
        }
        Ok(record)
    }

    /// Remove this module's own unfinished item folder. It only ever names a folder built from a checked id under `items/`.
    fn discard_unfinished(&self, id: &str) {
        if valid_id(id) {
            let _ = std::fs::remove_dir_all(self.item_dir(id));
        }
    }

    /// Set `source` aside. On success the file is in quarantine and gone from where it was; on any error it is where it was.
    pub fn quarantine_file(&self, source: &Path, reason: &str) -> Result<QuarantineRecord> {
        self.quarantine_with(source, reason, |_| {}, |_| {}, |p| std::fs::remove_file(p))
    }

    /// The real work, with three seams so tests can cause the failures that are hard to cause for real: `before_copy` runs after
    /// the file was hashed and before it is copied (a writer that changes it in between), `after_copy` runs once the copy is
    /// verified (a writer that changes it afterwards), and `remove` takes the original away (a locked file).
    fn quarantine_with(
        &self,
        source: &Path,
        reason: &str,
        before_copy: impl Fn(&Path),
        after_copy: impl Fn(&Path),
        remove: impl Fn(&Path) -> std::io::Result<()>,
    ) -> Result<QuarantineRecord> {
        let reason = check_reason(reason)?;
        require_regular_file(source)?;
        self.protected.check_writable(source)?;
        if crate::safepath::resolve_lenient(source).starts_with(&self.root) {
            return Err(Error::Invalid(
                "a file that is already in quarantine cannot be quarantined again".into(),
            ));
        }
        let original_path = source
            .to_str()
            .ok_or_else(|| {
                Error::Invalid(format!(
                    "{} is not valid text, so its name cannot be recorded",
                    source.display()
                ))
            })?
            .to_owned();

        let (sha256, size) = hash_file(source)?;
        let id = format!(
            "q-{:013}-{}-{:04}",
            now_unix() * 1000,
            &sha256[..12],
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        self.journal.begin(&id, "quarantine", &original_path)?;

        let finish_with_error = |e: Error, why: &str| -> Error {
            self.discard_unfinished(&id);
            let _ = self.journal.abort(&id, why);
            e
        };

        let dir = self.item_dir(&id);
        if let Err(e) = std::fs::create_dir(&dir).map_err(io(&dir)) {
            let _ = self.journal.abort(&id, "could not create the item folder");
            return Err(e);
        }
        let payload = self.payload(&id);
        before_copy(source);
        if let Err(e) = copy_new(source, &payload) {
            return Err(finish_with_error(e, "copy failed"));
        }
        match hash_file(&payload) {
            Ok(h) if h == (sha256.clone(), size) => {}
            Ok(_) => {
                return Err(finish_with_error(
                    Error::Invalid(
                        "the copy does not match the original; nothing was changed".into(),
                    ),
                    "copy did not match",
                ));
            }
            Err(e) => return Err(finish_with_error(e, "copy could not be read back")),
        }
        let record = QuarantineRecord {
            schema_version: SCHEMA_VERSION,
            id: id.clone(),
            original_path,
            size,
            sha256: sha256.clone(),
            reason,
            quarantined_unix: now_unix(),
            state: QuarantineState::Held,
            restored_unix: None,
        };
        if let Err(e) = self.write_record(&record) {
            return Err(finish_with_error(e, "record could not be written"));
        }

        after_copy(source);
        // Something may have written to the file since it was copied. Only a file that is still exactly what was copied is removed.
        match hash_file(source) {
            Ok(h) if h == (sha256, size) => {}
            Ok(_) => {
                return Err(finish_with_error(
                    Error::Invalid(
                        "the file changed while it was being quarantined; nothing was changed"
                            .into(),
                    ),
                    "source changed",
                ));
            }
            Err(e) => return Err(finish_with_error(e, "source could not be read again")),
        }
        if let Err(e) = remove(source) {
            return Err(finish_with_error(
                Error::Io {
                    path: source.to_path_buf(),
                    source: e,
                },
                "original could not be removed",
            ));
        }
        self.journal.commit(&id)?;
        Ok(record)
    }

    /// Everything in quarantine, oldest first.
    pub fn list(&self) -> Result<Listing> {
        let items = self.root.join("items");
        let mut listing = Listing::default();
        for entry in std::fs::read_dir(&items).map_err(io(&items))? {
            let entry = entry.map_err(io(&items))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            match self.read_record(&name) {
                Ok(r) => listing.records.push(r),
                Err(_) => listing.unreadable.push(name),
            }
        }
        listing
            .records
            .sort_by(|a, b| (a.quarantined_unix, &a.id).cmp(&(b.quarantined_unix, &b.id)));
        listing.unreadable.sort();
        Ok(listing)
    }

    /// Put the file back where it was. Refuses if anything is there now.
    pub fn restore(&self, id: &str) -> Result<PathBuf> {
        let record = self.read_record(id)?;
        let dest = PathBuf::from(&record.original_path);
        self.restore_to(id, &dest)
    }

    /// Put the file at `dest`, which must be an absolute path to a place that does not exist yet, in a folder that does.
    pub fn restore_to(&self, id: &str, dest: &Path) -> Result<PathBuf> {
        let record = self.read_record(id)?;
        if record.state != QuarantineState::Held {
            return Err(Error::Invalid(format!("{id} has already been restored")));
        }
        if !dest.is_absolute() {
            return Err(Error::Invalid(format!(
                "{} is not an absolute path",
                dest.display()
            )));
        }
        self.protected.check_writable(dest)?;
        if std::fs::symlink_metadata(dest).is_ok() {
            return Err(Error::Invalid(format!(
                "{} already exists; a restore never overwrites",
                dest.display()
            )));
        }
        match dest.parent() {
            Some(p) if p.is_dir() => {}
            _ => {
                return Err(Error::Invalid(format!(
                    "the folder for {} does not exist",
                    dest.display()
                )));
            }
        }
        let payload = self.payload(id);
        if hash_file(&payload)? != (record.sha256.clone(), record.size) {
            return Err(Error::Invalid(
                "the quarantined copy no longer matches its record, so it was not restored".into(),
            ));
        }

        let op = format!("restore-{id}");
        self.journal
            .begin(&op, "restore", &dest.to_string_lossy())?;
        if let Err(e) = copy_new(&payload, dest) {
            let _ = self.journal.abort(&op, "copy failed");
            return Err(e);
        }
        match hash_file(dest) {
            Ok(h) if h == (record.sha256.clone(), record.size) => {}
            other => {
                // The file at `dest` is the one this call just created, so removing it undoes only this call.
                let _ = std::fs::remove_file(dest);
                let _ = self.journal.abort(&op, "restored copy did not match");
                return Err(other.err().unwrap_or_else(|| {
                    Error::Invalid("the restored copy does not match; it was removed".into())
                }));
            }
        }
        self.finish_restore(&record)?;
        self.journal.commit(&op)?;
        Ok(dest.to_path_buf())
    }

    /// Mark the record restored, then remove the quarantined copy (the restored file was just checked to be identical to it).
    fn finish_restore(&self, record: &QuarantineRecord) -> Result<()> {
        let mut done = record.clone();
        done.state = QuarantineState::Restored;
        done.restored_unix = Some(now_unix());
        self.write_record(&done)?;
        let payload = self.payload(&record.id);
        std::fs::remove_file(&payload).map_err(io(&payload))
    }

    /// Settle operations that were interrupted (a crash, a power cut). Only cases where no data is at risk are settled;
    /// the rest are reported and left exactly as they are.
    pub fn recover(&self) -> Result<Recovery> {
        let mut out = Recovery::default();
        for p in self.journal.pending()? {
            match p.op.as_str() {
                "quarantine" if valid_id(&p.id) => {
                    self.recover_quarantine(&p.id, Path::new(&p.detail), &mut out)?
                }
                "restore" if p.id.strip_prefix("restore-").is_some_and(valid_id) => self
                    .recover_restore(
                        &p.id,
                        p.id.strip_prefix("restore-").unwrap_or(""),
                        Path::new(&p.detail),
                        &mut out,
                    )?,
                _ => out.needs_attention.push(format!(
                    "{}: an operation this version does not recognise ({})",
                    p.id, p.op
                )),
            }
        }
        Ok(out)
    }

    fn recover_quarantine(&self, id: &str, original: &Path, out: &mut Recovery) -> Result<()> {
        if std::fs::symlink_metadata(original).is_ok() {
            // The original was never removed, so nothing is lost: drop the unfinished copy.
            self.discard_unfinished(id);
            self.journal
                .abort(id, "recovered: the original is still in place")?;
            out.resolved.push(format!(
                "{id}: the original is still in place; the unfinished copy was removed"
            ));
            return Ok(());
        }
        match (self.read_record(id), hash_file(&self.payload(id))) {
            (Ok(r), Ok(h)) if h == (r.sha256.clone(), r.size) => {
                self.journal.commit(id)?;
                out.resolved.push(format!("{id}: the original was already removed and the quarantined copy is intact; marked complete"));
            }
            _ => out.needs_attention.push(format!(
                "{id}: the original is gone and the quarantined copy cannot be confirmed. Nothing was changed; look in {}",
                self.item_dir(id).display()
            )),
        }
        Ok(())
    }

    fn recover_restore(&self, op: &str, id: &str, dest: &Path, out: &mut Recovery) -> Result<()> {
        let record = match self.read_record(id) {
            Ok(r) => r,
            Err(_) => {
                out.needs_attention.push(format!(
                    "{op}: its record cannot be read. Nothing was changed"
                ));
                return Ok(());
            }
        };
        let want = (record.sha256.clone(), record.size);
        if record.state == QuarantineState::Restored {
            self.journal.commit(op)?;
            out.resolved
                .push(format!("{op}: already finished; marked complete"));
        } else if std::fs::symlink_metadata(dest).is_err() {
            self.journal
                .abort(op, "recovered: nothing had been restored yet")?;
            out.resolved.push(format!(
                "{op}: nothing had been restored; the quarantined copy is untouched"
            ));
        } else if hash_file(dest).is_ok_and(|h| h == want)
            && hash_file(&self.payload(id)).is_ok_and(|h| h == want)
        {
            self.finish_restore(&record)?;
            self.journal.commit(op)?;
            out.resolved.push(format!(
                "{op}: the file was restored and checked; the record was completed"
            ));
        } else {
            out.needs_attention.push(format!(
                "{op}: something is at {} that does not match. Nothing was changed",
                dest.display()
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct Fixture {
        _tmp: tempfile::TempDir,
        install: PathBuf,
        work: PathBuf,
        q: Quarantine,
    }

    /// A quarantine folder, a protected "installation" folder, and a work folder where files may be moved.
    fn fixture() -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let install = tmp.path().join("install");
        let work = tmp.path().join("work");
        std::fs::create_dir_all(&install).unwrap();
        std::fs::create_dir_all(&work).unwrap();
        let mut protected = ProtectedRoots::new();
        protected.add(&install).unwrap();
        let q = Quarantine::open(&tmp.path().join("quarantine"), protected).unwrap();
        Fixture {
            install: install.canonicalize().unwrap(),
            work: work.canonicalize().unwrap(),
            q,
            _tmp: tmp,
        }
    }

    fn put(dir: &Path, name: &str, data: &[u8]) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, data).unwrap();
        p
    }

    fn item_names(f: &Fixture) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(f.q.root.join("items"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    // ---- quarantining ----

    #[test]
    fn a_file_is_set_aside_verified_recorded_and_removed_from_where_it_was() {
        let f = fixture();
        let file = put(&f.work, "hook.dll", b"MZ some bytes");
        let r =
            f.q.quarantine_file(&file, "  unlisted program file  ")
                .unwrap();
        assert!(!file.exists(), "the original is gone");
        assert_eq!(std::fs::read(f.q.payload(&r.id)).unwrap(), b"MZ some bytes");
        assert_eq!(r.reason, "unlisted program file", "the reason is trimmed");
        assert_eq!(r.original_path, file.to_str().unwrap());
        assert_eq!(r.size, 13);
        assert_eq!(r.sha256.len(), 64);
        assert_eq!(r.state, QuarantineState::Held);
        assert!(valid_id(&r.id), "{}", r.id);
        assert!(f.q.journal.pending().unwrap().is_empty(), "committed");
        let listing = f.q.list().unwrap();
        assert_eq!(listing.records, [r]);
        assert!(listing.unreadable.is_empty());
    }

    #[test]
    fn the_same_content_twice_gets_two_ids() {
        let f = fixture();
        let a =
            f.q.quarantine_file(&put(&f.work, "a.dll", b"same"), "x")
                .unwrap();
        let b =
            f.q.quarantine_file(&put(&f.work, "b.dll", b"same"), "x")
                .unwrap();
        assert_ne!(a.id, b.id);
        assert_eq!(a.sha256, b.sha256);
        assert_eq!(f.q.list().unwrap().records.len(), 2);
    }

    #[test]
    fn an_empty_file_can_be_quarantined_and_restored() {
        let f = fixture();
        let file = put(&f.work, "empty.bin", b"");
        let r = f.q.quarantine_file(&file, "x").unwrap();
        assert_eq!(r.size, 0);
        f.q.restore(&r.id).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"");
    }

    #[test]
    fn a_large_file_is_streamed_and_survives_a_round_trip() {
        let f = fixture();
        let data: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
        let file = put(&f.work, "big.bin", &data);
        let r = f.q.quarantine_file(&file, "x").unwrap();
        assert_eq!(r.size, 3_000_000);
        f.q.restore(&r.id).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), data);
    }

    #[test]
    fn things_that_are_not_a_regular_absolute_file_are_refused_and_nothing_changes() {
        let f = fixture();
        let dir = f.work.join("folder");
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("inner.txt"), b"x").unwrap();
        for bad in [
            dir.clone(),
            f.work.join("missing.dll"),
            PathBuf::from("relative.dll"),
        ] {
            assert!(f.q.quarantine_file(&bad, "x").is_err(), "{}", bad.display());
        }
        assert!(dir.join("inner.txt").exists(), "a folder is left alone");
        assert!(item_names(&f).is_empty(), "no item was started");
        assert!(
            f.q.journal.pending().unwrap().is_empty(),
            "refused before anything was journaled"
        );
    }

    #[test]
    fn nothing_is_moved_out_of_a_protected_root() {
        let f = fixture();
        let original = put(&f.install, "game.exe", b"the original install");
        let sub = f.install.join("data");
        std::fs::create_dir(&sub).unwrap();
        let nested = put(&sub, "a.pak", b"nested");
        for p in [&original, &nested] {
            assert!(
                matches!(f.q.quarantine_file(p, "x"), Err(Error::Protected(_))),
                "{}",
                p.display()
            );
            assert!(p.exists(), "{} is untouched", p.display());
        }
        assert!(item_names(&f).is_empty());
        assert!(f.q.journal.pending().unwrap().is_empty());
    }

    #[test]
    fn a_dot_dot_path_cannot_reach_into_a_protected_root() {
        let f = fixture();
        let original = put(&f.install, "game.exe", b"x");
        let sneaky = f.work.join("..").join("install").join("game.exe");
        assert!(matches!(
            f.q.quarantine_file(&sneaky, "x"),
            Err(Error::Protected(_))
        ));
        assert!(original.exists());
    }

    #[test]
    fn a_file_already_in_quarantine_is_not_quarantined_again() {
        let f = fixture();
        let r =
            f.q.quarantine_file(&put(&f.work, "a.dll", b"x"), "x")
                .unwrap();
        assert!(
            matches!(f.q.quarantine_file(&f.q.payload(&r.id), "again"), Err(Error::Invalid(m)) if m.contains("already in quarantine"))
        );
        assert!(f.q.payload(&r.id).exists());
    }

    #[test]
    fn a_reason_is_required_and_must_be_plain_text() {
        let f = fixture();
        let file = put(&f.work, "a.dll", b"x");
        for bad in ["", "   ", "line\nbreak", "bell\u{7}", &"x".repeat(501)] {
            assert!(f.q.quarantine_file(&file, bad).is_err(), "{bad:?}");
        }
        assert!(file.exists());
        assert!(f.q.quarantine_file(&file, &"x".repeat(500)).is_ok());
    }

    #[test]
    fn the_quarantine_folder_cannot_be_inside_a_protected_root() {
        let tmp = tempfile::tempdir().unwrap();
        let install = tmp.path().join("install");
        std::fs::create_dir(&install).unwrap();
        let mut protected = ProtectedRoots::new();
        protected.add(&install).unwrap();
        assert!(matches!(
            Quarantine::open(&install.join("quarantine"), protected),
            Err(Error::Protected(_))
        ));
        assert!(
            !install.join("quarantine").exists(),
            "nothing was created inside the protected root"
        );
    }

    #[test]
    fn if_the_original_cannot_be_removed_everything_is_undone() {
        let f = fixture();
        let file = put(&f.work, "locked.dll", b"in use");
        let err =
            f.q.quarantine_with(
                &file,
                "x",
                |_| {},
                |_| {},
                |_| {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "locked",
                    ))
                },
            )
            .unwrap_err();
        assert!(matches!(err, Error::Io { .. }));
        assert_eq!(
            std::fs::read(&file).unwrap(),
            b"in use",
            "the original is where it was"
        );
        assert!(
            item_names(&f).is_empty(),
            "the copy and record were removed"
        );
        assert!(
            f.q.journal.pending().unwrap().is_empty(),
            "aborted, not left pending"
        );
        assert!(f.q.list().unwrap().records.is_empty());
    }

    #[test]
    fn a_file_that_changes_while_it_is_being_quarantined_is_not_removed() {
        let f = fixture();
        let file = put(&f.work, "busy.log", b"first");
        let removed = Cell::new(false);
        let err =
            f.q.quarantine_with(
                &file,
                "x",
                |_| {},
                |p| std::fs::write(p, b"second, longer").unwrap(),
                |p| {
                    removed.set(true);
                    std::fs::remove_file(p)
                },
            )
            .unwrap_err();
        assert!(matches!(err, Error::Invalid(m) if m.contains("changed while")));
        assert!(!removed.get(), "the remove step was never reached");
        assert_eq!(
            std::fs::read(&file).unwrap(),
            b"second, longer",
            "the newer content is kept"
        );
        assert!(item_names(&f).is_empty());
        assert!(f.q.journal.pending().unwrap().is_empty());
    }

    #[test]
    fn a_file_that_changes_between_hashing_and_copying_is_refused_and_nothing_is_removed() {
        // Same length, different bytes: only a comparison of the hashes sees it, a comparison of sizes does not.
        let f = fixture();
        let file = put(&f.work, "swapped.cfg", b"abcd");
        let removed = Cell::new(false);
        let err =
            f.q.quarantine_with(
                &file,
                "x",
                |p| std::fs::write(p, b"abce").unwrap(),
                |_| {},
                |p| {
                    removed.set(true);
                    std::fs::remove_file(p)
                },
            )
            .unwrap_err();
        assert!(
            matches!(&err, Error::Invalid(m) if m.contains("the copy does not match")),
            "{err:?}"
        );
        assert!(!removed.get(), "the remove step was never reached");
        assert_eq!(
            std::fs::read(&file).unwrap(),
            b"abce",
            "the original is where it was"
        );
        assert!(item_names(&f).is_empty(), "the bad copy was removed");
        assert!(
            f.q.journal.pending().unwrap().is_empty(),
            "aborted, not left pending"
        );
        assert!(f.q.list().unwrap().records.is_empty());
    }

    #[test]
    fn ids_must_have_exactly_the_shape_the_quarantine_makes() {
        let ok = "q-1761872000000-0123456789ab-0001";
        assert!(valid_id(ok));
        assert!(
            valid_id("q-1761872000000-0123456789ab-12345"),
            "four or more digits"
        );
        for bad in [
            "",
            "q",
            "q-1761872000000-0123456789ab",
            "q-1761872000000-0123456789ab-0001-extra",
            "x-1761872000000-0123456789ab-0001",
            "Q-1761872000000-0123456789ab-0001",
            "q-176187200000-0123456789ab-0001",
            "q-17618720000000-0123456789ab-0001",
            "q-176187200000a-0123456789ab-0001",
            "q-1761872000000-0123456789a-0001",
            "q-1761872000000-0123456789abc-0001",
            "q-1761872000000-0123456789AB-0001",
            "q-1761872000000-0123456789ag-0001",
            "q-1761872000000-0123456789ab-001",
            "q-1761872000000-0123456789ab-00a1",
            "q-1761872000000-0123456789ab-",
            "q--0123456789ab-0001",
            "../q-1761872000000-0123456789ab-0001",
            "q-1761872000000-0123456789ab-0001/..",
            "q-1761872000000-0123456789ab-0001\\x",
            "q-1761872000000-..----0001",
            " q-1761872000000-0123456789ab-0001",
        ] {
            assert!(!valid_id(bad), "{bad:?} must not be accepted");
        }
    }

    // ---- restoring ----

    #[test]
    fn a_restore_puts_the_same_bytes_back_and_then_says_it_is_done() {
        let f = fixture();
        let file = put(&f.work, "mod.dll", b"MZ the bytes");
        let r = f.q.quarantine_file(&file, "x").unwrap();
        let back = f.q.restore(&r.id).unwrap();
        assert_eq!(back, file);
        assert_eq!(std::fs::read(&file).unwrap(), b"MZ the bytes");
        assert!(
            !f.q.payload(&r.id).exists(),
            "the quarantined copy is removed only after the restored file was checked"
        );
        let listed = f.q.list().unwrap().records;
        assert_eq!(listed[0].state, QuarantineState::Restored);
        assert!(listed[0].restored_unix.is_some());
        assert!(f.q.journal.pending().unwrap().is_empty());
        assert!(
            matches!(f.q.restore(&r.id), Err(Error::Invalid(m)) if m.contains("already been restored")),
            "a second restore is refused"
        );
    }

    #[test]
    fn a_restore_never_overwrites() {
        let f = fixture();
        let file = put(&f.work, "mod.dll", b"original");
        let r = f.q.quarantine_file(&file, "x").unwrap();
        std::fs::write(&file, b"a newer file with the same name").unwrap();
        assert!(
            matches!(f.q.restore(&r.id), Err(Error::Invalid(m)) if m.contains("never overwrites"))
        );
        assert_eq!(
            std::fs::read(&file).unwrap(),
            b"a newer file with the same name"
        );
        assert!(
            f.q.payload(&r.id).exists(),
            "the quarantined copy is still held"
        );
        assert_eq!(f.q.list().unwrap().records[0].state, QuarantineState::Held);
    }

    #[test]
    fn a_restore_into_a_protected_root_is_refused() {
        let f = fixture();
        let r =
            f.q.quarantine_file(&put(&f.work, "mod.dll", b"x"), "x")
                .unwrap();
        let target = f.install.join("mod.dll");
        assert!(matches!(
            f.q.restore_to(&r.id, &target),
            Err(Error::Protected(_))
        ));
        assert!(!target.exists());
        assert!(f.q.payload(&r.id).exists());
    }

    #[test]
    fn a_tampered_quarantined_copy_is_not_restored() {
        let f = fixture();
        let file = put(&f.work, "mod.dll", b"good bytes");
        let r = f.q.quarantine_file(&file, "x").unwrap();
        std::fs::write(f.q.payload(&r.id), b"other bytes").unwrap();
        assert!(
            matches!(f.q.restore(&r.id), Err(Error::Invalid(m)) if m.contains("no longer matches"))
        );
        assert!(!file.exists(), "nothing was written");
        assert!(
            f.q.journal.pending().unwrap().is_empty(),
            "refused before anything was journaled"
        );
    }

    #[test]
    fn restore_to_goes_to_a_chosen_place_that_must_be_new_and_absolute() {
        let f = fixture();
        let r =
            f.q.quarantine_file(&put(&f.work, "mod.dll", b"x"), "x")
                .unwrap();
        assert!(f.q.restore_to(&r.id, Path::new("relative.dll")).is_err());
        assert!(
            f.q.restore_to(&r.id, &f.work.join("no_such_folder").join("mod.dll"))
                .is_err()
        );
        assert!(
            f.q.restore_to(&r.id, &f.work).is_err(),
            "an existing folder is not a free name"
        );
        let elsewhere = f.work.join("elsewhere.dll");
        assert_eq!(f.q.restore_to(&r.id, &elsewhere).unwrap(), elsewhere);
        assert_eq!(std::fs::read(&elsewhere).unwrap(), b"x");
    }

    #[test]
    fn an_id_can_never_name_a_path_outside_the_quarantine_folder() {
        let f = fixture();
        let outside = put(&f.work, "outside", b"x");
        for bad in [
            "",
            "..",
            "../work",
            "q-1",
            "q-0000000000000-aaaaaaaaaaaa",
            "q-0000000000000-aaaaaaaaaaaa-0001/..",
            "/etc/passwd",
            "q-0000000000000-AAAAAAAAAAAA-0001",
            &outside.to_string_lossy(),
        ] {
            assert!(!valid_id(bad), "{bad:?}");
            assert!(f.q.restore(bad).is_err(), "{bad:?}");
            assert!(f.q.restore_to(bad, &f.work.join("x")).is_err(), "{bad:?}");
        }
        assert!(valid_id("q-1798675200000-0123456789ab-0001"));
        assert!(outside.exists());
    }

    #[test]
    fn one_unreadable_record_does_not_hide_the_others() {
        let f = fixture();
        let good =
            f.q.quarantine_file(&put(&f.work, "a.dll", b"x"), "x")
                .unwrap();
        let broken = "q-1798675200000-0123456789ab-0001";
        std::fs::create_dir(f.q.item_dir(broken)).unwrap();
        std::fs::write(f.q.record_path(broken), b"{ not json").unwrap();
        std::fs::create_dir(f.q.root.join("items").join("stray-folder")).unwrap();
        let l = f.q.list().unwrap();
        assert_eq!(l.records.len(), 1);
        assert_eq!(l.records[0].id, good.id);
        assert_eq!(l.unreadable, [broken, "stray-folder"]);
    }

    // ---- recovery ----

    fn crash_state(f: &Fixture, source_removed: bool, payload: Option<&[u8]>) -> (String, PathBuf) {
        // Reproduces what is on disk when the program stops partway: build a real quarantine, then wind it back.
        let file = put(&f.work, "interrupted.dll", b"the real bytes");
        let r = f.q.quarantine_file(&file, "x").unwrap();
        // Journal: forget the commit by rewriting it as begin only.
        std::fs::write(
            f.q.root.join("journal.jsonl"),
            format!(
                "{}\n",
                serde_json::to_string(&crate::journal::Record::Begin {
                    id: r.id.clone(),
                    op: "quarantine".into(),
                    detail: r.original_path.clone()
                })
                .unwrap()
            ),
        )
        .unwrap();
        if !source_removed {
            std::fs::write(&file, b"the real bytes").unwrap();
        }
        match payload {
            Some(bytes) => std::fs::write(f.q.payload(&r.id), bytes).unwrap(),
            None => std::fs::remove_file(f.q.payload(&r.id)).unwrap(),
        }
        (r.id, file)
    }

    #[test]
    fn recovery_drops_an_unfinished_copy_when_the_original_is_still_there() {
        let f = fixture();
        let (id, file) = crash_state(&f, false, Some(b"half")); // the original is back, the copy is partial
        let out = f.q.recover().unwrap();
        assert_eq!(out.resolved.len(), 1, "{out:?}");
        assert!(out.needs_attention.is_empty());
        assert_eq!(
            std::fs::read(&file).unwrap(),
            b"the real bytes",
            "the original is untouched"
        );
        assert!(
            !f.q.item_dir(&id).exists(),
            "only the unfinished copy was removed"
        );
        assert!(f.q.journal.pending().unwrap().is_empty());
    }

    #[test]
    fn recovery_completes_a_quarantine_whose_copy_is_intact_and_original_is_gone() {
        let f = fixture();
        let (id, file) = crash_state(&f, true, Some(b"the real bytes"));
        let out = f.q.recover().unwrap();
        assert_eq!(out.resolved.len(), 1, "{out:?}");
        assert!(!file.exists());
        assert!(f.q.payload(&id).exists(), "the copy is kept");
        assert!(f.q.journal.pending().unwrap().is_empty());
    }

    #[test]
    fn recovery_leaves_alone_and_reports_a_case_where_data_may_be_at_risk() {
        let f = fixture();
        let (id, file) = crash_state(&f, true, None); // the original is gone and the copy is missing
        let out = f.q.recover().unwrap();
        assert!(out.resolved.is_empty(), "{out:?}");
        assert_eq!(out.needs_attention.len(), 1);
        assert!(out.needs_attention[0].contains(&id));
        assert!(!file.exists());
        assert!(f.q.item_dir(&id).exists(), "nothing was deleted");
        assert_eq!(
            f.q.journal.pending().unwrap().len(),
            1,
            "still pending, so it is reported again next time"
        );

        let (id2, _) = crash_state(&f, true, Some(b"a different copy"));
        let out2 = f.q.recover().unwrap();
        assert!(
            out2.needs_attention.iter().any(|m| m.contains(&id2)),
            "a copy that does not match its record is not trusted: {out2:?}"
        );
    }

    #[test]
    fn recovery_settles_an_interrupted_restore_only_when_it_is_certain() {
        let f = fixture();
        let file = put(&f.work, "mod.dll", b"bytes");
        let r = f.q.quarantine_file(&file, "x").unwrap();
        let begin = |f: &Fixture| {
            let rec = crate::journal::Record::Begin {
                id: format!("restore-{}", r.id),
                op: "restore".into(),
                detail: r.original_path.clone(),
            };
            std::fs::write(
                f.q.root.join("journal.jsonl"),
                format!("{}\n", serde_json::to_string(&rec).unwrap()),
            )
            .unwrap();
        };
        // Nothing restored yet: the copy is untouched and the operation is simply abandoned.
        begin(&f);
        let out = f.q.recover().unwrap();
        assert_eq!(out.resolved.len(), 1, "{out:?}");
        assert!(f.q.payload(&r.id).exists() && !file.exists());
        // The file was copied back and checked, but the record was not updated.
        std::fs::write(&file, b"bytes").unwrap();
        begin(&f);
        let out = f.q.recover().unwrap();
        assert_eq!(out.resolved.len(), 1, "{out:?}");
        assert_eq!(
            f.q.list().unwrap().records[0].state,
            QuarantineState::Restored
        );
        assert!(!f.q.payload(&r.id).exists());
        assert_eq!(std::fs::read(&file).unwrap(), b"bytes");
    }

    #[test]
    fn recovery_does_not_guess_when_a_different_file_is_at_the_restore_target() {
        let f = fixture();
        let file = put(&f.work, "mod.dll", b"bytes");
        let r = f.q.quarantine_file(&file, "x").unwrap();
        std::fs::write(&file, b"something else").unwrap();
        let rec = crate::journal::Record::Begin {
            id: format!("restore-{}", r.id),
            op: "restore".into(),
            detail: r.original_path.clone(),
        };
        std::fs::write(
            f.q.root.join("journal.jsonl"),
            format!("{}\n", serde_json::to_string(&rec).unwrap()),
        )
        .unwrap();
        let out = f.q.recover().unwrap();
        assert!(out.resolved.is_empty());
        assert_eq!(out.needs_attention.len(), 1);
        assert_eq!(
            std::fs::read(&file).unwrap(),
            b"something else",
            "left exactly as it was"
        );
        assert!(f.q.payload(&r.id).exists());
    }

    #[test]
    fn an_operation_this_version_does_not_know_is_reported_not_acted_on() {
        let f = fixture();
        f.q.journal
            .begin("mystery", "purge", "C:\\somewhere")
            .unwrap();
        f.q.journal.begin("../../etc", "quarantine", "x").unwrap();
        let out = f.q.recover().unwrap();
        assert_eq!(out.needs_attention.len(), 2, "{out:?}");
        assert!(out.resolved.is_empty());
    }

    #[test]
    fn there_is_no_way_to_delete_a_quarantined_file() {
        // A guard on the module's source. The only deletions are: its own unfinished item folder; the original after the copy was
        // verified and the original re-checked (the "move"); a restored file that failed its check (just created by this call);
        // and the quarantined copy after a verified restore. A new one must be added here on purpose, with its reason.
        let src = include_str!("quarantine.rs");
        let body = src.split("#[cfg(test)]").next().unwrap();
        let removals: Vec<&str> = body
            .lines()
            .filter(|l| l.contains("remove_file") || l.contains("remove_dir_all"))
            .map(str::trim)
            .collect();
        assert_eq!(
            removals,
            [
                "let _ = std::fs::remove_dir_all(self.item_dir(id));",
                "self.quarantine_with(source, reason, |_| {}, |_| {}, |p| std::fs::remove_file(p))",
                "let _ = std::fs::remove_file(dest);",
                "std::fs::remove_file(&payload).map_err(io(&payload))",
            ]
        );
        assert!(
            !body.contains("pub fn purge")
                && !body.contains("pub fn delete")
                && !body.contains("pub fn discard")
        );
    }
}
