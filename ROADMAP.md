# Roadmap — Soups Unified Launcher (SUL)

Staged priorities, not promises. Update status only with evidence.

1. **Foundation** — DONE in code, built and tested in Windows CI; not yet launched on a real machine: monorepo, shell, theme,
   registry scanner, compatibility matrix, library index, storage isolator, updater, portability, templates, safety core, Discord
   presence (Step 1 and Step 1.5). Redundant legacy files removed; see `PROJECT_STATUS.md`.
   Game Library grid with the tick-and-compatibility matrix and Steam Play is built (needs a run on Windows).
   Test coverage was extended on 2026-10-28 (Section 1 UI, the Tauri commands over IPC, the Windows registry and drive adapters); CI on
   `main` passes on Windows. The defects it found (wallpaper gallery, search box, tab strip) are fixed and merged into `main` (abf8d45, Windows CI passed), with the Windows-only cleanup. See `PROJECT_STATUS.md`.
   **Section 2 (slice 1 built on branch `feature/section-2-density-and-redactor`, not merged; the rest waits for the owner's go-ahead):** decisions in `docs/decisions/0004-*`,
   drawings in `docs/section-2-wireframes.md`. Done: the log redactor masks Discord webhook URLs (prerequisite for the webhook toggle); the Display density setting with the
   per-module override and the inline Details panel (placeholders, no live data). Next, smallest first: the read-only Security & Trust pane; the Media & Clipping Engine pane as a
   shell with every feature Not investigated; the Rust telemetry sampler; last, the webhook toggle with the URL in Windows Credential Manager (owner's decision).
   **Other next slices:** the per-game hub, then public player metrics by SteamID64.
2. Build management: import/verify, original/optimized/experimental slots, rollback; HD2 7.1.1 isolation research (separate adapter track).
3. Mod management: profiles, ownership-aware deploy/purge, dry-run, conflicts, backup/recovery.
4. Mod Builder: one real adapter path, then audio, Bink/RAD Video, patch workflow, Blender.
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
