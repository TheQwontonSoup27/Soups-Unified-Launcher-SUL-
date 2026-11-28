//! The threat log (Ch. 12 §2.8, Ch. 14 §14.13.5): an append-only file of [`ThreatRecord`]s, hash-chained so that an edit,
//! a deleted line or a reordering in the middle of it is noticed.
//!
//! A line is `{"seq":N,"prev":"<64 hex>","record":{...}}`, and `prev` is the SHA-256 of the whole previous line (without its
//! newline); the first line's `prev` is 64 zeros. [`verify`] re-computes the chain; [`JsonlSink::open`] runs it before
//! appending, and refuses a broken log rather than appending after the break and hiding it.
//!
//! **What this is and is not.** It is tamper-*evident* against edits, deletions and reordering inside the log. It does not stop
//! someone who can write the file from cutting off its end or replacing it whole with a freshly chained copy, because nothing
//! outside the file remembers the last line. That needs an anchor elsewhere (the hive-mind's copy, or a signed checkpoint);
//! none is built. It is a local audit file, not a secret and not a proof.
//!
//! [`ThreatSink`] is where the hive-mind plugs in later (`sul-hive-mind` does not exist yet): the watchdog hands every
//! record to a sink and does not care where it goes.

use crate::incident::ThreatRecord;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use sul_core::fsops::{self, ProtectedRoots};
use sul_core::{Error, Result, error::io};

/// The `prev` of the first line.
pub const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Somewhere threat records go.
pub trait ThreatSink {
    fn record(&mut self, record: &ThreatRecord) -> Result<()>;
}

/// Keeps records in memory (tests, and a sink for a caller that wants to forward them itself).
#[derive(Debug, Default)]
pub struct MemorySink {
    pub records: Vec<ThreatRecord>,
}

impl ThreatSink for MemorySink {
    fn record(&mut self, record: &ThreatRecord) -> Result<()> {
        self.records.push(record.clone());
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct Line {
    seq: u64,
    prev: String,
    record: ThreatRecord,
}

fn line_digest(line: &str) -> String {
    hex::encode(Sha256::digest(line.as_bytes()))
}

/// What a full read of a log found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainReport {
    pub lines: u64,
    /// The `prev` the next line must carry (the digest of the last line, or [`GENESIS`] for an empty log).
    pub tip: String,
}

fn broken(line: u64, why: &str) -> Error {
    Error::Invalid(format!("threat log chain broken at line {line}: {why}"))
}

/// Read a log from the start and check every line: it parses, `seq` counts up from 1, and `prev` is the digest of the line
/// before. A missing file is an empty log. The first problem is the error.
pub fn verify(path: &Path) -> Result<ChainReport> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ChainReport {
                lines: 0,
                tip: GENESIS.to_owned(),
            });
        }
        Err(e) => return Err(Error::io(path, e)),
    };
    let mut tip = GENESIS.to_owned();
    let mut count = 0u64;
    for text in BufReader::new(file).lines() {
        let text = text.map_err(io(path))?;
        count += 1;
        let line: Line =
            serde_json::from_str(&text).map_err(|_| broken(count, "not a log line"))?;
        if line.seq != count {
            return Err(broken(count, "the line number is wrong"));
        }
        if line.prev != tip {
            return Err(broken(count, "it does not follow the line before it"));
        }
        tip = line_digest(&text);
    }
    Ok(ChainReport { lines: count, tip })
}

/// Appends records to a chained JSON-lines file.
pub struct JsonlSink {
    path: PathBuf,
    file: File,
    next_seq: u64,
    tip: String,
}

