# 0011 — `manifest` and `trust/*` left `sul-core`: two new crates, and the package scan went to `sul-mod`

Status: **done on 2026-11-23 on branch `rebuild/r0-bootstrap`; not merged into `main`.** It carries out the placement the owner approved on 2026-11-21 and corrected on 2026-11-22 (see section 2). It is the first cut of R0 layer 3. The Windows CI result for the commit is recorded in `docs/SESSION_STATE.md`. Decision log entry: D-154.

In plain words: after the split in 0010, `sul-core` still held two things that are not "substrate": the SHA-256 build manifest and the old `trust/*` modules (integrity checks, the rule for when SUL may act without asking, webhook-address checking, the quarantine and the mod package scan). They now live where the new spec wants them: `sul-trust` (a crate the scaffold already had), a new `sul-quarantine`, and `sul-mod` for the package scan. Code and tests moved with `git mv`; no behaviour changed. Only imports, three visibilities and the places a test had to move (section 4) changed.

## 1. The crates now

Each depends only on the ones before it in this order. `sul-launcher` needs nothing above `sul-core`; the order is the build order, not a chain.

| Crate | Lines (src + tests) | What it holds (plain words) | Depends on |
|---|---:|---|---|
| `sul-core` | 3,054 | The substrate only: errors, safe paths and file operations, the recovery journal, logging, storage layout, names, the version log, portable mode, templates | nothing in SUL |
| `sul-trust` | 2,764 | The SHA-256 build manifest and its verification; the findings every check reports; integrity checks; the rule for when SUL may act without asking; webhook-address checking; and the scaffold's `blake3` digest check and trust tiers | `sul-core` |
| `sul-quarantine` | 1,231 | Set a file aside, verified and reversible (journaled). **Not encrypted yet** (section 6) | `sul-core`, `sul-trust` |
| `sul-launcher` | unchanged | Finding games, Steam attach, the tick matrix, capability labels, the library index, Discord presence | `sul-core` |
| `sul-builds` | unchanged | Managed builds, library roots, adapters, the launch manager, the catalogue | `sul-core`, `sul-trust`, `sul-launcher` |
| `sul-mod` | +1,377 (the package scan) | Mod store, profiles, deploy and purge, recipes, patches, external tools, **package scanning** | `sul-core`, `sul-trust`, `sul-launcher`, `sul-builds` |
| `sul-app` | unchanged | The orchestrator the desktop shell calls | all of the above except `sul-quarantine` |

Nothing depends on `sul-quarantine` yet: there is no mod manager to hand it a file. `sul-core` was 8,290 lines at the last commit and is 3,054 now: 5,236 lines left (`manifest` and `trust/*`). They add up to 5,372 in their new homes; the 136 extra are the two crate roots (`sul-trust`'s `lib.rs` was already there and is 68 lines now, `sul-quarantine`'s is 13), the headers and imports of the two moved test files, and a second copy of the junction test helper. No module was dropped: every moved file went with `git mv`.

## 2. Why the manifest is in `sul-trust`, not `sul-builds`

The owner's first answer (2026-11-21) was "the proposed split", which put `manifest` in `sul-builds`. That cannot be built: `trust/integrity` (to `sul-trust`) uses the manifest types (`BuildManifest`, `Variant`, `VerifyReport`) and `trust/quarantine` uses `manifest::hash_file`, but `sul-builds` sits above both, so they would have depended on a crate above them. Three ways out were put to the owner on 2026-11-22: (A) `manifest` goes to `sul-trust` with the integrity code and the crates above depend on it one way; (B) it stays in `sul-core`; (C) as first approved, but `integrity` also moves up to `sul-builds`, leaving `sul-trust` nearly empty. **The owner chose A** (recorded in `docs/SESSION_STATE.md`).

A matches the scaffold's own idea of `sul-trust` ("cryptographic manifest verification"), and the spec's two verification tiers (Ch. 14 §14.13.1: Tier 2 is "read-only baseline SHA-256 manifests") are exactly what `manifest` is.

## 3. Where each old module went

| Old path (in `sul-core`) | Now | Note |
|---|---|---|
| `manifest.rs` | `sul-trust/src/manifest.rs` | |
| `trust/mod.rs` | `sul-trust/src/findings.rs` | `Finding`, `Certainty`, `ProgramKind`, `NOT_A_GUARANTEE`, `extension`, `program_kind` |
| `trust/integrity.rs` | `sul-trust/src/integrity.rs` | |
| `trust/policy.rs` | `sul-trust/src/policy.rs` | the one test that needed the quarantine moved (section 4) |
| `trust/webhook.rs` | `sul-trust/src/webhook.rs` | still holds no `Serialize`, no file access (a test in `write_sites.rs` checks both, at the new path) |
| `trust/quarantine.rs` | `sul-quarantine/src/quarantine.rs` | |
| `trust/package.rs` | `sul-mod/src/package.rs` | reports in `sul-trust`'s `Finding` type; the spec's "mod package scanning" is Ch. 3 §2.13 (0009 row C) |

## 4. Changes beyond paths and imports

