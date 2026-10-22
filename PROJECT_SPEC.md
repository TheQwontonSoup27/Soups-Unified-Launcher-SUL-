# Soups Unified Launcher (SUL)

**Project specification and architecture reference**  
**Version:** 1.0 — consolidated requirements draft  
**Date:** 2026-10-22  
**Status:** Approved for project-file generation; architecture is a planning baseline, not proof of implementation or feasibility.  
**Purpose:** A durable, Claude-readable reference for design, implementation planning, and AI-assisted development.

> **Source-of-truth rule:** This document supersedes earlier brainstorm notes where they conflict. A feature is not considered implemented or feasible until researched and tested. Preserve explicit unknowns rather than inventing certainty.

## 1. Project vision

Build an open-source, modular desktop platform that gives users one interface for game libraries, isolated builds, mod management/building, security, profiling, social/invitations, gameplay capture, remote play, news, emulation, and customization. Keep the user interface coherent while implementing capabilities through game-specific adapters and optional integrations.

Project name: **Soups Unified Launcher (SUL)**. The project should be forkable, modifiable, renameable, rebrandable, and redistributable under the eventual chosen project license and the applicable licenses of dependencies/assets. Check name/trademark availability before public release.

### Product principles
- One interface; modular services and game-specific adapters underneath.
- Preserve the user's original installation; isolate managed builds and make destructive operations recoverable.
- Do not assume every game supports every feature.
- Use a per-game/per-build capability matrix and truthful status labels.
- Keep local library/build/mod functionality useful even when optional online services are unavailable.
- Treat security, privacy, licensing, accessibility, and recovery as design requirements from the first iteration.
- Prefer existing documented APIs and maintained tools before writing replacements.
- Avoid building a large generic backend before a real integration requires it.

### Capability status vocabulary
Every feature and adapter must declare one of:
- **Supported and tested** — test evidence exists for the stated environment/build.
- **Partially supported** — a defined subset works; limitations are listed.
- **Experimental** — implemented or under test, with reliability/compatibility not established.
- **Unsupported** — known not to work or intentionally not provided.
- **Not investigated** — no adequate feasibility assessment yet.

Never describe a planned, researched, or untested feature as working.

## 2. Shared application core

The core is infrastructure shared by modules, not an additional feature module.

1. **Application shell:** navigation, windows, common UI, accessibility, settings entry points.
2. **Storage service:** configurable library roots, drive mapping, free-space checks, safe migration and relinking.
3. **Manifest and metadata service:** game/build/profile IDs, versions, file inventories, hashes, provenance, compatibility, timestamps, status.
4. **Logging and recovery service:** structured logs, user-readable errors, action history, crash/interruption recovery.
5. **Permission and credential service:** consent, scoped permissions, secure token handling, access controls.
6. **Extension/adapter interface:** game adapters and optional third-party integrations with declared capabilities and versions.
7. **Installer/update/uninstall foundation:** safe application updates, migration and cleanup policies.
8. **Theme/branding hooks:** core shell should support theme tokens and branding assets early, even if advanced customization comes later.

Security fundamentals belong in the core from Stage 1: trust boundaries, least privilege, safe file operations, input validation, secure defaults, authentication/authorization, dependency and license tracking, security testing, and vulnerability reporting.

## 3. Primary module architecture

There are **10 primary feature modules**. Module icons listed here are the intended default icon concepts; actual image assets should be added to the repository only after checking their provenance and license. The included PNG mockups are design inspiration, not final UI requirements.

| # | Module | Intended icon concept | Purpose |
|---|---|---|---|
| 1 | Game Library and Build Management | Library / stacked game tiles | Organize games, installations, build variants, launch and rollback. |
| 2 | Mod Management and Mod Builder | Puzzle/gear / wrench | Manage profiles and create, validate, and package modifications. |
| 3 | Security and Trust | Shield / check | Scan packages, manage trust and credentials, and assess multiplayer integrity. |
| 4 | Diagnostics, Compatibility and Performance | Gauge / waveform | Test compatibility and measure runtime behavior. |
| 5 | Social Platform and Networking | People / linked nodes | Accounts, friends, invitations, lobbies, presence, and supported networking. |
| 6 | Gameplay Capture and Clipping | Record circle / film strip | Capture and organize clips, including supported Medal.tv integration. |
| 7 | Remote Play and Controller Handoff | Monitor / controller | Stream supported games and grant/revoke remote input. |
| 8 | News and Updates | Newspaper / refresh | Track official news, versions, and clearly labelled community sources. |
| 9 | Emulation | Gamepad / chip | Manage emulator engines, libraries, input, and supported netplay. |
| 10 | Launcher Customization, Theming and Rebranding | Palette / sliders | Themes, palettes, branding, and fork/rebrand support. |

