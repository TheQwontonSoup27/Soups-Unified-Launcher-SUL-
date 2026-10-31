# 0004 — Section 2: owner's answers to the wireframe review
Date: 2026-10-29 · Status: accepted by owner (answers to the five questions asked after the Section 2 wireframes).
**Update 2026-10-29 (owner):** go-ahead to build Section 2, first slice only: the Display density setting, per-module overrides and the inline Details panel with
placeholders and no live data. Webhook URL storage decided (below). Slice 1 is merged into `main` (6902aeb). **Update 2026-10-30 (owner):** merge approved, and go-ahead for slice 2, the read-only
Security & Trust pane (branch `feature/section-2-security-trust-pane`); see `PROJECT_STATUS.md`.
Nothing else in Section 2 is built. The wireframes, with these answers applied, are saved in `docs/section-2-wireframes.md`.

## The owner's answers and what they mean
1. **Discord webhook: "make an exception."** The Discord rule (Rich Presence only; no chat overlay, friends list, login or webhook UI) is relaxed
   for one thing: a Discord webhook *destination* for the Media & Clipping Engine's "Automated Social/Discord Webhooks" toggle (Full Feature Mode).
   - Scope applied (an interpretation, easy to widen): **only** that destination field. The rest of the rule stands: no chat overlay, no friends list, no login.
   - Rules already in force that now bind this feature (`CLAUDE.md`): off by default and opt-in; uploads minimal; no secrets in source control or plaintext logs.
     A webhook URL is a secret (anyone holding it can post to that channel), so it is not kept in the settings JSON, not logged, and shown masked.
   - **Prerequisite found in the code, now done (commit 7b4c812):** the log redactor (`crates/sul-core/src/logging.rs`) masked by field name only, so a webhook URL
     under `webhook_url` or `url` would have been written in clear. It now also treats `webhook` as a secret field name, and masks any Discord webhook URL
     inside any text value at any depth, and in the event name (`discord.com`, `discordapp.com`, `ptb.`/`canary.` hosts, any API version, a percent-encoded
     address inside another address). `redact_text` is public so that text from an HTTP error, which usually quotes the address, can be cleaned before it is
     shown. 12 unit tests and one end-to-end test that reads the log file back. It must be in place before the webhook toggle is built, and it is.
   - **Decided (owner, 2026-10-29): the URL is stored in Windows Credential Manager** (per user, encrypted by Windows). That needs one new dependency
     (`keyring` or the `windows` crate), to be chosen and justified when the webhook toggle is built. Not built yet. The URL never goes in the settings JSON.
   - What is posted, and when, is **not designed**. The row stays Not investigated.
2. **RAM: "we will try our best to minimize the factor ... I do not mind the process taking up more than intended as long as it is as efficient as it can ever be."**
   The figures in the wireframes (Minimal under 50 MB, Full Feature Mode about 90–140 MB) are **goals, not limits**. No feature is disabled or refused
   for exceeding them. Efficiency is the requirement: subsystems start only when switched on, nothing polls while idle, and the telemetry reports what is
   actually measured. The UI says "goal", not "target", and keeps "unmeasured" until something measures it.
   Assumption, not confirmed: the goals describe the engine's own increase, and the whole app with its WebView2 process will be higher.
3. **Companion Editor Handshake** — a toggleable Full Feature Mode setting meant to give SUL's media clipping/editing core a direct, automated link
   (a handshake) with an external companion code or video editor. No editor (for example VS Code or Premiere) and no protocol (local WebSocket,
   named pipe, IPC bridge) has been chosen, so it is a placeholder: **Not investigated**, unimplemented. **Minimal Mode's auto-transfer is independent of it.**
4. **Defaults:** layout density Compact across the launcher modules; "Details →" opens the inspector inline, not in a separate window; the lightweight
   Discord Rich Presence tab/toggle stays; the General Playtime Tracker stays an explicit placeholder.
5. **Section 1 fixes before Section 2 code: approved; "wire it" for the search box.** Done on branch `fix/section1-defects-windows-only`
   (tab strip, wallpaper gallery, search narrowing the Game Library); see `PROJECT_STATUS.md`. Merged into `main` (abf8d45) on the owner's go-ahead; Windows CI on that merge passed.

