//! `sul-mod`: mods, the way they are put into a managed build, and the tools to make them.
//!
//! The architecture spec names this crate (Ch. 3 §2.10, "Mod building and staging suite (sul-mod)") and places mod tooling
//! at release R1; the code here predates that release and is kept (`docs/decisions/0009`, row D). The spec's second backend,
//! the `sul-vfs` overlay, does not exist yet: today a profile is copied into a managed copy of the game, and the two are
//! meant to live side by side (decision 0009, point 4.5). The audio tool for Helldivers 2 named in Ch. 3 §2.11 is only
//! registered as an external tool here ([`builder::tools`]), never bundled or downloaded.
//!
//! | What it gives                                                                                  | Module      |
//! |------------------------------------------------------------------------------------------------|-------------|
//! | The mod store, named profiles with a load order, deploy, purge and undo with an ownership ledger | [`mods`]    |
//! | Reproducible recipes, reversible byte patches, the saved recipe book, the external tool registry | [`builder`] |
//! | Looking inside a mod package (a folder or a ZIP) before anything is unpacked, and saying what looks wrong | [`package`] |
//!
//! `package` was `sul-core::trust::package` until D-154: it is the mod package scan, and it reports in `sul-trust`'s
//! finding types.
//!
//! Depends on `sul-core`, `sul-trust`, `sul-launcher` and `sul-builds`. Nothing from `sul-app` may be used here.

pub mod builder;
pub mod mods;
pub mod package;
