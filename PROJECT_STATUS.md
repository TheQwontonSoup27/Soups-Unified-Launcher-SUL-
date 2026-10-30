# Project Status — Soups Unified Launcher (SUL)

**Last updated:** 2026-10-30 (Section 1 fixes merged into `main` at abf8d45; Section 2 slice 1, display density, built on branch `feature/section-2-density-and-redactor`, not yet merged)

## Decisions made (owner approved)
- Stack: Tauri 2 + Rust core + React/TypeScript UI. Windows first. (`docs/decisions/0001-stack.md`)
- Step 1.5 (`docs/decisions/0002-step-1-5-consolidation.md`): the long-term `PROJECT_SPEC.md` governs; app identifier stays
  `app.soups.unifiedlauncher`; the per-game tick-and-compatibility matrix is kept with the five status labels derived from it;
  Discord small-image asset key is supported, a scale slider is not offered.
- Steam (`docs/decisions/0003-steam-attach-launch-and-library-ipc.md`): no credentials are stored, ever; `steamworks` only attaches to the
  running client and only on request; games start with `steam://run/<id>`; public player metrics will come from public HTTP by SteamID64.
- Windows only (owner, 2026-10-29): SUL runs on Windows only. Nothing dedicated to another OS is kept: the macOS icon, the Unix-socket
  Discord transport, the Unix symlink test and the Android test branch were removed. What remains are small `#[cfg(not(windows))]`
  stubs that only refuse ("needs Windows") so that the Rust code still compiles off Windows (this is how it is checked in the Linux
  container); they hold no Linux behaviour. They can be deleted if the workspace should stop compiling anywhere but Windows.
- Section 2 (`docs/decisions/0004-section-2-decisions-from-the-wireframe-review.md`, wireframes in `docs/section-2-wireframes.md`; **only slice 1 (display density) is built**, on a feature branch):
  the Discord rule gets one exception, a webhook destination for the Media & Clipping Engine's "Automated Social/Discord Webhooks" toggle
  (opt-in, off by default, URL treated as a secret); RAM figures are goals, not limits; the Companion Editor Handshake is a placeholder (Not investigated),
  and Minimal mode's auto-transfer does not depend on it; defaults: Compact density, inline "Details →", keep the Discord RPC tab, Playtime Tracker stays a placeholder.
- Webhook URL storage (owner, 2026-10-29): Windows Credential Manager, never the settings file. The log redactor masks webhook URLs first (done, 7b4c812); the toggle itself is not built.
- Section 2 go-ahead (owner, 2026-10-29): first slice only (display density, per-module overrides, inline Details panel, placeholders with no live data).
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
`ui` (branch `feature/section-2-density-and-redactor`, not on `main` yet): a Display density card in Settings > Appearance (global Compact/Detailed, default Compact; lists and clears
overrides) and a Performance Overview on Diagnostics & Performance with a per-module strip (Inherit Global / Always Compact / Always Detailed) and an inline "Details →" panel.
Placeholders only: no figure is shown anywhere, the panel says the sampler is not built. Log redactor: also masks Discord webhook URLs in any text (`sul-core::logging::redact_text`).
`ui` (on `main`): Vite + React shell with the Step 1 sidebar (Game Library first; Settings and General Playtime Tracker last), the ten
roadmap modules as labelled placeholders, Team27 theme with contrast-checked token editor, backgrounds, borderless title bar,
splash, Discord Settings panel, capability legend with the "weakest status wins" headline rule, and the Game Library grid: scanned games
side by side, Play for Steam games, and an expandable compatibility matrix per card (eight tick boxes, verdict and capability badges).