### Module 1 — Game Library and Build Management

**1.1 Game Library**
- Catalogue, artwork, search, categories, detail pages, launch state, and capability status.
- Initial adapter targets: Helldivers 2, Terraria, RimWorld, and Stardew Valley. Project Zomboid may be considered later.

**1.2 Installation and Acquisition**
- Import existing installations; download/acquire historical builds only through authorized tools/access and where terms permit.
- Investigate SteamCMD/depot/manifest workflows where applicable; respect account permissions, access controls, and distribution terms.
- Never store passwords or session secrets in plaintext. Verify files before marking a build ready.

**1.3 Build and Version Manager**
- Separate **Original**, **Optimized**, **Legacy**, and **Experimental** builds as applicable. Allow additional named variants later.
- Each build manifest should record game ID, display name, base version/build, source/manifest ID, file inventory/hashes where feasible, patches/modifications, mod/profile references, compatibility, timestamps, state, and notes.
- Support validation, rollback, repair, staging, and atomic switching where practical.
- Do not silently overwrite or modify the original installation.

**1.4 Launch and Runtime Manager**
- Launch a selected build with documented arguments/configuration, track process status, collect opt-in diagnostic logs, and explain failures.
- Declare per-build network/launcher integration only when implemented and tested.

**1.5 Storage and Library Locations**
- Multiple configurable roots and drives, relinking, disk-space checks, safe migration, and explicit ownership rules.
- Never delete a file merely because it appears duplicated. Shared storage/hard links require robust integrity and ownership checks.
- Keep save data/settings separate from managed binaries where feasible.

**1.6 Save Data and Platform Transfer — deferred**
- Low priority; revisit near social/capture work in the roadmap.
- Investigate per game and per platform. Backup first; validate formats and compatibility. Do not promise universal Steam save conversion.

#### Helldivers 2 initial experiment
- Target build: **7.1.1**, acquired through the user's TimeDiver v2 workflow.
- Preserve the original Steam installation. Keep the legacy build in a separate managed location/copy with manifests and file verification.
- Current known risk: Steam may attempt to update the game to the latest version and interfere with the legacy launch. Test launch/authentication/update behavior without assuming independence or successful online play.
- First goal is reliable build isolation and truthful status reporting. Custom multiplayer/Steamworks replacement is separate research, not a prerequisite.
- Do not bypass anti-cheat/access controls or connect modified clients to official services unless explicitly supported and safe. Research legal/technical constraints before protocol or service emulation.

#### Other initial game adapter research
- **Terraria:** vanilla/tModLoader versions, profiles/dependencies, host-and-play and dedicated-server workflows, supported invitations, build compatibility.
- **RimWorld:** version-specific mod profiles, load order/dependencies, save safety, optional multiplayer-mod compatibility; do not imply base-game native multiplayer.
- **Stardew Valley:** game/SMAPI compatibility, mod profiles, co-op host/client behavior, platform connection options; do not assume launcher invitations replace the game's protocol.
- **Project Zomboid:** possible later adapter; not a committed first-wave target.

### Module 2 — Mod Management and Mod Builder

**2.1 Mod Profiles and Deployment**
- Named profiles, load order, dependencies, per-profile config, enable/disable, version compatibility, backups, logs, rollback.
- Include **Purge & Deploy** and a standalone **Purge** action. Provide dry-run preview, per-file ownership, exact affected paths, and recovery logs.
- Purge only files provably managed by the selected profile/mod. Never perform broad or ambiguous deletion.

**2.2 Mod Builder Workspace**
- One central workspace for supported patching, repacking, merging, experimental build assembly, validation, provenance, and reproducible recipes.
- Separate asset-level operations from runtime-code optimization; do not conflate them.

