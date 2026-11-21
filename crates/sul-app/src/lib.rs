//! `sul-app`: the orchestrator the desktop shell's commands call.
//!
//! [`manager::ManagerService`] ties the library, the managed builds, the mods and the launch manager together, so the Tauri
//! commands in `src-tauri` stay thin wrappers and every rule can be tested without a window. It holds no rules of its own
//! that belong lower down: those are in `sul-core`, `sul-launcher`, `sul-builds` and `sul-mod`.
//!
//! Depends on all four. Only `src-tauri` depends on this crate.

pub mod manager;
