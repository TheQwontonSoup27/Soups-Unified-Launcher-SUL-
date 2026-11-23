# 0010 — The old `sul-core` was split into five crates

Status: **done on 2026-11-21 on branch `rebuild/r0-bootstrap`; not merged into `main`.** It carries out the plan the owner approved on 2026-11-20 (`docs/decisions/0009`, "Owner answers"). The Windows CI result for the commit is recorded in `docs/SESSION_STATE.md`.

**Update 2026-11-23:** `manifest` and `trust/*`, which this record leaves in `sul-core` (sections 1, 5 and 6), have since moved: `sul-trust`, a new `sul-quarantine`, and the package scan in `sul-mod` (`docs/decisions/0011-trust-split.md`, D-154). The crate count is now seven. The text below is what was true on 2026-11-21.

In plain words: the old single crate `sul-core` held everything (32,772 lines of Rust with its tests): file safety, finding games, managed builds, mods, and the service the app calls. It is now five crates, each depending only on the ones before it. Code and tests moved with `git mv` and no behaviour was changed. The one deliberate change to the test count is explained in section 4.

## 1. The crates

| Crate | Lines (src, tests, examples) | What it holds (plain words) | Depends on |
|---|---:|---|---|
| `sul-core` | 8,290 | The substrate: errors, safe paths and file operations, the recovery journal, logging, storage layout, names, the version log, portable mode, templates. **Also still `manifest` and `trust/*`**: the owner has not yet said where they go (0009, step 6) | nothing in SUL |
| `sul-launcher` | 6,659 | Finding installed games on every drive, the Steam attach layer, the per-game tick matrix, capability labels, the library index, Discord presence. Owns the `steam` feature and the `winreg` dependency | `sul-core` |
| `sul-builds` | 5,111 | Managed copies of a game, library roots, game adapters, the launch and runtime manager, the catalogue. Has the `test-support` feature (section 3) | `sul-core`, `sul-launcher` |
| `sul-mod` | 7,036 | The mod store, profiles, deploy, purge and undo, recipes, reversible patches, the external tool registry. The name is the spec's own (Ch. 3 §2.10) | `sul-core`, `sul-launcher`, `sul-builds` |
| `sul-app` | 5,837 | `ManagerService`, the orchestrator the Tauri commands call | all four above |

`src-tauri` (`sul-tauri`) depends on all five. Its `steam` feature is now `sul-launcher/steam`. The layering is written into the workspace `Cargo.toml` and into each crate's top-of-file docs.

## 2. Why one commit and not four

The approved order was one cut at a time, launcher first. That order does not work as separate commits: at the old crate's last commit `adapters.rs` and `catalogue.rs` use `crate::capability`, so with the launcher group gone first they would still sit in `sul-core` and need `sul-launcher`, while `sul-launcher` needs `sul-core`'s error and file modules. Cargo does not allow a cycle between crates. The owner chose on 2026-11-20 to make all four cuts (`sul-launcher`, `sul-builds`, `sul-mod`, `sul-app`) in one green commit with one report. This is that commit.

## 3. What changed besides moving files

- **Seven items in `sul-core` went from `pub(crate)` to `pub`**, because modules that now live in other crates call them: `Error::io`, `Error::json`, `error::io`, `manifest::inventory`, `manifest::now_unix`, `names::is_reserved_windows_name`, `safepath::resolve_lenient`. This widens the substrate's public surface by exactly those seven; nothing else was opened.
- **`testkit` (the shared test fixtures) now has a home.** It is the open point 0009 left inside step 3. It stays in `sul-builds` behind a `test-support` feature (off by default, pulls in `tempfile`). `sul-builds`' own tests and the dev-dependencies of `sul-mod` and `sul-app` turn it on. **No shipped package may turn it on.**
- **A defect in the first draft of the split, found and fixed before the commit:** `sul-app`'s tests use `sul_builds::process::fake` and `sul_builds::adapters::fixtures`, which exist only when `test-support` is on, but `sul-app`'s manifest did not turn it on. `cargo test --workspace` passed anyway, because `sul-mod`'s dev-dependency switches the feature on for the whole build, but `cargo test -p sul-app` on its own failed to compile (8 errors). `sul-app` now has the dev-dependency. Every one of the six packages (`sul-core`, `sul-launcher`, `sul-builds`, `sul-mod`, `sul-app`, `sul-tauri`) was then checked to build its tests alone (`cargo test -p <pkg> --no-run`). A workspace run cannot show this kind of gap.
- **`tests/write_sites.rs`** (the source scan for where SUL writes files) now scans all five crates, its per-file allowlist uses the new paths, and it has one new test that fails if any shipped package enables `test-support`. Without these changes the moved files would have silently dropped out of the check (the first of the two coverage traps in 0009).
- **CI, job `core`** (name kept) now lints and tests `sul-core`, `sul-launcher`, `sul-builds`, `sul-mod` and `sul-app`, and the `steam`-feature compile check targets `sul-launcher`. Without this the moved code would have dropped out of Windows CI (the second trap in 0009), and the `steam` step would have failed because `sul-core` no longer has that feature.
- **The UI.** `ui/tests/no-network.test.ts` asserted that `crates/sul-core/src/discord.rs` exists, which would have failed after the move; it now names `sul-launcher`, and its "no network crate in a manifest" check covers all five crates instead of only `sul-core`. Wire-type and fake-service comments in `ui/src/lib/*.ts`, `ui/src/capabilities.ts`, `ui/src/discord.ts` and `ui/tests/*` now name the right Rust crate.
- **Test moved with its module:** the capability-evidence test left `sul-core`'s `tests/core.rs` and is now `crates/sul-launcher/tests/capability.rs`, because the module it tests moved.
- **Doc comments.** Old-spec references in the moved crates and the shell were rewritten (section 5).