## Verified 2026-10-30 (branch `feature/section-2-density-and-redactor`, commit 4ce3299, on top of `main` abf8d45)
Linux container, Node 22.23, cargo 1.97. Exact commands and results:
- `cargo fmt --all -- --check` → clean. `cargo clippy --workspace --all-targets -- -D warnings` → clean. `cargo clippy -p sul-core -p sul-tauri --all-targets --features steam -- -D warnings` → clean.
- `cargo test --workspace` → 221 passed, 0 failed: 162 `sul-core` unit tests (12 new, for the redactor), 16 in `tests/core.rs` (1 new: the log file never holds a webhook URL),
  17 in `tests/discord.rs`, 6 in `tests/steam.rs`, 20 in `sul-tauri`. `tests/windows_system.rs` compiles to nothing off Windows.
- `npm run typecheck` (in `ui`) → clean. `npm test` → 228 tests, 228 passed, 0 failed (37 new: 21 pure in `tests/density.test.ts`, 16 clicked-through DOM tests in
  `tests/dom-density.test.tsx`, 1 CSS guard in `tests/styles.test.ts`). `npm run build` → built. `npm audit` → 0 vulnerabilities.
- Mutation checks: 15 deliberate breaks of the new code (an override ignored; Inherit storing an override; unknown ids accepted; prototype names read as overrides;
  an unvalidated stored value; a stored override for an unknown module kept; Detailed not opening the panel; a Details button in Detailed; `aria-expanded` stuck; Clear overrides doing nothing; a made-up figure in the
  panel; settings never saved; the provider left out of the app; the Diagnostics route removed; the table-wrap CSS rule removed) were each caught by a failing test.
  One mutation of mine was invalid (a broken closing tag, so it failed to compile, which proves nothing); none of the existing tests clicked through the real app, so a whole-app test was added, and it caught the proper version of that break.
- Stress: `tests/dom-density.test.tsx` 15 of 15 runs passed.
- Real browser (headless Chromium, the built `ui/dist`, no Tauri window): no page errors; compact view is one line with no table; Details opens the panel inside the section,
  Hide details closes it; Always Compact set by pressing Enter on a focused control, and (separately) the first density control reached with Tab; the choice survives a reload; the Settings card lists the override. At 320, 360, 420, 520,
  700 and 1100 px wide there is no horizontal scroll and the table stays inside its card. At 420 px the first version overflowed the content column by 8 px (long
  table cells did not wrap): fixed with `overflow-wrap:anywhere` and smaller padding on narrow screens. The keyboard focus ring (3 px outline) shows on the new controls when reached with Tab.
  The check script is ad hoc and not in the repo. Screenshots were looked at, not compared pixel by pixel.
- Not done: nothing was run inside the real Tauri window; no screen reader was used (roles and labels are asserted in tests only); the measurements themselves do not exist yet.

## Verified in GitHub Actions: Section 2 slice 1 (windows-latest)
- Branch `feature/section-2-density-and-redactor`, commit 4ce3299, run 41022760240: `core`, `ui`, `tauri-shell` all succeeded (step results only; per-test output cannot be downloaded).
  This is the first Windows run of the redactor change and of the density UI. The commit after it only changes documentation.

## Verified in GitHub Actions: merge of the Section 1 fixes into `main`
- `main` abf8d45, run 40990782651 (windows-latest): `core`, `ui`, `tauri-shell` all succeeded (step results only).

## Verified 2026-10-29 (branch `fix/section1-defects-windows-only`, commit 7b92933; merged into `main` as abf8d45)
Linux container, Node 22, cargo 1.97. Exact commands and results:
- `cargo fmt --all --check` → clean. `cargo clippy --workspace --all-targets -- -D warnings` → clean. `cargo clippy -p sul-core --features steam
  --all-targets -- -D warnings` → clean.
- `cargo test --workspace` → 208 passed, 0 failed: 150 `sul-core` unit tests, 15 in `tests/core.rs`, 17 in `tests/discord.rs`, 6 in `tests/steam.rs`,
  20 in `sul-tauri`. (`tests/core.rs` lost its Unix symlink test and gained a Windows junction test, so its count here is one lower and the
  Windows run has one more; `tests/windows_system.rs` and the junction test run only on Windows.)
