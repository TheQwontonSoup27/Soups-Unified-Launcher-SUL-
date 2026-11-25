//! Where the keys to sealed quarantine copies are kept: the [`KeySource`] trait and its default, [`KeyFiles`].
//!
//! Every quarantined file is sealed under its own fresh random key (D-155, the owner's answer of 2026-11-22). Who holds the
//! key is a separate question, so it sits behind a trait: the quarantine asks a key source to make, return and forget the key
//! of an item, and does not care where it lives. At R0 layer 4 the vault supplies its own key source, and the keys move into
//! it with no change to the sealed format.
//!
//! **What the default does and does not do.** [`KeyFiles`] keeps each key in a file beside the item's record. That neutralises
//! a quarantined file: it is no longer a program that can run, be scanned as one, or be read as text by another tool, and
//! it comes back byte for byte. It does not keep a secret: anyone who can read the quarantine folder can read the key beside
//! the data. The protection against that is the vault, and until the vault exists this crate does not claim it.
//!
//! This is the only file in the crate besides `quarantine.rs` that creates or deletes a file, and it does so only for a key
//! file: created new (never over another), removed only with the item it belongs to.

use crate::seal::{ItemKey, KEY_BYTES};
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use sul_core::{Error, Result, error::io};
use zeroize::Zeroizing;

/// The item a key belongs to.
#[derive(Debug, Clone, Copy)]
pub struct ItemRef<'a> {
    /// The quarantine id (`q-<13 digits>-<12 hex>-<4+ digits>`), already checked, so it is safe to use in a name.
    pub id: &'a str,
    /// The folder the item lives in. A key source that keeps keys elsewhere ignores it; [`KeyFiles`] keeps the key here.
    pub dir: &'a Path,
}

/// Makes, returns and forgets the key of each quarantined item.
///
/// An implementation must make every key fresh and random (use [`ItemKey::generate`]), never hand one item's key out for
/// another, and report a key it cannot supply (a locked vault, a missing entry) as an error, not as a wrong key.
pub trait KeySource: Send + Sync {
    /// Make a new key for `item` and keep it so that [`get`](Self::get) returns it. Called once per item, before anything
    /// is sealed. It must refuse, not replace, a key that already exists for the item.
    fn create(&self, item: &ItemRef<'_>) -> Result<ItemKey>;

    /// The key `create` made for `item`.
    fn get(&self, item: &ItemRef<'_>) -> Result<ItemKey>;

    /// Let go of the key of `item`: its copy was restored, or never finished. A key that is not there is not an error.
    fn forget(&self, item: &ItemRef<'_>) -> Result<()>;
}

/// The default key source: one file, `key.bin`, in the item's own folder, holding the 32 key bytes.
#[derive(Debug, Default, Clone, Copy)]
pub struct KeyFiles;

const KEY_FILE: &str = "key.bin";

impl KeyFiles {
    fn path(item: &ItemRef<'_>) -> PathBuf {
        item.dir.join(KEY_FILE)
    }
}

impl KeySource for KeyFiles {
    fn create(&self, item: &ItemRef<'_>) -> Result<ItemKey> {
        let key = ItemKey::generate().map_err(|e| {
            Error::Invalid(format!(
                "no key could be made, so nothing was set aside ({e})"
            ))
        })?;
        let path = Self::path(item);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(io(&path))?;
        file.write_all(key.as_bytes()).map_err(io(&path))?;
        file.flush().map_err(io(&path))?;
        file.sync_all().map_err(io(&path))?;
        Ok(key)
    }

    fn get(&self, item: &ItemRef<'_>) -> Result<ItemKey> {
        let path = Self::path(item);
        let mut file = std::fs::File::open(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::Invalid(format!(
                    "the key to {} is missing, so its quarantined copy cannot be opened",
                    item.id
                ))
            } else {
                Error::Io {
                    path: path.clone(),
                    source: e,
                }
            }
        })?;
        let damaged = || {
            Error::Invalid(format!(
                "the key to {} is damaged, so its quarantined copy cannot be opened",
                item.id
            ))
        };
        let mut bytes = Zeroizing::new([0u8; KEY_BYTES]);
        match file.read_exact(&mut bytes[..]) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Err(damaged()),
            Err(e) => {
                return Err(Error::Io { path, source: e });
            }
        }
        // A key file is exactly the key: anything after it means it is not one of ours.
        let mut extra = [0u8; 1];
        if file.read(&mut extra).map_err(io(&path))? != 0 {
            return Err(damaged());
        }
        ItemKey::from_slice(&bytes[..]).ok_or_else(damaged)
    }

