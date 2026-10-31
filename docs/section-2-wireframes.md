# Section 2 wireframes (design only: nothing here is built)

Saved on 2026-10-29 from the review in the conversation, with the owner's answers applied (`docs/decisions/0004-section-2-decisions-from-the-wireframe-review.md`).
Where an answer changed a drawing, the drawing below shows the changed version. The sample figures in the drawings (such as "+6.2 MB") are
illustrations, not measurements. Every capability shown starts as **Not investigated** or **Experimental**; none is claimed tested.

Status of the Section 1 items the drawings touch (branch `fix/section1-defects-windows-only`): the Settings tab strip no longer collapses, and the sidebar
search is **built, for games only**: it narrows the Game Library by name, publisher and source, jumps there from other pages, and Escape clears it. The
grouped pop-up drawn in W2 (games, settings, modules) is **not built**; that is a possible later step.

These follow the implemented UI: the 280px sidebar, 40px title bar, `.card`, `.badge-*`, `.btn`, `.seg`, `.matrix-row` and the existing colour tokens.
No new colours or components, except the few listed at the end of 4d.

## Wireframes

These follow the implemented UI: the 280px sidebar, 40px title bar, `.card`, `.badge-*`, `.btn`, `.seg`, `.matrix-row` and the existing colour tokens. No new colours or components, except the few listed at the end.

### W1. Shell and sidebar stack (positions unchanged)

```
+---------------------------------------------------------------------------------------+
| (logo) Soups Unified Launcher    Early development - no features tested yet  _  []  x |
+----------------------------+----------------------------------------------------------+
| (logo)[ Search... ]  [RPC] |                                                          |
| presence message (12px)    | .content   padding 24/32, scrolls                        |
|                            |                                                          |
| Game Library               | +--------------------+                                   |
|                            | | Page title         |  .page-header                     |
| ROADMAP MODULES            | +--------------------+                                   |
| Mods & Mod Builder         |                                                          |
| Diagnostics & Performance  | <- hosts the Performance Overview (new)                  |
| Social & Networking        |                                                          |
| Capture & Clips            | <- clip library (Medal links), not engine config         |
| Remote Play                |                                                          |
| News & Updates             |                                                          |
| Emulation                  | <- registered emulators + status                         |
|    (spacer)                |                                                          |
| [ Settings ]  (active)     | <- engine config lives under Settings                    |
| General Playtime Tracker   | +------------------------------------------------+       |
|                            | | status bar (.status)                           |       |
+----------------------------+----------------------------------------------------------+
```

### W2. Top header, global search and the RPC master toggle

```
Title bar (40px, --titlebar)
 (logo 24px) Soups Unified Launcher ........ note ........ _ [] x
 Asset check: ui/public/logo.png 256x256 -> splash, title bar, sidebar (all load); favicon.png 64x64

Sidebar top row (existing)            RPC chip (existing, aria-checked)
 (logo 32px) [ Search...      ] [RPC]   off: outlined   on: filled --button
 presence message under the row (existing): "Discord presence is off."

Search focused, typing "hel". BUILT 2026-10-29 for games only: the Game Library narrows as you type.
The grouped pop-up below (games, settings, modules) is NOT built.
 (logo) [ hel|                 ] [RPC]
 +--------------------------------------+   popup colours (--popup / --popup-text)
 | GAMES                                |
 |  > Helldivers 2           Steam      |
 | SETTINGS                             |
 |    Media & Clipping Engine           |
 | MODULES                              |
 |    Diagnostics & Performance         |
 | Enter opens first | Esc closes | local index only (no network)
 +--------------------------------------+

RPC chip = master switch over presence only. Per-game capability gating stays
(Unsupported / Not investigated games never publish). No chat, friends or login UI. (The single webhook destination field
that decision 0004 allows lives in Media & Clipping, never here.)
```

### W3. Emulated game card (a new expander beside the existing matrix expander)

```
+- .game-card (emulated) ------------------------------------
| Example Game                          [Experimental]
| [Emulated] [Present]
| D:\Games\Example\game.xyz
| [ Play ] [ Compatibility matrix v ] [ Emulation v ]   <- NEW, same expand pattern as the matrix
|-----------------------------------------------------------
| EMULATION                                [Not investigated]
| Emulator core   [ pick a registered emulator        v ]
|                 SUL ships no emulators, BIOS, keys or ROMs. Add the path to yours.
|                 [ Add emulator... ]
| Graphics API    [ Auto | D3D11 | D3D12 | Vulkan | OpenGL ]   <- .seg; values the core
|                 can't use are disabled (.seg button:disabled)
| Launch args     [ --fullscreen                       ]
|                 Runs as: "C:\Emu\core.exe" "D:\...\game.xyz" --fullscreen
|                 Split into an argument list and run directly. No shell, no pipes or redirects.
| [ Save ] [ Reset to core default ]
+-----------------------------------------------------------
```

### W4. Settings sub-panes

The tab strip becomes five tabs, and I'd keep the existing Discord (RPC-only) tab. It needs the `.seg` fix first, plus `overflow-x:auto` for narrow windows.

```
Settings  (.page-header)
[ Appearance | Discord | Security & Trust | Controller | Media & Clipping ]   .seg role=tablist
```

#### 4a. Appearance (existing cards unchanged, one new card)
BUILT 2026-10-30, merged into `main` (6902aeb): the card, the per-module strip and the inline Details panel. Differences from the drawing: nothing is measured yet,
so the compact line reads `Engine total · RAM — | CPU —` instead of sample figures, and the card says what the settings mean; the per-module strip is on the Performance Overview (there is no Media & Clipping tab yet).

