# 0007 — Spec Module 1 and items 2.1 to 2.4: what was built, and the calls made on the way

Status: built on branch `feature/spec-modules-1-and-2-1-to-2-4`. **Not merged into `main`**; that waits for the owner's go-ahead.

## The request and its scope

Owner, 2026-11-01 (sent again 2026-11-03): "implement steps 1 to 2.4 and stop at 3, then report back". Confirmed with the owner as:
Module 1 (spec 1.1 to 1.5; 1.6 save-data transfer is marked *deferred* in the spec) and Module 2 items 2.1 to 2.4. Item 2.5 (Blender),
Module 3 (Security and Trust) and everything after it are not touched.

Owner answers used (2026-11-01):

- **Build manifest schema and migration** (spec section 7, an open decision): keep the existing JSON file with an integer
  `schema_version`, and migrate forward only. A newer version than this build knows is refused; an older one is upgraded one step
  at a time, and an exact copy of the old file is kept first.
- **First complete game adapter** (spec section 7): none yet. The adapter interface is tested with made-up game folders.
  The four named games are listed as *Not investigated*. The Helldivers 2 7.1.1 isolation stays a separate track.
- Windows only (earlier decision): nothing is written for other systems.

## What was built

Everything is library code in `crates/sul-core`. **No Tauri command and no UI calls any of it yet**, so using the app today
does none of this. Every test uses made-up folders and files; no real game, mod or tool was ever involved. Every row is
labelled *Experimental* in the capability matrix, and the real-game rows are *Not investigated*.

| Spec | Where | What it does |
|---|---|---|
| 1.1 Game library | `adapters.rs`, `catalogue.rs` | The one interface a game's specifics go through (recognise a folder and its version, plan a launch, say where mods may and may never write, declare capabilities). Owner-chosen categories and artwork (a reference to an image file the owner already has, never copied or fetched), search, and a detail view whose label is the weakest label on it. No adapter for a real game exists |
| 1.2 Installation | `builds.rs` | Import an existing install into a managed root as a copy, proven equal to the source before it is usable; register an install that stays where it is (only if its folder is already a protected location, so SUL cannot write into it). Nothing is downloaded (see "Not built") |
| 1.3 Build and version manager | `builds.rs`, `manifest.rs` | Original, Optimized, Legacy, Experimental and named slots; verify; switch the active build with one atomic write; roll back to the latest earlier build that still verifies; repair from a named source only with content whose hash matches the build's own manifest; migrate to another root. Manifest schema 2 adds source/manifest id, patches, profiles, compatibility notes and verification/update times |
| 1.4 Launch and runtime manager | `runtime.rs`, `process.rs` | Start only a Ready build, through its adapter's plan, with no shell, the program checked to lie inside the build folder, bounded arguments and environment; one running instance per game; follow the process and explain why it did not start or ended early; diagnostic output only when asked, capped and redacted |
| 1.5 Storage and library locations | `roots.rs` | Several roots on several drives, each with a marker file so a moved or swapped drive is noticed, relinking only to a folder with the right marker, no overlap between roots or with a protected or reserved location (the save-data sandbox), free-space checks through a seam, forgetting a root only edits SUL's list |
| 2.1 Profiles and deploy | `mods/` | Mod store (verified copies in a root of kind `Mods`), named profiles with load order, requirements, pinned versions and enable/disable, deploy into a managed build with an ownership ledger, dry run for every deploy and purge, **Purge**, **Purge & Deploy**, and undo of the latest purge |
| 2.2 Builder workspace | `builder/recipe.rs` | A recipe is an ordered list of asset-level steps (layer a mod, patch a file). A dry run lists the exact files and hashes; a run writes them into a workspace folder, checks them against the plan and keeps a provenance record. A step that would run code inside the game can be written down but the recipe is refused |
| 2.3 Asset and format tools | `builder/patch.rs`, `builder/tools.rs` | Reversible same-length byte patches that name the exact file by SHA-256, never change the original and are undone by hash check. A registry of external tools with version, size, SHA-256 and the owner's licence notes; a tool that changed is not run until the owner accepts it; a run records the arguments and what it added, changed or removed in the workspace |
| 2.4 Audio modding integration | `builder/tools.rs`, `docs/investigations/audio-modding-2026-11-01.md` | Treated as an external integration, as the spec says. Only the records and the research note exist; nothing audio-specific is built |

## Rules that hold throughout

- **Nothing in this work deletes a file** except the temp file that SUL's own single-file copy made (the Security & Trust quarantine, built earlier, is separate and not used here). A file that has to go out of the way
  is moved into a recovery folder. Old copies after a migration are only listed as retired. The write-site tripwire
  (`tests/write_sites.rs`) fails if any other code deletes, and now also checks that the one deletion in `fsops.rs` is only ever
  the copy's own temp file.
- **A purge touches only what the ownership ledger proves SUL put there**, and only while the file still has exactly the hash that
  was deployed. A changed file, a file not in the ledger, or a folder where a file was is left alone and reported. It is hashed
  again immediately before it is moved, in case it changed after the plan.
- **An original install is not written to.** Deploying into an install that stays in place is refused, and SUL's safe file operations
  refuse any write whose destination is inside a protected location (the way an original install is marked).
