# Project Status — Soups Unified Launcher (SUL)

**Last updated:** 2026-10-27 (Steam manager, library IPC commands, Game Library grid; on branch `feature/steam-ipc-library`)

## Decisions made (owner approved)
- Stack: Tauri 2 + Rust core + React/TypeScript UI. Windows first. (`docs/decisions/0001-stack.md`)
- Step 1.5 (`docs/decisions/0002-step-1-5-consolidation.md`): the long-term `PROJECT_SPEC.md` governs; app identifier stays
  `app.soups.unifiedlauncher`; the per-game tick-and-compatibility matrix is kept with the five status labels derived from it;
  Discord small-image asset key is supported, a scale slider is not offered.
- Steam (`docs/decisions/0003-steam-attach-launch-and-library-ipc.md`): no credentials are stored, ever; `steamworks` only attaches to the
  running client and only on request; games start with `steam://run/<id>`; public player metrics will come from public HTTP by SteamID64.
- Still open: licence, manifest schema migration policy, social/networking, Medal access level, first full game adapter, where a
  Steam Web API key would live (it is a secret).

## What exists
`crates/sul-core` (Rust): registry game scanner (Steam libraries on every drive, GOG, Uninstall keys, drive probing) behind
`RegistryAccess` / `FileSystem` seams; per-game compatibility matrix with the five capability labels derived from it; persisted
library index; storage isolator; baseline tracking and version self-updater with rollback; portable mode; JSON templates;
capability model with the evidence rule; safe paths; protected roots, atomic write, isolated copy; recovery journal; redacting JSONL
logger; SHA-256 build manifests; Discord Rich Presence over local IPC (opt-in); read-only Steam helpers (build id and update policy
recorded, Steam-library guard); `SteamManager` (thread-safe launch by `steam://run/<id>`, opt-in `steamworks` attachment behind the
`steam` feature); `LibraryService` (scan, list, matrix, tick: the rules behind the UI).
`src-tauri`: Tauri container, least-privilege capabilities, commands `app_info`, `discord_set_enabled`, `list_games`,
`scan_games_command`, `get_game_matrix`, `update_feature_tick`, `launch_game_command` (`src-tauri/src/commands.rs`).
`ui`: Vite + React shell with the Step 1 sidebar (Game Library first; Settings and General Playtime Tracker last), the ten
roadmap modules as labelled placeholders, Team27 theme with contrast-checked token editor, backgrounds, borderless title bar,
splash, Discord Settings panel, capability legend with the "weakest status wins" headline rule, and the Game Library grid: scanned games
side by side, Play for Steam games, and an expandable compatibility matrix per card (eight tick boxes, verdict and capability badges).

## Verified (actually run, 2026-10-27, branch `feature/steam-ipc-library`)
Linux container, Node 22, cargo 1.97. Exact commands and results:
- `cargo fmt --all -- --check` → clean.
- `cargo clippy --workspace --all-targets -- -D warnings` → clean. `cargo clippy -p sul-core --features steam --all-targets -- -D warnings` → clean
  (the real `steamworks` 0.13.1 compiles against the code).
- `cargo test --workspace` → 176 passed, 0 failed (136 `sul-core` unit tests, 13 in `tests/core.rs`, 15 in `tests/discord.rs`,
  6 in `tests/steam.rs`, 6 in `sul-tauri`).
