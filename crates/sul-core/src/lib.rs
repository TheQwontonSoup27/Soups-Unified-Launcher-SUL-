//! `sul-core`: the substrate of Soups Unified Launcher (SUL).
//!
//! What every other SUL crate stands on, and nothing that knows about games, builds or mods. It is the lowest of five
//! crates, each depending only on the ones before it in this list:
//!
//! 1. `sul-core` (this crate)
//! 2. `sul-launcher`: finding installed games, the Steam attach layer, the compatibility matrix, the library index
//! 3. `sul-builds`: managed builds, library roots, game adapters, the launch and runtime manager, the catalogue
//! 4. `sul-mod`: the mod store, profiles, deploy and purge, recipes, reversible patches, external tools
//! 5. `sul-app`: the orchestrator the desktop shell's commands call
//!
//! How the old single crate was divided, and why the cuts were made together: `docs/decisions/0010-sul-core-split.md`.
//!
//! | What it gives                                                                         | Module          |
//! |---------------------------------------------------------------------------------------|-----------------|
//! | The one error type every crate shares (the counterpart of the base error enum in Ch. 1 §3.4) | [`error`] |
//! | Relative paths and symlinks never escape a root                                       | [`safepath`]    |
//! | Protected roots, atomic writes, isolated copies                                       | [`fsops`]       |
//! | Interrupted multi-step operations are detectable                                      | [`journal`]     |
//! | Structured JSONL logs with secret redaction                                           | [`logging`]     |
//! | SHA-256 build manifests, read-only verification                                       | [`manifest`]    |
//! | Integrity checks, package scanning, quarantine, the act-without-asking rule, webhook-address checks | [`trust`] |
//! | Where each game's isolated data lives                                                 | [`storage`]     |
//! | Safe Windows file names                                                               | [`names`]       |
//! | The version log that makes rollback possible                                          | [`updater`]     |
//! | Standard install or self-contained portable copy                                      | [`portability`] |
//! | Named, sectioned templates exported to and imported from a folder                     | [`templates`]   |
//!
//! `manifest` and `trust` are kept here for now: the owner has not yet said where they belong
//! (`docs/decisions/0009-sul-core-merge-map.md`, "Owner answers").

pub mod error;
pub mod fsops;
pub mod journal;
pub mod logging;
pub mod manifest;
pub mod names;
pub mod portability;
pub mod safepath;
pub mod storage;
pub mod templates;
pub mod trust;
pub mod updater;

pub use error::{Error, Result};