    fn forget(&self, item: &ItemRef<'_>) -> Result<()> {
        let path = Self::path(item);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::Io { path, source: e }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item<'a>(dir: &'a Path) -> ItemRef<'a> {
        ItemRef {
            id: "q-1798675200000-0123456789ab-0001",
            dir,
        }
    }

    #[test]
    fn a_key_is_made_kept_beside_the_item_and_read_back_identical() {
        let tmp = tempfile::tempdir().unwrap();
        let made = KeyFiles.create(&item(tmp.path())).unwrap();
        let on_disk = std::fs::read(tmp.path().join("key.bin")).unwrap();
        assert_eq!(on_disk.len(), KEY_BYTES);
        assert_eq!(&on_disk[..], made.as_bytes());
        let got = KeyFiles.get(&item(tmp.path())).unwrap();
        assert_eq!(got.as_bytes(), made.as_bytes());
    }

    #[test]
    fn a_key_is_never_made_over_an_existing_one() {
        let tmp = tempfile::tempdir().unwrap();
        let first = KeyFiles.create(&item(tmp.path())).unwrap();
        assert!(KeyFiles.create(&item(tmp.path())).is_err());
        let still = KeyFiles.get(&item(tmp.path())).unwrap();
        assert_eq!(
            still.as_bytes(),
            first.as_bytes(),
            "the first key is untouched"
        );
    }

    #[test]
    fn two_items_get_two_different_keys() {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let ka = KeyFiles.create(&item(a.path())).unwrap();
        let kb = KeyFiles.create(&item(b.path())).unwrap();
        assert_ne!(ka.as_bytes(), kb.as_bytes());
    }

    #[test]
    fn a_missing_key_is_said_to_be_missing_not_wrong() {
        let tmp = tempfile::tempdir().unwrap();
        let err = KeyFiles.get(&item(tmp.path())).unwrap_err();
        assert!(matches!(err, Error::Invalid(m) if m.contains("is missing")));
    }

    #[test]
    fn a_key_file_that_is_not_exactly_a_key_is_damaged() {
        let tmp = tempfile::tempdir().unwrap();
        for bytes in [&b""[..], &[1u8; 31][..], &[1u8; 33][..], &[1u8; 64][..]] {
            std::fs::write(tmp.path().join("key.bin"), bytes).unwrap();
            let err = KeyFiles.get(&item(tmp.path())).unwrap_err();
            assert!(
                matches!(err, Error::Invalid(m) if m.contains("is damaged")),
                "{} bytes",
                bytes.len()
            );
        }
    }

    #[test]
    fn the_only_deletion_in_this_file_is_the_key_of_one_item() {
        // A guard on the file's source: it may remove `key.bin` in an item's own folder and nothing else. A new deletion must be
        // added here on purpose, with its reason.
        let src = include_str!("keys.rs");
        let body = src.split("#[cfg(test)]").next().unwrap();
        let removals: Vec<&str> = body
            .lines()
            .filter(|l| l.contains("remove_file") || l.contains("remove_dir"))
            .map(str::trim)
            .collect();
        assert_eq!(removals, ["match std::fs::remove_file(&path) {"]);
    }

    #[test]
    fn forgetting_removes_the_key_and_forgetting_twice_is_fine() {
        let tmp = tempfile::tempdir().unwrap();
        KeyFiles.create(&item(tmp.path())).unwrap();
        std::fs::write(tmp.path().join("record.json"), b"{}").unwrap();
        KeyFiles.forget(&item(tmp.path())).unwrap();
        assert!(!tmp.path().join("key.bin").exists());
        assert!(
            tmp.path().join("record.json").exists(),
            "only the key was removed"
        );
        KeyFiles.forget(&item(tmp.path())).unwrap();
    }
}
