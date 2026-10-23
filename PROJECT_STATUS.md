# Project Status — Soups Unified Launcher (SUL)

**Last updated:** 2026-10-22

## Decisions made (owner approved)
- Stack: Tauri 2 + Rust core + React/TypeScript UI. Windows first. (docs/decisions/0001-stack.md)
- Still open: license, manifest schema migration policy, social/networking, Medal access level, first full game adapter.

## Built this session
- `crates/sul-core`: capability model with overclaim validation, safe relative paths, build manifests (scan/verify), protected roots + atomic write + isolated copy, recovery journal, redacting JSONL logger. Integration tests in `crates/sul-core/tests/core.rs`.
- `ui/`: app shell, sidebar for 10 modules, theme tokens (4 presets), Overview and Appearance pages, placeholders for the rest.
- `src-tauri/`: minimal wrapper with least-privilege capabilities.
- `.github/workflows/ci.yml`, `docs/SETUP-WINDOWS.md`, `docs/capability-matrix/`.

## Verified (actually run)
- `ui`: `node build.mjs` succeeded; `tsx --test tests/*.test.ts tests/*.test.tsx` → 13 pass, 0 fail (Linux container, Node 22).
- Screenshot check of Overview (dark) and Appearance (light): rendered, no console errors.

## Verified in GitHub Actions (run 39880270552, commit e5a64b0, windows-latest only)
- `core`: `cargo test --workspace` passed (sul-core unit/integration tests). Per-test output not read: raw log download is blocked from the authoring session, only job/step results are visible.
- `ui`: `npm install`, `npm run typecheck`, `npm test`, `npm run build` all passed.
- `tauri-shell`: `cargo check` of the desktop wrapper passed. The earlier failure was most likely the missing `src-tauri/icons` (placeholder icons added); the exact error was never read.
- Decision 2026-10-22: Windows only for now. All Ubuntu CI jobs removed.

## NOT verified
- The desktop app has never been launched (`tauri dev` / installer build not run). Only `cargo check` passed.
- No real game install has been touched by any code. No game adapter exists.
- Helldivers 2 7.1.1 isolation/offline launch: not investigated.
- Icons are generated placeholders, not final branding.

## Next action
1. Launch the desktop app on Windows (`npx tauri dev`) and report what you see.
2. Then build the Helldivers 2 Build Manager slice: import an install, hash it, isolated copy, verify, launch tracking.
