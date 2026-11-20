# SESSION STATE: check this first on every run

Last updated: 2026-11-20, end of the network-rule step. Branch: `rebuild/r0-bootstrap` (nothing is on `main`).

In plain words: this is the hand-over log named in `CLAUDE.md`. The owner's standing preference is that a log they assign is read at the start of every run and updated at the end of each milestone. Read order: this file, then `CLAUDE.md`, then `HANDOFF.md` (working notes about the owner and the environment; where it disagrees with the new spec, the new spec wins), then only the scoped parts of the spec that the task needs.

## Owner decisions in force

- **Merge, new design wins.** The old `crates/sul-core` (and the rest of the V5.1 build) is merged into the new design: keep every old feature and design choice that does not contradict the new design; the new design's code takes precedence and old code must not override it; the result must be compatible. Nothing is parked in a `legacy/` folder.
- **`sul-core` is not just renamed (2026-11-19).** Look into the old code, judge what can and cannot be implemented under the new plans, then merge it or rewrite the core, and delete the old traces of the old build.
- **Network rule (2026-11-19): swap now.** The old no-network CI rule is replaced by the new spec's declared-surface checks. Done, see below.
- **Portable mode stays (2026-11-19)** (`portable.marker`); the owner wants it later for another portable version of the app.
- **Cleanup first.** Redundant files are removed before commits (done, see below). Old research stays: decisions `0001` to `0009`, investigations, capability matrix, `references.md`, design images.
- **Push:** the owner approved pushing the branch `rebuild/r0-bootstrap`. Nothing goes into `main` without an explicit go-ahead and passing tests.
- **Stop points:** report back after each step, consult on conflicts, do not run ahead.
- **License is not settled in the repository.** The new `Cargo.toml` declares `MIT OR Apache-2.0`; the old build deliberately left it unset; there are no LICENSE files. Do not add any until the owner confirms and gives the copyright holder's name.

## Done

**Bootstrap milestone (2026-11-18, commits `371fda2`, `6b4a9a0`):**

- Removed the superseded `PROJECT_SPEC.md`, `PROJECT_STATUS.md`, `ROADMAP.md` (still on the archive branch `archive/legacy-pre-ground-up-2026-11-09`, commit `131ca0c`); old `CLAUDE.md` kept as `docs/legacy/CLAUDE-v5.1.md`; pointers explained in `docs/legacy/README.md`.
- Placed the 11 uploaded files at their canonical paths: new `CLAUDE.md`, `README.md`, `SUL-BRIEF.md`, `SUL-ARCHITECTURE.md`, `SUL-LEDGER/{gates,baselines,failures,guards,releases}.md`, `SUL-DECISIONS.md`, `.cargo/config.toml`, and the crates `sul-ipc-types`, `sul-ring`, `sul-wire`, `sul-trust`, `sul-metrics`, `xtask`.
- Merged the workspace `Cargo.toml` (new members, dependencies and profiles; old members `sul-core`, `sul-net`, `src-tauri` kept; resolver 3 kept).
- Fixes to the supplied files: added the `cargo xtask` alias; ran rustfmt on the new crates; filled the empty Chapter Index of `SUL-ARCHITECTURE.md` (17 rows, Chapter 1 at line 26).
- Updated the places that named the removed docs and added the `r0-substrate` job to the Windows CI.

**Merge map and network rule (2026-11-19 to 2026-11-20):**