**2.3 Game Asset and Format Tools**
- Game-specific extraction/repacking, video/text/audio workflows, conversion, and format validation only where supported.
- Keep the original files and tool settings/version records. Record reversible patches.

**2.4 Audio Modding Integration**
- Integrate the existing **Raiding from Pants / Pants' HD2 Audio Modder** workflow where technically and legally possible; investigate Wwise workflows and repacking requirements.
- Treat the tool as an external integration unless redistribution permission is established.

**2.5 Blender and 3D Asset Integration**
- Launch Blender and support compatible add-ons, import/export, conversion, and validation workflows. Do not assume all game formats are supported.

**2.6 Conflict and Compatibility Analysis**
- Show files added, replaced, changed, or removed by each mod; list every mod touching an overlapping path, hashes, effective override/winner, and possible resolutions.
- Classify as verified conflict, possible conflict, resolved, intentional, or untested.
- File overlap is not automatically a behavioural conflict; behavioural conflicts can also occur across different files.

**2.7 Experimental Build Construction**
- Combine selected modifications and optimizations into a named build using a reproducible recipe.
- Validate against a clean/known-good baseline and retain rollback.

**2.8 Modpack Export and Sharing**
- Export share codes/manifests, source links, versions, dependencies, file hashes where appropriate, and license/provenance details.
- Let recipients inspect contents, permissions, file changes, and warnings before download/deployment.
- Do not package copyrighted game files or unlicensed assets by default. Use authorized source links or user-provided content.
- Hosting may be local, user-selected, or a later service; do not assume SUL must host all mod files.

**2.9 Mod Publishing Integrations**
- Investigate Nexus Mods, Mod.io, and “AIK Mods” (exact service identity/API still unconfirmed).
- Use documented APIs, approved upload workflows, export packages, and license checks. Do not assume upload APIs exist or grant permission.

**2.10 Bink / RAD Video modding workflow**
- Bink video handling and conversion; integrate RAD Video Tools where applicable and permitted.
- Where a video mod requires code/metadata/reference edits, document a controlled hex-editing or patch workflow with original backups and reversible patches.
- Typical flow: convert/prepare video → adjust supported references/code if necessary → deploy assets → validate playback.
- RAD Video Tools is proprietary unless verified otherwise. Initially integrate supported external tools; do not copy proprietary source or bundle tools without permission. Evaluate open-source alternatives separately.

### Module 3 — Security and Trust

**3.1 Mod and Package Scanning:** suspicious files, known signatures, provenance, hashes, warnings, quarantine and restore.

**3.2 User Review and Quarantine:** show risk evidence and let the user inspect before trust/deployment. Automated scanning is not a guarantee of safety.

**3.3 Account and Credential Security:** secure token storage, scoped permissions, session revocation, and no secrets in logs.

**3.4 Remote Access Security:** authentication, session codes/expiry, least privilege, rate limits, host override, and protected remote input.

**3.5 Extension Trust and Provenance:** source, signer/hash where available, version, permissions, license, and update history.

**3.6 Multiplayer Integrity and Cheat Detection Framework**
- Integrated SUL capability, not a promise of universal anti-cheat or a ban-every-mod policy.
- Main target: detect unauthorized changes to gameplay-relevant state/statistics that violate a session's declared rules and harm other participants. Harmless personal changes such as XP or premium-currency changes are not the central priority unless they affect session rules or others.
- Expected integrity rules must be tied to a specific game/build/session. Known-good builds and experimental builds may have different intentional stats/modifications; do not flag an allowed experimental build solely because it differs from vanilla.
- Use reliable game data, supported instrumentation, and per-game checks. Track evidence and confidence; distinguish confirmed violations from suspicious discrepancies.
- Desync, bugs, version mismatches, legitimate mods, and host configuration can cause false positives. Do not claim to detect every cheat, DLL injection, or tampering method.
- Potential research includes Dark Souls 3-style edited-stat detection and inspection of a Helldivers 2 anti-cheat mod if the user supplies it and inspection is allowed by applicable permissions/licenses. Do not promise feasibility before analysis.
- SUL host authority applies to SUL-managed sessions or adapters that actually control the session. It does not grant kick powers in unrelated official matchmaking.
- Response ladder: (1) personal block from future SUL-managed sessions with that host/player; (2) social block; (3) evidence-based report; (4) SUL service restrictions only for evidenced serious abuse/malicious behavior under published rules. Permanent platform bans require strong evidence, proportionate policy, and appeal/review.
- Reports may include SUL account, session/build IDs, timestamps, and integrity findings/logs. Do not collect hardware IDs or IP addresses by default. Define purpose, access, retention, and disclosure if any sensitive telemetry is later required.

