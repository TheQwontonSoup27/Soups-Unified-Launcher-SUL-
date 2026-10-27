//! `sul-core`: backend engine for Soups Unified Launcher (SUL).
//!
//! Section 1 of the Step 1 spec (game discovery, matrix, storage, updater, templates):
//!
//! | Spec item                              | Module          |
//! |----------------------------------------|-----------------|
//! | Steam headless network layer           | [`steam`]       |
//! | Registry scanner                       | [`registry`]    |
//! | Per-game tick-and-compatibility matrix | [`matrix`]      |
//! | Persisted library (scan + ticks)       | [`library`]     |
//! | Library service for the UI (scan, matrix, tick) | [`service`] |
//! | Storage isolator                       | [`storage`]     |
//! | Baseline tracking & version self-updater | [`updater`]   |
//! | Engine portability matrix              | [`portability`] |
//! | Template serialization engine          | [`templates`]   |
//!
//! Engineering rules from `CLAUDE.md` (safe file operations, recovery, truthful capability labels,
//! no secrets in logs, opt-in presence):
//!
//! | Rule                                              | Module         |
//! |---------------------------------------------------|----------------|
//! | Five-status capability labels with an evidence rule | [`capability`] |
//! | Relative paths and symlinks never escape a root   | [`safepath`]   |
//! | Protected roots, atomic writes, isolated copies   | [`fsops`]      |
//! | Interrupted multi-step operations are detectable  | [`journal`]    |
//! | Structured JSONL logs with secret redaction       | [`logging`]    |
//! | SHA-256 build manifests, read-only verification   | [`manifest`]   |
//! | Discord Rich Presence over local IPC (opt-in)     | [`discord`]    |
//!
//! Sections 2-4 of the Step 1 spec (UI shell, game hub tabs, overlay) are not represented here yet.

pub mod capability;
pub mod discord;
pub mod error;
pub mod fsops;
pub mod fsys;
pub mod journal;
pub mod library;
pub mod logging;
pub mod manifest;
pub mod matrix;
pub mod portability;
pub mod registry;
pub mod safepath;
pub mod service;
pub mod steam;
pub mod storage;
pub mod templates;
pub mod updater;

mod names;

pub use error::{Error, Result};
