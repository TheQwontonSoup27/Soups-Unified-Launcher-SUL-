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

## Verified in GitHub Actions (run 39761280234, commit d8f6e91)
- `cargo test --workspace` step succeeded for `sul-core` on windows-latest and ubuntu-latest (job-level success; per-test output not read, log download blocked from the authoring session).
- `ui` job succeeded (npm install, npm test, npm run build).
- `cargo check` of the Tauri wrapper FAILED on both OSes; cause not yet read.
- Decision 2026-10-22: Windows only for now. All Ubuntu CI jobs removed.

## NOT verified
- Rust core on the next CI run (workflow changed to Windows-only; rerun pending).
- **Tauri wrapper: `cargo check` failing in CI**, reason unread.
- **TypeScript typecheck (`tsc --noEmit`) not run:** @types packages unavailable offline.
- `ui` uses `npm install` in CI (no lockfile yet, could not generate one offline).
- No game adapter exists. Helldivers 2 7.1.1 isolation/offline launch: not investigated.

## Next action
1. Run `cargo test --workspace` and `npm install && npm test && npx tsc --noEmit` on a machine with network; fix whatever fails.
2. Then build the Helldivers 2 Build Manager slice: import an install, hash it, isolated copy, verify, launch tracking.
