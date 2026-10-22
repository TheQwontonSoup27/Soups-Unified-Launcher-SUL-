# Claude handoff — Soups Unified Launcher (SUL)

Use this file as operating instructions for work on the Soups Unified Launcher repository. The authoritative product requirements are in `PROJECT_SPEC.md`. Design inspiration images are in `design-references/`.

## Project intent
Build an open-source, modular desktop game launcher with 10 feature modules: Game Library & Build Management; Mod Management & Mod Builder; Security & Trust; Diagnostics/Compatibility/Performance; Social Platform & Networking; Gameplay Capture & Clipping (including supported Medal.tv integration); Remote Play & Controller Handoff; News & Updates; Emulation; Customization/Theming/Rebranding.

## Before coding
1. Read `PROJECT_SPEC.md` fully.
2. Inspect the repository, current state, branch, language/framework, dependency files, tests, and any user changes. Do not assume the repository is empty.
3. Read or create `PROJECT_STATUS.md` and `ROADMAP.md`.
4. Report current state, confirmed facts, unknowns, and the smallest useful next step.
5. Propose a plan before major architectural changes. Do not silently choose unresolved decisions in Section 7 of the spec.

## Non-negotiable engineering rules
- Preserve original game installations. Managed build variants must be isolated, verified, and recoverable.
- Safe file operations, permissions, validation, least privilege, structured logging, and recovery are required from the first implementation stage.
- Never perform ambiguous deletion. Mod purge must be ownership-aware, previewable, logged, and reversible where practical.
- No secrets in source control or plaintext logs. Keep telemetry and uploads opt-in and minimal.
- Do not redistribute proprietary game files, copyrighted game assets, unlicensed binaries, or credentials.
- Treat game-specific networking, Steamworks replacement, universal cheat detection, Medal cloud access, and emulator features as unverified until documented and tested.
- Do not bypass anti-cheat/access controls or provider restrictions. Use supported APIs and integration points.
- Distinguish confirmed integrity violations from suspicious signals; account for false positives.
- The host may grant/revoke remote input and must have an immediate override. Remote input must not become general desktop control.
- Every capability must be labelled Supported and tested / Partially supported / Experimental / Unsupported / Not investigated.
- Do not claim a test, build, integration, or research result unless it actually happened.

## Work style
- Prefer small vertical slices over building every module simultaneously.
- Inspect before editing; preserve working behavior and uncommitted work.
- Add automated tests for critical logic and a reproducible manual test plan for hardware/game integrations.
- Run relevant checks and report exact commands and results. State clearly when checks were not run.
- Update `PROJECT_STATUS.md` and `ROADMAP.md` after each session.
- Keep a capability matrix with game/build/OS/adapter version, status, test date, evidence, and limitations.
- Record major design decisions in `docs/decisions/` if/when the repository structure is established.

## Recommended first task
Inspect the existing repository and recommend a stack plus a first vertical slice: app shell/navigation, theme tokens, module placeholders, structured logging, safe storage abstraction, manifest types, and automated tests. Provide a plan for approval before any broad rewrite. Treat Helldivers 2 version 7.1.1 isolation as a separate testable adapter track; do not make it a prerequisite for the base launcher.

## Design inspiration
Review all four PNGs in `design-references/`. They are conceptual references for hierarchy and status communication, not final assets or pixel-perfect mandates. Keep the UI readable, restrained, accessible, and explicit about capability status. The Medal mockup intentionally shows provider/API boundaries rather than implying unrestricted access.