### Module 4 — Diagnostics, Compatibility and Performance

**4.1 Diagnostics and Activity History:** readable errors, action timeline, logs, build verification, recovery status.

**4.2 Compatibility Testing:** game/build/OS/mod/profile/driver/tool matrix and repeatable test plans.

**4.3 Runtime Performance Profiler:** CPU/GPU/memory, frame times, load times, and Lua/callback timing where instrumentation permits.

**4.4 Optimization Analysis:** compare clean/modded/experimental builds under recorded conditions; report actual measurements and uncertainty. Do not assume fewer function calls automatically improve performance.

Core owns shared logging; this module owns analysis UI, workflows, and interpretation. Every performance comparison should record hardware, OS, game/build, mods, settings, test method, sample size/duration, and whether the result is measured or inferred.

### Module 5 — Social Platform and Networking

Lightweight and modular; do not turn the project into a full-scale social network prematurely.

**5.1 Accounts and Player Profiles:** independent SUL accounts, separate from Steam/publisher identity.

**5.2 Friends, Invitations and Lobbies:** contacts, invitations, session discovery and status where supported.

**5.3 Networking Backend:** direct/P2P connections first where suitable; optional relays if NAT/firewall conditions require them. A coordination/rendezvous service is distinct from a relay, and both are distinct from a dedicated game simulation server.

**5.4 Game Networking Adapters:** game-specific integration, LAN/system-link where supported. Do not assume system-link or online multiplayer exists in every title.

**5.5 Community Feed / Per-game Hubs:** basic user posts, game-specific hubs, updates, community resources, modpacks, clip links, privacy/block/report controls, spam protection.

**5.6 Private Messaging:** basic DMs are deferred/back-burner. Before implementation, define abuse reporting, blocking, rate limits, moderation responsibility, privacy defaults, account recovery, retention/deletion, and operating costs.

#### Discord integration
- Rich Presence, supported invite links, optional account linking, posting to Discord, and clip/link sharing where APIs, permissions, and limits allow.
- A **global Rich Presence toggle** belongs in the top-left/global launcher UI, not only inside per-game settings. When off, SUL must stop publishing activity. When on, SUL attempts the relevant supported per-game integration. Unsupported games must show unsupported status.
- Aim for a per-game integration/script where feasible; do not promise full coverage.
- Discord friend import/DMs require supported API permissions and policy compliance; do not assume unrestricted access to friend lists.
- Posting a URL does not guarantee an embed. MP4 attachment and hosted link workflows are distinct and subject to Discord limits and API access.

#### Networking and Steamworks boundary
- A custom API does not automatically replace a game's embedded Steamworks calls. Steamworks may supply matchmaking, lobbies, networking, authentication, achievements, or other services; individual games use different subsets.
- A shim/replacement may require authentication, protocol, anti-cheat, DRM, and terms research. Keep this as a separate exploratory track, not a prerequisite for build isolation or the core launcher.
- The user's always-on/reachable laptop may host initial coordination services, but does not solve all NAT/firewall cases or provide a relay automatically. Test direct P2P first; add relays only if required and operationally feasible.

### Module 6 — Gameplay Capture and Clipping

**Primary integration requested: Medal.tv.** “Metal” in the discussion was interpreted as Medal.tv.

**6.1 Medal Integration**
- Integrate via documented APIs and supported desktop-app interfaces.
- Launch or communicate with the Medal application through SUL where supported, creating a unified SUL entry point for supported clipping workflows.
- Medal's account, hosted services, and cloud storage remain controlled by Medal.

