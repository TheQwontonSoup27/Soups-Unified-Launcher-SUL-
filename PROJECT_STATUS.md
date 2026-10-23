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

## Verified in GitHub Actions (run 39880270552, commit e5a64b0, windows-latest only)
- `core`: `cargo test --workspace` passed (sul-core unit/integration tests). Per-test output not read: raw log download is blocked from the authoring session, only job/step results are visible.
- `ui`: `npm install`, `npm run typecheck`, `npm test`, `npm run build` all passed.
- `tauri-shell`: `cargo check` of the desktop wrapper passed. The earlier failure was most likely the missing `src-tauri/icons` (placeholder icons added); the exact error was never read.
- Decision 2026-10-22: Windows only for now. All Ubuntu CI jobs removed.

## NOT verified
- The desktop app has never been launched (`tauri dev` / installer build not run). Only `cargo check` passed.
- No real game install has been touched by any code. No game adapter exists.
- Helldivers 2 7.1.1 isolation/offline launch: not investigated.
- Icons are generated placeholders, not final branding.

## Branding and appearance (2026-10-23)
Built: Team27 logo as app/window/tab icon (src-tauri/icons, ui/public); splash screen (logo centred on logo-purple, click or ~1.8 s to skip);
custom borderless title bar (purple) with minimize / maximize / close; default Team27 theme (green app background, purple title bar,
purple buttons with black text, green pop-ups); per-token colour editor with 4.5:1 contrast check; background chooser (solid, 19-picture gallery,
or the user's own picture / GIF / video stored in IndexedDB) with readability dimming; 4 other theme presets.
Verified here (Linux container, headless Chromium): `ui` builds, 23/23 UI tests pass, screenshots of splash, main window, Appearance,
gallery background, custom GIF upload and pop-up dialog look as specified, no console errors.
NOT verified: title-bar window buttons inside the real Tauri window (browser preview only); borderless window on Windows; video backgrounds
(GIF/image tested, video not); typecheck of the new code and the Tauri permission names (CI `cargo check` / `npm run typecheck` decide).
Wallpaper JPEGs are git-ignored on purpose: the repo is public and the pictures' licences are unconfirmed (one has a third-party watermark,
five are only 480x299). The gallery shows empty on a fresh clone until the files are dropped into `ui/public/wallpapers/`.

## Discord, Steam library and references (2026-10-23)
Built: Discord Rich Presence over Discord's documented IPC (`crates/sul-core/src/discord.rs`, off by default, global toggle in the title bar,
unsupported / not-investigated games never publish; setup in docs/discord.md); read-only Steam library scanner and `steam://` launch URL
(`crates/sul-core/src/steam.rs`, no Steam password involved); licence review of the reference repos (docs/references.md); design note on keeping a
managed legacy build outside Steam's update reach (docs/steam-and-updates.md).
Verified here: UI builds and UI tests pass (Linux container). Rust additions are NOT verified until CI runs `cargo test --workspace` on Windows.
Not verified: Discord against a live Discord client; any of this against a real Steam install; Helldivers 2 7.1.1 behaviour (still Not investigated).
Wallpaper licences: no reverse-image-search tool is available to the authoring session, so none could be confirmed public domain.

## Next action
1. Launch the desktop app on Windows (`npx tauri dev`) and report what you see.
2. Then build the Helldivers 2 Build Manager slice: import an install, hash it, isolated copy, verify, launch tracking.
