# References: every link and named source

This is the one place for links and named sources. It lists the links the owner sent, the projects whose licences were checked, the pages
read for each investigation, the libraries the decision records checked, and the services the spec names. A test
(`crates/sul-core/tests/references.rs`) fails when a document in the repository carries a web address that is not listed here, so the list stays complete.

**Status words.** *Read* means a page was opened and its text used (through the fetch tool, which returns a short answer written by a small summarising
model, not the page itself). *Front page only* means that and no more. *Listed, not read* means a search or a document named it and nobody opened it.
*Not investigated* is the capability-matrix word: nothing was downloaded, built or run, so nothing here says the project works, is safe, or fits SUL.

**Two rules that apply to everything below.** SUL's own licence is undecided (`PROJECT_SPEC.md` section 7), so **no code is copied from any project listed
here**, and a GPL project would limit that choice. And nothing learned from a project below lets SUL bypass anti-cheat, access controls or a provider's
restrictions (`CLAUDE.md`).

## 1. Links the owner sent on 2026-11-05

The owner wrote: "these links will absolutely help you." Which module or question each one is meant to inform was not said, so none is tied to a
feature yet. Each was opened once (front page, through the fetch tool) on 2026-11-05 to record its own tagline and licence. **None was cloned, built, run,
or read beyond that.** The taglines are the repositories' own words; the notes are what SUL's rules mean for each, not a finding about the project.

| Link | Its own tagline | Licence the page shows | Status | Notes for SUL |
|---|---|---|---|---|
| https://github.com/morluto/rea | "Reverse engineer anything with agents, from app behavior down to native binaries." | MIT | Not investigated (front page only) | A reverse-engineering tool. If it is ever pointed at a game's files, the spec's rule applies: inspection only where the applicable permissions and licences allow it (`PROJECT_SPEC.md`, the research item on anti-cheat mods). |
| https://github.com/Zagitta/gpu-unlock | "Your GeForce runs AI at half speed. This fixes it for 20/30/40 series cards. Up to 2× FP16 / BF16 / TF32 tensor throughput." | GPL-2.0 (the README says the kernel module and local packaging are "GPL-2.0-only") | Not investigated (front page only) | Which operating system it supports was not checked. SUL is Windows only. GPL-2.0, so nothing may be copied. |
| https://github.com/Raphire/Win11Debloat | "A simple, lightweight PowerShell script that allows you to remove pre-installed apps, disable telemetry," (the tagline continues; the fetch cut it) | MIT | Not investigated (front page only) | A Windows system-settings script. If SUL ever offers a system change, it must be opt-in, previewed, logged and reversible like every other change. Nothing like it is built. |
| https://github.com/Gathin23/cleanyourtv | "Debloat any Android or Google TV over Wi-Fi and get an Apple TV-style home." | MIT | Not investigated (front page only) | Android TV. SUL is Windows only, so at most an idea source. |
| https://github.com/yuliskov/SmartTube | "Browse media content with your own rules on Android TV" | MIT | Not investigated (front page only) | Android TV. At most an idea source. |
| https://github.com/yuliskov/SmartTube/releases | The releases list of SmartTube | n/a | Front page only | The page showed "32.68 Stable" as the latest release (dated "20 Oct", no year in the answer) with APK builds for arm64-v8a, armeabi-v7a, universal and x86, each with a VirusTotal analysis. A link only: SUL does not download, keep or redistribute third-party binaries. |
| https://github.com/ReVanced/revanced-manager | "💊 Application to use ReVanced on Android." | GPL-3.0 | Not investigated (front page only) | Android. GPL-3.0, so nothing may be copied. |
| https://github.com/pi-hole/pi-hole | "A black hole for Internet advertisements." | The page links a LICENSE file; the answer did not name it | Not investigated (front page only) | A network-level blocker. SUL's network code lives only in `crates/sul-net` and is opt-in (decision 0006). |

## 2. Reference projects: licences and what SUL may do with them

Checked 2026-10-23 by reading each repository's LICENSE file and README (not legal advice). On 2026-11-05 each page below was opened once to confirm that
it exists and shows the same licence as recorded here. The five GitHub addresses were built from the `owner/repo` names this file always used.

