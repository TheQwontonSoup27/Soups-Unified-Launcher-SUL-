# 0006 — Owner directions of 2026-10-31 (spec V5.1 attached), checked before any code

Status: **proposal, nothing built.** The owner sent spec "V5.1" and three architecture directions. CLAUDE.md asks for a plan before major architectural work, and `PROJECT_SPEC.md` section 7 lists decisions that must not be finalised silently.
This record says what was asked, what was checked (2026-10-31, Linux container, crate sources downloaded from the registry and read; nothing was built or run on Windows), and what is proposed.

## What was asked
1. **Live change tracking** with OS notification hooks (`watch_dir`, `notify`, Windows `ReadDirectoryChangesW` via IOCP) in a background Tokio worker `sul-watchdog`, targeting <0.1% idle CPU and no unnecessary disk wake-ups.
2. **App integration** through `sul_pack_governor`: integrity checks and file monitoring continuously in the background at ABOVE_NORMAL priority, non-essential diagnostics suppressed during gaming, and the Two-Tier Verification Boundary (Tier 1 internal files: Ed25519 Minisign and quarantine; Tier 2 game and anti-cheat files: read-only SHA-256 baseline manifests).
3. **Webhooks, online streams, local transcription:** `reqwest` with `default-features = false` and `rustls-tls`; fetched text pushed over `sul-ipc` (shared memory) to an `egui` overlay; speech-to-text, translation and text-to-speech local only (`whisper.cpp`, Piper) under the 500 MB VRAM cap.

## What was checked
| Item | Finding | Evidence |
|---|---|---|
| Which spec governs | The repo's `PROJECT_SPEC.md` is **v1.0** (2026-10-22: ten modules, "a planning baseline, not proof"). V5.1 is a different document (different scope, different numbering: its section 7 is not the open-decisions list). Not adopted. | Both read in full |
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

## Proposed first slice (needs the owner's approval)
1. **Keep the network out of the core by a crate boundary.** A new workspace crate `sul-net` (webhook sender only, off by default) holds `reqwest`. The tripwires change from "no network crate anywhere" to "network crates only in `sul-net`'s graph; `sul-core` and any audio crate stay clean". That keeps "local only" enforceable for speech.
2. **`sul-watchdog` as a read-only watcher** on `notify`: debounced events re-run the existing `integrity` check on the files that changed and report with the existing confirmed/suspicious classes. No automatic quarantine, no execution revocation. Background priority below normal while a game runs. A manual measurement plan for idle CPU on Windows.
3. **Webhook sender** after the owner designs the payload; address from Windows Credential Manager.

Not started: the overlay and `sul-ipc`, speech-to-text and text-to-speech, the pack governor (no AI data packs exist), Tier 1 signing, any renderer hook.

## Questions for the owner
1. Does V5.1 replace `PROJECT_SPEC.md` v1.0, or is it a separate line of work?
2. `reqwest`: 0.12.x with `rustls-tls` exactly as written (works now; recommended for the first slice), or 0.13.x with `rustls`?
3. Priority of background integrity work while a game runs: below normal (recommended) or above normal as written?
4. On a confirmed hash mismatch of SUL's own files: show a card and wait for the user, or quarantine automatically?
5. Webhook payload: what is posted, and to which destination setting?