## Wireframe changes these answers cause (the Media & Clipping Engine pane)
```
+- Media & Clipping Engine ------------------------- [Not investigated]
| Mode   [ Minimal | Full Feature Mode ]        goal: as lean as possible (unmeasured)
| View   [ Inherit Global | Always Compact | Always Detailed ]   now: Compact (from Global)
| Engine total   [ +11.0 MB | 0.4% CPU ]  [ Details -> ]
+----------------------------------------------------------------
+- Features ------------------------------------------------------   (Full mode only, at the bottom)
| [x] Auto-Event Log Watcher       [Experimental]       [ +6.2 MB | 0.1% CPU ] [Details ->]  Running       [Reset]
| [ ] Companion Editor Handshake   [Not investigated]   off     (no editor or protocol chosen yet)
| [x] Multi-Track Audio Stems      [Not investigated]   [ +3.1 MB | 0.2% CPU ] [Details ->]  Resetting...  [Reset]
| [ ] Automated Social/Discord Webhooks  [Not investigated]   off
|       shown when ticked:  Destination  Discord webhook  [ https://discord.com/api/webhooks/...a1b2 ]  [Test] [Remove]
|       Nothing is sent until this is ticked AND a URL is added. The URL is a secret: masked here, never logged.
|       What gets posted, and when, is not designed yet.
+----------------------------------------------------------------
Minimal mode:   Mode [ *Minimal | Full Feature Mode ]   goal: as lean as possible (unmeasured)
                [x] Auto-transfer clips to editor      <- the only control; works without the Companion Editor Handshake
```
One more drawing changes because of answer 1: Settings → Security & Trust, row "Data sharing", no longer says "Nothing leaves this PC" without
qualification. It now says nothing leaves this PC by default, and that the one switch that can send anything is Automated Social/Discord Webhooks
(off until ticked and given a destination). The exception must not make that pane overclaim.
Everything else in the wireframes is unchanged. The sample figures in the pane (such as "+6.2 MB") are illustrations, not measurements.

## Order of the slices, and where they stand
(1) The Display density setting (global Compact/Detailed plus the per-module override) and the inline Details inspector with no live data: **built and merged into `main`** (6902aeb, Windows CI passed). (2) The read-only Security & Trust pane: **built on a feature branch, awaiting the owner's go-ahead to merge**. (3) The Media &
Clipping Engine pane as a shell with every feature Not investigated. (4) The Rust telemetry sampler (CPU and threads are measurable; per-subsystem RAM, frame-time
variance and per-process interrupts are not, and stay Not investigated). (5) The webhook toggle with Credential Manager storage, last and only on its own go-ahead.
Slices 3 to 5 are not started; each needs the owner's go-ahead.

## Choices made while building slice 1 (easy to change)
- The Performance Overview is on the Diagnostics & Performance page, as drawn. Its one section is the Media & Clipping Engine, whose stored id is `clipping-engine`.
  It is not the Capture & Clips page (the clip library), which the wireframe keeps separate.
- Settings has no Media & Clipping tab yet, so for now the module's own strip is only on the Performance Overview; the Settings card edits the global view
  and lists and clears overrides. Both edit one stored value (`sul.density` in the browser's local storage).
- Compact shows "Engine total · RAM — | CPU —" and nothing else: dashes, never sample figures. The Details panel has the rows of the finished one, every figure a
  dash, and says in words that the sampler is not built.

## Choices made while building slice 2 (easy to change)
- **A correction to the wireframe's Data sharing row.** It said the one switch that can send anything is Automated Social/Discord Webhooks. That was incomplete: Discord Rich Presence, which is built, also sends data
  (the text "Browsing the launcher" and an optional small image name go to the Discord app, which shows them on the user's profile). The page names both, lists Rich Presence as Experimental with whether it is on right now,
  and lists the webhook toggle as Not investigated and not built. Rich Presence was already opt-in and off by default, so the default claim "SUL sends nothing off this PC" still holds.
- **"No network" is stated narrowly.** *(Corrected 2026-10-31. This bullet first said the Tauri framework brings an HTTP client, `reqwest`, into the build. That is wrong for Windows: it came from a cargo listing that merges every platform;
  Tauri declares `reqwest` only for Android and Apple mobile targets. See `docs/investigations/security-and-trust-2026-10-31.md`.)* SUL's own Rust code uses no network API; the Windows dependency list contains no HTTP, socket, TLS or WebSocket crate
  (`.github/scripts/check-network-crates.mjs`, run in CI); and the window may only load the app's own files. The page says traffic was never captured, that WebView2 was not audited, and that the inside of the framework and Windows crates was searched as text, not read.
  `ui/tests/no-network.test.ts` fails if network code appears in SUL's own source; it reads source text, so it is a tripwire, not a proof.
- **Rules are not capabilities.** Credentials and Data sharing are tagged "Design rule, not an audit" and carry no capability label; the five rows do. The headline is the weakest of the five (Not investigated).
- The page is read-only. Presence is switched under Settings > Discord; the page only shows whether it is on. Nothing on the page can change a setting.
- The `CHECKED_ON` date in `ui/src/security.ts` is the day the statements were checked against the code. It must be changed only when they are checked again.

