# Roadmap — Soups Unified Launcher (SUL)

Staged priorities, not promises. Update status only with evidence.

> **2026-11-09: a ground-up rebuild from a new spec is starting; the stages below are the old plan and may be superseded. Read `HANDOFF.md` first.**

1. **Foundation** — DONE in code, built and tested in Windows CI; not yet launched on a real machine: monorepo, shell, theme,
   registry scanner, compatibility matrix, library index, storage isolator, updater, portability, templates, safety core, Discord
   presence (Step 1 and Step 1.5). Redundant legacy files removed; see `PROJECT_STATUS.md`.
   Game Library grid with the tick-and-compatibility matrix and Steam Play is built (needs a run on Windows).
   Test coverage was extended on 2026-10-28 (Section 1 UI, the Tauri commands over IPC, the Windows registry and drive adapters); CI on
   `main` passes on Windows. The defects it found (wallpaper gallery, search box, tab strip) are fixed and merged into `main` (abf8d45, Windows CI passed), with the Windows-only cleanup. See `PROJECT_STATUS.md`.
   **Section 2 (slices 1 and 2 merged into `main`, the latter at ce7d44d; the rest waits for the owner's go-ahead):** decisions in `docs/decisions/0004-*`,
   drawings in `docs/section-2-wireframes.md`. Done: the log redactor masks Discord webhook URLs (prerequisite for the webhook toggle); the Display density setting with the
   per-module override and the inline Details panel (placeholders, no live data). The read-only Security & Trust pane (slice 2, a report with no controls). Next, smallest first: the Media & Clipping Engine pane as a
   shell with every feature Not investigated; the Rust telemetry sampler; last, the webhook toggle with the URL in Windows Credential Manager (owner's decision).
   **Security & Trust investigation (merged into `main` at ce7d44d):** done as tested library code, not used by the app: integrity classifier (confirmed vs suspicious), static package scan, reversible quarantine, webhook address check,
   plus tripwires for every Rust file write and for network crates in the Windows build. Not done: the webhook sender and Windows Credential Manager storage (owner decisions on payload, client and dependency); wiring any of this into the app (what a person is asked, where the quarantine lives);
   capturing real network traffic; auditing WebView2. See `docs/investigations/security-and-trust-2026-10-31.md`.
   **Round 2 (2026-10-31 evening, branch `feature/spec-v5-1-owner-answers`, merged into `main` 2026-11-01 as 7338559):** the rule now covers a game's files (a confirmed risky edit blocks launching through SUL; a game the owner mods themselves gets a warning they can override; integrity and anti-cheat risk do the same), known-harmful entries carry their source (owner, hive-mind, Jarvis), and `sul-net` exists as the empty home for all network code. Nothing calls any of it yet.
   **Spec V5.1 (owner, 2026-10-31): V5.1 builds on v1.0 and is built later.** The current build was checked against it (`docs/decisions/0006-*`): the Tier 1/Tier 2 rule for acting without asking is built (`trust::policy`); `sul-watchdog` waits (owner: not yet);
   V5.1's dependency list has lines that do not resolve (`mmap2`, `watch_dir` 0.2, `ort` 2.0) or are Linux only (`shmem-ipc`), corrected in the record; the `reqwest` choice is recommended there (0.13 with `rustls-no-provider` and `ring`) and not added.
   **Other next slices:** the per-game hub, then public player metrics by SteamID64.
2. Build management: import/verify, original/optimized/experimental slots, rollback; HD2 7.1.1 isolation research (separate adapter track).
   **Library code built 2026-11-03 (merged into `main` 2026-11-04 as 178e84f; decision 0007):** adapter interface and catalogue, import and register-in-place, slots, rollback, repair, migrate, manifest schema 2, library roots, launch and runtime manager. Not done: any UI or Tauri command, a real game adapter, acquisition (no download code; a first look at one SteamCMD page, `docs/investigations/steamcmd-acquisition-2026-11-03.md`), HD2 7.1.1 isolation.
   **Screens and commands (decision 0008, branch `feature/ui-modules-1-and-2-1-to-2-4`, merged into `main` 2026-11-09 as 7299ef2, CI on `main` run 42700201764 passed):** slice A built 2026-11-04: library folders and managed builds on the Game Library page, with nine `manager_*` commands. Slice B built 2026-11-06: the Mods & Mod Builder page (mods, profiles, deploy, purge, undo) with eleven `mods_*` commands; Deploy is refused for every real game until an adapter exists. Slice C1 built 2026-11-07: recipes and patches on the same page (the builder panel, seven `builder_*` commands); previews and runs are refused for every real game until an adapter exists. Slice C2 built 2026-11-07: the external-tools panel on the same page (record a program, notice a changed file, preview then start, follow it, keep a record), with eight `tools_*` commands; in the app, Start really starts the recorded program (no real tool has ever been run) and SUL cannot cancel or confine it. Slice D1 built 2026-11-08: the Play panel on the Game Library page (every build with its state, Play held with the reason, running state followed on a timer, launch history) with three `launch_*` commands; Play is refused for every real game until an adapter exists, the game-file risk policy is not connected to launch and SUL cannot stop a started program. Slice D2 built 2026-11-09: categories you give a game, a category filter row (combined with the sidebar search) and a Details panel inside each game card (builds, adapter, capability label, running state, notes, categories, artwork reference) with four commands (`catalogue_view`, `game_detail`, `catalogue_set_categories`, `catalogue_set_artwork`); artwork is only a remembered path, and the image is not shown until the owner grants the Tauri permission that would allow it. Slices A to D are all built and merged into `main`; spec 2.5 onward and Module 3 are not started and wait for the owner.
3. Mod management: profiles, ownership-aware deploy/purge, dry-run, conflicts, backup/recovery.
   **Library code built 2026-11-03 (merged with the above):** mod store, profiles with load order, deploy with an ownership ledger, dry run, Purge, Purge & Deploy, undo of the latest purge, recovery folders. Not done: conflict analysis (spec 2.6), any UI, use on a real game.
4. Mod Builder: one real adapter path, then audio, Bink/RAD Video, patch workflow, Blender.
   **Library code built 2026-11-03 (merged with the above):** recipes with provenance, reversible byte patches, an external-tool registry; audio tools are records and a research note only (`docs/investigations/audio-modding-2026-11-01.md`). Not done: any real format tool, audio, Bink/RAD Video, Blender (2.5, not started on purpose), experimental build construction (2.7).
5. Modpack export/sharing, scanning/quarantine, provenance and licensing.
6. Diagnostics, compatibility matrix, measured profiling.
7. Social/networking foundations and supported Discord integration.
8. Capture/clipping with supported Medal.tv integration.
9. Remote play and host-controlled manual controller handoff.
10. News and update monitoring.
11. Emulator adapters and tested features.
12. Deferred: save transfer, broader publishing APIs, DMs, integrity adapters, Steamworks replacement, relays, emulator-engine development.

Open item from the Step 1 spec that conflicts with `CLAUDE.md` and needs an owner decision before work starts: Direct3D/Vulkan
overlay hooks (anti-cheat risk). Persistent Steam credentials were decided against (`docs/decisions/0003`).
