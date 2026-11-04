# Audio modding tools (spec 2.4): what was read, what was found, what is not established

Date of the reading: 2026-11-01 (evening, Toronto time). Written down 2026-11-03, because the code points here
(`builder::tools::known_tools`) and the file had not been saved.

## The question

Spec 2.4 says: integrate the existing **Raiding from Pants / Pants' HD2 Audio Modder** workflow "where technically and
legally possible", investigate Wwise workflows and repacking requirements, and "treat the tool as an external
integration unless redistribution permission is established".

## How it was checked, and how far that goes

- One web search, and three public pages read with the fetch tool. The fetch tool gives back an answer written by a
  small summarising model, not the page itself, so the wording below is that summary, not a line-by-line reading.
- **Nothing was downloaded, installed or run.** No release file was opened, no source code read, and the text of the
  licence was not read.

Pages read:

1. `https://github.com/RaidingForPants/hd2-audio-modder/releases/tag/v1.10` (release notes)
2. `https://github.com/RaidingForPants/hd2-audio-modder` (repository page)
3. `https://boxofbiscuits97.github.io/HD2-Modding-Wiki/audio/overview.html` (a community modding guide)

The search also listed a repository under another account (`Dekr0/hd2-audio-modder`, a wiki "Usage" page) and a Nexus
Mods page. They were not read, so how that repository relates to the first one is not known.

## What the pages said

| Point | What was found | Where |
|---|---|---|
| What the repository is | `hd2-audio-modder`, by the GitHub user `RaidingForPants`, for modding Helldivers 2 audio (sound banks, music segments, in-game text) | 1 |
| Licence | The file list shows a LICENSE file, and the README navigation and the About sidebar link it as the "MIT license". The licence text was not shown. There is no separate licence badge | 2 |
| Redistribution or permission statement | None in the README. Its only related line credits the Helldivers 2 Blender add-on authors "for letting me use some of their code" | 2 |
| Release notes | The 1.10 notes state no licence and no redistribution terms | 1 |
| Platforms | The README has set-up sections for Windows, Linux and macOS. The 1.10 notes mention audio playback on Linux and macOS | 1, 2 |
| Prebuilt programs | The README says prebuilt executables made with PyInstaller are in the releases tab | 2 |
| What it needs | Python 3.10 or later to run from source; vgmstream for audio playback (to be placed beside the Python code); PortAudio and tkinter on Linux, PortAudio and Python on macOS. Wwise and FFmpeg are not mentioned | 1, 2 |
| Name | The community guide calls it "the Audio Modding Tool" (`audio_modder.exe`). It does not use the name "Pants' HD2 Audio Modder" | 3 |
| Game format | "Helldivers 2 can only read .wem files"; other audio is converted to .wem | 3 |
| Wwise | The guide lists Wwise, version 2023.1.7.8574 through the Audiokinetic Launcher, as what converts audio to .wem. It does not say whether Wwise is needed to repack | 3 |
| Output | The generated mod file goes into the game's `data` folder beside the vanilla files; its name and format are not given | 3 |

## What is not established

- That `RaidingForPants/hd2-audio-modder` is the tool the spec means by "Raiding from Pants / Pants' HD2 Audio Modder".
  The guide uses a different name. **This needs the owner's confirmation.**
- The text of the licence, and whether an MIT licence covers everything in a release, including vgmstream and any
  other part that comes with a prebuilt program.
- Audiokinetic's (Wwise) licence terms. They were not read, so nothing is assumed about redistribution.
- Whether Wwise is needed to repack, and whether another way to make `.wem` files exists.
- Whether the tool works with the current game version, whether the game accepts what it writes, and anything about
  anti-cheat. SUL has never run it.

## What this means for SUL

- Both tools are **Not investigated** in the capability matrix. Reading three pages is not an assessment of
  feasibility.
- SUL records an external tool and runs it only if the owner has pointed it at a copy they installed themselves. It
  does not copy, bundle or download either tool (`crates/sul-core/src/builder/tools.rs`, decision 0007).
- The audio part of the Helldivers 2 workflow is not built, and no real game adapter exists yet, so nothing here
  can be tried end to end.
