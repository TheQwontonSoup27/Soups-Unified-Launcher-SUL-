//! The only place network code may live in Soups Unified Launcher (owner decision, 2026-10-31; `docs/decisions/0006-*`).
//!
//! It is empty on purpose, and nothing depends on it yet. `sul-core`, and any future audio or speech crate, must stay free of
//! network crates so that "local only" can be checked by looking at their dependency lists. When a network feature is approved
//! (webhooks, online text for the overlay, the updater), its client code goes here and the app reaches it through a small,
//! explicit interface, off by default and switched on by the owner.
//!
//! Tripwires that keep this true: `crates/sul-core/tests/write_sites.rs` (this crate stays empty and unreferenced, and no other
//! manifest may declare a network or credential crate), and the declared-surface guard in `cargo xtask ac-regression`
//! (`xtask/src/network.rs`, Ch. 16 §16.15.14: network crates may enter a package's Windows graph only through `sul-net`).
