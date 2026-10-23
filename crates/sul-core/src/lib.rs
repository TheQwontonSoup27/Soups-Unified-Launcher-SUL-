//! sul-core: shared infrastructure for Soups Unified Launcher.
//! No game-specific logic lives here; adapters declare capabilities through [`capability`].

pub mod capability;
pub mod discord;
pub mod error;
pub mod fsops;
pub mod journal;
pub mod logging;
pub mod manifest;
pub mod safepath;
pub mod steam;

pub use error::{Error, Result};
