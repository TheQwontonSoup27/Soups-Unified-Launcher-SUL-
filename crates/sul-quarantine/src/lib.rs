//! `sul-quarantine`: set a file aside where nothing will use it, sealed, verified and reversible.
//!
//! This is the old `sul-core::trust::quarantine` module in a crate of its own (D-154), so that the quarantine can carry its
//! own dependencies (the cipher, a key source supplied by the vault) without pulling them into the substrate. It depends on
//! `sul-core` (file operations, the recovery journal, the error type) and on `sul-trust` (`manifest::hash_file`, the SHA-256
//! that proves every copy). Nothing depends on it yet: there is no mod manager to hand it a file.
//!
//! **Sealed since D-155.** Ch. 14 §14.13.4 asks for "copy → verify → record → verify → remove into encrypted quarantine".
//! A quarantined copy is now sealed with XChaCha20-Poly1305 (the vault's cipher) in the STREAM construction, in 64 KiB
//! chunks; the format is this crate's own and [`seal`] documents it. "Verify" means opening the sealed copy and hashing what comes out,
//! so a copy that does not decrypt to exactly the original is never trusted. Every file has its own fresh random key. The
//! keys sit behind the [`keys::KeySource`] trait: by default in a file beside the item's record ([`keys::KeyFiles`]), and at
//! R0 layer 4 in the vault.
//!
//! **What that is and is not.** It neutralises a file: the copy cannot run, cannot be scanned as the program it was, and
//! comes back byte for byte. It does not keep a secret from anyone who can read the quarantine folder, because the default
//! key sits in it; that protection arrives with the vault, and until then nothing here claims it. Copies made before D-155 were
//! stored as they were; they are still listed and restored (a version 1 record), and are not changed.
//!
//! The rule for when SUL may quarantine a file without asking is [`sul_trust::policy`]; this crate does the moving. The test
//! that checks the two agree is `tests/policy_agrees.rs`.

#![forbid(unsafe_code)]

pub mod keys;
pub mod quarantine;
pub mod seal;