- No credentials, no network code (the Windows dependency check still shows no network crate), nothing off by default that sends data.

## Calls made that the spec and the owner did not spell out (please review)

1. **A launch that finds the program of an in-place install differing from its recorded baseline goes ahead with a warning.**
   The game may simply have updated. This is **not** the owner's game-file rule from decision 0006 (a confirmed risky edit blocks a
   launch through SUL unless the owner mods that game, when it is a warning the owner can override). `trust::policy` is not connected
   to the launch: it needs to be told whether a change is a *confirmed* integrity or anti-cheat risk, and nothing yet can say so (which
   adapter decides the risk kind is still open in 0006). A hash difference alone is not that. For a build SUL manages, a changed
   program **is** refused. Wiring the policy in needs the owner's decision on the open items in 0006.
2. A build with a mod profile deployed is **not moved to another root until it has been purged**, and a build whose mod records cannot
   be read is not moved. The ownership ledger lives beside the build, so moving only the build would leave it behind.
3. Deploy copies each file (a mod's file is never hard-linked to the store). A hard link is used only as the last step that puts the finished temp copy in place, because it fails when the destination exists, so a file another program created while the copy ran is not replaced.
4. Profiles cannot be deleted (there is no remove operation), because a profile may be the record of what is deployed.
5. A recipe that contains a runtime-code step is refused outright, not run partly, because SUL has nothing that can run, check or
   undo it.
6. Limits are chosen here, not measured: patch files up to 64 MiB are read whole into memory, 10 000 hunks per patch, 50 000 files
   per mod, 64 arguments of 4096 characters per launch, and so on. They are constants in the code.
7. A tool run has no timeout (SUL cannot stop a program it started) and cannot be confined to the workspace folder, so a file the tool
   writes elsewhere is not in its record.
8. The two tools spec 2.4 names are recorded as *Not investigated*. "Pants' HD2 Audio Modder" is taken to be the GitHub repository
   `RaidingForPants/hd2-audio-modder`; **that is a guess the owner should confirm** (see the research note).

## An independent review, and what it found

A separate agent, read-only and given the rules and the code but not the author's conclusions, reviewed the purge and deploy code.
Each finding was checked by hand before anything was changed. The ones that held, and what was done:

| Finding | Fix |
|---|---|
| Moving a build that has mods deployed left the ownership records behind, so a later purge could take mod files for the game's own | The move is refused until the profile is purged; unreadable records also refuse |
| An undo of a purge that stopped part way could not be finished | The ledger is written first, as a deploy does; a purge then finishes the job |
| The forbidden-folder check looked at how a path was spelled (`data/./anticheat`, `data//anticheat`, `data/../anticheat`) | `..` is refused outright, `./` and doubled slashes are folded, and the place a write would really land is checked as well |
| A small race: a file another program created while a mod file was being copied could be overwritten | The finished copy is put in place with a hard link, which fails if the file exists. Where hard links are unavailable (FAT/exFAT, some shares) it falls back to a check then a rename, which keeps a window of microseconds. **That fallback is untested** |
| A file changed between the purge plan and the move could be carried off as SUL's | The file is hashed again right before it moves |
| Manifest entries were matched ignoring letter case, which could merge two files on a layout that keeps them apart | Matched exactly, as the ledger spells them |

Each fix has a test, and 17 deliberate-bug checks (a break of the fix in the production code) were each caught by a test. The same
review is **not** a security audit.

## Verified

See `PROJECT_STATUS.md` for the exact commands, counts and the Windows CI run.

## Not built, and not verified

- **Spec 1.2 acquisition**: downloading or acquiring historical builds. No download code exists; only importing an install the owner
  already has is built. The SteamCMD investigation is a first look at one Valve page (`docs/investigations/steamcmd-acquisition-2026-11-03.md`):
  it does not mention older builds or manifest ids, says nothing about terms, and its examples put a password on the command line,
  which SUL's no-passwords rule (0003) rules out. Depot and manifest workflows are not investigated.
- **Spec 1.6** save-data transfer: deferred by the spec.
- **No UI and no Tauri command** for any of Module 1 or 2.1 to 2.4. The Game Library in the app still works as before.
- **No real game adapter**, no real mod, no real tool. Helldivers 2 7.1.1 isolation is untouched. Whether a game accepts a deployed
  or patched file, and whether any of this survives anti-cheat, is not known and not tested.
- Windows-specific behaviour that Linux tests cannot show: 8.3 short names (`ANTICH~1`), junction behaviour and the hard-link
  fallback on FAT/exFAT. Windows CI runs the same tests; it does not exercise those cases.
- A power cut on a real disk, a real Steam client and a real drive being unplugged were never tested.
- Spec 2.5 and later, and Module 3 onward, as asked.

## Still open for the owner

- Which game gets the first real adapter, and who decides whether a change to its files is a confirmed risk (0006).
- Connecting `trust::policy` to the launch, and the per-game "I mod this myself" setting (0006).
- Whether to investigate acquisition further (depot and manifest workflows, and the terms) next, or build the UI for what exists.
- Confirm which repository is the audio modder meant in spec 2.4.
- The project licence (still open from spec section 7).
