//! `sul-launcher`: finding the owner's games and attaching to the platforms they come from.
//!
//! The launcher layer of the first release (R0): Steam library parsing and the Steam attach layer have counterparts in the
//! architecture spec (Ch. 3 §2.1 Game library and §2.14 Steam IPC and Tauri bridge). The tick-and-compatibility matrix, the
//! capability labels, the persisted library index and Discord presence have none there (`docs/decisions/0009`, row A3 and
//! row E); they are kept. The spec keeps the library index in SQLite (Ch. 12 §2.1); this crate still uses JSON files until
//! that move is made (decision 0009, point 4.3).
//!
//! | What it gives                                                                                         | Module         |
//! |-------------------------------------------------------------------------------------------------------|----------------|
//! | Registry scanner: installed games on every drive (Steam library folders, GOG, uninstall keys, "Games" folders) | [`registry`] |
//! | A read-only filesystem view, so discovery can be tested against in-memory fixtures                    | [`fsys`]       |
//! | Steam attach layer (never sees or stores a credential)                                                | [`steam`]      |
//! | Per-game tick-and-compatibility matrix                                                                | [`matrix`]     |
//! | Five-status capability labels with an evidence rule                                                   | [`capability`] |
//! | The persisted library: scan results plus each game's ticks                                            | [`library`]    |
//! | The library as the UI uses it (scan, list, matrix, tick)                                              | [`service`]    |
//! | Discord Rich Presence over local IPC (opt-in)                                                         | [`discord`]    |
//!
//! Depends on `sul-core` only. The optional `steam` feature turns on the real Steamworks bindings.
//!
//! Not to be confused with the spec's `sul-compat`, which resolves configuration conflicts (Ch. 1 §5); [`matrix`] is a
//! per-game table of ticks and is a different thing.

pub mod capability;
pub mod discord;
pub mod fsys;
pub mod library;
pub mod matrix;
pub mod registry;
pub mod service;
pub mod steam;
