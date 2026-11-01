# 0006 — Owner directions of 2026-10-31 (spec V5.1 attached), checked before any code

Status: **directions recorded and checked; the owner answered on 2026-10-31 (see "Owner answers"). Only `trust::policy` is built.** The owner sent spec "V5.1" and three architecture directions. CLAUDE.md asks for a plan before major architectural work, and `PROJECT_SPEC.md` section 7 lists decisions that must not be finalised silently.
This record says what was asked, what was checked (2026-10-31, Linux container, crate sources downloaded from the registry and read; nothing was built or run on Windows), and what is proposed.

## What was asked
1. **Live change tracking** with OS notification hooks (`watch_dir`, `notify`, Windows `ReadDirectoryChangesW` via IOCP) in a background Tokio worker `sul-watchdog`, targeting <0.1% idle CPU and no unnecessary disk wake-ups.
2. **App integration** through `sul_pack_governor`: integrity checks and file monitoring continuously in the background at ABOVE_NORMAL priority, non-essential diagnostics suppressed during gaming, and the Two-Tier Verification Boundary (Tier 1 internal files: Ed25519 Minisign and quarantine; Tier 2 game and anti-cheat files: read-only SHA-256 baseline manifests).
3. **Webhooks, online streams, local transcription:** `reqwest` with `default-features = false` and `rustls-tls`; fetched text pushed over `sul-ipc` (shared memory) to an `egui` overlay; speech-to-text, translation and text-to-speech local only (`whisper.cpp`, Piper) under the 500 MB VRAM cap.

## What was checked
| Item | Finding | Evidence |
|---|---|---|
| Which spec governs | The repo's `PROJECT_SPEC.md` is **v1.0** (2026-10-22: ten modules, "a planning baseline, not proof"). V5.1 is a different document (different scope, different numbering: its section 7 is not the open-decisions list). **Resolved by the owner (2026-10-31): V5.1 builds on v1.0.** v1.0 stays the base; V5.1 is built later. | Both read in full; owner answer |
| `watch_dir` | Exists (1.0.1; the spec says 0.2). It tails new text from files and is built on `notify`. It is not an OS hook, so it adds nothing over `notify`. | Source of `watch_dir-1.0.1` |
| `notify` on Windows | Uses `ReadDirectoryChangesW` with overlapped I/O and completion routines, **not an I/O completion port**. Its worker thread waits with a 100 ms timeout (`WaitForSingleObjectEx(..., 100, ...)`), so it wakes up to ten times a second while idle. Idle CPU is therefore small but not zero, and was not measured. It needs no Tokio. The spec's 6.1 is old; 8.2.1 is the latest stable, 9.0.0 is a release candidate. | Source of `notify-8.2.1/src/windows.rs` |
| `mmap2` | Does not exist on the registry. The crate is `memmap2` (0.9.12). | `cargo info` |
| `shmem-ipc` | Its own docs say it works **only on Linux** (memfd, eventfd). It cannot be `sul-ipc` on Windows. `ipmpsc` 0.5.1 has Windows code paths (`cfg(windows)`) and is a candidate; not built or run here. | Source and README of `shmem-ipc-0.3.0`; `ipmpsc-0.5.1/src/lib.rs` |
| `reqwest` + `rustls-tls` | `rustls-tls` exists in **0.12.28** (bundled Mozilla roots, `ring`). It does **not** exist in **0.13.5**, the version already in `Cargo.lock` through Tauri's mobile-only dependencies; there the feature is `rustls` (aws-lc-rs and the Windows certificate store) or `rustls-no-provider`. Written as given, the line fails to resolve on 0.13. | `Cargo.toml` of both versions |
| Effect of adding `reqwest` | The Windows build gains hyper, rustls and tokio's `net` feature. That ends the statement "no network crate is compiled into the app". `check-network-crates.mjs`, `no-network.test.ts`, `write_sites.rs` and the Security & Trust page text are built to fail when that happens and must be changed in the same commit. | Decision 0005 |
| `sul_pack_governor` | In the spec (5.7, 12.8) it lazy-loads AI data packs and is suppressed during gaming. It does not run integrity checks; those are `sul-watchdog` (12.7). `sul-watchdog` also names a heartbeat monitor (12.6) and a hardware telemetry thread (8). | Spec text |
| ABOVE_NORMAL priority | Games normally run at NORMAL priority, so a background hasher or watcher *above* normal can take CPU from the game, the opposite of the stated goal. Not measured. | Reasoning only |
| Two-Tier boundary | Matches what already exists: read-only manifests (`BuildManifest::verify`, `trust::integrity`) and `ProtectedRoots`, which refuse moving anything out of an original install. Tier 1 signing is not built: the Tauri updater covers update files only, and "internal binary watermarks" are undefined. "Completely prevent false-positive bans" cannot be promised: the effect of watching or hashing files under Easy Anti-Cheat or GameGuard was never tested (CLAUDE.md: unverified). | Code and CLAUDE.md |
| Auto-quarantine | Spec 12.7 says any unauthorized change "immediately triggers execution revocation" and a move into an encrypted quarantine. `PROJECT_SPEC.md` 3.2 says the user reviews first; CLAUDE.md forbids ambiguous deletion and requires confirmed violations to be kept apart from suspicious signals. A change event is a signal (updates, antivirus and installers all cause them), not a verdict. The built quarantine is unencrypted, user-initiated and journaled. | Spec text, `trust::quarantine` |
| Overlay | Showing text in an overlay needs an overlay window. A separate transparent click-through window needs no hook. Hooking the game's renderer (`minhook` 0.9.0 exists) is the open anti-cheat item in `ROADMAP.md` and needs its own owner decision. | `ROADMAP.md` |
| Local speech | `whisper-rs` 0.16.0 and `piper-rs` 0.2.0 exist. Windows build, VRAM use, latency and the combined 500 MB cap with the language model were not tested. Models must be downloaded once (a network use; hash and licence of each model unverified). "Complete offline privacy" holds only if the audio code has no network crate in its dependency graph. | `cargo info` |