- **Three visibilities became public**: `Finding::confirmed`, `Finding::suspicious` and `findings::extension` were `pub(crate)` and are now `pub`, because the package scan in `sul-mod` shares them with the integrity checks. Nothing else was widened.
- **Tests that needed two crates moved up.** `the_rule_and_the_quarantine_agree_end_to_end` (policy and quarantine) moved to `sul-quarantine/tests/policy_agrees.rs`. Seven integration tests that used the manifest moved from `sul-core/tests/core.rs` to `sul-trust/tests/manifest.rs`: the three `manifest_*` tests, the three `hash_file_*` tests, and `isolated_copy_never_touches_source_and_verifies_clean` (it checks `sul_core::fsops::isolated_copy` with a manifest as the witness, so it needs both crates, and `sul-trust` is the lowest crate that has both). The Windows-only junction helper used by the package tests exists once in `sul-trust` and once in `sul-mod`, each beside its tests, so no test-support feature is needed. Test count is unchanged: **756 Rust tests (workspace without `sul-tauri`) before and after**, 56 in `sul-tauri`, 601 in the UI.
- **The tripwires followed the code.** `crates/sul-core/tests/write_sites.rs` now scans `sul-trust` and `sul-quarantine` (a crate that is not in its list is not scanned, which is the first coverage trap in 0009), its allow-list and its deletion and secret-name rules name the new paths, and its two manifest lists carry the new crates. `ui/tests/no-network.test.ts` checks all seven manifests.
- **`xtask`**: the `forbid(unsafe_code)` list (D-153) now has six crates; `sul-quarantine` joined in the commit that created it, as D-153 requires. `release-check`, `config-check` and the boundary rules needed no change: no boundary rule in `guards.md` names `sul-quarantine` (the one line that mentions it limits what the Coder AI may touch, which is not a crate-boundary kind), so none armed.
- **CI**: `sul-quarantine` is linted and tested in the `r0-substrate` job beside `sul-trust`; the `core` job keeps its five crates and its name.
- **`Cargo.lock`** gained one package (`sul-quarantine`) and the new dependency edges, and `sul-core` lost its `sha2` and `hex` edges (they went with the manifest to `sul-trust`). No version, checksum or source line of any existing package changed (checked on the diff, regenerated offline).
- **Old-spec comments in the moved code** were rewritten as they moved (the rule in 0010 section 6): the two-tier wording now cites Ch. 14 §14.13.1, the threat log Ch. 14 §14.13, the orchestrator agent (Jarvis) Ch. 14 §14.2.1. One was flagged instead: `PatchRef` in `manifest.rs` cites the old spec's 1.3, which has no counterpart in the new spec (0009 row B).

## 5. What was run (Linux container, rustc 1.97, 2026-11-23)

`cargo fmt --all --check` clean; `cargo clippy --workspace --all-targets --locked -- -D warnings` and `-p sul-launcher --features steam` clean; each package's tests built alone (`--no-run`, 14 packages); `cargo test --workspace --exclude sul-tauri` 756 passed, 0 failed; `cargo test -p sul-tauri` 56 passed; UI typecheck, 601 of 601 tests, build; `config-check`, `electron-audit`, `ac-regression`, `release-check --release R0`, `ipc-check` and `pipeline --release R0` all exit 0. Clippy found one real leftover (a test import in `policy.rs` that only the moved test used); it was removed.

Deliberate-bug checks, one at a time, each restored byte for byte and compared: **8 of 8 CAUGHT, each for the intended reason.** `#![forbid(unsafe_code)]` removed from `sul-quarantine`; a file write added to `sul-trust` (the inventory flags the new site, which also proves the crate is scanned); a file deletion added to `sul-trust` (only the quarantine may delete); a secret-named binding added to `sul-trust` (only `webhook.rs` is exempt); `policy::decide` no longer quarantining a known-harmful own file (the cross-crate test fails on its own); manifest `verify` never reporting a modified file (the moved test fails); a network crate (`mio`) added to `sul-quarantine` (the network guard names it); a banned HTTP crate added to `sul-trust`'s manifest (the credential-and-network tripwire names it).

**Not run:** Windows. The two Windows-only junction tests (`sul-trust` `findings.rs`, `sul-mod` `package.rs`) and `sul-launcher`'s `windows_system.rs` run zero tests here; GitHub's Windows CI is their only check. Miri was not run (not installed here).

## 6. Not done, and open

1. **The quarantine is not encrypted.** Ch. 14 §14.13.4 asks for "copy → verify → record → verify → remove into encrypted quarantine". This commit moved the code and did not add encryption; the crate documentation and the module header say so. The owner's design (2026-11-22): a fresh random key per quarantined file, stored beside its record, behind a key-source trait so the vault supplies the key at R0 layer 4 (`chacha20poly1305` is already a workspace dependency). It neutralises a file; it does not keep a secret until the vault exists. This is the next cut.
2. **The known-harmful list has no counterpart in the new spec** (no match for "known-harmful" in `SUL-ARCHITECTURE.md`). `trust::policy` keeps the owner's 2026-10-31 rule (known-harmful alterations are automatic, unknown ones manual; decision 0006) as written. Not connected to launch, as before.
3. **Older records still say `sul-core`.** Decisions 0005 to 0010, the Security and Trust investigation and the capability matrix's crate column were written when the code was in `sul-core`; each carries a dated pointer here instead of being rewritten.
4. **`trust/webhook` now sits in `sul-trust`**, as the owner's network-list answer allowed. Nothing calls it, and nothing sends a webhook.
