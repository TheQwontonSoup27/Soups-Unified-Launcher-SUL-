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

## 1. Links the owner sent

**SUL in two sentences, for a reader who has not seen it.** SUL is a Windows-only, open-source, modular game launcher (a Tauri 2 desktop app with a Rust core and a React/TypeScript interface). Its ten modules cover game library and build management, mod management and a mod builder, security and trust, diagnostics and compatibility, social features, gameplay capture and clipping, remote play, news and updates, emulation, and theming. Its standing rules (`CLAUDE.md`): original game installs are never modified; nothing bypasses anti-cheat, access controls or a provider's restrictions; telemetry and uploads are opt-in and minimal; no third-party game files or binaries are redistributed.

The owner sent eight links on 2026-11-05 ("these links will absolutely help you") and ten more on 2026-11-09. Which module or question each one is meant to inform was not said for either batch, so none is tied to a feature yet.

**How these rows were made.** Each page was opened once through the fetch tool, which returns a short answer written by a small summarising model, not the page itself. The first eight were opened on 2026-11-05 for the tagline and licence and again on 2026-11-09 for the plain description; the ten new ones were opened on 2026-11-09. **None was cloned, built, run, or read beyond its front page.** The taglines are the repositories' own words. "What it is and does" is a short plain-words summary of what the page said, so a reader can grasp the capability without opening the link; it is not a test result and says nothing about whether the project works, is safe, or fits SUL. "Notes for SUL" is what SUL's rules mean for each one, not a finding about the project.

### 1a. Sent on 2026-11-05

| Link | Its own tagline | What it is and does | Licence the page shows | Status | Notes for SUL |
|---|---|---|---|---|---|
| https://github.com/morluto/rea | "Reverse engineer anything with agents, from app behavior down to native binaries." | A toolkit that lets AI coding agents (through MCP, or through its own `rea` command-line tool) inspect targets such as native binaries, JavaScript/Electron apps, .NET assemblies, Android APKs, firmware, websites and running-process behaviour, and report findings together with the evidence. Needs Node.js 22.x, 24.x or 26.12.1 or newer. An overall list of supported platforms is not stated. | MIT for its own code; third-party material under its own licences | Not investigated (front page only) | A reverse-engineering tool. If it is ever pointed at a game's files, the spec's rule applies: inspection only where the applicable permissions and licences allow it (`PROJECT_SPEC.md`, the research item on anti-cheat mods). |
| https://github.com/Zagitta/gpu-unlock | "Your GeForce runs AI at half speed. This fixes it for 20/30/40 series cards. Up to 2× FP16 / BF16 / TF32 tensor throughput." | A Linux kernel module that, at boot, removes NVIDIA's cap on half-rate FP16, BF16 and TF32 tensor maths on GeForce RTX 20, 30 and 40 cards (30 series stable, 20 and 40 experimental, 50 unsupported), with no driver patch or firmware flash. Results are identical, only faster, for AI prefill, fine-tuning and image generation; the page says there is no gain for gaming, token decoding or INT8/FP8/INT4 models. The README warns that laptop unlocks can silently reset and that a hung card keeps its memory buffers reserved until power-off. | GPL-2.0 (the README says the kernel module and local packaging are "GPL-2.0-only") | Not investigated (front page only) | Linux only according to its README (the author does not maintain a Windows version, and it says to treat unverified Windows installers with extreme caution), so it cannot run on SUL's platform. GPL-2.0, so nothing may be copied. |
| https://github.com/Raphire/Win11Debloat | "A simple, lightweight PowerShell script that allows you to remove pre-installed apps, disable telemetry," (the tagline continues; the fetch cut it) | A PowerShell script for Windows 10 and 11 that removes preinstalled apps, turns off telemetry and other tracking, and applies customisations. It needs no installation: run a one-line PowerShell command, or double-click Run.bat and accept the administrator (UAC) prompt, or run the .ps1 file from an elevated PowerShell. It takes command-line parameters. The page says all changes can be reverted and most apps reinstalled from the Microsoft Store (exact steps not stated) and says to use it at your own risk. | MIT | Not investigated (front page only) | A Windows system-settings script. If SUL ever offers a system change, it must be opt-in, previewed, logged and reversible like every other change. Nothing like it is built. |
| https://github.com/Gathin23/cleanyourtv | "Debloat any Android or Google TV over Wi-Fi and get an Apple TV-style home." | Turns on network debugging (ADB) on an Android or Google TV, then uses scripts on a computer on the same network to back up the TV's settings, disable (not uninstall) manufacturer telemetry, ads and preloaded apps, and install a small replacement home-screen launcher; a restore script undoes it. The computer needs Python 3, JDK 17 to 22 and Android SDK platform 34. Fire TV gets only the debloat part; Samsung, LG, Roku, Vizio and Hisense are unsupported. Warnings: ADB is unencrypted so it should be turned off afterwards, and firmware updates can bring removed packages back. | MIT | Not investigated (front page only) | Android TV. SUL is Windows only, so at most an idea source. |
| https://github.com/yuliskov/SmartTube | "Browse media content with your own rules on Android TV" | A free, open-source media client for Android TVs and TV boxes (for example NVIDIA Shield and Chromecast with Google TV, Android 4.3 or newer). TV-friendly interface with SponsorBlock integration, adjustable playback speed, 60 fps and HDR where the hardware allows, live chat, and casting from a linked phone. Not for phones, tablets, Tizen, webOS or Apple TV. Installed by sideloading an APK rather than from an app store; the README warns that some past builds were affected by malware, so downloads must be verified. | MIT | Not investigated (front page only) | Android TV. At most an idea source. |
| https://github.com/yuliskov/SmartTube/releases | The releases list of SmartTube | The page where SmartTube's installable APK files are published, one set per CPU type, each with a VirusTotal analysis link. | n/a | Front page only | The page showed "32.68 Stable" as the latest release (dated "20 Oct", no year in the answer) with APK builds for arm64-v8a, armeabi-v7a, universal and x86, as seen on 2026-11-05. A link only: SUL does not download, keep or redistribute third-party binaries. |
| https://github.com/ReVanced/revanced-manager | "💊 Application to use ReVanced on Android." | An Android app that applies patches to other Android apps using ReVanced Patcher. It includes a downloader for fetching the app and lets the user choose which patches to apply. How the patching works inside, and which patches exist, is not stated on the page. | GPL-3.0 | Not investigated (front page only) | Android. GPL-3.0, so nothing may be copied. |
| https://github.com/pi-hole/pi-hole | "A black hole for Internet advertisements." | A network-wide blocker of ads and unwanted content that runs on your own Linux machine as a DNS sinkhole (it can also act as the network's DHCP server). It has a web dashboard and a command-line interface and needs no software on the other devices: you point your router's DHCP clients at it for DNS. Installed with a one-line curl command, from a cloned repository with bash, or with Docker. | The page links a LICENSE file; the answer did not name it | Not investigated (front page only) | A network-level blocker. SUL's network code lives only in `crates/sul-net` and is opt-in (decision 0006). |