## 4. Test counts

Before (container, `cargo test --workspace --exclude sul-tauri`): 636. After: **637**. The one extra test is the new `test-support` guard in `write_sites.rs` (14 to 15). The capability test moved between files (17 to 16 in `tests/core.rs`, 0 to 1 in `sul-launcher/tests/capability.rs`) and nothing else was added or removed; every other file has the same number of tests as before.

| Package | Tests |
|---|---:|
| `sul-core` (unit 153, `core` 15, `references` 6, `write_sites` 15) | 189 |
| `sul-launcher` (unit 124, `capability` 1, `discord` 17, `steam` 6; `windows_system` is Windows-only and runs 0 here) | 148 |
| `sul-builds` | 80 |
| `sul-mod` | 93 |
| `sul-app` | 109 |
| the four substrate crates and `xtask` | 18 |
| **Total without `sul-tauri`** | **637** |

`sul-tauri` (56) and the UI (601) are unchanged.

## 5. The old-spec references

The old spec (`PROJECT_SPEC.md`) is no longer in the tree; it is on the archive branch `archive/legacy-pre-ground-up-2026-11-09`. Comments that cited it were rewritten under the owner's rule: a match becomes the new spec's chapter, anything that cannot be matched is **flagged, not given an invented citation**. Where a comment keeps an old item number it now says "old spec".

The item names below come from decision 0007 and the code comments. For the unmatched rows I searched the whole of `SUL-ARCHITECTURE.md` (and read the hits), not only Chapter 3.

| Old-spec item | What it is | New-spec counterpart |
|---|---|---|
| 1.1 Game library | game adapters; catalogue (categories, artwork, search, detail page) | Adapters: the per-game launch profile, Ch. 3 §2.24 (0009 row A3, point 4.8). **Catalogue: flagged.** Ch. 3 §2.1 is the new game library but names no categories or artwork |
| 1.2 Installation, 1.3 Build and version manager | import or register a game, slots, rollback, repair | None (0009 row B) |
| 1.4 Launch and runtime manager | start a build and follow it | None (0009 row B). A launch must also obey the signed `anti_cheat` block (Ch. 3 §2.24, Ch. 16 §16.3): R0 layer 5, not done |
| 1.5 Storage and library locations | library roots | None (0009 row B) |
| 2.1 Profiles and deploy | mod store, profiles, deploy, purge, undo | Crate level only: `sul-mod`, Ch. 3 §2.10, release R1 (0009 row D). The spec has no profiles, load order, deploy or purge (0 matches for each); its model is the `sul-vfs` overlay (Ch. 3 §2.12). Both are meant to coexist (0009, point 4.5) |
| 2.2 Builder workspace | reproducible recipes | **Flagged.** None found. The 9 hits for "recipe" in the spec are all AI model-training recipes, none about mods |
| 2.3 Asset and format tools | reversible byte patches; the external tool registry | **Flagged** for patches (0 matches for reversible patch, byte patch or hex edit). Tool registry: Ch. 3 §2.11 names the tool but not a registry |
| 2.4 Audio modding integration | treat the tool as external | Matched: Ch. 3 §2.11 names `RaidingForPants/hd2-audio-modder` (0009 section 3) |
| 2.6 | telling a verified mod conflict from an overlap | **Flagged.** None found (0 matches for mod conflict detection) |
| 2.7 | making a new build from recipe steps | **Flagged.** None found |
| 3.1 | scanning a mod for malware | Matched: Ch. 3 §2.13 "Mod package scanning security" (0009 row C). The scanner is still in `sul-core` (`trust/package.rs`, placement undecided) |
| "section 1" to "section 4" (matrix, registry, steam) | the old spec's module sections | Registry: Ch. 3 §2.1 and §2.14 (0009 row A1). Steam: Ch. 3 §2.14 (row A2). Matrix: none (row A3). Discord presence: none (row E) |

## 6. Left alone on purpose, and open points

1. **A user-visible sentence still says "spec 2.4".** `crates/sul-mod/src/builder/tools.rs`, in the `not_established` text of the known-tools list ("That the repository is the tool spec 2.4 means; ..."). It is shown in the window, so changing it changes what the owner sees. Not changed without a go-ahead.
2. **The window's module list still says "Module 1" and "Module 2"** (`ui/src/modules/registry.tsx`, and the subtitle in `ui/src/views/ModsView.tsx`). That is the UI shell, 0009 point 4.7, whose answer is still the working default: swap the placeholder list as each subsystem arrives.
3. **Old-spec references remain in the modules that did not move**: `templates`, `storage`, `updater`, `portability`, `manifest` and `trust/policy` (11 comment lines), plus a historical sentence in `tests/references.rs` that is true as written. The rule is "rewrite as each module moves", so these wait for the substrate pass and for the placement of `manifest` and `trust`.
4. **`manifest` and `trust/*` have no owner decision yet** (0009, step 6). I read Ch. 14 §2.13 and Ch. 3 §2.13 before asking; that comes next.
5. **Decisions 0001 to 0008 are records of what was true then.** They still say `sul-core` for the crate as it was; they were not rewritten. Pointers a reader would follow (0003, 0008 and the audio-modding investigation) were updated to the new paths. The capability matrix's "component" column still says `sul-core 0.1.0` for rows tested before the split; a dated note at the top of the matrix says where each module lives now.
6. **Windows-only code was not compiled here** (the container has no Windows toolchain). The Windows result comes from CI only; see `docs/SESSION_STATE.md`, gaps 1 and 7.