**6.2 Clip Library and Cloud References**
- SUL stores clip URLs and permitted metadata for organization/findability; it does not duplicate Medal's cloud videos or copy the entire Medal database.
- Display previews/details only where API permissions allow. Respect authentication, visibility, privacy, deletion, and expiry.

**6.3 Capture and Replay Buffer**
- Use Medal recording/clipping, hotkeys, replay buffer, and auto-clipping where supported. Keep the capture engine modular so another engine can be integrated later.

**6.4 Sharing and Discord**
- Share Medal clip links through SUL social features. Support Discord link posting and MP4 attachments only where APIs, permissions, file-size limits, and provider functionality allow.

**Implementation boundary:** Medal's documented API access and auto-clipping interfaces are limited and may require provider approval and the Medal desktop app running locally. This is not permission to embed every Medal feature or access its entire private cloud database. Use supported interfaces; do not copy proprietary software or bypass account restrictions. Verify the current developer requirements before implementing.

### Module 7 — Remote Play and Controller Handoff

**7.1 Gameplay Streaming:** stream host gameplay video/audio to invited participants through a supported engine.

**7.2 Remote Controller Input:** send scoped controller input to the intended game/emulator process, not general desktop control.

**7.3 Controller Handoff and Permissions:** host can allow view-only, accept control requests, grant/revoke input, and trigger an immediate override/kill switch. Manual handoff is the first target. Automatic rules (e.g., hand off after a death) are optional later work and require reliable game-state detection.

**7.4 Emulator Remote Play Integration:** integrate supported emulator streaming/input features where available.

Distinguish: (1) host-screen streaming; (2) remote input into the host's game; (3) genuine multiplayer/netplay with multiple compatible instances. These are separate capabilities. System link is game/platform-specific. Remote play does not itself create online multiplayer. Streaming needs bandwidth/latency/encoding/decoding and may require relays; prefer established tools if their interfaces and licenses fit.

### Module 8 — News and Updates

**8.1 Official Game News:** supported official announcements and release notes.

**8.2 Version and Build Monitoring:** monitor build/version changes without replacing preserved builds.

**8.3 Community Sources:** clearly label community reports, rumours, datamined information, and leaks separately from verified official information.

**8.4 Update History:** source, timestamp, link to original, per-game subscriptions, filtering, read/unread, concise summaries.

Investigate each source's API, terms, rate limits, and access permissions. Do not mirror private/access-controlled Discord content without authorization. If a source fails or changes API, degrade gracefully. Notifications must never auto-update a preserved build; instead offer inspection, comparison, separate import/build creation where permitted, or dismissal.

### Module 9 — Emulation

**9.1 Emulated Game Library**

**9.2 Emulator Engine Adapters:** investigate Dolphin, PCSX2, RPCS3, and other candidates individually; integrate existing engines before writing/forking one.

**9.3 Input and Controller Configuration**

**9.4 Emulator Networking and Netplay:** only through emulator-supported/documented features; verify each engine's current capabilities independently.

**9.5 Emulator and ROM Storage:** manage ROM/disc images, emulator binaries, saves/memory cards/configs, profiles, and metadata. Respect applicable law and licenses; do not distribute copyrighted game images or BIOS files.

**9.6 Emulator Engine Development:** long-term/optional; not a prerequisite to integrating existing engines.

Frontend should remain game-centric and engines remain backend adapters. Support controller setup, local multiplayer, supported netplay, and remote input/handoff where feasible. Remote play and netplay are distinct. Large ROMs, media, saves, emulator binaries, and archived builds may live on external HDD; launcher/core and performance-sensitive games may live on M.2 SSD. Some games load more slowly from HDD; test before imposing blanket SSD requirements.

### Module 10 — Launcher Customization, Theming and Rebranding

**10.1 Theme Presets:** dark, light, purple, green, gray, and combinations.

**10.2 Custom Color Palettes:** background, panels, text, accent, borders, status colors.

**10.3 Appearance Settings:** contrast and supported accessibility settings.

**10.4 Persistent Preferences**

**10.5 Theme Import/Export**

