# SESSION STATE: check this first on every run

Last updated: 2026-11-18, end of the bootstrap milestone. Branch: `rebuild/r0-bootstrap` (nothing is on `main`).

In plain words: this is the hand-over log named in `CLAUDE.md`. The owner's standing preference is that a log they assign is read at the start of every run and updated at the end of each milestone. Read order: this file, then `CLAUDE.md`, then `HANDOFF.md` (working notes about the owner and the environment; where it disagrees with the new spec, the new spec wins), then only the scoped parts of the spec that the task needs.

## Owner decisions in force

- **Merge, new design wins.** The old `crates/sul-core` (and the rest of the V5.1 build) is merged into the new design: keep every old feature and design choice that does not contradict the new design; the new design's code takes precedence and old code must not override it; the result must be compatible. Nothing is parked in a `legacy/` folder.
- **Cleanup first.** Redundant files are removed before commits (done, see below). Old research stays: decisions `0001` to `0008`, investigations, capability matrix, `references.md`, design images.
- **Push:** the owner approved pushing the branch `rebuild/r0-bootstrap`. Nothing goes into `main` without an explicit go-ahead and passing tests.
- **Stop points:** report back after each step, consult on conflicts, do not run ahead.
- **License is not settled in the repository.** The new `Cargo.toml` declares `MIT OR Apache-2.0`; the old build deliberately left it unset; there are no LICENSE files. Do not add any until the owner confirms and gives the copyright holder's name.

## Done in the bootstrap milestone

- Removed the superseded `PROJECT_SPEC.md`, `PROJECT_STATUS.md`, `ROADMAP.md` (still on the archive branch `archive/legacy-pre-ground-up-2026-11-09`, commit `131ca0c`); old `CLAUDE.md` kept as `docs/legacy/CLAUDE-v5.1.md`; pointers explained in `docs/legacy/README.md`.
- Placed the 11 uploaded files at their canonical paths: new `CLAUDE.md`, `README.md`, `SUL-BRIEF.md`, `SUL-ARCHITECTURE.md`, `SUL-LEDGER/{gates,baselines,failures,guards,releases}.md`, `SUL-DECISIONS.md`, `.cargo/config.toml`, and the crates `sul-ipc-types`, `sul-ring`, `sul-wire`, `sul-trust`, `sul-metrics`, `xtask`.
- Merged the workspace `Cargo.toml` (new members, dependencies and profiles; old members `sul-core`, `sul-net`, `src-tauri` kept; resolver 3 kept).
- Fixes to the supplied files: added the `cargo xtask` alias (it did not work without it); ran rustfmt on the new crates; filled the empty Chapter Index of `SUL-ARCHITECTURE.md` (17 rows, Chapter 1 at line 26; each row checked against the file).
- Updated the places that named the removed docs (`crates/sul-core/tests/references.rs`, `discord.rs` comment, `ui/src/modules/registry.tsx`, `ui/tests/styles.test.ts`) and added a `r0-substrate` job to the Windows CI.

## Verification (run in the Linux container, rustc 1.97)

| Command | Result |
|---|---|
| `cargo check --workspace` | pass |
| `cargo fmt --all --check` | clean |
| `cargo clippy` with `-D warnings` on the new crates, `xtask`, and `sul-core` | pass |
| `cargo test --workspace --exclude sul-tauri` | 622 passed, 0 failed (618 in `sul-core`, 4 in the new crates) |
| `cargo test -p sul-tauri` | 56 passed, 0 failed (618 + 56 = the 674 old Rust tests in HANDOFF, plus the 4 new ones = 678, all pass) |
| `cargo xtask config-check`, `electron-audit`, `ac-regression`, `release-check --release R0` | all exit 0 (but see gap 2) |
| `node .github/scripts/check-network-crates.mjs` | pass: no network crate in `sul-core` or `sul-tauri` |
| UI: `npm run ui:typecheck`, `npm test --workspace ui`, `npm run ui:build` | pass; 615 of 615 tests |
| GitHub Windows CI, commit `371fda2` (run 44436843640) | all four jobs success: `r0-substrate`, `core`, `ui`, `tauri-shell`. This is the first Windows compile of the new crates. |

## Known gaps (do not claim past these)

1. **Windows-only code cannot be compiled in this container** (the Windows toolchain cannot be downloaded). The Windows tests in `sul-core` (`windows_system`) run zero tests here. GitHub's Windows CI is the only check for them and for the Win32 layers (R0 layers 4 to 6).
2. **The xtask gates are placeholders.** `release-check` only checks edition and MSRV in `Cargo.toml` and that the word "electron" is absent; it does not read `SUL-LEDGER/releases.md`. `ac-regression`, `bench-telemetry`, `simulate-config`, `plan` print text only. A green gate does not yet mean R0 conforms. This is R0 layer 2.
3. **`sul-wire` alignment:** `rkyv` needs aligned input and the ring's `try_pop` returns a plain `Vec<u8>`. The test passes, but nothing guarantees alignment. Harden in layer 1 and add a test.
4. Thirteen other branches exist on GitHub. HANDOFF says they are all merged into `main`; this was not verified (shallow clone). None was touched.

## Next

1. (Done: Windows CI is green on the bootstrap commit.)
2. Merge map: classify each old `sul-core` module against the new spec chapters, list real conflicts, and ask the owner before moving code. Likely areas: the old ten-module UI registry versus the new subsystem list; the old "network code only in `sul-net`" rule versus the new spec's declared network surfaces; the old `sul-core` name versus the new substrate crate of the same name.
3. R0 layer 1 and 2: harden `sul-ipc-types` and `sul-wire`, make the xtask gates real.

Next command: `cargo check --workspace` (to confirm the baseline is still green), then start the merge map.
