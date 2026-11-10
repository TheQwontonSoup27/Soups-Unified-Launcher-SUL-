# Soups Unified Launcher (SUL)

An open-source, modular desktop game launcher: Tauri 2, a Rust core and a React/TypeScript UI. Windows first.
Early development. A ground-up rebuild from a new spec is starting (2026-11-09): read `HANDOFF.md` first. `PROJECT_SPEC.md` is the old V5.1 spec; `CLAUDE.md` holds the engineering rules; `PROJECT_STATUS.md` and `ROADMAP.md` describe the build as it stood at the archive branch `archive/legacy-pre-ground-up-2026-11-09`.

## Layout
- `crates/sul-core`: Rust core (game scanner, compatibility matrix, storage, updater, safe file operations, manifests, Discord presence).
- `src-tauri`: desktop container; thin commands over `sul-core`.
- `ui`: React shell (Vite).
- `docs/`: setup, decisions, capability matrix, design references, and `docs/references.md` (every link and named source, with its status).

## Commands
```
npm install
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
npm run ui:typecheck
npm run ui:test
npm run ui:build
npm run dev
```
`cargo test -p sul-core` runs the core alone (no WebView or WebKitGTK needed). Setup on Windows: `docs/SETUP-WINDOWS.md`.

## Status
Every capability carries one of: Supported and tested, Partially supported, Experimental, Unsupported, Not investigated.
Nothing is marked "Supported and tested" without a test date and evidence; see `docs/capability-matrix/README.md`.
CI builds and tests the merged code on Windows; the desktop app itself has not been launched from it yet. Licence: not yet chosen.