**10.6 Application Branding and Rebranding**
- In-app: displayed name, logo, icon, theme.
- Build-time: executable displayed name, application icon, installer branding, package metadata where the OS/build system permits.
- Support forks, modifications, renaming, and redistribution under the selected license and third-party terms.
- Accessible contrast, visible focus states, keyboard-usable controls.
- Intended default module icon concepts are listed in Section 3; add actual icon assets later after provenance/license review.

## 4. Cross-cutting requirements

- **Security from day one:** permissions, safe paths, least privilege, validation, secure credentials, safe updates, dependency tracking, security tests, vulnerability reporting.
- **Privacy:** local-first by default where practical; opt-in diagnostics and uploads; minimize telemetry; document collection, purpose, access, retention, and deletion.
- **Licensing/provenance:** track licenses for source, dependencies, game adapters, external tools, artwork, icons, and bundled assets. The project license does not override third-party licenses.
- **Testing:** unit tests for critical file operations and manifests; integration tests for adapters; recovery tests for interrupted operations; security tests for permissions and remote input.
- **Capability matrix:** record feature, game, build, OS, adapter/integration version, test date, evidence, limitations, and status.
- **Offline resilience:** core local functions should not depend on the optional social service being available.
- **No hidden destructive actions:** confirm destructive operations, preview changes, log them, and offer recovery where feasible.
- **Accessibility:** readable contrast, clear focus, understandable warnings and usable controls.
- **No unsupported claims:** mark estimates and hypotheses; distinguish tested results from intended design.

## 5. Development priority roadmap

1. **Foundation:** app shell, navigation, theme tokens, game library skeleton, storage service, manifests, logging/recovery, safe file operations, security fundamentals, tests and project status files.
2. **Version management:** import/verify builds, original/optimized/experimental slots, rollback; investigate Helldivers 2 7.1.1 isolation without altering original Steam install.
3. **Mod management:** profiles, ownership-aware deploy/purge, dry-run, conflict listing, backups/recovery.
4. **Mod Builder and tool integrations:** one working game-specific path first; then audio, Bink/RAD Video, controlled hex/patch workflow, Blender.
5. **Modpack sharing and trust:** manifests, share/export, recipient review, scanning/quarantine, provenance and license warnings.
6. **Diagnostics/profiling:** establish repeatable baseline comparisons and measured reports.
7. **Social/networking foundations:** independent accounts and invitations only after security/service scope is designed; Discord integration as supported.
8. **Gameplay capture/clipping:** Medal integration via approved/documented APIs and desktop interface; link library and sharing.
9. **Remote play/controller handoff:** integrate existing streaming/input components, manual host-controlled handoff.
10. **News/updates:** official sources and labelled community feeds.
11. **Emulation:** integrate existing engines and test each capability.
12. **Deferred/advanced work:** save transfer, broader publishing APIs, DMs, advanced per-game multiplayer integrity adapters, Steamworks replacement research, custom relay infrastructure, and emulator-engine development.

Theme/rebranding hooks should be designed into the shell at Stage 1 even if the advanced theme editor and build-time branding come later. Security fundamentals must begin at Stage 1, even though the full Security and Trust module spans later phases. Save transfer remains in Module 1 but is low priority near social/capture work. Advanced multiplayer replacement is research, not a promised capability.

## 6. Suggested repository layout

Adapt to the selected language/framework; do not commit to a stack until trade-offs are reviewed.

```text
SoupsUnifiedLauncher/
  README.md
  CLAUDE.md
  PROJECT_SPEC.md
  PROJECT_STATUS.md
  ROADMAP.md
  CONTRIBUTING.md
  SECURITY.md
  LICENSE (chosen after review)
  docs/
    architecture/
    game-adapters/
    capability-matrix/
    decisions/
    design-references/
  src/
    app-shell/
    core/
      storage/
      manifests/
      logging-recovery/
      permissions/
      extensions/
    modules/
      game-library-builds/
      mod-management-builder/
      security-trust/
      diagnostics-performance/
      social-networking/
      capture-clipping/
      remote-play/
      news-updates/
      emulation/
      customization/
    adapters/
      games/
      emulators/
      third-party/
  tests/
    unit/
    integration/
    recovery/
    security/
  scripts/
```

This is illustrative. The coding assistant should recommend a concrete layout based on the selected stack and repository state rather than creating every directory empty.

