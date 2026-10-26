# Roadmap — Soups Unified Launcher (SUL)

Staged priorities, not promises. Update status only with evidence.

1. **Foundation** — DONE in code, built and tested in Windows CI; not yet launched on a real machine: monorepo, shell, theme,
   registry scanner, compatibility matrix, library index, storage isolator, updater, portability, templates, safety core, Discord
   presence (Step 1 and Step 1.5). Redundant legacy files removed; see `PROJECT_STATUS.md`.
   **Next slice:** Game Library showing real scan results and the tick-and-compatibility matrix, then the per-game hub.
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

Open items from the Step 1 spec that conflict with `CLAUDE.md` and need an owner decision before work starts: persistent Steam
credentials, and Direct3D/Vulkan overlay hooks (anti-cheat risk).
