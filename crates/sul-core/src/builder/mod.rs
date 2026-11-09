//! The mod builder workspace (spec 2.2) and the records the asset tools need (spec 2.3, 2.4).
//!
//! What is here, and what is not:
//!
//! * [`patch`]: a **reversible byte patch** (spec 2.3, "record reversible patches"). It says "in the file whose
//!   SHA-256 is X, at these offsets, these exact bytes become those bytes". Applying one writes a **new** file and
//!   never changes the original; it refuses a file that is not exactly the one the patch was made for or whose
//!   bytes are not what the patch expects; and the same patch run backwards gives the original file back, checked
//!   by hash.
//! * [`book`]: the **saved recipes**, one list file, replaced only from the version the caller opened.
//! * [`recipe`]: a **reproducible recipe** (spec 2.2). An ordered list of asset-level steps (layer an imported
//!   mod, patch a file) that builds a folder of files in the workspace. The same recipe over the same inputs gives
//!   the same files, and a dry run says so before anything is written. Every run keeps a **provenance record**:
//!   which mod and which patch produced each file, with hashes and the licence text the owner wrote down.
//!   Asset-level steps and runtime-code steps are different classes of operation (spec 2.2); a runtime-code step
//!   can be written down but a recipe that has one is refused, because SUL has nothing that can run or check it.
//! * [`tools`]: a **registry of external tools** (spec 2.3, 2.4). SUL records a tool's version, size and SHA-256
//!   and what is known about its licence, notices when it changes, and records what it was asked to do and what
//!   changed in the workspace. It never copies, bundles or downloads a tool.
//!
//! Not done: nothing here understands a real game's file format (no extraction, repacking, Wwise or Bink
//! handling), nothing has been run against a real mod or a real tool, and the recipe steps cannot yet make a new
//! build (spec 2.7). Every test uses made-up files. The module writes only into the workspace folder and its own
//! state files, never into a game build, and it deletes nothing.

pub mod book;
pub mod patch;
pub mod recipe;
pub mod tools;

use serde::{Deserialize, Serialize};

/// What kind of change an operation makes. Spec 2.2: "separate asset-level operations from runtime-code
/// optimization; do not conflate them."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpClass {
    /// Changes the bytes of files that are data: layering, merging, patching.
    Asset,
    /// Changes code that runs inside the game (a script, a hook, an injected library). Not supported.
    RuntimeCode,
}
