//! `sul-builds`: managed copies of a game, where they live, and how one is started.
//!
//! The new architecture spec has no counterpart for managed builds, library roots, the runtime manager or the catalogue
//! (`docs/decisions/0009`, row B), so they are kept as they were. The game adapters correspond to the per-game launch
//! profile in `game_registry.json` (Ch. 3 §2.24); a launch plan has to be checked against that profile's signed
//! `anti_cheat` block before anything starts (decision 0009, point 4.8), which is R0 layer 5 work and is not done yet.
//!
//! | What it gives                                                                              | Module        |
//! |--------------------------------------------------------------------------------------------|---------------|
//! | The one interface through which a game's specifics reach the rest of SUL                   | [`adapters`]  |
//! | Starting a program: the one place (besides the Steam launcher) that runs another program   | [`process`]   |
//! | Library roots: the folders managed builds and mods live in, each with a marker file        | [`roots`]     |
//! | The build registry: imports, slots, rollback, repair, with a protected original            | [`builds`]    |
//! | Launch and runtime manager: start a build, follow it, say why it did not start             | [`runtime`]   |
//! | The catalogue: categories, artwork references, search, the detail page                     | [`catalogue`] |
//!
//! Depends on `sul-core` and `sul-launcher` (the capability labels). Nothing from `sul-mod` or `sul-app` may be used here.
//!
//! # Test support
//!
//! The `test-support` feature compiles [`testkit`] (a library on a temporary disk with a made-up game installed in a
//! protected "original" folder) and `roots::fake` so that the tests of crates above this one can use them. It is off by
//! default and no shipped package may turn it on; `crates/sul-core/tests/write_sites.rs` checks that.

pub mod adapters;
pub mod builds;
pub mod catalogue;
pub mod process;
pub mod roots;
pub mod runtime;

#[cfg(any(test, feature = "test-support"))]
pub mod testkit;