| Project | Licence | What SUL does |
|---|---|---|
| [leem919/timedivers-manager-v2](https://github.com/leem919/timedivers-manager-v2) ("Version manager for Steam games.") | **No licence file** (all rights reserved by default) | Do not copy code. Its README documents the workflow openly (steamapps folders, SteamDB manifests, DepotDownloader or the Steam console, import version). SUL implements its own version of the idea. |
| [ebkr/r2modmanPlus](https://github.com/ebkr/r2modmanPlus) ("A simple and easy to use mod manager for several games using Thunderstore.") | MIT | May reuse code with the copyright notice kept. Profiles export as a `.r2x` zip (YAML) or as a share **code**; the code is an upload to Thunderstore's hosted API, so a SUL share code needs a service to host it. Plan: file export first, hosted code later (spec: do not assume hosting). |
| [desertcod98/DeSteam](https://github.com/desertcod98/DeSteam) (a Steam DRM unpacker) | GPL-3.0 | **Not used.** It removes Steam's copy-protection wrapper from game executables, which conflicts with the spec's no-DRM-bypass rule (and GPL-3 would apply to SUL). |
| [GabrielTaiga/Taiga-Launcher-for-Steam](https://github.com/GabrielTaiga/Taiga-Launcher-for-Steam) | Apache-2.0 | Adds non-Steam games to Steam. Not about blocking updates. May be consulted for ideas (game scanning, artwork); nothing copied. |
| [discord/discord-rpc](https://github.com/discord/discord-rpc) | MIT (deprecated by Discord: its README says it "has been deprecated in favor of Discord's GameSDK") | Protocol reference only. SUL has its own implementation: `crates/sul-core/src/discord.rs`. |
| Arsenal ([rsnl.gg](https://rsnl.gg)) | Not checked; closed source as far as known | No code copied. Features may be re-implemented independently. The address was taken from the earlier version of this file and not opened. |

## 3. Pages read for an investigation

| Address | What it is | Status | Where it is used |
|---|---|---|---|
| https://developer.valvesoftware.com/wiki/SteamCMD | Valve's developer wiki page on SteamCMD | Read (one page, 2026-11-03) | `docs/investigations/steamcmd-acquisition-2026-11-03.md` |
| https://steamdb.info/blog/manifest-request-codes | A SteamDB post, "manifest request codes" | Listed, not read | the same note |
| https://github.com/RaidingForPants/hd2-audio-modder | The Helldivers 2 audio modding tool. The owner confirmed on 2026-11-04 that this is the target; SUL registers it as an external tool | Read (front page, 2026-11-01). Capability: Not investigated | `docs/investigations/audio-modding-2026-11-01.md`, decision 0007, `crates/sul-core/src/builder/tools.rs` |
| https://github.com/RaidingForPants/hd2-audio-modder/releases/tag/v1.10 | Release notes of version 1.10 | Read (2026-11-01) | the audio note |
| https://boxofbiscuits97.github.io/HD2-Modding-Wiki/audio/overview.html | A community Helldivers 2 modding guide (audio overview) | Read (2026-11-01) | the audio note |
| `Dekr0/hd2-audio-modder` (a repository under another account, with a wiki "Usage" page; GitHub address assumed) | A search result | Listed, not read; how it relates to the repository above is unknown | the audio note |
| A Nexus Mods page for the audio tool (address not recorded) | A search result | Listed, not read | the audio note |

Named in the audio and SteamCMD notes and in section 2 but not opened: Audiokinetic **Wwise** (version 2023.1.7.8574 through the Audiokinetic Launcher, per the community guide),
**vgmstream**, **FFmpeg**, **DepotDownloader**, the **Steam console**, **SteamDB**.

## 4. Libraries and tools the decision records checked

Read from the sources the crate registry serves (downloaded and read in a Linux container; nothing built or run on Windows). Versions are as recorded on the
date in each record. This list is of what was named, not an endorsement.

| Name | What the repository's records say | Record |
|---|---|---|
| `steamworks` (steamworks-rs) | Version 0.13.1 source read 2026-10-27; used only to attach to a running Steam client | `docs/decisions/0003-steam-attach-launch-and-library-ipc.md` |
| `notify` | 6.1.1 resolves for `6.1`; 8.2.1 the latest stable and 9.0.0 a release candidate on 2026-10-31; uses `ReadDirectoryChangesW` on Windows | `docs/decisions/0006-spec-v5-1-directions-checked-against-the-crates.md` |
| `watch_dir` | 1.0.1 exists (the spec said 0.2); tails new text from files | 0006 |
| `reqwest`, `rustls`, `ring`, `aws-lc-rs` / `aws-lc-sys` | `reqwest` 0.12.28 and 0.13.5 compared in four dependency-graph variants; `rustls` 0.23 with `ring` recommended; not verified on Windows | 0006; `docs/investigations/security-and-trust-2026-10-31.md` |
| `ort` | No stable 2.0 on the registry; needs an exact `2.0.0-rc.N` | 0006 |
| `minisign-verify` | 0.3.0 exists; key custody undefined | 0006 |
| `whisper-rs`, `piper-rs` | 0.16.0 and 0.2.0 exist; Windows build, VRAM use and latency untested | 0006 |
| `egui`, `whisper.cpp`, Piper | Named by spec V5.1 (overlay and local speech); their addresses are not recorded in the repository | 0006 |
| `tokio`, `windows`, `windows-sys`, `wry`, Tauri 2 | Read as part of the Windows dependency graph; 42 network-related crates are on the deny list in `.github/scripts/check-network-crates.mjs` | the security note |

## 5. Services and projects the spec names (no link in the spec; none investigated)

Every one of these is **Not investigated** unless a section above says otherwise. The spec says the exact identity or access level of several is unconfirmed.

- **Medal.tv** (module 6): the integration may use only documented interfaces; which API access level is approved is an open question in the spec.
- **Nexus Mods, Mod.io, "AIK Mods"**: the spec says to investigate them; the identity and API of "AIK Mods" are unconfirmed.
- **Steamworks, SteamCMD, depots and manifests**: the spec says to respect account permissions, access controls and distribution terms.
- **Discord**: Rich Presence over the local app, and webhooks (decisions 0002 and 0004, `docs/discord.md`).
- **Emulators** (module 9): the spec names Dolphin, PCSX2 and RPCS3, to be investigated one at a time.
- **Game adapter targets**: Helldivers 2 (isolating version 7.1.1 is a separate track), Terraria, RimWorld, Stardew Valley; Project Zomboid later. The owner's plan also names Elite Dangerous (a controller preset).
- **Thunderstore**: the hosting service behind r2modman's share codes (section 2).

## 6. Files the owner sent, and where they are

| File | What it is | Where |
|---|---|---|
| `PROJECT_SPEC.md` | The product spec, v1.0 (2026-10-22: "a planning baseline, not proof") | repository root |
| `CLAUDE.md` | Operating instructions for work on the repository | repository root |
| Four design mockups | Conceptual references for hierarchy and status wording; not final assets | `docs/design-references/01-launcher-overview.png`, `02-build-manager.png`, `03-mod-builder.png`, `04-capture-medal.png`. The spec and `CLAUDE.md` call the folder `design-references/` at the root; the files are under `docs/` |
| Spec "V5.1" | A longer spec with architecture directions, sent 2026-10-31 in the conversation; **not stored in the repository** | its dependency lines and directions are checked in decision 0006 |
| The first archive of legacy files | Unpacked and sorted into what to keep and what to remove | `docs/decisions/0002-step-1-5-consolidation.md` |

## 7. This project's own addresses

- https://github.com/TheQwontonSoup27/Soups-Unified-Launcher-SUL- (the repository; in `Cargo.toml` and `PROJECT_SPEC.md`)
- Setup pages the docs send a person to: https://rustup.rs (`docs/SETUP-WINDOWS.md`) and https://discord.com/developers/applications (`docs/discord.md`)

## 8. Addresses in the repository that are not references

These appear in code, tests and example text. They are deliberately not links to follow.

- Made-up Discord webhook addresses used as test inputs and as examples of what the log redactor masks (they start with `https://discord.com/api/webhooks`, or
  with `https://relay.example`, `https://example.com`, `https://github.com/owner/repo/settings/hooks`). None is real.
- Local development addresses: `http://localhost:1420` (`src-tauri/tauri.conf.json`), `http://localhost:5173/` (`ui/tests/dom-env.ts`), `http://tauri.localhost` (`src-tauri/src/ipc_tests.rs`).
- `https://schema.tauri.app/config/2`: the JSON schema named inside `src-tauri/tauri.conf.json`.

## How to add a link

Put it in the section it belongs to with its status. If it was not opened, say so. The test in `crates/sul-core/tests/references.rs` reads the Markdown
documents of the repository and fails if one of them carries a web address that this file does not contain.