## 7. Open decisions — do not silently finalize

- Programming language and desktop UI framework.
- Supported operating systems and minimum versions.
- Initial release scope and first complete end-to-end game adapter.
- Project license and contribution policy.
- Build manifest schema and migration strategy.
- Social account/authentication model, hosting and operating costs.
- Which networking backend is first; whether any relay is required.
- Which Medal API/access level is approved and available.
- Capture engine fallback if Medal cannot provide required supported functionality.
- Exact identity/API for “AIK Mods”.
- Whether/when shared content storage is justified.
- Feasibility of each game's network integration and multiplayer integrity checks.
- Current licensing/API status of third-party tools, services, and artwork.

Claude should present options and trade-offs before making major decisions that affect architecture, cost, security, licensing, or user data. Do not block useful local functionality on unresolved social/networking research.

## 8. Required engineering workflow for Claude

1. Read `CLAUDE.md`, `PROJECT_SPEC.md`, `PROJECT_STATUS.md`, and `ROADMAP.md` before making changes. If status/roadmap do not exist yet, create concise files after inspecting the repository.
2. Inspect the actual repository, current branch, file tree, dependency versions, and existing tests before proposing edits. Do not assume an empty/new repository.
3. Summarize the current implementation and identify the smallest useful next task.
4. Separate verified facts, assumptions, proposed design, and unverified research.
5. Before destructive or broad changes, provide a plan and a rollback point; ask for approval when required.
6. Make small, reviewable changes. Do not replace working code with a speculative large rewrite.
7. Run the relevant tests/build/lint checks where available. Report exactly which commands ran and their real outcomes. If not run, state that.
8. Update `PROJECT_STATUS.md` and `ROADMAP.md` after each work session, including blockers and next action.
9. Keep the capability matrix current; never mark a feature supported without test evidence.
10. Preserve original game files, credentials, user data, and uncommitted work. Do not include proprietary game files, secrets, copyrighted assets, or unlicensed third-party binaries in source control.
11. When investigating external APIs/tools, check current documentation, permissions, version constraints, licenses, and terms before implementation.
12. Treat AI-generated code as ordinary code requiring review, tests, and maintenance.

### First recommended Claude task
Do not attempt all ten modules at once. First inspect the repository and recommend the stack and the smallest vertical slice that establishes the app shell, module navigation, theme tokens, structured logging, a safe storage abstraction, manifest types, and automated tests. Then propose a plan for approval before making a large implementation change. Keep Helldivers 2 7.1.1 isolation as a separate, testable adapter track.

## 9. Design references included with this specification

The `design-references/` directory contains conceptual mockups for inspiration only. They are embedded below so they can be reviewed alongside the requirements.

### Launcher overview

![Conceptual SUL overview with navigation, game cards, activity, and capability statuses](design-references/01-launcher-overview.png)

### Build manager

![Conceptual isolated build slots and legacy-build warning](design-references/02-build-manager.png)

### Mod Builder

![Conceptual mod workspace, file conflicts, purge preview, and performance comparison](design-references/03-mod-builder.png)

### Capture and Medal integration

![Conceptual clip library with explicit Medal integration boundaries](design-references/04-capture-medal.png)

These are not production UI assets, do not define final pixel measurements, and contain illustrative placeholders rather than real metrics. They are provided so Claude can understand intended information hierarchy and visual direction. Recreate the design using the selected UI framework; do not treat the PNGs as reusable product icons or final art.

## 10. Release readiness checklist

- [ ] Project license, dependency licenses, and asset provenance reviewed.
- [ ] Clean install/update/uninstall and data migration tested.
- [ ] Original installation preservation and recovery tests pass.
- [ ] Build manifest and file verification behavior documented.
- [ ] Mod deploy/purge dry-run and recovery tests pass.
- [ ] Credentials are not stored in plaintext or leaked into logs.
- [ ] Remote input has authentication, explicit host consent, scoped process access, and immediate revocation.
- [ ] Capability matrix distinguishes tested and untested features.
- [ ] Supported APIs and provider permissions verified for integrations.
- [ ] Security and privacy documentation published.
- [ ] Reproducible build/test instructions available.
- [ ] Known limitations and unsupported capabilities clearly disclosed.