### 1b. Sent on 2026-11-09

Two of these are organisation pages, not single projects (n8n-io and calendly). One page could not be read (calendly). One is something SUL must not use (the last row).

| Link | Its own tagline | What it is and does | Licence the page shows | Status | Notes for SUL |
|---|---|---|---|---|---|
| https://github.com/yt-dlp/yt-dlp | "A feature-rich command-line audio/video downloader" | A command-line program (a fork of youtube-dl) that downloads audio and video from thousands of websites. It can choose formats, merge separate video and audio streams with ffmpeg, extract audio, embed thumbnails and metadata, fetch subtitles and use SponsorBlock data. Standalone binaries for Windows, Linux/BSD and macOS, or install with pip on Python 3.11 or newer; the page also mentions use as a library. | Unlicense (the page says the release executables include GPLv3+ code) | Not investigated (front page only) | A tool a person installs and runs themselves. If SUL ever registered it as an external tool (as it does the audio tool, decision 0007), SUL would still not download, keep or redistribute the binary. The terms of the sites it downloads from apply (`CLAUDE.md`). Nothing is built or checked. |
| https://github.com/ollama/ollama | "Get up and running with Kimi, GLM, MiniMax, DeepSeek, gpt-oss, Qwen, Gemma and other models." | A program that runs open AI language models on your own computer (or in the cloud) and lets other tools talk to them. Used from a command line (`ollama run` with a model name), through a REST API on localhost port 11434, or through Python and JavaScript libraries. Install commands for macOS, Windows and Linux, plus an official Docker image. | MIT | Not investigated (front page only) | A local model runner. Spec V5.1 keeps voice and text AI local (whisper.cpp, Piper) under a small VRAM budget (decision 0006); nothing here has been tested against that budget and no AI feature is built. At most an idea source. |
| https://github.com/immich-app/immich | "High performance self-hosted photo and video management solution." | A self-hosted server for your own photos and videos, with web and mobile apps: upload and view, automatic backup, duplicate prevention, several users, albums and sharing, and search by metadata, objects, faces and CLIP (describe a picture in words to find it). Install steps are in its own documentation site; which operating systems it runs on is not stated on the page. | AGPL-3.0 | Not investigated (front page only) | A media-library server. At most an idea source for browsing and searching a library of captured clips. AGPL-3.0, so nothing may be copied. |
| https://github.com/openai/whisper | "Robust Speech Recognition via Large-Scale Weak Supervision." | OpenAI's open speech-recognition model: multilingual speech recognition, speech translation and language identification, in six sizes (tiny, base, small, medium, large, turbo). Used from the command line or from Python; needs ffmpeg and Python with PyTorch, and roughly 1 to 10 GB of video memory depending on model size (other hardware needs not stated). | MIT (code and model weights) | Not investigated (front page only) | The original Python model. Spec V5.1 names local speech-to-text through whisper.cpp, a separate C++ port, and decision 0006 checked the `whisper-rs` crate; neither is built. The page's video-memory figures are above the 500 MB budget the owner set for local speech, so this model as published would not fit that budget unchanged. |
| https://github.com/plausible/analytics | "Open source, privacy-first web analytics. Lightweight, cookie-free Google Analytics alternative. Self-hosted or cloud." | A website-analytics service that shows visitor numbers on one page: goals and conversions, a real-time view, emailed reports, a Google Search Console link, API and CSV export. The page says it stores no personal data or IP addresses and uses no cookies. Runs as a paid cloud service (hosted in the EU) or self-hosted as the Community Edition, which lacks premium features such as funnels and single sign-on. | AGPLv3 for the Community Edition; MIT for the tracker script | Not investigated (front page only) | A website tool, not a desktop one. At most an idea source for what privacy-respecting measurement looks like. SUL's telemetry is opt-in and minimal (`CLAUDE.md`) and none is built. AGPL, so nothing may be copied. |
| https://github.com/appflowy-io/appflowy | "Bring projects, wikis, and teams together with AI." | An open-source alternative to Notion: notes and wiki-style documents, databases for tasks and projects with kanban boards, comments on rows, AI summaries and an "ask AI" feature for team wikis. Apps for macOS, Windows and Linux, plus iPhone and Android 10 or newer; it can be self-hosted. Built with Flutter and Rust. | AGPL-3.0 | Not investigated (front page only) | A documents-and-projects app. At most an idea source (SUL uses Rust with React inside Tauri). AGPL-3.0, so nothing may be copied. |
| https://github.com/penpot/penpot | "Penpot: The open-source design platform for Product teams that need scalable collaboration." | An open-source design and prototyping tool that runs in the browser, hosted or self-hosted (Docker and Kubernetes are mentioned). Real-time collaboration, design tokens, components and variants, CSS Grid and Flex layout, an inspect mode that outputs SVG, CSS and HTML, a plugin system, and an open API with an MCP server. Built on open standards: SVG, CSS, HTML and JSON. | MPL-2.0 | Not investigated (front page only) | A design tool. At most an idea source for design tokens and theming (module 10); SUL already has theme tokens of its own. Nothing copied. |
| https://github.com/n8n-io | An organisation page, so there is no single tagline | The GitHub organisation of n8n, a company based in Germany. Its main project, n8n, is a node-based workflow-automation tool that combines a visual builder with custom code, can be self-hosted or used in the cloud, and has 400+ integrations. The page lists 10 of the organisation's 53 repositories, including n8n, n8n-docs, n8n-hosting, task-runner-launcher and Terraform modules for Google Cloud and AWS. | The page calls n8n "fair-code" and does not name the licence; not checked | Not investigated (organisation page only) | An organisation page, not one project. At most an idea source for webhook and automation flows (the owner's clipping plan has a Discord webhook toggle). Fair-code is not the same as an open-source licence, so check the actual licence before relying on any of it. |
| https://github.com/calendly | Not read | **Not read:** the fetch failed twice (a rate limit, then a server error) and a web search did not show the page's contents. Only the name is known, and that from general knowledge, not from the page: Calendly is a scheduling service. What the organisation's public repositories contain was not checked. If the open-source scheduler Cal.com was meant, send that link. | Not checked | Not investigated (page could not be read) | Nothing known ties it to a SUL module. |
| https://gist.github.com/aamiaa/204cd9d42013ded9faf646fae7f89fbb | The gist's title is "Complete Recent Discord Quest" (author aamiaa) | A script, run in a browser console, that completes the Discord "quests" a user is enrolled in by faking the video-watching, game-playing, streaming or activity progress Discord asks for, so the reward is earned without doing the task. The page itself warns that Discord has begun suspending accounts caught automating this. | GPL-3.0 | Not investigated (front page only) | **Not usable by SUL.** It works by spoofing progress to get around a provider's rules and risks account suspension. `CLAUDE.md` forbids bypassing provider restrictions, and it goes beyond what SUL's Discord support does (Rich Presence over the local app, decisions 0002 and 0004). Listed only because the owner asked for every link to be kept. No code was copied or used. Running it is the person's own risk to their Discord account. |

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
