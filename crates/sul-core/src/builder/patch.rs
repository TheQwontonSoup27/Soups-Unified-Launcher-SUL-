//! Reversible byte patches (spec 2.3: "keep the original files … record reversible patches"; spec 2.10 asks for
//! the same for controlled hex edits).
//!
//! A patch names the exact file it was made for by SHA-256 and lists hunks: at this offset, these old bytes
//! become those new bytes. The rules that make it reversible and safe:
//!
//! * **The original is never changed.** [`apply`] reads one file and writes a different, new file. It refuses to
//!   overwrite anything, refuses the same path for both, and refuses a destination in a protected root.
//! * **It must be the right file.** A file whose SHA-256 is not the one the patch names is refused, and so is a
//!   hunk whose old bytes are not exactly what is at its offset. Nothing is written when anything is wrong.
//! * **Same length only.** A hunk replaces bytes with the same number of bytes, so no later offset moves. A patch
//!   that would insert or remove bytes is not representable here.
//! * **It can be undone, and that is checked.** [`revert`] applies the swapped hunks to the patched file and only
//!   writes the result if its SHA-256 is exactly the original's.
//!
//! Limits (chosen here, not measured): files up to [`MAX_PATCH_FILE_BYTES`] are read whole into memory, at most
//! [`MAX_HUNKS`] hunks of at most [`MAX_HUNK_BYTES`] bytes each. A larger file is refused, not streamed.
//!
//! Tested on made-up files only. Whether a game accepts a patched file is not something this checks.

use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::io;
use crate::fsops::{self, ProtectedRoots};
use crate::mods::{validate_name, validate_text};
use crate::{Error, Result};

pub const MAX_PATCH_FILE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_HUNKS: usize = 10_000;
pub const MAX_HUNK_BYTES: usize = 1 << 20;

/// One change: the bytes at `offset` are `old_hex` and become `new_hex` (same length).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hunk {
    pub offset: u64,
    pub old_hex: String,
    pub new_hex: String,
}

/// A set of hunks for one exact file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BytePatch {
    pub id: String,
    pub description: String,
    /// SHA-256 (lower-case hex) of the only file this patch applies to.
    pub expect_sha256: String,
    /// In increasing offset order, not overlapping.
    pub hunks: Vec<Hunk>,
}

/// What an apply or a revert did, kept so the patch can be reversed and the change explained later.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchOutcome {
    pub patch_id: String,
    pub before_sha256: String,
    pub after_sha256: String,
    pub hunks: usize,
    pub bytes_changed: u64,
}

struct Decoded {
    offset: usize,
    old: Vec<u8>,
    new: Vec<u8>,
}

pub(crate) fn sha_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

impl BytePatch {
    /// Check the patch on its own, without any file.
    pub fn validate(&self) -> Result<()> {
        self.decode().map(|_| ())
    }

    fn decode(&self) -> Result<Vec<Decoded>> {
        validate_name("patch id", &self.id)?;
        validate_text("patch description", &self.description, 200)?;
        if !is_sha256(&self.expect_sha256) {
            return Err(Error::Invalid(
                "a patch must name its file by a 64-character lower-case hexadecimal SHA-256"
                    .into(),
            ));
        }
        if self.hunks.is_empty() || self.hunks.len() > MAX_HUNKS {
            return Err(Error::Invalid(format!(
                "a patch has 1 to {MAX_HUNKS} hunks; this one has {}",
                self.hunks.len()
            )));
        }
        let mut out = Vec::with_capacity(self.hunks.len());
        let mut end_of_previous = 0u64;
        for (i, h) in self.hunks.iter().enumerate() {
            let n = i + 1;
            let old = hex::decode(&h.old_hex).map_err(|_| {
                Error::Invalid(format!("hunk {n}: the old bytes are not hexadecimal"))
            })?;
            let new = hex::decode(&h.new_hex).map_err(|_| {
                Error::Invalid(format!("hunk {n}: the new bytes are not hexadecimal"))
            })?;
            if old.is_empty() || old.len() > MAX_HUNK_BYTES {
                return Err(Error::Invalid(format!(
                    "hunk {n}: it must change 1 to {MAX_HUNK_BYTES} bytes"
                )));
            }
            if old.len() != new.len() {
                return Err(Error::Invalid(format!(
                    "hunk {n}: the old and new bytes must be the same length"
                )));
            }
            if old == new {
                return Err(Error::Invalid(format!("hunk {n}: it changes nothing")));
            }
            if h.offset < end_of_previous {
                return Err(Error::Invalid(format!(
                    "hunk {n}: it overlaps the one before it or is out of order"
                )));
            }
            end_of_previous = h
                .offset
                .checked_add(old.len() as u64)
                .ok_or_else(|| Error::Invalid(format!("hunk {n}: the offset is too large")))?;
            let offset = usize::try_from(h.offset)
                .map_err(|_| Error::Invalid(format!("hunk {n}: the offset is too large")))?;
            out.push(Decoded { offset, old, new });
        }
        Ok(out)
    }