impl JsonlSink {
    /// Open (creating it if needed) and check the existing chain. A broken chain is an error: the caller decides what to do
    /// (set the file aside and start a new one, and report that as an incident), the sink never appends after a break.
    pub fn open(protected: &ProtectedRoots, path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fsops::create_dir(protected, parent)?;
        }
        protected.check_writable(path)?;
        let report = verify(path)?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(io(path))?;
        Ok(Self {
            path: path.to_path_buf(),
            file,
            next_seq: report.lines + 1,
            tip: report.tip,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// How many lines the log holds.
    pub fn len(&self) -> u64 {
        self.next_seq - 1
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl ThreatSink for JsonlSink {
    fn record(&mut self, record: &ThreatRecord) -> Result<()> {
        let line = Line {
            seq: self.next_seq,
            prev: self.tip.clone(),
            record: record.clone(),
        };
        let text = serde_json::to_string(&line)?;
        // One write of the whole line and its newline, so a crash leaves a whole line or an incomplete last line that
        // `verify` reports, never two lines run together.
        let mut bytes = text.clone().into_bytes();
        bytes.push(b'\n');
        self.file.write_all(&bytes).map_err(io(&self.path))?;
        self.file.sync_data().map_err(io(&self.path))?;
        self.tip = line_digest(&text);
        self.next_seq += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::incident::Reason;
    use std::fs;

    fn rec(n: u64) -> ThreatRecord {
        ThreatRecord {
            event: "file_deviation".into(),
            code: Some("E_WATCHDOG_TAMPER".into()),
            specific_code: None,
            confirmed: true,
            reason: Reason::HashMismatch,
            sha256: Some(format!("{n:064x}")),
            observed_unix: n,
            why: None,
        }
    }

    fn open(dir: &tempfile::TempDir) -> JsonlSink {
        JsonlSink::open(
            &ProtectedRoots::new(),
            &dir.path().join("logs/threats.jsonl"),
        )
        .unwrap()
    }

    #[test]
    fn a_new_log_is_empty_and_records_are_chained_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let mut sink = open(&dir);
        assert!(sink.is_empty());
        for n in 1..=3 {
            sink.record(&rec(n)).unwrap();
        }
        assert_eq!(sink.len(), 3);
        let report = verify(sink.path()).unwrap();
        assert_eq!(report.lines, 3);
        let text = fs::read_to_string(sink.path()).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains(&format!("\"prev\":\"{GENESIS}\"")));
        // The second line names the digest of the first, computed here independently of the writer's helper.
        let first_digest = hex::encode(Sha256::digest(lines[0].as_bytes()));
        assert!(
            lines[1].contains(&format!("\"prev\":\"{first_digest}\"")),
            "{}",
            lines[1]
        );
        assert_eq!(report.tip, hex::encode(Sha256::digest(lines[2].as_bytes())));
    }

    #[test]
    fn reopening_continues_the_chain() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut sink = open(&dir);
            sink.record(&rec(1)).unwrap();
            sink.record(&rec(2)).unwrap();
        }
        let mut sink = open(&dir);
        assert_eq!(sink.len(), 2);
        sink.record(&rec(3)).unwrap();
        assert_eq!(verify(sink.path()).unwrap().lines, 3);
    }

    fn lines_of(path: &Path) -> Vec<String> {
        fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn rewrite(path: &Path, lines: &[String]) {
        fs::write(path, lines.join("\n") + "\n").unwrap();
    }

    #[test]
    fn an_edited_line_a_deleted_line_and_a_reordering_are_each_noticed() {
        let dir = tempfile::tempdir().unwrap();
        let path = {
            let mut sink = open(&dir);
            for n in 1..=4 {
                sink.record(&rec(n)).unwrap();
            }
            sink.path().to_path_buf()
        };
        let original = lines_of(&path);

        // An edit in the middle (the digest of a threat changed): the next line's `prev` no longer matches.
        let mut edited = original.clone();
        edited[1] = edited[1].replace(&format!("{:064x}", 2), &format!("{:064x}", 99));
        rewrite(&path, &edited);
        assert!(verify(&path).unwrap_err().to_string().contains("line 3"));

        // A line removed from the middle.
        let mut deleted = original.clone();
        deleted.remove(1);
        rewrite(&path, &deleted);
        assert!(verify(&path).unwrap_err().to_string().contains("line 2"));

        // Two lines swapped.
        let mut swapped = original.clone();
        swapped.swap(1, 2);
        rewrite(&path, &swapped);
        assert!(verify(&path).is_err());

        // The first line removed: the second no longer starts from the genesis value.
        rewrite(&path, &original[1..]);
        assert!(verify(&path).is_err());

        // A line that is not a log line, and one with a repeated number.
        let mut junk = original.clone();
        junk.push("not json".into());
        rewrite(&path, &junk);
        assert!(verify(&path).unwrap_err().to_string().contains("line 5"));
        let mut repeated = original.clone();
        repeated.push(original[3].clone());
        rewrite(&path, &repeated);
        assert!(verify(&path).is_err());

        // The untouched log is fine, so none of the above passed by accident.
        rewrite(&path, &original);
        assert_eq!(verify(&path).unwrap().lines, 4);
    }

    #[test]
    fn a_changed_line_number_on_the_last_line_is_noticed_though_no_later_line_vouches_for_it() {
        // Found by a deliberate-bug check, 2026-11-26. The `prev` chain catches a change to any line that another line
        // follows, because the next line quotes its digest. Nothing follows the last line, so only its number can tell.
        let dir = tempfile::tempdir().unwrap();
        let path = {
            let mut sink = open(&dir);
            for n in 1..=3 {
                sink.record(&rec(n)).unwrap();
            }
            sink.path().to_path_buf()
        };
        let mut lines = lines_of(&path);
        let changed = lines[2].replace("\"seq\":3", "\"seq\":4");
        assert_ne!(changed, lines[2], "the test must really change the line");
        lines[2] = changed;
        rewrite(&path, &lines);
        let why = verify(&path).unwrap_err().to_string();
        assert!(
            why.contains("line 3") && why.contains("line number"),
            "{why}"
        );
    }

    #[test]
    fn a_broken_log_is_not_appended_to() {
        let dir = tempfile::tempdir().unwrap();
        let path = {
            let mut sink = open(&dir);
            sink.record(&rec(1)).unwrap();
            sink.record(&rec(2)).unwrap();
            sink.path().to_path_buf()
        };
        let mut lines = lines_of(&path);
        lines[0] = lines[0].replace("E_WATCHDOG_TAMPER", "E_NOTHING_TO_SEE");
        rewrite(&path, &lines);
        let before = fs::read(&path).unwrap();
        assert!(JsonlSink::open(&ProtectedRoots::new(), &path).is_err());
        assert_eq!(
            fs::read(&path).unwrap(),
            before,
            "the file was left as it was"
        );
    }

    #[test]
    fn a_half_written_last_line_is_reported_not_hidden() {
        let dir = tempfile::tempdir().unwrap();
        let path = {
            let mut sink = open(&dir);
            sink.record(&rec(1)).unwrap();
            sink.path().to_path_buf()
        };
        let mut text = fs::read_to_string(&path).unwrap();
        text.push_str("{\"seq\":2,\"prev\":\"ab");
        fs::write(&path, text).unwrap();
        assert!(verify(&path).unwrap_err().to_string().contains("line 2"));
    }

    #[test]
    fn a_missing_file_is_an_empty_log_and_a_protected_folder_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let report = verify(&dir.path().join("nothing.jsonl")).unwrap();
        assert_eq!((report.lines, report.tip.as_str()), (0, GENESIS));
        let mut protected = ProtectedRoots::new();
        protected.add(dir.path()).unwrap();
        let err = JsonlSink::open(&protected, &dir.path().join("t.jsonl"))
            .err()
            .unwrap();
        assert!(matches!(err, Error::Protected(_)), "{err}");
    }

    #[test]
    fn the_memory_sink_keeps_what_it_is_given() {
        let mut sink = MemorySink::default();
        sink.record(&rec(1)).unwrap();
        sink.record(&rec(2)).unwrap();
        assert_eq!(sink.records, vec![rec(1), rec(2)]);
    }
}