## Labels
Every figure in the directions (<0.1% CPU, <0.5 ms frame cost, 500 MB VRAM, sub-10 ms latency, zero disk wake-ups) is a **goal, not a result**, the same as the RAM figures in decision 0004. Until measured on the owner's machine each feature stays **Not investigated**.

## Proposed first slice (first version of this record; see "Owner answers" for what was decided)
1. *(Not yet decided.)* **Keep the network out of the core by a crate boundary.** A new workspace crate `sul-net` (webhook sender only, off by default) holds `reqwest`. The tripwires change from "no network crate anywhere" to "network crates only in `sul-net`'s graph; `sul-core` and any audio crate stay clean". That keeps "local only" enforceable for speech.
2. *(Owner, 2026-10-31: not yet.)* **`sul-watchdog` as a read-only watcher** on `notify`: debounced events re-run the existing `integrity` check on the files that changed and report with the existing confirmed/suspicious classes. No automatic quarantine, no execution revocation. Background priority below normal while a game runs. A manual measurement plan for idle CPU on Windows.
3. **Webhook sender** after the owner designs the payload; address from Windows Credential Manager.

Not started: the overlay and `sul-ipc`, speech-to-text and text-to-speech, the pack governor (no AI data packs exist), Tier 1 signing, any renderer hook.

