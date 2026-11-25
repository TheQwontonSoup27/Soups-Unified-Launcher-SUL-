//! Quarantine: set a file aside where nothing will use it, verified and reversible.
//!
//! What it does: seal a copy of the file into the quarantine folder, open the copy and check what comes out hash for hash,
//! record where it came from and why, and only then remove the original. Every step is written to a recovery journal first. A
//! quarantined file can be put back byte for byte, and is never overwritten on the way.
//!
//! What it will not do:
//! * touch anything inside a protected root (the original game installation): nothing is moved out of one or restored into one;
//! * follow or move links, folders or anything but a regular file;
//! * delete anything that is not its own unfinished copy. There is no purge: a permanent delete needs its own design
//!   (preview, confirmation, log). The only file this module removes after a restore is the quarantined copy (and its key, through
//!   the key source), and only after the restored file has been checked to be identical to it.
//!
//! The copy is sealed with a key of its own (D-155; the format is in [`crate::seal`], the keys in [`crate::keys`]). A copy
//! made before D-155 was stored as it was; its record says so (version 1) and it is restored as it always was.
//!
//! Limits: a restored file has new timestamps and default permissions (the copy does not carry them), and a file that another
//! program has open and locked cannot be removed, in which case the quarantine is undone and an error is returned.

use crate::keys::{ItemRef, KeyFiles, KeySource};
use crate::seal::{self, ItemKey};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use sul_core::fsops::{ProtectedRoots, atomic_write};
use sul_core::journal::Journal;
use sul_core::{Error, Result, error::io};
use sul_trust::manifest::hash_file;

/// Records written now. Version 1 (a copy stored as it was, no `cipher`) is still read.
const SCHEMA_VERSION: u32 = 2;
const PLAIN_SCHEMA_VERSION: u32 = 1;
/// What a version 2 record says its copy is sealed with: XChaCha20-Poly1305, STREAM-BE32, the format of [`crate::seal`].
pub const CIPHER: &str = "xchacha20poly1305-stream-be32";
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
    /// How the copy is stored: [`CIPHER`] for a sealed copy (every record at version 2), `None` for a copy stored as it was
    /// (a version 1 record, made before D-155).
    #[serde(default)]
    pub cipher: Option<String>,
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
    keys: Box<dyn KeySource>,
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

/// Create the new file `to` (never over an existing one), let `fill` write it, and flush it to disk. If anything fails after the
/// file was created, the half-written file this call made is taken away again, so a failed copy never leaves a partial file
/// behind. A failure to create it (because something is already there) removes nothing: that file is not this call's.
fn create_new_with(to: &Path, fill: impl FnOnce(&mut std::fs::File) -> Result<()>) -> Result<()> {
    let mut dst = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)
        .map_err(io(to))?;
    let done = fill(&mut dst).and_then(|()| {
        dst.flush().map_err(io(to))?;
        dst.sync_all().map_err(io(to))
    });
    if done.is_err() {
        drop(dst);
        let _ = std::fs::remove_file(to);
    }
    done
}

/// Copy `from` to a new file `to` as it is (a copy made before D-155), flushed to disk.
fn copy_new(from: &Path, to: &Path) -> Result<()> {
    let mut src = std::fs::File::open(from).map_err(io(from))?;
    create_new_with(to, |dst| {
        std::io::copy(&mut src, dst).map_err(io(to))?;
        Ok(())
    })
}

/// Seal `from` into the new file `to` under `key`, flushed to disk.
fn seal_new(from: &Path, to: &Path, key: &ItemKey) -> Result<()> {
    let src = std::fs::File::open(from).map_err(io(from))?;
    create_new_with(to, |dst| seal::seal(key, src, dst).map_err(|e| e.at(to)))
}

/// Open the sealed copy `from` into the new file `to`, flushed to disk.
fn open_new(from: &Path, to: &Path, key: &ItemKey) -> Result<()> {
    let src = std::fs::File::open(from).map_err(io(from))?;
    create_new_with(to, |dst| {
        seal::open(key, src, |plain| dst.write_all(plain)).map_err(|e| match e {
            // A failure to write the restored file is about `to`; every other failure is about the sealed copy.
            seal::SealError::Sink(source) => Error::Io {
                path: to.to_path_buf(),
                source,
            },
            other => other.at(from),
        })
    })
}