    /// The patched content of `bytes`, which must be exactly the file this patch was made for. Pure: no files.
    pub fn apply_bytes(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        let hunks = self.decode()?;
        let sha = sha_hex(bytes);
        if sha != self.expect_sha256 {
            return Err(Error::Invalid(format!(
                "this patch is for a file with SHA-256 {}, but this file's is {sha}",
                self.expect_sha256
            )));
        }
        let mut out = bytes.to_vec();
        for (i, h) in hunks.iter().enumerate() {
            let n = i + 1;
            let end = h
                .offset
                .checked_add(h.old.len())
                .ok_or_else(|| Error::Invalid(format!("hunk {n}: the offset is too large")))?;
            let here = bytes.get(h.offset..end).ok_or_else(|| {
                Error::Invalid(format!(
                    "hunk {n}: offset {} is beyond the end of the file ({} bytes)",
                    h.offset,
                    bytes.len()
                ))
            })?;
            if here != h.old.as_slice() {
                return Err(Error::Invalid(format!(
                    "hunk {n}: the bytes at offset {} are not the bytes the patch expects",
                    h.offset
                )));
            }
            out[h.offset..end].copy_from_slice(&h.new);
        }
        Ok(out)
    }

    /// What applying this patch to `before` gave as `after`.
    pub(crate) fn outcome(&self, before: &[u8], after: &[u8]) -> PatchOutcome {
        PatchOutcome {
            patch_id: self.id.clone(),
            before_sha256: sha_hex(before),
            after_sha256: sha_hex(after),
            hunks: self.hunks.len(),
            bytes_changed: before.iter().zip(after).filter(|(a, b)| a != b).count() as u64,
        }
    }

    /// The patch that undoes this one on the file it produced. `outcome` must be the result of applying this patch.
    pub fn reverse(&self, outcome: &PatchOutcome) -> Result<BytePatch> {
        if outcome.patch_id != self.id {
            return Err(Error::Invalid(
                "that outcome is not the result of this patch".into(),
            ));
        }
        let description: String = self.description.chars().take(190).collect();
        Ok(BytePatch {
            id: format!("{}-reverse", self.id),
            description: format!("Undo: {description}"),
            expect_sha256: outcome.after_sha256.clone(),
            hunks: self
                .hunks
                .iter()
                .map(|h| Hunk {
                    offset: h.offset,
                    old_hex: h.new_hex.clone(),
                    new_hex: h.old_hex.clone(),
                })
                .collect(),
        })
    }
}

/// Read a plain file (not a link, not a folder) up to `limit` bytes.
pub(crate) fn read_plain(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let meta = std::fs::symlink_metadata(path).map_err(io(path))?;
    if !meta.is_file() {
        return Err(Error::Invalid(format!(
            "{} is not a plain file (a link or a folder is not patched)",
            path.display()
        )));
    }
    if meta.len() > limit {
        return Err(Error::Invalid(format!(
            "{} is {} bytes; patches are limited to {limit} bytes (the file is not streamed)",
            path.display(),
            meta.len()
        )));
    }
    std::fs::read(path).map_err(io(path))
}

/// Refuse an overwrite before anything is written. The file being patched always exists, so naming it as the
/// destination is refused here too: a patch never changes a file in place.
fn check_destination(protected: &ProtectedRoots, dst: &Path) -> Result<()> {
    protected.check_writable(dst)?;
    if std::fs::symlink_metadata(dst).is_ok() {
        return Err(Error::DestinationExists(dst.to_path_buf()));
    }
    Ok(())
}

