# 0004 — Section 2: owner's answers to the wireframe review
Date: 2026-10-29 · Status: accepted by owner (answers to the five questions asked after the Section 2 wireframes). **Nothing in Section 2 is built.**
Build starts only when the owner says so. The wireframes, with these answers applied, are saved in `docs/section-2-wireframes.md`.

## The owner's answers and what they mean
1. **Discord webhook: "make an exception."** The Discord rule (Rich Presence only; no chat overlay, friends list, login or webhook UI) is relaxed
   for one thing: a Discord webhook *destination* for the Media & Clipping Engine's "Automated Social/Discord Webhooks" toggle (Full Feature Mode).
   - Scope applied (an interpretation, easy to widen): **only** that destination field. The rest of the rule stands: no chat overlay, no friends list, no login.
   - Rules already in force that now bind this feature (`CLAUDE.md`): off by default and opt-in; uploads minimal; no secrets in source control or plaintext logs.
     A webhook URL is a secret (anyone holding it can post to that channel), so it is not kept in the settings JSON, not logged, and shown masked.
   - **Prerequisite found in the code:** the log redactor (`crates/sul-core/src/logging.rs`) masks by field name only (`password`, `passwd`, `token`,
     `secret`, `authorization`, `api_key`, `apikey`, `cookie`, `session`). A webhook URL under `webhook_url` or `url` would be written in clear.
     Before the feature ships: add `webhook` to that list and also mask any value that looks like `discord.com/api/webhooks/…`, with tests.
   - **Not decided (owner to choose):** where the URL is stored. Recommended: Windows Credential Manager (per user, encrypted by Windows),
     which needs one new dependency (`keyring` or the `windows` crate). Alternative: a file under the data folder encrypted with Windows DPAPI.
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
   (tab strip, wallpaper gallery, search narrowing the Game Library); see `PROJECT_STATUS.md`. Not yet merged into `main`.

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

## Still open before Section 2 code
- Where the webhook URL is stored (see 1).
- Go-ahead to build, and the first slice. Suggested order, smallest first: (1) the Display density setting (global Compact/Detailed plus the per-module
  override) and the inline Details inspector with no live data; (2) the read-only Security & Trust pane; (3) the Media & Clipping Engine pane as a shell
  with every feature Not investigated; (4) the Rust telemetry sampler (CPU and threads are measurable; per-subsystem RAM, frame-time variance and per-process
  interrupts are not, and stay Not investigated).
