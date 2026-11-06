# Project Status — Soups Unified Launcher (SUL)

**Last updated:** 2026-11-06 (spec Module 1 and items 2.1 to 2.4 were merged into `main` as 178e84f on the owner's go-ahead, and CI on `main` passed; slices A and B of the screens and Tauri commands for them (library folders and managed builds; mods, profiles, deploy, purge and undo) are built on branch `feature/ui-modules-1-and-2-1-to-2-4`, decision 0008, **not merged**; the rest of this line is the state of `main` on 2026-11-01) (2026-11-01: slice 2, the Security & Trust investigation, the V5.1 alignment, the game-file policy, list sources and the `sul-net` boundary are all on `main` (merge 7338559, Windows CI run 41286114241: `core`, `ui`, `tauri-shell` succeeded))

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
- Security & Trust investigation (owner task, 2026-10-30; `docs/decisions/0005-security-and-trust-investigation.md`, full report `docs/investigations/security-and-trust-2026-10-31.md`): built only what needs no new owner decision, as library code that nothing in the app calls and that no page control can reach;
  the webhook sender and the Windows Credential Manager storage were deliberately not built (blockers in the report); the page stays read-only.
- Owner directions and answers 2026-10-31 (spec V5.1 attached; `docs/decisions/0006-spec-v5-1-directions-checked-against-the-crates.md`): V5.1 builds on v1.0 and is built later; `sul-watchdog` not yet. Round 1: on SUL's own files a known-harmful alteration is quarantined automatically and an unknown one manually; `reqwest` recommended as 0.13 with `rustls-no-provider` and `ring` (not added).
  Round 2 (evening): `sul-net` crate boundary yes (built, empty); priority below normal until measured (2026-11-01: above normal only if it causes no micro stutters in intensive games; nothing measured yet); the owner, hive-mind and Jarvis may add to the known-harmful list (every entry records its source; a list entry never auto-quarantines a file that matches SUL's own recorded hash);
  a game's files edited in a way that risks its integrity or its standing with an anti-cheat: both kinds do the same, SUL does not launch the game (it never touches the files), except games the owner mods themselves, which get a warning the owner can override. All of this is `trust::policy`, a decision function that nothing calls yet; no known-harmful list exists, so nothing is automatic.
- Spec Module 1 and 2.1 to 2.4 (owner, 2026-11-01, sent again 2026-11-03: "implement steps 1 to 2.4 and stop at 3"; `docs/decisions/0007-spec-modules-1-and-2-1-to-2-4.md`): built as library code in `sul-core`, with no Tauri command and no UI; manifest schema: keep the JSON file, forward-only migration; first real game adapter: none yet. **Merged into `main` on 2026-11-04 as 178e84f (owner's go-ahead); CI on `main`, run 41785465232, succeeded (`core`, `ui`, `tauri-shell`).** Owner answers, 2026-11-04: `RaidingForPants/hd2-audio-modder` is the audio tool meant (SUL registers it as an external tool; its wrapper logic may be customised or rewritten for compatible audio infrastructure, since the tool's own pipeline is tailored to Helldivers 2 asset formats); a build with mods deployed may not be moved until a purge (stays locked); the launch warning for an in-place install whose program differs from its baseline is approved as written.
- UI and Tauri commands for Module 1 and 2.1 to 2.4 (owner, 2026-11-04: "Proceed with building the UI components and Tauri IPC commands for Modules 1.1 through 2.4"; plan: `docs/decisions/0008-ui-and-ipc-for-modules-1-and-2-1-to-2-4.md`): built in slices A to D, each tested and pushed on its own, on branch `feature/ui-modules-1-and-2-1-to-2-4`. Folders are typed in as paths (no folder picker: that needs the dialog plugin and a new permission, which is an owner decision). Nothing merges into `main` without the owner's go-ahead. See the sections "UI and Tauri commands, slice A" and "slice B" below for what exists.
- References (owner, 2026-11-05: eight links "that will absolutely help", plus "store every reference and link" in one place and, if possible, in a separate repository named `references`): `docs/references.md` is that one place. It lists the eight new links (each opened once as a front page through the fetch tool; **none cloned, built or run, all Not investigated**; which module each should inform was not said), the reference projects with their licences, every page read for an investigation, the libraries the decision records checked, and the services the spec names. `crates/sul-core/tests/references.rs` fails if any document in the repository carries a web address the list lacks (it does not check that an address still works). **The separate `references` repository was not created:** GitHub refused the call from this session (HTTP 403: sessions are bound to their configured repositories), so the owner would create it (steps given in the session reply). No code is copied from any listed project; SUL's own licence is still undecided.
- Still open: licence, social/networking, Medal access level, first full game adapter (none yet), where a
  Steam Web API key would live (it is a secret). The manifest migration policy was settled 2026-11-01 (0007).

## What exists
`crates/sul-core` (Rust): registry game scanner (Steam libraries on every drive, GOG, Uninstall keys, drive probing) behind
`RegistryAccess` / `FileSystem` seams; per-game compatibility matrix with the five capability labels derived from it; persisted
library index; storage isolator; baseline tracking and version self-updater with rollback; portable mode; JSON templates;
capability model with the evidence rule; safe paths; protected roots, atomic write, isolated copy; recovery journal; redacting JSONL
logger; SHA-256 build manifests; Discord Rich Presence over local IPC (opt-in); read-only Steam helpers (build id and update policy
recorded, Steam-library guard); `SteamManager` (thread-safe launch by `steam://run/<id>`, opt-in `steamworks` attachment behind the
`steam` feature); `LibraryService` (scan, list, matrix, tick: the rules behind the UI).
`src-tauri`: Tauri container, least-privilege capabilities, commands `app_info`, `discord_set_enabled`, `list_games`,
`scan_games_command`, `get_game_matrix`, `update_feature_tick`, `launch_game_command` (`src-tauri/src/commands.rs`); on the feature branch also the nine `manager_*` commands (slice A) and the eleven `mods_*` commands (slice B).
`ui` (Section 2 slices, on `main` since 6902aeb unless stated): a Display density card in Settings > Appearance (global Compact/Detailed, default Compact; lists and clears
overrides) and a Performance Overview on Diagnostics & Performance with a per-module strip (Inherit Global / Always Compact / Always Detailed) and an inline "Details →" panel.
Placeholders only: no figure is shown anywhere, the panel says the sampler is not built. Log redactor: also masks Discord webhook URLs in any text (`sul-core::logging::redact_text`).
`ui` (slice 2, on `main` since ce7d44d): Settings > Security & Trust is a read-only report instead of a placeholder: a headline that is the weakest label on the page
(Not investigated), two design rules (Credentials, Data sharing) each saying how they were checked and what was not, what could send data off this PC (Discord Rich Presence, Experimental, with whether it is on right now;
the webhook toggle, Not investigated, not built), and three protections (package scanning, quarantine, integrity checks) that are not built. No controls. A tripwire test fails if network code is added to SUL's own source.
`ui` (investigation branch, on top of slice 2): the same page now labels those three protections Experimental, says first that they are tested code the app does not call, and states limits; the webhook row stays Not investigated, so the headline stays Not investigated.
`crates/sul-core::trust` (on `main` since ce7d44d; **library code only, no Tauri command and no UI calls it**): `integrity` (a build folder against its saved manifest, read-only; confirmed violations vs allowed changes vs suspicious signals),
`package` (names and sizes of a mod folder or .zip central directory; confirmed structural problems vs suspicious signals; nothing decompressed), `quarantine` (journaled, verified, reversible move of one file; refuses original game installs; no purge), `webhook` (strict check of a Discord webhook address; the secret part never printed).
`crates/sul-core/tests/write_sites.rs` and `.github/scripts/check-network-crates.mjs` are tripwires: every Rust place that changes a file is listed with a reason, and the Windows dependency graph is checked for network crates (the latter runs in CI).
`ui` (on `main`): Vite + React shell with the Step 1 sidebar (Game Library first; Settings and General Playtime Tracker last), the ten
roadmap modules as labelled placeholders, Team27 theme with contrast-checked token editor, backgrounds, borderless title bar,
splash, Discord Settings panel, capability legend with the "weakest status wins" headline rule, and the Game Library grid: scanned games
side by side, Play for Steam games, and an expandable compatibility matrix per card (eight tick boxes, verdict and capability badges).

## UI and Tauri commands, slice A: library folders and managed builds (branch `feature/ui-modules-1-and-2-1-to-2-4`, not merged)
Built: `sul-core::manager::ManagerService` (one lock over library roots, protected roots, journal, disk reader and build registry; returns plain views); nine Tauri commands (`manager_view`, `manager_add_root`, `manager_relink_root`, `manager_forget_root`, `manager_import_build`, `manager_register_in_place`, `manager_verify_build`, `manager_activate_build`, `manager_rollback_slot`), run on the blocking thread pool; `ui/src/lib/manager.ts` and `ui/src/components/ManagedBuildsPanel.tsx`, mounted under the games on the Game Library page: a Folders panel (list with free space and problems, add, point at a new place, stop listing) and a Builds panel (per game, Check files, Make active, Roll back, "Add a build" with copy-into-a-folder or record-in-place), a warning for operations that never finished, and a plain message in a browser with no backend. Copying, recording in place and stopping to list a folder each ask first; the core refuses unsafe requests whatever the page sends. The original install is protected from the moment the library lists it (at start-up and after every scan), and an import or record always takes the game from the library, never a typed source path; a game the last scan did not find is refused. The panel is labelled Experimental and says no adapter for a real game exists. Not in slice A: repair and migrate buttons, the Mods tab (slice B), the Builder and Tools tabs (C), Play for a managed build, the catalogue and the game detail view (D).
Verified 2026-11-04 in the Linux container (cargo 1.97.0, Node 22): `cargo fmt --all --check` exit 0; `cargo clippy --workspace --all-targets --locked -- -D warnings` exit 0; `cargo clippy -p sul-core --features steam --all-targets --locked -- -D warnings` exit 0; `cargo test --workspace --locked` 536 passed, 0 failed (453 in sul-core's library tests including 14 for the manager service; 30 in sul-tauri, of which 10 are new IPC tests that send real messages to the commands in Tauri's mock runtime); `node .github/scripts/check-network-crates.mjs` exit 0 (31 / 218 / 221 crates); in `ui`: `npm run typecheck` exit 0, `npm test` 318 passed, 0 failed, `npm run build` exit 0. Deliberate-bug checks (one production line broken at a time, then the tests): 13 on the manager service, all caught; 12 on the commands, of which 4 were first not caught (relink and rollback arguments, protection after a scan, the variant of a record-in-place), tests were added and all were then caught, and one mutation did not compile; 54 on the UI controller and panel in the first run, 51 caught; the 3 that were not (a blank path for a folder move, the size display at exactly 1000 bytes, the Experimental label) got tests, 2 more badge breaks were added, and all of these are now caught except 1 left alive on purpose (the word "Experimental:" at the start of a sentence beside the badge that already says it). One display bug was found this way and fixed (999,999 bytes showed as "1000 KB"). The page was also loaded in a headless browser against a pretend backend at 1280 and 960 pixels wide: no horizontal overflow, and the layout was looked at.
The `write_sites.rs` tripwire flagged the new IPC test file (it makes made-up folders in a temporary folder); it was added to the list with a reason, not exempted. Windows CI (`windows-latest`) on commit 8c35f7b, run 41941940666: `core`, `ui` and `tauri-shell` succeeded. Not verified: anything in a real Tauri window, with a real game, or on a real disk; copying a large game (no progress bar and no cancel exist, and the page says so).

## UI and Tauri commands, slice B: mods, profiles, deploy, purge and undo (same branch, not merged)
Built: `sul-core::manager::modding` (the mod half of `ManagerService`: import a mod folder as a verified copy, check a stored mod, make and edit profiles, preview and run a deploy, preview and run a purge, undo the latest purge, Purge & Deploy; every answer is a plain, serialisable view); `AdapterRegistry` now holds `GameAdapter + Send` adapters so the service can own it; eleven Tauri commands (`mods_view`, `mods_import`, `mods_verify`, `mods_create_profile`, `mods_edit_profile`, `mods_plan_deploy`, `mods_deploy`, `mods_plan_purge`, `mods_purge`, `mods_undo_purge`, `mods_purge_and_deploy`); `ui/src/lib/mods.ts` (backend interface, answer-shape checks, a controller with no React in it), `ui/src/components/ModsPanel.tsx` and `ui/src/views/ModsView.tsx` (the Mods & Mod Builder page, which also says plainly what is not built). The page lists mods, profiles (load order, enabled switch, version, settings, notes, rename, move to another build) and what is deployed per build, with history. Deploy, purge, undo and Purge & Deploy each show a preview of what they would do and ask first; Cancel and Escape send nothing; a blocked deploy cannot be confirmed; the core plans again whatever the page shows, and refuses a stale or unsafe request. A game with no adapter gets no preview and no deploy (a purge and an undo need no adapter), so **in the app every deploy is refused today: no adapter for a real game exists**, and the page says so. Moving a profile to another build is refused while it is deployed; the notes a person writes about a mod (source, licence, game versions) are labelled as their words and not checked. Labelled Experimental.
Not in slice B: the builder, patches and external-tools pages (slice C), launch/runtime, catalogue and game detail (slice D), conflict analysis, a folder picker, progress or cancel for a large copy, a way to pick which files of a mod to deploy.
Verified 2026-11-06 in the Linux container (cargo 1.97.0, Node 22): `cargo fmt --all --check` exit 0; `cargo clippy --workspace --all-targets --locked -- -D warnings` exit 0; `cargo clippy -p sul-core --features steam --all-targets --locked -- -D warnings` exit 0; `cargo test --workspace --locked` 566 passed, 0 failed (473 in sul-core's library tests, 19 of them the mod service's; 35 in sul-tauri, 5 of them new IPC tests that send real messages to the mods commands in Tauri's mock runtime, using a small made-up adapter); `node .github/scripts/check-network-crates.mjs` exit 0 (31 / 218 / 221 crates); in `ui`: `npm run typecheck` exit 0, `npm test` 384 passed, 0 failed (66 new: 22 for the controller, 44 clicked through with a fake backend, including "nothing is sent before the question is answered" and the exact command names and arguments sent to the real `invoke`), `npm run build` exit 0. Deliberate-bug checks (one production line broken at a time, then the tests): 5 on the mod service, of which 2 were not caught at first (a profile moved to another game's build; a mod layout that turns invalid after registration), a test was added for each and both are now caught; 4 on the commands, all caught (one first attempt did not compile and was redone); 15 on the controller and page, 12 caught at once, 1 pattern had to be corrected and was then caught, 1 was not caught (the sentence saying what a purge may and may not do was not pinned), a test now pins it and it is caught, plus a 15th added for the "needs no adapter" sentence, caught. 24 breaks in all, chosen by hand: this is not every rule in the code. Bugs found this way and fixed in the tests only; no production bug was found. The IPC field-list pin failed once after `slot` was added to a build's mod state; the pin was updated (the page uses `slot`). The built page was also loaded in headless Chromium against a pretend backend at 1280 and 960 pixels wide: no horizontal overflow and no console errors at either width; the 960 screenshot was looked at, the 1280 one was only measured.
Windows CI (`windows-latest`) on commit aea949a, run 42166336099: `core`, `ui` and `tauri-shell` succeeded (this runs the same made-up-folder tests on Windows, plus the Windows-only system tests; it does not run a Tauri window or a real game). Not verified: anything in a real Tauri window, with a real game or mod, or on a real disk; deploying at all (no real adapter, so only made-up game folders were ever deployed to, in tests); a large mod (no progress, no cancel, and the page says so).

## Spec Module 1 and 2.1 to 2.4 (merged into `main` 2026-11-04 as 178e84f)
`crates/sul-core` gained, as library code that nothing in the app calls: `adapters` and `catalogue` (spec 1.1), `builds` and manifest schema 2 with forward-only migration (1.2, 1.3), `process` and `runtime` (1.4), `roots` (1.5), `mods` (2.1: store, profiles, deploy, Purge, Purge & Deploy, undo of the latest purge), `builder` (2.2 recipes and provenance, 2.3 reversible byte patches and an external-tool registry, 2.4 the records only). Everything is tested on made-up folders and files; no real game, mod or tool was involved. Not built: spec 1.2 acquisition (no download code; the SteamCMD look is one page, `docs/investigations/steamcmd-acquisition-2026-11-03.md`), 1.6 (deferred by the spec), any real game adapter. (At merge time nothing in the app called any of this; slice A above adds screens and commands for folders and builds only.) Details, calls made and the independent review: decision 0007; audio-tool research: `docs/investigations/audio-modding-2026-11-01.md`.

## Verified 2026-11-03 (branch `feature/spec-modules-1-and-2-1-to-2-4`, on top of `main` 7338559 and the docs branch db3a33a)
Linux container, Node 22.23.4, cargo 1.97.0. Exact commands and results (one script, `fmt`, both clippy runs, `cargo test --workspace --locked`, the network check):
- `cargo fmt --all --check` → exit 0. `cargo clippy --workspace --all-targets --locked -- -D warnings` → exit 0. `cargo clippy -p sul-core --features steam --all-targets --locked -- -D warnings` → exit 0.
- `cargo test --workspace --locked` → 508 passed, 0 failed (332 on `main` before this branch).
- `node .github/scripts/check-network-crates.mjs` → exit 0 (31 / 218 / 221 crates, no network crate, no tokio net feature, no Windows networking feature).
- `npm run typecheck` (in `ui`) → exit 0. `npm test` → 275 passed, 0 failed, 0 todo. The branch changes no UI file. `npm run build` and `npm audit` were not re-run for this branch.
- Mutation checks, 2026-11-03 (a deliberate break of production code, then the tests): 17 breaks of the fixes that came out of the independent review (copy put over an existing file, stale temp name, failed copy leaving its temp file, build moved with mods deployed, unreadable mod records accepted, undo writing no ledger first, undo not marked deployed, landing-place check never refusing, purge not re-hashing before a move, manifest matched ignoring case, `..` and `./` not handled, a folder removed where a file was, undo not checking the put-back original or the removed copy) were each caught by a test; 4 breaks of the new deletion tripwire (fsops deleting something other than its temp file, another file deleting, fsops removing a folder) were each caught. An earlier campaign by separate agents in separate copies of the repository closed other gaps (for example the check that refuses a manifest newer than this build); I did not re-run it and its full results are not in the repository.
- A first run of the full script this session failed two `write_sites` tests because `fsops.rs` now removes its own temp file and makes a hard link; the tripwire was updated deliberately (reason written beside the entry) and tightened, not loosened blindly.
- Windows CI: see "Verified in GitHub Actions: spec Module 1 and 2.1 to 2.4" below. Nothing was run in a real Tauri window, against a real game, Steam client, mod or tool.

## Verified in GitHub Actions: spec Module 1 and 2.1 to 2.4 (windows-latest)
- Branch `feature/spec-modules-1-and-2-1-to-2-4`. Run 41743845275 on 7009725 (the code and the first docs): `core`, `ui`, `tauri-shell` all succeeded. Run 41744013116 on b7feb56 (adds the SteamCMD note and doc pointers): all three succeeded. Step results only; per-test output was not read. The commit that records these two results is docs-only and was not given its own CI run. Merged into `main` on 2026-11-04 as 178e84f; CI on `main` (run 41785465232, push) succeeded: `ui`, `tauri-shell`, `core` (step results only).

## Verified in GitHub Actions: round 2 of the V5.1 answers (windows-latest)
- Branch `feature/spec-v5-1-owner-answers`, commit b65791a, run 41257429049: `core`, `ui`, `tauri-shell` all succeeded (step results only). Merged into `main` on 2026-11-01 (7338559). First Windows build of the `sul-net` crate.

## Verified 2026-10-31 evening (branch `feature/spec-v5-1-owner-answers`, on top of `main` 95ba76b)
Linux container, Node 22.23, cargo 1.97. Exact commands and results:
- `cargo fmt --all -- --check` → clean. `cargo clippy --workspace --all-targets -- -D warnings` → clean. `cargo clippy -p sul-core -p sul-tauri --all-targets --features steam -- -D warnings` → clean. `cargo metadata --locked --offline` → succeeds (the lock file matches the manifests).
- `cargo test --workspace` → 332 passed, 0 failed (325 on `main`, plus 5 in `trust::policy`, now 14, plus 2 in `tests/write_sites.rs`, now 13). `npm run typecheck` (in `ui`) → exit 0. `npm test` → 275 passed, 0 failed. `npm run build` → built. `npm audit` → 0 vulnerabilities.
  `node .github/scripts/check-network-crates.mjs` → exit 0 (31 / 218 / 221 crates, no network crate).
- Mutation checks: 20 deliberate breaks of `trust::policy` (the game-file block, the owner-modded override, the two risk kinds, a suspicious signal blocking a game, the override leaking to other tiers, the baseline guard, hash case, list-entry validation and duplicates, source and risk kind dropped from decisions, the warning losing its wording, the automatic and block tests widened, the tier order and prefix logic) and 7 of the `sul-net` boundary tripwire, all caught.
  Three of my own mutations were invalid at first (two did not compile, one had no edit in it); each was redone correctly before counting.
- Not done: Windows CI for this branch (see below once it exists). Nothing calls `trust::policy`, no Play button uses it, no known-harmful list exists, the per-game "I mod this myself" switch is not built. `sul-net` has no code. Nothing was run in the real Tauri window.

## Verified 2026-10-31 (branch `feature/spec-v5-1-alignment`, on top of `main` e68d2a8)
Linux container, Node 22.23, cargo 1.97. Exact commands and results:
- `cargo fmt --all -- --check` → clean. `cargo clippy --workspace --all-targets -- -D warnings` → clean. `cargo clippy -p sul-core -p sul-tauri --all-targets --features steam -- -D warnings` → clean.
- `cargo test --workspace` → 325 passed, 0 failed (316 before, plus 9 new in `trust::policy`). `npm run typecheck` (in `ui`) → exit 0. `npm test` → 275 passed, 0 failed. `npm run build` → built. `node .github/scripts/check-network-crates.mjs` → exit 0 (31 / 218 / 221 crates, no network crate).
- Mutation checks on `trust::policy`: 12 deliberate breaks (automatic for every own file; automatic for an unknown change; automatic outside SUL's folders; the list ignored; a vendor file quarantined; the folder check order swapped; folder check by text prefix; list entries too long; non-hex accepted; case-sensitive list; the automatic test widened; a reason saying "safe"), each caught by a failing test.
- Dependency experiment for `reqwest`, and the check of V5.1's dependency lines: scratch crates outside the repo, resolved against the registry (`cargo tree --target x86_64-pc-windows-msvc`, `cargo info`). **Nothing was compiled**, so whether the recommended variant builds on Windows (NASM for `ring`) is unconfirmed.
- Not done: Windows CI for this branch (see the section below once it exists); nothing calls `trust::policy` and no known-harmful list exists; nothing was run in the real Tauri window.

## Verified in GitHub Actions: spec V5.1 alignment branch (windows-latest)
- Branch `feature/spec-v5-1-alignment`, commit 73c7b04, run 41235353984: `core`, `ui`, `tauri-shell` all succeeded (step results only). Merged into `main` as 95ba76b; run 41238632634 on the merge commit also passed all three jobs.

## Verified 2026-10-31 (branch `feature/security-trust-investigation`, on top of `feature/section-2-security-trust-pane` 55e942a)
Linux container, Node 22.23, cargo 1.97. Exact commands and results (full detail in `docs/investigations/security-and-trust-2026-10-31.md`):
- `cargo fmt --all -- --check` → clean. `cargo clippy --workspace --all-targets -- -D warnings` → clean. `cargo clippy -p sul-core --features steam --all-targets -- -D warnings` → clean.
- `cargo test --workspace` → 316 passed, 0 failed: 246 `sul-core` unit tests (84 in `trust::`: integrity 16, package scanning 30, quarantine 28, webhook 7, shared 3), 16 `tests/core.rs`, 17 `tests/discord.rs`, 6 `tests/steam.rs`,
  11 `tests/write_sites.rs` (new), 20 `sul-tauri`. `tests/windows_system.rs` and the `#[cfg(windows)]` junction/link tests compile to nothing off Windows.
- `npm run typecheck` (in `ui`) → clean. `npm test` → 275 tests, 275 passed, 0 failed. `npm run build` → built.
- `node .github/scripts/check-network-crates.mjs` → exit 0: Windows target `x86_64-pc-windows-msvc`, normal dependencies, `sul-core` 31 crates, `sul-tauri` 218, `sul-tauri` with `steam` 221; no network crate, no tokio `net` feature, no Windows networking feature.
- Mutation checks (deliberate one-line breaks, each must fail a test): in the work itself 27 of the Rust trust code, 6 of the write-site tripwire and 12 of the page, all caught after three gaps were closed. Re-run before commit with a fresh sample: 10 of the trust code, 5 of the tripwire, 10 of the page.
  One survived: the compression-ratio limit of package scanning was untested at its boundary; a boundary test was added and four breaks of that limit are now caught. A mutation that added a crate to a manifest made cargo rewrite `Cargo.lock`; it was restored from git.
- Real browser (headless Chromium on the built `ui/dist`, no Tauri window), widths 320, 360, 420, 700, 900, 1100 px, every view and every Settings tab (72 measurements): no horizontal overflow, no page error, no console error, no `unhandledrejection`,
  no request to another origin; Security & Trust: four cards inside the content column, five rows plus a headline, no button, field, link or tab stop, no mention of `reqwest`. The scripts are ad hoc and not in the repo.
- **Not done / unconfirmed** (list with reasons in the report): network traffic was never captured; WebView2's own network behaviour; the inside of Tauri, tokio, wry and the Windows crates (searched as text only); the webhook sender and Credential Manager storage (not built);
  the integrity, scanning and quarantine code has never run on real mods or a real install and is not reachable from the app; the `#[cfg(windows)]` tests ran only in Windows CI; nothing was run in the real Tauri window; no screen reader.

## Verified in GitHub Actions: merge of slice 2 and the investigation into `main` (windows-latest)
- `main` ce7d44d, run 41216314604: `core`, `ui`, `tauri-shell` all succeeded (step results only). Local re-test of the merged `main` before pushing (the merge tree is identical to the tested branch tip af104d5): `cargo fmt --check` clean, clippy `-D warnings` clean (default and `steam`),
  `cargo test --workspace` 316 passed 0 failed, `npm run typecheck` exit 0, `npm test` 275 passed 0 failed, `npm run build` built, `npm audit` 0 vulnerabilities, `check-network-crates.mjs` exit 0. Section 2 re-run on the branch the same day: 12 redactor tests, the density, Security & Trust and tripwire UI tests (106) 10 of 10 runs passed,
  and headless Chromium on the built UI (density controls, Details panel, reload persistence, Security & Trust at six widths) showed no overflow, no error and no outside request. The real Tauri window was not run.

## Verified in GitHub Actions: Security & Trust investigation branch (windows-latest)
- Branch `feature/security-trust-investigation`, commit 50840b6, run 41210242339: `core`, `ui`, `tauri-shell` all succeeded (step results only; per-test output cannot be downloaded, so the `#[cfg(windows)]` junction and link tests are known to have run
  only through the `cargo test` step passing). First Windows run of the `trust` modules, of `tests/write_sites.rs`, and of `check-network-crates.mjs`: on the runner it reported the same lists as the Linux container
  (`sul-core` 31 crates, `sul-tauri` 218, `sul-tauri` with `steam` 221; no network crate, no tokio `net` feature, no Windows networking feature). CI also warns that Node.js 20 actions are deprecated (`actions/checkout@v4`, `actions/setup-node@v4`); not addressed.

## Verified 2026-10-30 (branch `feature/section-2-security-trust-pane`, commit d9ec2ec, on top of `main` 6902aeb)
Linux container, Node 22.23, cargo 1.97. Exact commands and results:
- Rust: not changed in this slice. On the merged `main` 6902aeb (before this branch) `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo clippy -p sul-core -p sul-tauri --all-targets --features steam -- -D warnings` were clean, and `cargo test --workspace` → 221 passed, 0 failed.
- `npm run typecheck` (in `ui`) → clean. `npm test` → 258 tests, 258 passed, 0 failed (30 new: 19 pure in `tests/security.test.ts` and `tests/no-network.test.ts`, 11 clicked-through in
  `tests/dom-security.test.tsx`; one existing Settings test was updated because the placeholder text is gone). `npm run build` → built. `npm audit` → 0 vulnerabilities.
- Mutation checks: 15 deliberate breaks, each caught by a failing test: Rich Presence labelled Supported and tested; its limit removed; the headline taking the first row instead of the weakest;
  quarantine claiming to work; integrity checks merging suspicious with confirmed; the data-sharing text dropping what was not checked; the credentials text promising safety; the "right now" fact stuck on off; the
  rule tag removed; a button added to the read-only page; the tab not showing the panel; and four network tripwires (`std::net` added to `sul-core`, `reqwest` declared in `Cargo.toml`, a second `fetch` in the window code,
  an address written in a comment).
- Stress: `tests/dom-security.test.tsx` and `tests/dom-settings.test.tsx` together, 15 of 15 runs passed.
- Real browser (headless Chromium, built `ui/dist`, no Tauri window): at 320, 420, 700 and 1100 px wide no page error, no horizontal scroll, all four cards inside the content column, six labels (one headline, five rows) matching the
  tests, no button, field or link in the panel; "Right now" read off, then on after the sidebar presence switch was clicked; no request was made to any other origin during the run. The script is ad hoc and not in the repo.
- Claims the page makes and how each was checked (this was done by reading, 2026-10-30): the settings the window keeps (searched the window code for storage use); no network API in SUL's own Rust code and no HTTP client declared (searched; **correction 2026-10-31: the part about `cargo tree` that said the Tauri framework brings in `reqwest`/`hyper` was wrong for the Windows build.** It came from `cargo tree --target all`, which merges every platform;
  Tauri declares `reqwest` only for Android and Apple mobile targets, and for `x86_64-pc-windows-msvc` there is no `reqwest`, `hyper`, `mio` or `socket2`; see the investigation report); the window's only request is for the wallpaper list file the app ships.
  **Not done:** network traffic was never captured; what the Tauri framework does with its HTTP client was not audited; nothing was run inside the real Tauri window; no screen reader was used.

## Verified in GitHub Actions: Section 2 slice 2 (windows-latest)
- Branch `feature/section-2-security-trust-pane`, commit d9ec2ec, run 41025191144: `core`, `ui`, `tauri-shell` all succeeded (step results only). The commit after it only changes documentation.

## Verified in GitHub Actions: merge of slice 1 into `main` (windows-latest)
- `main` 6902aeb, run 41024434409: `core`, `ui`, `tauri-shell` all succeeded (step results only). Local re-test of the merged `main` before pushing: 221 Rust tests and 228 UI tests passed.

## Verified 2026-10-30 (branch `feature/section-2-density-and-redactor`, commit 4ce3299, on top of `main` abf8d45; merged into `main` as 6902aeb)
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
- Spec Module 1 and 2.1 to 2.4 (merged): never run against a real game, mod, Steam client or tool. The hard-link fallback for FAT/exFAT volumes, Windows 8.3 short names (`ANTICH~1`) and a power cut on a real disk were not tested. Only folders and builds (slice A, not merged) have a screen; mods, the builder, tools, launch and the catalogue still have none. See decisions 0007 and 0008.
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
0. Now (2026-11-06): slices C (builder, patches and tools) and D (launch, catalogue, game detail) of decision 0008; slice B (mods) is built and passed Windows CI. Owner decisions still waiting: (a) go-ahead to merge `feature/ui-modules-1-and-2-1-to-2-4` into `main` when the owner wants it; (b) the spec 1.2 acquisition investigation (SteamCMD/depot) and spec 2.5 onward, not started; (c) the open items in decision 0007 (first real adapter and its risk-kind decision, connecting `trust::policy` to launch); (d) whether to add the dialog plugin (and its permission) so folders can be picked instead of typed.
   (The older items below are kept as history; the ones about `fix/section1-defects-windows-only`, Section 2 and the webhook URL were settled and merged earlier.)
1. On Windows: `git checkout main`, `git pull`, `npm install`, `cargo test --workspace` (it runs more tests than the Linux container, because
   `tests/windows_system.rs` only runs on Windows), `npm run dev`; press "Scan for games", open a card's Compatibility, tick a box, and press
   Play on a Steam game; report what you see.
2. Then the per-game hub (Play, tabs, per-game settings), and the public player-metrics fetch by SteamID64.
3. Helldivers 2 Build Manager stays a separate adapter track.