- `docs/decisions/0009-sul-core-merge-map.md`: every old `sul-core` module classified against the new spec, with eight conflicts or open points and the owner's answers recorded at the end.
- **Network rule swapped.** `xtask/src/network.rs` (14 unit tests) runs inside `cargo xtask ac-regression` and `pipeline`: for the Windows target it checks the normal dependency graph of every shipped package. A network crate (42 on the deny list, unchanged), tokio's `net` feature or a Windows networking feature fails the gate unless it enters through the declared package `sul-net`; Rust source outside `sul-net` may not name a networking API. Same results as the old script on today's graph (`sul-core` 31 crates, `sul-tauri` 218, 221 with all features). Two deliberate breaks (a `std::net` use in `sul-ring`, a `mio` dependency in `sul-trust`) were both caught, then restored.
- Removed: `.github/scripts/check-network-crates.mjs`, its `.d.mts` stub, `ui/tests/network-crates.test.ts` (14 tests, replaced one for one), and the CI step that ran them. `surface-failure.ps1` now also picks up the guards' `PROBLEM` lines.
- Made the placeholder gates say so in their own output: `release-check`, `pipeline`, `bench-telemetry`, `simulate-config` and `plan` no longer print text that claims work they do not do.
- Updated the pointers in `sul-net`, the capability matrix, `references.md` and the Security & Trust investigation (dated note, text kept).
- Trimmed `HANDOFF.md` (stale read order, the old "where things stand", and the first-message template went, the V5.1 directions list was condensed to one bullet pointing at decision 0006; owner and environment notes stayed; `main`'s commit was corrected to `cc29fcb`).
- **Redundancy scan of the repository (248 tracked files):** no junk, no empty files, one byte-identical pair (`src-tauri/icons/128x128@2x.png` and `ui/public/logo.png`: different consumers, kept). All 12 feature branches on GitHub have zero commits that `main` lacks (checked through the GitHub compare API); none was deleted.

## Verification (run in the Linux container, rustc 1.97)

| Command | Result |
|---|---|
| `cargo check --workspace --locked` | pass |
| `cargo fmt --all --check` | clean |
| `cargo clippy -p xtask --all-targets --locked -- -D warnings` | pass |
| `cargo test --workspace --exclude sul-tauri --locked` | 636 passed, 0 failed (618 `sul-core`, 4 new substrate crates, 14 `xtask`) |
| `cargo test -p sul-tauri --locked` | 56 passed, 0 failed |
| `cargo xtask config-check`, `electron-audit`, `ac-regression`, `release-check --release R0`, `pipeline --release R0` | all exit 0 (see gap 2) |
| UI: `npm run ui:typecheck`, `npm test --workspace ui`, `npm run ui:build` | pass; 601 of 601 tests (615 before, minus the 14 for the removed script) |
| GitHub Windows CI for this step | **pending** (to be recorded here once the run finishes) |

## Known gaps (do not claim past these)

1. **Windows-only code cannot be compiled in this container** (the Windows toolchain cannot be downloaded). The Windows tests in `sul-core` (`windows_system`) run zero tests here. GitHub's Windows CI is the only check for them and for the Win32 layers (R0 layers 4 to 6). The network guard's `cargo tree --target x86_64-pc-windows-msvc` was run here; it needs no Windows toolchain, but its first run on the Windows runner is part of the pending CI result.
2. **The xtask gates are still thin.** `ac-regression` has two real guards (workspace manifest, network surfaces); structural refusals, crate boundaries, feature orthogonality and Windows-only IPC are not implemented. `release-check` checks edition, MSRV and the word "electron" only; it does not read `SUL-LEDGER/releases.md`. `bench-telemetry`, `simulate-config` and `plan` do nothing. Their output now says so. A green gate does not yet mean R0 conforms. Making them real is R0 layer 2.
3. **`sul-wire` alignment:** `rkyv` needs aligned input and the ring's `try_pop` returns a plain `Vec<u8>`. The test passes, but nothing guarantees alignment. Harden in layer 1 and add a test.
4. **The old no-network tripwires that remain.** `crates/sul-core/tests/write_sites.rs` (keeps `sul-net` empty and unreferenced) and `ui/tests/no-network.test.ts` (no network API in the window's source) were left on purpose: they are stricter than the guard and still true. They change in the commit that adds the first network dependency, together with the Security & Trust page text.
5. **Two old network surfaces are not on the spec's declared list** (Ch. 16 §16.15.14): the Discord webhook and the Steam Web API. Nothing calls either. The owner has not yet been asked to add them to the list.
6. **The source part of the guard reads text, not syntax.** It can miss a networking call that does not name one of its patterns, and it does not look inside WebView2, the standard library or Steam.

## Next

1. **Awaiting the owner: the `sul-core` step.** The sequence below is a proposal; no code moves before it is approved. It follows from the owner's instruction (merge or rewrite, delete the old traces, not just a rename) and from `docs/decisions/0009`.
   1. Read the old crate's real structure (which modules use which), so the split follows the code and not only the spec's wording.
   2. Settle the home crate for each group the spec is silent about (managed builds and library roots, the library index and tick matrix, the file-safety helpers, Discord presence).
   3. Move the groups out of `crates/sul-core` into their homes one at a time with `git mv` (history kept), the tests moving with the code, every commit green. What is expected to stay (to be confirmed by step 1) is the substrate: error type, logging and redaction, journal, safe paths and file operations. That is what the new spec means by `sul-core`; it then grows into config loading, IPC, crash handling and the watchdog at R0 layer 3.
   4. Delete what the new design supersedes (old traces), listing each deletion for the owner first.
2. R0 layer 1 and 2: harden `sul-ipc-types` and `sul-wire` (gap 3), make the xtask gates real (gap 2).

Next command: `cargo check --workspace --locked` (to confirm the baseline is still green), then the owner's reply on step 1.
