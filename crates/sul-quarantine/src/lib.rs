//! `sul-quarantine`: set a file aside where nothing will use it, verified and reversible.
//!
//! This is the old `sul-core::trust::quarantine` module in a crate of its own (D-154), so that the quarantine can carry its
//! own dependencies (encryption, a key source supplied by the vault) without pulling them into the substrate. It depends on
//! `sul-core` (file operations, the recovery journal, the error type) and on `sul-trust` (`manifest::hash_file`, the SHA-256
//! that proves every copy). Nothing depends on it yet: there is no mod manager to hand it a file.
//!
//! **Not encrypted yet.** Ch. 14 §14.13.4 asks for "copy → verify → record → verify → remove into encrypted quarantine". The
//! move, the verification and the restore exist and are tested; the encryption does not. A quarantined copy is stored as it
//! was. The design the owner chose (2026-11-22) is a fresh random key per quarantined file, stored beside its record, behind
//! a key-source trait so that the vault supplies the key at R0 layer 4; it neutralises a file, it does not keep a secret.
//!
//! The rule for when SUL may quarantine a file without asking is [`sul_trust::policy`]; this crate does the moving. The test
//! that checks the two agree is `tests/policy_agrees.rs`.

#![forbid(unsafe_code)]

pub mod quarantine;