- `npm run ui:typecheck` → clean. `npm test --workspace ui` → 190 tests, 190 passed, 0 failed, 0 todo. `npm run ui:build` → built. `npm audit` → 0 vulnerabilities.
- Real browser (headless Chromium, the built `ui/dist`, a server that answers a missing file with the app's own page and status 200 as the packaged app
  does, a mock Tauri backend with two games): 14 of 14 checks passed. The script is an ad-hoc file, not in the repo. It measured the tab strip
  in a 500 px high window: 2 px high with the old rule, 41 px with the fix; with all 19 wallpapers listed and no pictures, 0 tiles and 0 broken
  images; with 2 pictures present, exactly 2 tiles; search narrowing, clearing, Escape and jumping to the Game Library from Settings.
- The new tests were mutation-checked: 8 deliberate breaks of the fixes (filter logic, gallery filter, page jump, Escape, match count) were each caught
  by a failing test; the CSS rule has a text guard test only (its effect is what the browser check measured).
- GitHub Actions, windows-latest, run 40954835437 on 7b92933: `core`, `ui`, `tauri-shell` all succeeded (step results only; per-test output
  cannot be downloaded). First Windows run of the junction test and of the Discord transport test. The run before it (40909116597, 6d22edb)
  failed: `NOT_WINDOWS` in `discord.rs` was dead code on Windows (`-D warnings`), which Linux cannot see; fixed in 7b92933.

### Defects fixed in this branch
- Settings tab strip collapsed to 2 px in a short window (a column flex item with `overflow` set may shrink to nothing). Nothing in the content
  column shrinks now (`ui/src/styles.css`).
- Wallpaper gallery showed 19 broken tiles on a fresh clone (manifest committed, pictures git-ignored). It now lists only wallpapers whose thumbnail
  loads, and says where to put pictures when there are none. The thumbnail is loaded as an image rather than fetched, because the dev server and
  the packaged app answer a missing file with status 200.
- The sidebar search box did nothing. It now narrows the Game Library by name, publisher and where the game was found (every word must match);
  typing on another page jumps to the Game Library; Escape clears it. Chosen scope: games only; no other search targets exist yet.
- Flaky tests, present on `main` since the previous merge: the settings DOM tests failed about 1 run in 8 (stress run on `main`: 1 of 20; on
  the branch before the fix: 3 of 20). Tests asserted after one timer tick while the fake IndexedDB needs several turns, and the removal dialog closes only
  after the delete finishes. A failing assertion that held a DOM element made Node try to print the element, so the process ran out of memory
  and looked hung. Added `waitFor` and `absent` helpers, converted 27 such assertions, tested the helpers. After: settings file 60 of 60 runs,
  other DOM files 25 of 25 each, full UI suite 12 of 12.

## Verified 2026-10-28 (merged `main`, commit a19a4ba)
Linux container, Node 22, cargo 1.97. Exact commands and results:
- `cargo fmt --all --check` → clean. `cargo clippy --workspace --all-targets -- -D warnings` → clean.
  `cargo clippy -p sul-core --features steam --all-targets -- -D warnings` → clean.
- `cargo test --workspace` → 208 passed, 0 failed: 150 `sul-core` unit tests, 16 in `tests/core.rs`, 16 in `tests/discord.rs`,
  6 in `tests/steam.rs`, 20 in `sul-tauri` (14 of them send real IPC messages to the commands through Tauri's mock runtime, using the
  real command list). `tests/windows_system.rs` compiles to nothing off Windows, so it ran 0 tests here.
- `npm ci` → 0 vulnerabilities. `npm run ui:typecheck` → clean. `npm test --workspace ui` → 164 tests: 163 passed, 0 failed, 1 `todo`
  (the wallpaper defect below). `npm run ui:build` → built. `npm audit` → 0 vulnerabilities.