```
+- Theme ................ (unchanged)
+- Colours (15 tokens, 4.5:1 check) ... (unchanged)
+- Background: solid | gallery | custom, readability dimming ... (unchanged)
+- NEW: Display density ----------------------------------------
| Global master switch   [ Compact | Detailed ]        (default: Compact)
| Compact:  [ +6.2 MB | 0.1% CPU ] [ Details -> ]
| Detailed: full telemetry cards
| Overrides in use: Media & Clipping = Always Detailed   [ Clear overrides ]
+----------------------------------------------------------------
```

#### 4b. Security & Trust (only what exists, nothing overclaimed)
BUILT 2026-10-30 on branch `feature/section-2-security-trust-pane` (not merged). Differences from the drawing: the Data sharing row also names Discord Rich Presence (it can send data too, and is Experimental, off by default,
with "Right now: on/off"); the evidence for each rule is written out with what was not checked; the five rows are the two senders and the three protections; no controls. See decision 0004, slice 2.

```
+----------------------------------------------------------------
| [Not investigated]   headline = weakest row below
| Credentials   SUL stores no passwords, tokens or API keys.          (design rule, not an audit)
| Data sharing  Nothing leaves this PC by default. Telemetry and uploads are off. The one switch that can
|               send anything is Automated Social/Discord Webhooks (Media & Clipping, Full mode): off until
|               ticked and given a destination.
| Package scanning ................ [Not investigated]
| Quarantine ...................... [Not investigated]
| Integrity checks ................ [Not investigated]  (confirmed vs suspicious kept separate)
+----------------------------------------------------------------
```

#### 4c. Controller: Switch Pro preset for Elite Dangerous

```
+- Switch Pro mapping preset - Elite Dangerous ----- [Not investigated]
| Controller  Nintendo Switch Pro        status: Detected / Not detected (read-only)
| Preset      [ SUL default - Switch Pro v ]  [ Duplicate ]
| Input            Elite Dangerous action          (example rows, real list comes from the game's format)
| Left stick       Pitch / Yaw                      [ Rebind ]
| ZR               Fire primary                     [ Rebind ]
| [ Preview changes ] [ Apply ] [ Restore previous ]
| Apply backs up the current bindings first, never overwrites the original,
| and Restore puts it back. Per-game on/off stays in the matrix "Controller remap" tick.
+----------------------------------------------------------------
```

#### 4d. Media & Clipping Engine

Full Feature Mode, Compact view:

```
+- Media & Clipping Engine ------------------------- [Not investigated]
| Mode   [ Minimal | Full Feature Mode ]        goal: as lean as possible (unmeasured)
| View   [ Inherit Global | Always Compact | Always Detailed ]   now: Compact (from Global)
| Engine total   [ +11.0 MB | 0.4% CPU ]  [ Details -> ]
+----------------------------------------------------------------
+- Features ------------------------------------------------------   (Full mode only, at the bottom)
| [x] Auto-Event Log Watcher     [Experimental]       [ +6.2 MB | 0.1% CPU ] [Details ->]  Running        [Reset]
| [ ] Companion Editor Handshake [Not investigated]   off     (no editor or protocol chosen yet)
| [x] Multi-Track Audio Stems    [Not investigated]   [ +3.1 MB | 0.2% CPU ] [Details ->]  Resetting...   [Reset]
| [ ] Automated Social/Discord Webhooks [Not investigated]   off
|       shown when ticked:  Destination  Discord webhook  [ https://discord.com/api/webhooks/...a1b2 ]  [Test] [Remove]
|       Nothing is sent until this is ticked AND a URL is added. The URL is a secret: masked here, never logged.
|       What gets posted, and when, is not designed yet.
+----------------------------------------------------------------
```

Minimal mode shows only this:

```
| Mode [ *Minimal | Full Feature Mode ]   goal: as lean as possible (unmeasured)
| Engine total [ +x MB | y% CPU ] [ Details -> ]
| [x] Auto-transfer clips to editor        <- the only control; works without the Companion Editor Handshake
```

Detailed inspector (opens inline under the row, like the matrix overlay; also used on Performance Overview):

```
+- Auto-Event Log Watcher [Experimental] ------------------- Running
|                         Predicted spec   Live (10 s EMA)   Delta
| RAM (this subsystem)       +6.0 MB          +6.2 MB        +0.2 MB
| CPU (this subsystem)        0.1 %            0.1 %          0.0
| Frame-time variance          --              n/a            --
| Active threads   #7412 log-tail  #7413 parser
| Kernel interrupts   1,820 /s system-wide (not attributable to SUL)
| 1 Hz sampling, EMA alpha = 2/(10+1) = 0.18. Local only, nothing uploaded.
| [ Reset subsystem ]   Last reset 14:02:11 OK
+----------------------------------------------------------------
```

Async reset indicator (per subsystem, in an `aria-live="polite"` region, text plus badge so colour isn't the only signal):

```
Running  ->  Resetting... (UI stays usable)  ->  Reset OK 14:02  |  Reset failed: <reason> [Retry]
```

Reset restarts only that subsystem. It never touches saved clips or game files.

Performance Overview (Diagnostics & Performance page): each module section header carries the same `[ Inherit Global | Always Compact | Always Detailed ]` strip. It edits the same stored value as the Settings pane, so there are two doors to one setting. Effective view = the local value unless it is Inherit, in which case the global value.

**New UI pieces needed:** a styled `<select>` (reusing the `input[type=text]` look), a small telemetry-badge style built from `.badge`, and a table style for Predicted vs Live. Feature toggles reuse the `.matrix-row` / `.matrix-check` pattern.