/// The SHA-256 and size of what the sealed copy at `path` holds, found by opening it. Nothing is written.
fn open_digest(path: &Path, key: &ItemKey) -> Result<(String, u64)> {
    let src = std::fs::File::open(path).map_err(io(path))?;
    seal::open_digest(key, src).map_err(|e| e.at(path))
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
    /// Open (creating if needed) a quarantine folder, keeping each copy's key in a file beside its record ([`KeyFiles`]). The
    /// folder itself must not be inside a protected root.
    pub fn open(root: &Path, protected: ProtectedRoots) -> Result<Self> {
        Self::open_with_keys(root, protected, KeyFiles)
    }

    /// Like [`open`](Self::open), with the keys held by `keys` instead of beside each record. This is the seam the vault uses
    /// to take custody of them (R0 layer 4); the sealed format does not change.
    pub fn open_with_keys(
        root: &Path,
        protected: ProtectedRoots,
        keys: impl KeySource + 'static,
    ) -> Result<Self> {
        protected.check_writable(root)?;
        std::fs::create_dir_all(root.join("items")).map_err(io(root))?;
        let root = root.canonicalize().map_err(io(root))?;
        let journal = Journal::open(&root.join("journal.jsonl"))?;
        Ok(Self {
            root,
            protected,
            journal,
            keys: Box::new(keys),
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
        // Version 1 is a copy stored as it was and says no cipher; version 2 is sealed and names exactly the cipher this
        // version knows. Any other pairing is a record somebody changed or a version from the future: it is not guessed at.
        let consistent = matches!(
            (record.schema_version, record.cipher.as_deref()),
            (PLAIN_SCHEMA_VERSION, None) | (SCHEMA_VERSION, Some(CIPHER))
        );
        if record.id != id || !consistent {
            return Err(Error::Invalid(format!(
                "the record for {id} is not one this version understands"
            )));
        }
        Ok(record)
    }

    /// The key to a record's copy: `None` for a version 1 copy (stored as it was, nothing to open), the key source's answer
    /// for a sealed one. A key that cannot be had is an error that says so; it is never mistaken for a copy that does not match.
    fn key_for(&self, record: &QuarantineRecord) -> Result<Option<ItemKey>> {
        if record.cipher.is_none() {
            return Ok(None);
        }
        let dir = self.item_dir(&record.id);
        self.keys
            .get(&ItemRef {
                id: &record.id,
                dir: &dir,
            })
            .map(Some)
    }

    /// The SHA-256 and size of what the copy of `record` holds: opened with `key` if it is sealed, read as it is if not.
    fn payload_digest(
        &self,
        record: &QuarantineRecord,
        key: Option<&ItemKey>,
    ) -> Result<(String, u64)> {
        let payload = self.payload(&record.id);
        match key {
            Some(key) => open_digest(&payload, key),
            None => hash_file(&payload),
        }
    }

    /// Whether the copy of `record` still holds exactly what the record says. A key that cannot be had is an error; a copy
    /// that is missing, cut short, changed or sealed under another key simply does not match (`Ok(false)`).
    fn payload_matches(&self, record: &QuarantineRecord) -> Result<bool> {
        let key = self.key_for(record)?;
        Ok(self
            .payload_digest(record, key.as_ref())
            .is_ok_and(|h| h == (record.sha256.clone(), record.size)))
    }

    /// Remove this module's own unfinished item folder, and let the key source let go of the item's key. It only ever names a
    /// folder built from a checked id under `items/`.
    fn discard_unfinished(&self, id: &str) {
        if valid_id(id) {
            let dir = self.item_dir(id);
            let _ = self.keys.forget(&ItemRef { id, dir: &dir });
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
        if sul_core::safepath::resolve_lenient(source).starts_with(&self.root) {
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
        // The key comes first: nothing is sealed until there is a key to seal it with, and a key that cannot be made (no random
        // source, a locked vault) stops the quarantine before the original is touched.
        let key = match self.keys.create(&ItemRef { id: &id, dir: &dir }) {
            Ok(k) => k,
            Err(e) => return Err(finish_with_error(e, "no key could be made")),
        };
        before_copy(source);
        if let Err(e) = seal_new(source, &payload, &key) {
            return Err(finish_with_error(e, "copy failed"));
        }
        // "Verify" is: open the sealed copy and hash what comes out. A copy that does not open, or opens to something else, is
        // never trusted.
        match open_digest(&payload, &key) {
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
            cipher: Some(CIPHER.to_owned()),
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
        // A key that cannot be had is its own error and says so. Only then is the copy checked: opened and hashed, or, for a
        // version 1 copy, hashed as it is.
        let key = self.key_for(&record)?;
        match self.payload_digest(&record, key.as_ref()) {
            Ok(h) if h == (record.sha256.clone(), record.size) => {}
            Ok(_) | Err(Error::Invalid(_)) => {
                return Err(Error::Invalid(
                    "the quarantined copy no longer matches its record, so it was not restored"
                        .into(),
                ));
            }
            Err(e) => return Err(e),
        }

        let op = format!("restore-{id}");
        self.journal
            .begin(&op, "restore", &dest.to_string_lossy())?;
        let copied = match &key {
            Some(key) => open_new(&payload, dest, key),
            None => copy_new(&payload, dest),
        };
        if let Err(e) = copied {
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
        std::fs::remove_file(&payload).map_err(io(&payload))?;
        // The copy is gone, so its key opens nothing. If the key source cannot let go of it right now the restore has still
        // succeeded and no data is at risk, so that is not reported as a failure of the restore.
        let dir = self.item_dir(&record.id);
        let _ = self.keys.forget(&ItemRef {
            id: &record.id,
            dir: &dir,
        });
        Ok(())
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
        // Intact means: the record reads, its key can be had, and the copy opens to exactly what the record says. Anything less
        // (including a key that cannot be had) is left alone and reported, because deciding would risk the only copy.
        match self.read_record(id) {
            Ok(r) if self.payload_matches(&r).unwrap_or(false) => {
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
            && self.payload_matches(&record).unwrap_or(false)
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
    use crate::seal::KEY_BYTES;
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};
    use std::sync::{Arc, Mutex};

    struct Fixture {
        _tmp: tempfile::TempDir,
        install: PathBuf,
        work: PathBuf,
        q: Quarantine,
    }

    /// A quarantine folder (keys beside each record, the default), a protected "installation" folder, and a work folder where
    /// files may be moved.
    fn fixture() -> Fixture {
        fixture_by(|root, protected| Quarantine::open(root, protected).unwrap())
    }

    /// The same, with the keys held by `keys`: stands in for the vault taking custody (R0 layer 4).
    fn fixture_with_keys(keys: impl KeySource + 'static) -> Fixture {
        fixture_by(|root, protected| Quarantine::open_with_keys(root, protected, keys).unwrap())
    }

    fn fixture_by(open: impl FnOnce(&Path, ProtectedRoots) -> Quarantine) -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let install = tmp.path().join("install");
        let work = tmp.path().join("work");
        std::fs::create_dir_all(&install).unwrap();
        std::fs::create_dir_all(&work).unwrap();
        let mut protected = ProtectedRoots::new();
        protected.add(&install).unwrap();
        let q = open(&tmp.path().join("quarantine"), protected);
        Fixture {
            install: install.canonicalize().unwrap(),
            work: work.canonicalize().unwrap(),
            q,
            _tmp: tmp,
        }
    }

    /// What the test key source keeps and was asked to do. Shared, so a test can look at it while the quarantine owns the source.
    #[derive(Default)]
    struct MemoryState {
        keys: Mutex<HashMap<String, [u8; KEY_BYTES]>>,
        forgotten: AtomicUsize,
        /// While set, `create` and `get` fail the way a locked vault would.
        locked: AtomicBool,
    }

    /// A key source that keeps keys in memory and writes nothing to disk.
    #[derive(Clone, Default)]
    struct MemoryKeys(Arc<MemoryState>);

    impl KeySource for MemoryKeys {
        fn create(&self, item: &ItemRef<'_>) -> Result<ItemKey> {
            if self.0.locked.load(SeqCst) {
                return Err(Error::Invalid("the vault is locked".into()));
            }
            let key = ItemKey::generate().unwrap();
            let mut keys = self.0.keys.lock().unwrap();
            if keys.contains_key(item.id) {
                return Err(Error::Invalid("a key already exists for this item".into()));
            }
            keys.insert(item.id.to_owned(), *key.as_bytes());
            Ok(key)
        }
        fn get(&self, item: &ItemRef<'_>) -> Result<ItemKey> {
            if self.0.locked.load(SeqCst) {
                return Err(Error::Invalid("the vault is locked".into()));
            }
            let keys = self.0.keys.lock().unwrap();
            keys.get(item.id)
                .and_then(|bytes| ItemKey::from_slice(bytes))
                .ok_or_else(|| Error::Invalid("the vault has no key for this item".into()))
        }
        fn forget(&self, item: &ItemRef<'_>) -> Result<()> {
            self.0.forgotten.fetch_add(1, SeqCst);
            self.0.keys.lock().unwrap().remove(item.id);
            Ok(())
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
        // The stored copy is a sealed one, not the file as it was: the header, the 13 bytes, one 16-byte tag.
        let sealed = std::fs::read(f.q.payload(&r.id)).unwrap();
        assert_eq!(&sealed[..4], b"SULQ");
        assert_eq!(sealed.len(), 32 + 13 + 16);
        assert!(
            !sealed.windows(8).any(|w| w == b"MZ some "),
            "a run of the original shows in the stored copy"
        );
        assert_eq!(r.schema_version, 2);
        assert_eq!(r.cipher.as_deref(), Some(CIPHER));
        assert_eq!(
            std::fs::read(f.q.item_dir(&r.id).join("key.bin"))
                .unwrap()
                .len(),
            KEY_BYTES,
            "the default key source keeps the key beside the record"
        );
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
        assert!(
            !f.q.item_dir(&r.id).join("key.bin").exists(),
            "the key goes with the copy it opened"
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

    // ---- sealing and keys ----

    fn key_path(f: &Fixture, id: &str) -> PathBuf {
        f.q.item_dir(id).join("key.bin")
    }

    #[test]
    fn every_copy_has_its_own_key_and_the_same_file_never_seals_the_same_way() {
        let f = fixture();
        let a =
            f.q.quarantine_file(&put(&f.work, "a.dll", b"same bytes"), "x")
                .unwrap();
        let b =
            f.q.quarantine_file(&put(&f.work, "b.dll", b"same bytes"), "x")
                .unwrap();
        assert_ne!(
            std::fs::read(key_path(&f, &a.id)).unwrap(),
            std::fs::read(key_path(&f, &b.id)).unwrap(),
            "two items, two keys"
        );
        assert_ne!(
            std::fs::read(f.q.payload(&a.id)).unwrap(),
            std::fs::read(f.q.payload(&b.id)).unwrap(),
            "the same bytes seal differently every time"
        );
    }

    #[test]
    fn a_missing_or_damaged_key_is_reported_as_that_and_nothing_is_restored() {
        let f = fixture();
        let file = put(&f.work, "mod.dll", b"good bytes");
        let r = f.q.quarantine_file(&file, "x").unwrap();
        let key = key_path(&f, &r.id);
        let saved = std::fs::read(&key).unwrap();

        std::fs::remove_file(&key).unwrap();
        assert!(matches!(f.q.restore(&r.id), Err(Error::Invalid(m)) if m.contains("is missing")));
        std::fs::write(&key, &saved[..16]).unwrap();
        assert!(matches!(f.q.restore(&r.id), Err(Error::Invalid(m)) if m.contains("is damaged")));
        assert!(!file.exists(), "nothing was written");
        assert!(f.q.payload(&r.id).exists(), "the copy is still held");
        assert!(f.q.journal.pending().unwrap().is_empty());

        // The refusals lost nothing: with the key back, the copy opens.
        std::fs::write(&key, &saved).unwrap();
        f.q.restore(&r.id).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"good bytes");
    }

    #[test]
    fn another_items_key_does_not_open_a_copy() {
        let f = fixture();
        let a_file = put(&f.work, "a.dll", b"first");
        let a = f.q.quarantine_file(&a_file, "x").unwrap();
        let b =
            f.q.quarantine_file(&put(&f.work, "b.dll", b"second"), "x")
                .unwrap();
        std::fs::copy(key_path(&f, &b.id), key_path(&f, &a.id)).unwrap();
        assert!(
            matches!(f.q.restore(&a.id), Err(Error::Invalid(m)) if m.contains("no longer matches"))
        );
        assert!(!a_file.exists(), "nothing was written");
    }

    #[test]
    fn a_sealed_copy_that_was_changed_cut_short_or_extended_is_never_restored() {
        let f = fixture();
        let data: Vec<u8> = (0..150_000u32).map(|i| (i % 251) as u8).collect();
        let file = put(&f.work, "big.bin", &data);
        let r = f.q.quarantine_file(&file, "x").unwrap();
        let payload = f.q.payload(&r.id);
        let good = std::fs::read(&payload).unwrap();
        // 150,000 bytes are three chunks (64 KiB, 64 KiB, the rest), each with a 16-byte tag, after the 32-byte header.
        assert_eq!(good.len(), 32 + 150_000 + 3 * 16);
        let chunk = 65_536 + 16;
        let mut cases: Vec<(String, Vec<u8>)> = Vec::new();
        for at in [2, 40, 32 + chunk + 5, good.len() - 1] {
            let mut bad = good.clone();
            bad[at] ^= 1;
            cases.push((format!("a changed byte at {at}"), bad));
        }
        // Cuts at exact chunk boundaries are the ones a careless format would accept.
        for cut in [32 + chunk, 32 + 2 * chunk, good.len() - 1] {
            cases.push((format!("cut to {cut} bytes"), good[..cut].to_vec()));
        }
        let mut longer = good.clone();
        longer.push(0);
        cases.push(("one byte added".into(), longer));
        for (what, bytes) in cases {
            std::fs::write(&payload, &bytes).unwrap();
            assert!(
                matches!(f.q.restore(&r.id), Err(Error::Invalid(m)) if m.contains("no longer matches")),
                "{what}"
            );
            assert!(!file.exists(), "{what}: nothing was written");
        }
        assert!(f.q.journal.pending().unwrap().is_empty());
        std::fs::write(&payload, &good).unwrap();
        f.q.restore(&r.id).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), data);
    }

    #[test]
    fn a_copy_that_opens_but_to_something_else_is_not_trusted() {
        // This is the hash check on its own, not the cipher: the replacement is a perfectly good sealed copy under the right key.
        let f = fixture();
        let file = put(&f.work, "mod.dll", b"what the record says");
        let r = f.q.quarantine_file(&file, "x").unwrap();
        let dir = f.q.item_dir(&r.id);
        let key =
            f.q.keys
                .get(&ItemRef {
                    id: &r.id,
                    dir: &dir,
                })
                .unwrap();
        let mut other = Vec::new();
        seal::seal(&key, &b"something else entirely"[..], &mut other).unwrap();
        std::fs::write(f.q.payload(&r.id), other).unwrap();
        assert!(
            matches!(f.q.restore(&r.id), Err(Error::Invalid(m)) if m.contains("no longer matches"))
        );
        assert!(!file.exists());
    }

    /// What a copy made before D-155 looks like on disk: the file as it was, and a version 1 record that has no `cipher` field.
    fn legacy_item(f: &Fixture, id: &str, original: &Path, bytes: &[u8]) {
        std::fs::create_dir(f.q.item_dir(id)).unwrap();
        std::fs::write(f.q.payload(id), bytes).unwrap();
        let (sha256, size) = hash_file(&f.q.payload(id)).unwrap();
        let record = QuarantineRecord {
            schema_version: 1,
            id: id.into(),
            original_path: original.to_str().unwrap().into(),
            size,
            sha256,
            reason: "made before D-155".into(),
            quarantined_unix: 1_700_000_000,
            state: QuarantineState::Held,
            restored_unix: None,
            cipher: None,
        };
        let mut json = serde_json::to_value(&record).unwrap();
        json.as_object_mut().unwrap().remove("cipher");
        std::fs::write(
            f.q.record_path(id),
            serde_json::to_string_pretty(&json).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn a_copy_made_before_sealing_is_still_listed_and_restored_as_it_was() {
        let f = fixture();
        let id = "q-1798675200000-0123456789ab-0001";
        let original = f.work.join("old.dll");
        legacy_item(&f, id, &original, b"MZ stored as it was");
        let listing = f.q.list().unwrap();
        assert!(listing.unreadable.is_empty(), "{listing:?}");
        assert_eq!(listing.records.len(), 1);
        assert_eq!(listing.records[0].schema_version, 1);
        assert_eq!(listing.records[0].cipher, None);
        assert_eq!(f.q.restore(id).unwrap(), original);
        assert_eq!(std::fs::read(&original).unwrap(), b"MZ stored as it was");
        assert!(!f.q.payload(id).exists());
        assert!(
            !key_path(&f, id).exists(),
            "no key was ever needed, so none was made"
        );
    }

    #[test]
    fn a_record_that_pairs_a_version_with_the_wrong_cipher_is_not_trusted() {
        let f = fixture();
        let r =
            f.q.quarantine_file(&put(&f.work, "a.dll", b"x"), "x")
                .unwrap();
        for (version, cipher) in [
            (1u32, Some(CIPHER)),
            (2, None),
            (2, Some("rot13")),
            (3, Some(CIPHER)),
            (0, None),
        ] {
            let mut bad = r.clone();
            bad.schema_version = version;
            bad.cipher = cipher.map(str::to_owned);
            f.q.write_record(&bad).unwrap();
            assert_eq!(
                f.q.list().unwrap().unreadable,
                std::slice::from_ref(&r.id),
                "version {version}, cipher {cipher:?}"
            );
            assert!(
                f.q.restore(&r.id).is_err(),
                "version {version}, cipher {cipher:?}"
            );
        }
    }

    #[test]
    fn a_key_source_other_than_the_default_takes_custody_of_the_keys() {
        let keys = MemoryKeys::default();
        let f = fixture_with_keys(keys.clone());
        let file = put(&f.work, "mod.dll", b"MZ vault bytes");
        let r = f.q.quarantine_file(&file, "x").unwrap();
        assert!(!key_path(&f, &r.id).exists(), "no key beside the record");
        assert_eq!(keys.0.keys.lock().unwrap().len(), 1);
        assert_eq!(f.q.restore(&r.id).unwrap(), file);
        assert_eq!(std::fs::read(&file).unwrap(), b"MZ vault bytes");
        assert!(
            keys.0.keys.lock().unwrap().is_empty(),
            "the key was let go with the copy"
        );
        assert_eq!(keys.0.forgotten.load(SeqCst), 1);
    }

    #[test]
    fn if_no_key_can_be_made_nothing_is_set_aside_and_the_original_is_untouched() {
        let keys = MemoryKeys::default();
        keys.0.locked.store(true, SeqCst);
        let f = fixture_with_keys(keys);
        let file = put(&f.work, "mod.dll", b"bytes");
        assert!(
            matches!(f.q.quarantine_file(&file, "x"), Err(Error::Invalid(m)) if m.contains("locked"))
        );
        assert_eq!(std::fs::read(&file).unwrap(), b"bytes");
        assert!(item_names(&f).is_empty(), "no folder was left behind");
        assert!(f.q.journal.pending().unwrap().is_empty());
    }

    #[test]
    fn a_locked_key_source_is_reported_as_locked_and_recovery_does_not_decide() {
        let keys = MemoryKeys::default();
        let f = fixture_with_keys(keys.clone());
        let (id, file) = crash_state(&f, true, CopyState::Intact);
        keys.0.locked.store(true, SeqCst);
        assert!(matches!(f.q.restore(&id), Err(Error::Invalid(m)) if m.contains("locked")));
        let out = f.q.recover().unwrap();
        assert!(out.resolved.is_empty(), "{out:?}");
        assert!(
            out.needs_attention.iter().any(|m| m.contains(&id)),
            "{out:?}"
        );
        assert_eq!(
            f.q.journal.pending().unwrap().len(),
            1,
            "still pending, reported again next time"
        );
        assert!(
            f.q.payload(&id).exists() && !file.exists(),
            "nothing was changed"
        );
        // Once the source can answer, the same copy is confirmed and the operation is completed.
        keys.0.locked.store(false, SeqCst);
        let out = f.q.recover().unwrap();
        assert_eq!(out.resolved.len(), 1, "{out:?}");
        assert!(f.q.journal.pending().unwrap().is_empty());
    }

    #[test]
    fn a_failed_copy_leaves_no_partial_file_and_never_touches_one_that_was_already_there() {
        let tmp = tempfile::tempdir().unwrap();
        let to = tmp.path().join("new.bin");
        let failed = create_new_with(&to, |f| {
            f.write_all(b"half").map_err(io(&to))?;
            Err(Error::Invalid("stopped".into()))
        });
        assert!(matches!(failed, Err(Error::Invalid(m)) if m == "stopped"));
        assert!(!to.exists(), "the half-written file this call made is gone");

        let existing = tmp.path().join("existing.bin");
        std::fs::write(&existing, b"not ours").unwrap();
        let ran = Cell::new(false);
        let refused = create_new_with(&existing, |_| {
            ran.set(true);
            Ok(())
        });
        assert!(refused.is_err());
        assert!(
            !ran.get(),
            "nothing was written to a file that was already there"
        );
        assert_eq!(std::fs::read(&existing).unwrap(), b"not ours");
    }

    // ---- recovery ----

    /// What the stored copy looks like after the program stopped partway.
    enum CopyState<'a> {
        /// The sealed copy and its key, exactly as the quarantine made them.
        Intact,
        /// Something else where the copy should be (a partial write, a copy of other bytes).
        Replaced(&'a [u8]),
        /// No copy at all.
        Missing,
    }

    fn crash_state(f: &Fixture, source_removed: bool, copy: CopyState<'_>) -> (String, PathBuf) {
        // Reproduces what is on disk when the program stops partway: build a real quarantine, then wind it back.
        let file = put(&f.work, "interrupted.dll", b"the real bytes");
        let r = f.q.quarantine_file(&file, "x").unwrap();
        // Journal: forget the commit by rewriting it as begin only.
        std::fs::write(
            f.q.root.join("journal.jsonl"),
            format!(
                "{}\n",
                serde_json::to_string(&sul_core::journal::Record::Begin {
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
        match copy {
            CopyState::Intact => {}
            CopyState::Replaced(bytes) => std::fs::write(f.q.payload(&r.id), bytes).unwrap(),
            CopyState::Missing => std::fs::remove_file(f.q.payload(&r.id)).unwrap(),
        }
        (r.id, file)
    }

    #[test]
    fn recovery_drops_an_unfinished_copy_when_the_original_is_still_there() {
        let f = fixture();
        let (id, file) = crash_state(&f, false, CopyState::Replaced(b"half")); // the original is back, the copy is partial
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
        let (id, file) = crash_state(&f, true, CopyState::Intact);
        let out = f.q.recover().unwrap();
        assert_eq!(out.resolved.len(), 1, "{out:?}");
        assert!(!file.exists());
        assert!(f.q.payload(&id).exists(), "the copy is kept");
        assert!(
            f.q.item_dir(&id).join("key.bin").exists(),
            "and so is the key that opens it"
        );
        assert!(f.q.journal.pending().unwrap().is_empty());
    }

    #[test]
    fn recovery_leaves_alone_and_reports_a_case_where_data_may_be_at_risk() {
        let f = fixture();
        let (id, file) = crash_state(&f, true, CopyState::Missing); // the original is gone and the copy is missing
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

        let (id2, _) = crash_state(&f, true, CopyState::Replaced(b"a different copy"));
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
            let rec = sul_core::journal::Record::Begin {
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
        let rec = sul_core::journal::Record::Begin {
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
        // A guard on the module's source. The only deletions are: the half-written file that `create_new_with` itself just made
        // (it opens with create_new, so it is never a file that was already there, and a failure to create removes nothing);
        // its own unfinished item folder; the original after the copy was verified and the original re-checked (the "move");
        // a restored file that failed its check (just created by this call); and the quarantined copy after a verified
        // restore. A new one must be added here on purpose, with its reason. The keys are removed by the key source, in
        // `keys.rs`, which has a guard of its own.
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
                "let _ = std::fs::remove_file(to);",
                "let _ = std::fs::remove_dir_all(self.item_dir(id));",
                "self.quarantine_with(source, reason, |_| {}, |_| {}, |p| std::fs::remove_file(p))",
                "let _ = std::fs::remove_file(dest);",
                "std::fs::remove_file(&payload).map_err(io(&payload))?;",
            ]
        );
        assert!(
            !body.contains("pub fn purge")
                && !body.contains("pub fn delete")
                && !body.contains("pub fn discard")
        );
    }
}
