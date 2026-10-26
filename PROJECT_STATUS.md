# Project Status — Soups Unified Launcher (SUL)

**Last updated:** 2026-10-26 (Step 1.5: feature recovery and consolidation)

## Decisions made (owner approved)
- Stack: Tauri 2 + Rust core + React/TypeScript UI. Windows first. (`docs/decisions/0001-stack.md`)
- Step 1.5 (`docs/decisions/0002-step-1-5-consolidation.md`): the long-term `PROJECT_SPEC.md` governs; app identifier stays
  `app.soups.unifiedlauncher`; the per-game tick-and-compatibility matrix is kept with the five status labels derived from it;
  Discord small-image asset key is supported, a scale slider is not offered.
- Still open: licence, manifest schema migration policy, social/networking, Medal access level, first full game adapter.

## What exists
`crates/sul-core` (Rust): registry game scanner (Steam libraries on every drive, GOG, Uninstall keys, drive probing) behind
`RegistryAccess` / `FileSystem` seams; per-game compatibility matrix with the five capability labels derived from it; persisted
library index; storage isolator; baseline tracking and version self-updater with rollback; portable mode; JSON templates;
capability model with the evidence rule; safe paths; protected roots, atomic write, isolated copy; recovery journal; redacting JSONL
logger; SHA-256 build manifests; Discord Rich Presence over local IPC (opt-in); read-only Steam helpers (`steam://` launch URL,
build id and update policy recorded, Steam-library guard).
`src-tauri`: Tauri container, least-privilege capabilities, `app_info` and `discord_set_enabled` commands.
`ui`: Vite + React shell with the Step 1 sidebar (Game Library first; Settings and General Playtime Tracker last), the ten
roadmap modules as labelled placeholders, Team27 theme with contrast-checked token editor, backgrounds, borderless title bar,
splash, Discord Settings panel, capability legend with the "weakest status wins" headline rule.

## Verified (actually run, this session)
Linux container, Node 22, cargo 1.97. Exact commands and results:
- `cargo fmt --all -- --check` → clean.
- `cargo clippy --workspace --all-targets -- -D warnings` → clean.
- `cargo test --workspace` → 139 passed, 0 failed (105 `sul-core` unit tests, 13 in `tests/core.rs`, 15 in `tests/discord.rs`,
  5 in `tests/steam.rs`, 1 in `sul-tauri`).
- `npm run ui:typecheck` → clean. `npm run ui:test` → 33 passed, 0 failed. `npm run ui:build` → built (44 modules).

## Verified earlier by the owner (before the merge, on the Step 1 archive, Windows)
- Registry scanner archive: 78 tests passed and 38 games were mapped. That run predates this merge and used the scanner on its own.
  Whether all 38 are correct, or whether games were missed, was not checked against a list.

## NOT verified
- The merged branch has not been compiled or tested on Windows yet. The Windows-only adapters (`WindowsRegistry`, `FixedDrives`)
  are compiled only there; this session's Linux container cannot build them. GitHub Actions on `windows-latest` is the first check.
- The desktop app has not been launched from this branch (`npm run dev`). The borderless title-bar buttons and the move from
  `withGlobalTauri` to `@tauri-apps/api` have never run inside a real Tauri window.
- Discord against a live Discord client; any of this against a real game install beyond the owner's scan; no game adapter exists.
- Helldivers 2 7.1.1 isolation/offline launch: Not investigated.
- Icons are the Team27 set, not reviewed as final branding. Wallpaper JPEGs are git-ignored (licences unconfirmed; docs in
  `ui/public/wallpapers/README.md`), so the gallery is empty on a fresh clone.
- The Step 1 spec's persistent Steam credentials and Direct3D/Vulkan overlay hooks conflict with `CLAUDE.md` (no secrets in source or
  logs, no anti-cheat bypass); not started, to be settled when those steps come up.

## Branch and safety net
- `main` was never modified by the integration work until the owner asked for the merge. The legacy state is `9ece116`,
  tagged locally as `legacy-pre-step-1.5`. See `docs/decisions/0002-step-1-5-consolidation.md` for the tag-push note.

## Next action
1. On Windows: `git fetch`, `git checkout main`, `npm install`, `cargo test --workspace`, `npm run dev`; report what you see.
2. Then the Game Library slice: a `scan_games` Tauri command feeding real scan results and the tick-and-compatibility matrix into
   the Game Library page (replacing the mock data), then the per-game hub.
3. Helldivers 2 Build Manager stays a separate adapter track.