## Owner answers (2026-10-31)
1. **V5.1 builds on v1.0.** V5.1 is built in the future. For now the current build is brought into line with it where that is unambiguous and breaks no standing rule, so that `main` holds the right code when V5.1 work starts (table below).
2. **`sul-watchdog`: not yet.**
3. **Tier 1 quarantine: a known-harmful alteration is automatic; an unknown one is manual.** Built as `crates/sul-core/src/trust/policy.rs`, one pure function (`decide`) that touches no file. Choices I made inside the rule, open to change:
   - *Known harmful* means the SHA-256 of the file's current content is on a list the caller supplies. No such list exists, so **today nothing is automatic**. A changed hash, a change event or a suspicious signal is *unknown*, not known-harmful.
   - The only automatic case is SUL's own file (inside a registered internal folder) with known-harmful content. A file inside a protected original install is never moved, edited or signed, even when its content is on the list (the action for a game's files was refined later the same day: see "Owner answers, round 2"). A file in neither area is never automatic.
   - Unknown and confirmed (for example, a hash that differs from the baseline): ask the owner. Unknown and only suspicious: report, no card. (The spec's "halt execution" for vendor files was undefined here; the owner answered in round 2.)
   - Still not built: the encrypted quarantine folder (spec 12.7), the list itself and where it comes from (`sul-hive-mind`), and anything that feeds `decide`.
4. **`reqwest`: the owner asked which option is best.** See the next section.

## Owner answers, round 2 (2026-10-31, evening)
1. **Merge `feature/spec-v5-1-alignment` into `main`: yes.** Done before this round was recorded: `main` 95ba76b, Windows CI run 41238632634 passed.
2. **`sul-net` crate boundary: yes.** Built as an empty crate (below).
3. **Priority of background integrity work while a game runs.** The answer was "yes" to a question that offered two options. I took it as accepting the recommendation: **below normal**, because games run at normal priority. Nothing is built that sets a priority (the watchdog waits). If above normal was meant, this line changes.
4. **Who may add to the known-harmful list: the owner, the hive-mind and Jarvis** (the spec names `sul-hive-mind` in 12.7 and Jarvis in 12.1). Built: every entry records its source (`Owner`, `HiveMind`, `Jarvis`), the reason, and when; the owner can remove any entry; a decision that rests on an entry says whose it was. Because entries can now come from software, I added one guard (my choice, open to change): **a list entry never auto-quarantines a file whose content is exactly what SUL recorded for it**. The rule is about *altered* files; a match against SUL's own baseline means the list or the baseline is wrong, so a person decides. Not built: where the list is stored, how the hive-mind or Jarvis deliver entries, and any network access for it.
5. **A game's files edited in a way that risks the game's integrity or its standing with Easy Anti-Cheat.** The owner: there is a clear distinction between the two, but both do the same; and for games the owner mods themselves the restriction does not apply and is a warning the owner can override. Built in `trust::policy`:
   - *The restriction* (what "halt execution" now means): for a game's files with a **confirmed** edit, SUL **does not launch the game** and points at the vendor's own verification (Steam's "Verify integrity of game files"). It never edits, moves or signs the game's files. It can only stop launches that go through SUL; nothing here stops the owner starting the game another way.
   - *Two kinds, one action:* `RiskKind::GameIntegrity` and `RiskKind::AntiCheatFlag` are kept apart in the data so a card can say which one it is, and a test checks that they always produce the same action. Which kind applies is decided by a game adapter; SUL does not claim to know what any anti-cheat flags, and no anti-cheat file name is hard-coded.
   - *Owner-modded games:* `GameModStatus::OwnerModded` turns the block into `WarnOwnerOverridable`: a warning that says the game's file check or its anti-cheat may object, that a ban is the vendor's decision, and that SUL does nothing to hide the change. The flag must come from an explicit choice by the owner for that game; it is never inferred from file changes and nothing else may set it. A test checks that it changes nothing for SUL's own files or any other tier. This is a warning and an override, not a way around an anti-cheat.
   - *My choice, open to change:* only a **confirmed** edit blocks. A suspicious signal about a game's files (an unlisted program file, for example) is reported, because updates, overlays and antivirus cause those and a block would be a false alarm. The caller should log every override.
   - Not built: the per-game switch for "I mod this game myself", the Play button using the decision, and the adapters that say which kind of risk an edit carries.

### `sul-net`
`crates/sul-net` is a workspace member with no dependencies and no code. All network code goes there when a network feature is approved; `sul-core` and any audio or speech crate stay free of network crates, so "local only" can be checked from their dependency lists. `write_sites.rs` now scans its source for file writes and has a test that it stays empty and that nothing refers to it (and that no other manifest declares a network or credential crate). The first network dependency (variant C above) is added there, in the same commit that changes those tripwires, `check-network-crates.mjs`, `no-network.test.ts` and the Security & Trust page text. The network check still covers `sul-core` and `sul-tauri` only; `sul-net` is not in the app's build yet.

## `reqwest`: the best option (dependency experiment, 2026-10-31)
Method: scratch crates outside the repo, resolved against the registry, `cargo tree --target x86_64-pc-windows-msvc -e normal`, each with `tauri` 2 and `tauri-plugin-updater` 2.14.1, because V5.1 (12.5) will have both. Nothing was compiled and nothing was run on Windows.

| Variant | Windows packages | reqwest copies | TLS crypto |
|---|---|---|---|
| Updater alone | 329 | 1 (0.13.5) | `ring`, `rustls-platform-verifier` |
| A. `reqwest` 0.12, `rustls-tls` (as written) + updater | 423 | **2** (0.12.28 and 0.13.5) | `ring`, bundled `webpki-roots`, platform verifier |
| B. `reqwest` 0.13, `rustls` + updater | 338 | 1 | **`aws-lc-rs` and `aws-lc-sys` (C code) and `ring`** |
| **C. `reqwest` 0.13, `rustls-no-provider`, plus `rustls` with `ring` + updater** | **330** | **1** | **`ring` only**, Windows certificate store |

**Recommendation: C.** `reqwest = { version = "0.13", default-features = false, features = ["rustls-no-provider", "json"] }` and `rustls = { version = "0.23", default-features = false, features = ["ring", "std", "tls12"] }`, with `ring` installed as the process-wide provider at startup.
Why: the updater already brings reqwest 0.13.5 and `ring`, so an HTTP client of SUL's own costs one more package; 0.12 doubles the HTTP client (94 more packages); plain `rustls` on 0.13 adds a second crypto library that compiles C code. 0.13 is also the line Tauri itself uses.
Not verified: that it builds on Windows (the build scripts of both `ring` and `aws-lc-sys` mention NASM, so whether a Windows machine needs NASM installed is unknown until a Windows CI build tries it); how the Windows certificate store behaves for Discord's certificates on the owner's machine; and `rustls-no-provider` fails at runtime if the provider is not installed first.
It is **not added now.** When it is, `check-network-crates.mjs`, `no-network.test.ts`, `write_sites.rs` and the Security & Trust page text must change in the same commit (decision 0005). The updater alone has the same effect, so the "no network crate" statement ends when either arrives.

## V5.1 dependency lines that do not work as written (checked against the registry, 2026-10-31)
| V5.1 line | Finding | Use instead |
|---|---|---|
| `mmap2 = "0.2"` | no such crate | `memmap2` 0.9.12 |
| `watch_dir = "0.2"` | no such version; 1.0.1 exists and tails new text from files | `notify` for watching; `watch_dir` only for log tailing |
| `ort = "2.0"` | no non-prerelease 2.0 on the registry (matches no version) | pin an exact `2.0.0-rc.N` |
| `shmem-ipc = "0.2"` | resolves (0.3.0 is latest) but is **Linux only** by its own README | `ipmpsc` (0.5.1 has Windows paths; not run here) |
| `notify = "6.1"` | resolves (6.1.1); 8.2.1 is the latest stable, 9.0.0 is a release candidate | 8.2.1 |
| `minhook = "0.2"` | resolves; latest is 0.9.0. Hooking a game's renderer is the open anti-cheat item | owner decision first |
| `reqwest` with `rustls-tls` | exists only in 0.12 | variant C above |
| `tokio` with `full` | includes `net`, which the network check forbids today | enable features per crate, with the decision above |
Resolved as written but old: `rkyv` 0.7 (0.8.18 latest), `gilrs` 0.10 (0.11.2), `axum` 0.7 (0.8.9), `llama-cpp-bindings` 0.24.0 (0.26.0), `tokio` 1.40 (1.54.1).

## Where the current build stands against V5.1 (v1.0 stays the base)
| V5.1 item | Current build | What was done |
|---|---|---|
| Workspace layout (11.1): `src-tauri`, `crates/sul-core`, `[workspace.dependencies]` | The same layout and mechanism | Aligned. Nothing to change |
| Tier 2: read-only SHA-256 manifests, never modify external files (12.7) | `BuildManifest::verify`, `trust::integrity`, `ProtectedRoots` | Aligned (Experimental, not wired into the app) |
| Tier 1: known-harmful automatic, unknown manual (owner, 12.7); Tier 2: block launch on risky edits, warning for owner-modded games | Was not expressed anywhere | **Built:** `trust::policy` (`Tier`, `Tiering`, `KnownHarmful` with sources, `decide`, `RiskKind`, `GameModStatus`), 14 tests, 20 deliberate breaks caught (round 1 had 9 tests and 12 breaks) |
| Tier 1: encrypted quarantine, Ed25519 Minisign, binary watermarks (12.7) | Quarantine is unencrypted and user-initiated; no signing | Not built. `minisign-verify` 0.3.0 exists; key custody and "watermark" are undefined |
| `sul-watchdog`, `sul_pack_governor`, `.sulpack`, Jarvis/Coder, llama.cpp | None exist | Not started (owner: watchdog not yet) |
| Overlay and `sul-ipc` (6) | None exist | Not started; `shmem-ipc` is Linux only; a renderer hook is an open anti-cheat decision |
| Tauri capabilities (11.3: `fs:default`, `process:default`, `shell:allow-open`, windows `main` and `overlay`) | Least privilege: window actions for the title bar only; a test asserts no file, shell or network permission | **Not changed.** CLAUDE.md requires least privilege; each permission is granted when the feature that needs it is built |
| Release profile (11.1: `opt-level = "z"`, full LTO, `panic = "abort"`, `strip = true`) | `lto = "thin"`, `codegen-units = 1`, `strip = "symbols"`, default `opt-level` and `panic` | Not changed: no code depends on it, it is a one-line change when V5.1 work starts. With `panic = "abort"` a panic ends the app, and the panic-recovery test in `steam.rs` assumes unwinding |
| Data folder (8, 12.7: `%APPDATA%\SUL\...`) | Tauri's per-app folder named by the identifier `app.soups.unifiedlauncher` (decision 0002), chosen in one place (`src-tauri/src/lib.rs`, `portability::data_dir`) | Not changed: moving it needs a migration and an owner choice. One injection point, so it stays a small change |
| Storage (8: SQLite in WAL mode) | JSON files written atomically; the library index and manifests carry a schema number | Not changed. Moving to SQLite waits for the open "schema migration policy" decision |
| `game_registry.json` (11.5) | The registry scanner and library index have their own shapes | Later, when the per-game hub is built |
| Linux and multi-architecture targets (13.1, 11.4) | Windows only (owner, 2026-10-29) | Not adopted; section 13 is optional in V5.1 |
| Local speech (3, 5.3) | Not built | Not started. `whisper-rs` 0.16.0 and `piper-rs` 0.2.0 exist; Windows build, VRAM and the 500 MB cap are untested |

## Still open (needs the owner)
1. Where does the known-harmful list live, and how do the hive-mind and Jarvis deliver entries? (Both may add to it; neither exists yet.)
2. Which game adapter decides whether an edit is a `GameIntegrity` or an `AntiCheatFlag` risk, and from what evidence?
3. Where does the owner declare "I mod this game myself" (a per-game setting), and should the override be logged for each launch?
4. Priority while a game runs: confirm **below normal** (round 2 took "yes" as accepting the recommendation).
5. Webhook payload and destination setting.
6. Data folder name and migration (`%APPDATA%\SUL` versus the identifier folder), and whether V5.1's capability scopes may be granted when their features are built.
