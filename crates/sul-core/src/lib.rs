//! `sul-core`: the substrate of Soups Unified Launcher (SUL).
//!
//! What every other SUL crate stands on, and nothing that knows about games, builds, mods or trust checks. It is the lowest
//! of seven crates, each depending only on the ones before it in this list:
//!
//! 1. `sul-core` (this crate)
//! 2. `sul-trust`: SHA-256 build manifests and their verification, integrity checks, the act-without-asking rule, webhook-address checks
//! 3. `sul-quarantine`: setting a file aside, verified and reversible
//! 4. `sul-launcher`: finding installed games, the Steam attach layer, the compatibility matrix, the library index
//! 5. `sul-builds`: managed builds, library roots, game adapters, the launch and runtime manager, the catalogue
//! 6. `sul-mod`: the mod store, profiles, deploy and purge, recipes, reversible patches, external tools, package scanning
//! 7. `sul-app`: the orchestrator the desktop shell's commands call
//!
//! How the old single crate was divided, and why the cuts were made together: `docs/decisions/0010-sul-core-split.md`. Where
//! the build manifest and the trust checks went, and why the manifest is in `sul-trust` and not `sul-builds`:
//! `docs/decisions/0011-trust-split.md` (D-154).
//!
//! | What it gives                                                                         | Module          |
//! |---------------------------------------------------------------------------------------|-----------------|
//! | The one error type every crate shares (the counterpart of the base error enum in Ch. 1 §3.4) | [`error`] |
//! | Relative paths and symlinks never escape a root                                       | [`safepath`]    |
//! | Protected roots, atomic writes, isolated copies                                       | [`fsops`]       |
//! | Interrupted multi-step operations are detectable                                      | [`journal`]     |
//! | Structured JSONL logs with secret redaction                                           | [`logging`]     |
//! | Where each game's isolated data lives                                                 | [`storage`]     |
//! | Safe Windows file names                                                               | [`names`]       |
//! | The version log that makes rollback possible                                          | [`updater`]     |
//! | Standard install or self-contained portable copy                                      | [`portability`] |
//! | Named, sectioned templates exported to and imported from a folder                     | [`templates`]   |

pub mod error;
pub mod fsops;
pub mod journal;
pub mod logging;
pub mod names;
pub mod portability;
pub mod safepath;
pub mod storage;
pub mod templates;
pub mod updater;

pub use error::{Error, Result};
