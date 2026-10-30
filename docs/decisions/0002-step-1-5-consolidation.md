# 0002 — Step 1.5: consolidating the Step 1 scaffold with the earlier codebase
Date: 2026-10-25 · Status: accepted by owner ("APPROVED, defaults on all 5 decisions")

The repository on `main` held an earlier codebase (capability/safety core, Discord, Steam helpers, Team27 UI).
Step 1 produced a separate scaffold (registry scanner, compatibility matrix, library index, storage, updater,
portability, templates, Vite shell). Step 1.5 merged them on branch `step-1.5` instead of overwriting either.

## Decisions (owner-approved)
1. **Which spec governs.** The long-term `PROJECT_SPEC.md` in the repo governs; the short Step 1 spec covered the Step 1 scaffold only.
   Consequence: the ten roadmap modules stay in the UI as truthfully labelled placeholders (`ui/src/modules/registry.tsx`), alongside the
   Step 1 sidebar shape (icon, search and presence switch on top; Game Library first; Settings and General Playtime Tracker last).
   Security & Trust and Appearance live inside Settings, as the Step 1 spec says.
2. **App identifier** stays `app.soups.unifiedlauncher` (not the scaffold's `com.thequwontonsoup.sul`).
3. **Status model.** The per-game tick-and-compatibility matrix is kept. The five capability labels are layered on top:
   a row's label is *derived* from its compatibility verdict plus an optional evidence claim, so the two cannot disagree
   (`FeatureState::status`, `CompatibilityMatrix::headline`, `capability_entries`). "Supported and tested" still needs a test date and evidence.
4. **Safety net.** The legacy state is `main` at 9ece116, which this work never pushes to; the integration work goes to `step-1.5`.
   The local tag `legacy-pre-step-1.5` points at the same commit. **Pushing that tag to GitHub was refused (HTTP 403) from the authoring
   session**, so the tag exists locally only; create it from any machine with `git tag legacy-pre-step-1.5 9ece116 && git push origin legacy-pre-step-1.5`.
5. **Discord corner image.** An asset key for the small image is supported end to end (`Presence::small_image`, `discord_set_enabled`,
   Settings → Discord). A "scale to N%" slider is **not** offered: as far as is known the activity payload has no size field, Discord fixes it.

## Outcome
- 2026-10-26: `step-1.5` (04c968f) passed Windows CI (run 40276520399) and was fast-forwarded into `main` at the owner's request.
  `main` therefore contains the pre-merge history, so 9ece116 stays reachable.
- Four redundant legacy files (`ui/build.mjs`, `ui/src/components.tsx`, `ui/src/overview.tsx`, `ui/public/icon.svg`) stayed tracked
  after the merge because the authoring session's permission system blocked the first deletion attempt. They were deleted on
  2026-10-26 after the owner asked again; see `PROJECT_STATUS.md`, "Wipeout".

## Choices made while merging (not in the owner's list; reversible)
- **Bundler: Vite**, from the scaffold, replacing the earlier esbuild script (`ui/build.mjs`). Tests keep the earlier `tsx --test` runner.
- **`src-tauri` is a workspace member** (scaffold layout), superseding the "excluded from the workspace" line of 0001.
  `cargo test --workspace` therefore builds the Tauri shell too; `cargo test -p sul-core` does not.
- **`@tauri-apps/api` instead of `withGlobalTauri`**: the webview no longer exposes a global `window.__TAURI__`. The CSP is unchanged.
- The earlier `Vdf` / `scan_steamapps` Steam API was dropped in favour of the scaffold's registry-aware `registry::steam_library`; the earlier
  behaviours (build id and update policy recorded read-only, default-folder fallback, tolerant parsing) were ported into it and tested there.
- One `Error` enum: the earlier variants (unsafe path, protected root, journal, manifest, Discord) were merged into the scaffold's.