/// Apply `patch` to `src` and write the result to the new file `dst`. `src` is only read.
pub fn apply(
    protected: &ProtectedRoots,
    patch: &BytePatch,
    src: &Path,
    dst: &Path,
) -> Result<PatchOutcome> {
    apply_limited(protected, patch, src, dst, MAX_PATCH_FILE_BYTES)
}

fn apply_limited(
    protected: &ProtectedRoots,
    patch: &BytePatch,
    src: &Path,
    dst: &Path,
    limit: u64,
) -> Result<PatchOutcome> {
    check_destination(protected, dst)?;
    let before = read_plain(src, limit)?;
    let after = patch.apply_bytes(&before)?;
    let outcome = patch.outcome(&before, &after);
    fsops::atomic_write(protected, dst, &after)?;
    Ok(outcome)
}

/// Undo a patch: write the original content of `patched` to the new file `dst`, but only if reversing the hunks
/// gives a file whose SHA-256 is exactly the one recorded in `outcome.before_sha256`.
pub fn revert(
    protected: &ProtectedRoots,
    patch: &BytePatch,
    outcome: &PatchOutcome,
    patched: &Path,
    dst: &Path,
) -> Result<PatchOutcome> {
    check_destination(protected, dst)?;
    let back = patch.reverse(outcome)?;
    let current = read_plain(patched, MAX_PATCH_FILE_BYTES)?;
    let restored = back.apply_bytes(&current)?;
    if sha_hex(&restored) != outcome.before_sha256 {
        return Err(Error::Invalid(
            "reversing the patch does not give back the recorded original file, so nothing was written".into(),
        ));
    }
    let result = back.outcome(&current, &restored);
    fsops::atomic_write(protected, dst, &restored)?;
    Ok(result)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn sample() -> Vec<u8> {
        (0..2000u32).map(|i| (i % 251) as u8).collect()
    }

    /// A patch for `bytes` that sets each (offset, new bytes) pair, reading the old bytes from `bytes`.
    pub fn patch_for(id: &str, bytes: &[u8], edits: &[(usize, &[u8])]) -> BytePatch {
        BytePatch {
            id: id.into(),
            description: format!("test patch {id}"),
            expect_sha256: sha_hex(bytes),
            hunks: edits
                .iter()
                .map(|(off, new)| Hunk {
                    offset: *off as u64,
                    old_hex: hex::encode(&bytes[*off..*off + new.len()]),
                    new_hex: hex::encode(new),
                })
                .collect(),
        }
    }

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn a_patch_makes_a_new_file_and_leaves_the_original_alone() {
        let d = dir();
        let src = d.path().join("a.bin");
        let original = sample();
        std::fs::write(&src, &original).unwrap();
        let p = patch_for("p1", &original, &[(10, &[0xAA, 0xBB]), (500, &[1, 2, 3])]);
        let dst = d.path().join("out/a.patched");
        let outcome = apply(&ProtectedRoots::new(), &p, &src, &dst).unwrap();

        assert_eq!(
            std::fs::read(&src).unwrap(),
            original,
            "the original is untouched"
        );
        let out = std::fs::read(&dst).unwrap();
        assert_eq!(out.len(), original.len());
        assert_eq!(&out[10..12], &[0xAA, 0xBB]);
        assert_eq!(&out[500..503], &[1, 2, 3]);
        assert_eq!(outcome.before_sha256, sha_hex(&original));
        assert_eq!(outcome.after_sha256, sha_hex(&out));
        assert_eq!(outcome.hunks, 2);
        assert_eq!(outcome.bytes_changed, 5);
    }

    #[test]
    fn reverting_gives_back_exactly_the_original_file() {
        let d = dir();
        let src = d.path().join("a.bin");
        let original = sample();
        std::fs::write(&src, &original).unwrap();
        let p = patch_for("p1", &original, &[(0, &[9, 9, 9]), (1999, &[0])]);
        let patched = d.path().join("a.patched");
        let outcome = apply(&ProtectedRoots::new(), &p, &src, &patched).unwrap();
        let back = d.path().join("a.restored");
        let r = revert(&ProtectedRoots::new(), &p, &outcome, &patched, &back).unwrap();
        assert_eq!(std::fs::read(&back).unwrap(), original);
        assert_eq!(r.after_sha256, outcome.before_sha256);
        assert_eq!(r.before_sha256, outcome.after_sha256);
    }

    #[test]
    fn the_same_patch_on_the_same_file_always_gives_the_same_bytes() {
        let original = sample();
        let p = patch_for("p1", &original, &[(7, &[5, 5])]);
        assert_eq!(
            p.apply_bytes(&original).unwrap(),
            p.apply_bytes(&original).unwrap()
        );
    }

    #[test]
    fn a_file_that_is_not_the_one_the_patch_was_made_for_is_refused_and_nothing_is_written() {
        let d = dir();
        let original = sample();
        let p = patch_for("p1", &original, &[(10, &[1])]);
        let mut other = original.clone();
        other[1000] ^= 0xFF;
        let src = d.path().join("other.bin");
        std::fs::write(&src, &other).unwrap();
        let dst = d.path().join("out.bin");
        assert!(apply(&ProtectedRoots::new(), &p, &src, &dst).is_err());
        assert!(!dst.exists());
    }

    #[test]
    fn a_hunk_whose_old_bytes_are_wrong_is_refused() {
        let original = sample();
        let mut p = patch_for("p1", &original, &[(10, &[1, 2])]);
        p.hunks[0].old_hex = "ffff".into();
        let err = p.apply_bytes(&original).unwrap_err().to_string();
        assert!(err.contains("not the bytes the patch expects"), "{err}");
    }

    #[test]
    fn a_hunk_past_the_end_of_the_file_is_refused() {
        let original = sample();
        let mut p = patch_for("p1", &original, &[(10, &[1, 2])]);
        p.hunks[0].offset = 1999;
        assert!(p.apply_bytes(&original).is_err());
        p.hunks[0].offset = u64::MAX;
        assert!(p.validate().is_err());
    }

    #[test]
    fn a_malformed_patch_is_refused_before_any_file_is_read() {
        let original = sample();
        let good = patch_for("p1", &original, &[(10, &[1, 2]), (20, &[3])]);
        assert!(good.validate().is_ok());

        let mut p = good.clone();
        p.hunks.clear();
        assert!(p.validate().is_err(), "no hunks");

        let mut p = good.clone();
        p.hunks.swap(0, 1);
        assert!(p.validate().is_err(), "out of order");

        let mut p = good.clone();
        p.hunks[1].offset = 11;
        assert!(p.validate().is_err(), "overlap");

        let mut p = good.clone();
        p.hunks[0].new_hex = "01".into();
        assert!(p.validate().is_err(), "different length");

        let mut p = good.clone();
        p.hunks[0].new_hex = p.hunks[0].old_hex.clone();
        assert!(p.validate().is_err(), "changes nothing");

        let mut p = good.clone();
        p.hunks[0].new_hex = "zz".repeat(2);
        assert!(p.validate().is_err(), "not hex");

        let mut p = good.clone();
        p.hunks[0].old_hex = String::new();
        p.hunks[0].new_hex = String::new();
        assert!(p.validate().is_err(), "empty");

        let mut p = good.clone();
        p.expect_sha256 = "ABC".into();
        assert!(p.validate().is_err(), "bad hash");

        let mut p = good.clone();
        p.expect_sha256 = p.expect_sha256.to_uppercase();
        assert!(
            p.validate().is_err(),
            "upper-case hash is not the canonical form"
        );

        let mut p = good.clone();
        p.id = "../x".into();
        assert!(p.validate().is_err(), "bad id");

        let mut p = good;
        p.description = "bell\u{7}".into();
        assert!(
            p.validate().is_err(),
            "control character in the description"
        );
    }

    #[test]
    fn the_hunk_count_and_size_limits_are_exact() {
        let original = sample();
        let one = patch_for("p1", &original, &[(0, &[9])]);
        let mut p = one.clone();
        p.hunks = (0..MAX_HUNKS as u64)
            .map(|i| Hunk {
                offset: i * 2,
                old_hex: "00".into(),
                new_hex: "01".into(),
            })
            .collect();
        assert!(p.validate().is_ok(), "exactly the limit");
        p.hunks.push(Hunk {
            offset: MAX_HUNKS as u64 * 2,
            old_hex: "00".into(),
            new_hex: "01".into(),
        });
        assert!(p.validate().is_err(), "one more");

        let mut big = one.clone();
        big.hunks[0].old_hex = "00".repeat(MAX_HUNK_BYTES);
        big.hunks[0].new_hex = "01".repeat(MAX_HUNK_BYTES);
        assert!(big.validate().is_ok());
        big.hunks[0].old_hex = "00".repeat(MAX_HUNK_BYTES + 1);
        big.hunks[0].new_hex = "01".repeat(MAX_HUNK_BYTES + 1);
        assert!(big.validate().is_err());
    }

    #[test]
    fn an_existing_destination_is_never_overwritten_and_in_place_is_refused() {
        let d = dir();
        let original = sample();
        let src = d.path().join("a.bin");
        std::fs::write(&src, &original).unwrap();
        let p = patch_for("p1", &original, &[(10, &[1])]);

        let taken = d.path().join("taken.bin");
        std::fs::write(&taken, b"keep me").unwrap();
        assert!(matches!(
            apply(&ProtectedRoots::new(), &p, &src, &taken),
            Err(Error::DestinationExists(_))
        ));
        assert_eq!(std::fs::read(&taken).unwrap(), b"keep me");

        assert!(apply(&ProtectedRoots::new(), &p, &src, &src).is_err());
        assert!(apply(&ProtectedRoots::new(), &p, &src, &d.path().join("./a.bin")).is_err());
        assert_eq!(std::fs::read(&src).unwrap(), original);
    }

    #[test]
    fn a_protected_destination_a_folder_and_an_oversized_file_are_refused() {
        let d = dir();
        let original = sample();
        let src = d.path().join("a.bin");
        std::fs::write(&src, &original).unwrap();
        let p = patch_for("p1", &original, &[(10, &[1])]);

        let guarded = d.path().join("install");
        std::fs::create_dir_all(&guarded).unwrap();
        let mut protected = ProtectedRoots::new();
        protected.add(&guarded).unwrap();
        assert!(apply(&protected, &p, &src, &guarded.join("a.patched")).is_err());
        assert!(!guarded.join("a.patched").exists());

        assert!(apply(&ProtectedRoots::new(), &p, d.path(), &d.path().join("x")).is_err());

        let dst = d.path().join("small-limit");
        assert!(
            apply_limited(
                &ProtectedRoots::new(),
                &p,
                &src,
                &dst,
                original.len() as u64 - 1
            )
            .is_err()
        );
        assert!(!dst.exists());
        assert!(
            apply_limited(
                &ProtectedRoots::new(),
                &p,
                &src,
                &dst,
                original.len() as u64
            )
            .is_ok()
        );
    }

    #[test]
    fn a_tampered_patched_file_cannot_be_reverted() {
        let d = dir();
        let original = sample();
        let src = d.path().join("a.bin");
        std::fs::write(&src, &original).unwrap();
        let p = patch_for("p1", &original, &[(10, &[1, 2])]);
        let patched = d.path().join("a.patched");
        let outcome = apply(&ProtectedRoots::new(), &p, &src, &patched).unwrap();

        let mut tampered = std::fs::read(&patched).unwrap();
        tampered[1500] ^= 1;
        std::fs::write(&patched, &tampered).unwrap();
        let back = d.path().join("a.restored");
        assert!(revert(&ProtectedRoots::new(), &p, &outcome, &patched, &back).is_err());
        assert!(!back.exists());
    }

    #[test]
    fn an_outcome_from_another_patch_cannot_be_used_to_revert() {
        let original = sample();
        let p1 = patch_for("p1", &original, &[(10, &[1])]);
        let p2 = patch_for("p2", &original, &[(10, &[2])]);
        let after = p1.apply_bytes(&original).unwrap();
        let outcome = p1.outcome(&original, &after);
        assert!(p2.reverse(&outcome).is_err());
        assert!(p1.reverse(&outcome).is_ok());
    }

    #[test]
    fn a_patch_survives_a_round_trip_through_json() {
        let original = sample();
        let p = patch_for("p1", &original, &[(10, &[1, 2]), (30, &[7])]);
        let text = serde_json::to_string(&p).unwrap();
        let q: BytePatch = serde_json::from_str(&text).unwrap();
        assert_eq!(p, q);
        assert_eq!(
            q.apply_bytes(&original).unwrap(),
            p.apply_bytes(&original).unwrap()
        );
    }
}