- `npm run ui:typecheck` → clean. `npm run ui:test` → 67 passed, 0 failed. `npm run ui:build` → built.
- Browser check (headless Chromium, built UI, **mock** Tauri backend returning the service's JSON shapes): three cards side by side; scan summary;
  eight checkboxes per matrix; a tick appears only after the saved reply; Play sends `launch_game_command {appId}`; Steam headless is disabled
  and explained on a GOG game; no console errors; no horizontal scroll at 420 px.

## Verified in GitHub Actions (run 40486703642, commit 7501547, windows-latest)
- `core`: fmt, `cargo clippy -p sul-core --all-targets -- -D warnings`, `cargo test -p sul-core`, and the new
  `cargo clippy -p sul-core --features steam --all-targets -- -D warnings` all passed. This is the first Windows compile of the
  `cmd /C start` launcher, the registry running-client check and the `steamworks` attachment. Compiling is not running: none of it has
  touched a real Steam client.
- `ui`: `npm ci`, typecheck, tests, build passed. `tauri-shell`: `npm run ui:build`, clippy and tests for `sul-tauri` passed.
- Per-test counts for this run were not read (job and step results only).

## Verified in GitHub Actions (run 40276520399, commit 04c968f, windows-latest)
- `core`: `cargo fmt --all --check`, `cargo clippy -p sul-core --all-targets -- -D warnings`, `cargo test -p sul-core` all passed.
  This is the first build of the merged code on Windows, including the Windows-only `WindowsRegistry` and `FixedDrives`.
- `ui`: `npm ci`, `npm run ui:typecheck`, `npm test --workspace ui`, `npm run ui:build` all passed.
- `tauri-shell`: `npm run ui:build`, `cargo clippy -p sul-tauri --all-targets -- -D warnings`, `cargo test -p sul-tauri` passed.
- Per-test counts for this run were not read (job and step results only).

## Verified earlier by the owner (before the merge, on the Step 1 archive, Windows)
- Registry scanner archive: 78 tests passed and 38 games were mapped. That run predates this merge and used the scanner on its own.
  Whether all 38 are correct, or whether games were missed, was not checked against a list.

## NOT verified
- The merged build has not been run by the owner on Windows (only CI built and tested it). The scanner's real-machine result for the
  merged code is unconfirmed.
- The desktop app has not been launched from this branch (`npm run dev`). The borderless title-bar buttons, the move from
  `withGlobalTauri` to `@tauri-apps/api`, and the new commands have never run inside a real Tauri window: the browser check above used a mock.
- Launching: `cmd /C start steam://run/<id>` and the registry check for a running Steam have never run against a real Steam. CI compiles the
  Windows code but cannot start Steam. Whether Steam starts a game depends on Steam (ownership, installation, updates).
- The `steamworks` attachment (`SteamManager::connect`) has never connected to a client and nothing in the UI calls it.
- Public player metrics over HTTP (playtime, achievements, owned games): not built.
- `-silent` is not used when Steam must be started, so a closed Steam shows its window.
- Discord against a live Discord client; any of this against a real game install beyond the owner's scan; no game adapter exists.
- Helldivers 2 7.1.1 isolation/offline launch: Not investigated.
- Icons are the Team27 set, not reviewed as final branding. Wallpaper JPEGs are git-ignored (licences unconfirmed; docs in
  `ui/public/wallpapers/README.md`), so the gallery is empty on a fresh clone.
- Persistent Steam credentials: decided against (decision 0003). The Direct3D/Vulkan overlay hooks still conflict with `CLAUDE.md` (no
  anti-cheat bypass); not started, to be settled when that step comes up.

## Branch and safety net
- `step-1.5` was merged into `main` by fast-forward (9ece116 → 04c968f) at the owner's request; both branches are on GitHub.
  The legacy state is commit `9ece116`, tagged locally as `legacy-pre-step-1.5`. Pushing that tag was refused (HTTP 403) from the
  authoring session, so it is not on GitHub; see `docs/decisions/0002-step-1-5-consolidation.md`.

## Wipeout (done 2026-10-26)
Four legacy files were removed after checks showed the build does not use them: `ui/build.mjs` (old esbuild script; it would have
written an inconsistent `dist/`), `ui/src/components.tsx` (replaced by `ui/src/components/`), `ui/src/overview.tsx` (replaced by
`ui/src/views/GameLibrary.tsx` and `weakestStatus` in `ui/src/capabilities.ts`), and `ui/public/icon.svg` (unreferenced placeholder art).
Checks: an import-graph walk from `index.html` and the tests found these were the only unreachable UI files; every Rust file is reachable
through `mod` declarations; the UI bundle had identical content hashes with and without them.
Left in place on purpose: the Windows Store icon sizes in `src-tauri/icons/` (standard `tauri icon` output, not referenced by
`tauri.conf.json`), and the design-reference PNGs in `docs/design-references/` (`CLAUDE.md` and `PROJECT_SPEC.md` point at
`design-references/` at the repo root, which does not exist; the path in those two files is the mismatch, not the images).

## Next action
1. On Windows: `git fetch`, `git checkout feature/steam-ipc-library`, `npm install`, `cargo test --workspace`, `npm run dev`; press
   "Scan for games", open a card's Compatibility, tick a box, and press Play on a Steam game; report what you see.
2. Then the per-game hub (Play, tabs, per-game settings), and the public player-metrics fetch by SteamID64.
3. Helldivers 2 Build Manager stays a separate adapter track.