- The new UI tests run in happy-dom (a DOM simulation), not in a real browser or a real Tauri window. The new tests were spot-checked by
  breaking the code and confirming a test failed.
- New dev dependencies: `happy-dom` 21.0.4 (older versions have GHSA-834v-8h66-3h79), `@happy-dom/global-registrator`, `fake-indexeddb`.

## Verified in GitHub Actions (windows-latest)
- `main` a19a4ba, run 40804766886: `core`, `ui`, `tauri-shell` all succeeded. The branch before the merge (a366110, run 40804049014) also did.
- This is the first Windows run of `crates/sul-core/tests/windows_system.rs`: it reads real registry keys and the drive list, runs the real
  scan and saves/reopens a library. The runner has no games or Steam, so this checks shape, safety and consistency, not whether the
  scan finds the right games.
- `cargo test -p sul-tauri` ran to completion on Windows, including the IPC tests.
- CI now also builds and links the real app binary (`cargo build -p sul-tauri`) and checks that the Common Controls v6 manifest is inside the
  `.exe`. Nothing launched the app.
- Per-test counts were not read: downloads of raw job logs are refused (HTTP 403). Only step results and timings were read.
- The first run on the branch (40765233357, f06d318) failed, and testing found two real defects, both fixed: (1) an unused import in
  `windows_system.rs` that failed `clippy -D warnings` on Windows; (2) the `sul-tauri` test executable would not start on Windows
  (`0xc0000139`, `STATUS_ENTRYPOINT_NOT_FOUND`) because only the app binary got the Common Controls v6 manifest that `tao`/`wry` need.
  `src-tauri/build.rs` now gives that manifest to the linker, so every executable of the package carries it; the manifest file is
  `src-tauri/windows-app-manifest.xml` (identical to tauri-build's default).
- When a cargo step fails, `.github/scripts/surface-failure.ps1` puts the cause in a run annotation (readable with
  `gh api repos/<owner>/<repo>/check-runs/<job id>/annotations`).

## Known defects
- None open from the 2026-10-28 list: the wallpaper gallery, the search box and the collapsed tab strip are fixed and on `main` (abf8d45).

## Earlier: verified 2026-10-27 (branch `feature/steam-ipc-library`, before the new tests)
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
  `ui/public/wallpapers/README.md`), so a fresh clone has an empty gallery that says where to put pictures (19 are listed, none installed).
- `SystemOpener` (the real `cmd /C start steam://run/<id>`) is deliberately not run in CI: it would open a URL on the runner. The IPC tests
  use a fake opener.
- The Windows adapters have run only on a GitHub runner (no games, no Steam), never on the owner's machine in this session.
- Persistent Steam credentials: decided against (decision 0003). The Direct3D/Vulkan overlay hooks still conflict with `CLAUDE.md` (no
  anti-cheat bypass); not started, to be settled when that step comes up.

## Branch and safety net
- `test/untested-coverage` was merged into `main` with a merge commit (a19a4ba) at the owner's request, after its CI was green. The branch
  is still on GitHub. `feature/steam-ipc-library` was merged earlier (0204541).
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
0. Owner decisions waiting: (a) merge `fix/section1-defects-windows-only` into `main`; (b) go-ahead to build Section 2, starting with the smallest slice
   (Display density setting and the inline Details inspector, no live data); (c) where the webhook URL is stored, needed only before the webhook toggle is built.
1. On Windows: `git checkout main`, `git pull`, `npm install`, `cargo test --workspace` (it runs more tests than the Linux container, because
   `tests/windows_system.rs` only runs on Windows), `npm run dev`; press "Scan for games", open a card's Compatibility, tick a box, and press
   Play on a Steam game; report what you see.
2. Then the per-game hub (Play, tabs, per-game settings), and the public player-metrics fetch by SteamID64.
3. Helldivers 2 Build Manager stays a separate adapter track.
