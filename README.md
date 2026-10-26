# Soups Unified Launcher (SUL)

An open-source, modular desktop game launcher: Tauri 2, a Rust core and a React/TypeScript UI. Windows first.
Early development. Read `PROJECT_SPEC.md` (requirements), `CLAUDE.md` (engineering rules), `PROJECT_STATUS.md` and `ROADMAP.md`.

## Layout
- `crates/sul-core`: Rust core (game scanner, compatibility matrix, storage, updater, safe file operations, manifests, Discord presence).
- `src-tauri`: desktop container; thin commands over `sul-core`.
- `ui`: React shell (Vite).
- `docs/`: setup, decisions, capability matrix, design references.

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
The merged build has not yet been run on Windows. Licence: not yet chosen.
