# 0014 — `sul-watchdog`: the integrity watchdog's logic (R0 layer 3, cut 3)

Status: **built and verified in the Linux container on 2026-11-26 on branch `rebuild/r0-bootstrap`; not merged into `main`; Windows CI green (commit `b02d00e`, run 45942630461, read 2026-11-27).** About 3,700 lines of it were written in an earlier session that stopped before anything was run or recorded (the tree had one test that did not compile and one stale test). This session fixed those, found the gaps listed in section 5 with deliberate-bug checks, closed them, wrote this record and the D-157 entry, and wired the crate into the xtask guard and CI. It is the third cut of R0 layer 3 and the first step of the owner's answer of 2026-11-25 (D-156, question 4: "`sul-watchdog`, then `sul-crash`; stop and report after each").

In plain words: SUL now has the *logic* of the watchdog and a real file-change watcher, and nothing that touches a live Windows process yet. It can (1) record a fingerprint of every file SUL trusts and check a baseline's signature before believing it, (2) notice within a second that a guarded file was written, replaced by a link, deleted or planted, (3) look at a snapshot of SUL's own process and report memory that is readable, writable and executable, hooked function starts, foreign threads and replaced images, with a list the person controls for things like antivirus, and (4) write everything it finds to a log that notices if a line is edited, removed or reordered, and that holds no file path and no free text. It only reports. Acting on a report (quarantining, turning a feature off, telling the person) is left to the caller, which is the only one that knows what is in use. **Green tests here mean the logic is right for the inputs it was given, not that SUL is protected**: the part that reads a real process, and the thread that runs the watchdog at low priority, are the next cut.

## 1. What changed

| Where | What |
|---|---|
| `crates/sul-watchdog/` (new, 4,287 lines, 84 tests) | `baseline` (the signed list of guarded files and the rules that choose them), `guard` (disk against baseline, one shot, links never followed), `monitor` (the same, driven by file-change events, with debounce, overflow handling and a backstop), `scan` (analysis of a process snapshot and the allowlist), `incident` (what a detection is, and the log-safe record of it), `threat_log` (hash-chained log and the sink trait the hive-mind will plug into), `code` (the three failure codes, checked against the ledger by a test), and `Watchdog` in `lib.rs` (ties them together). `#![forbid(unsafe_code)]`. |
| `crates/sul-trust/src/lib.rs` | `verify_ed25519`: a strict RFC 8032 signature check (refuses weak keys and small-order signatures that the plain check accepts). Tier 1 in Ch. 14 §14.13.1 names Ed25519; until now `sul-trust` held only the digest check. 5 new tests, two of them the RFC 8032 vectors. The Minisign file format is **not** built. |
| `Cargo.toml`, `Cargo.lock` | `sul-watchdog` joins the workspace; `notify` 9 with `default-features = false` is the one new third-party dependency (Windows: `ReadDirectoryChangesW`; the Windows dependency tree is `windows-sys`, `walkdir` and small helpers, no `mio`, no sockets). |
| `xtask/src/unsafe_code.rs`, `xtask/src/main.rs` | `sul-watchdog` joins the `forbid(unsafe_code)` list (D-153: a new crate joins in the commit that creates it); the gate's summary text no longer hard-codes a crate count. The network guard covers it by itself (it reads the whole Windows graph of every shipped package: `sul-watchdog: 64 crates`, no network crate). |
| `.github/workflows/ci.yml` | `sul-watchdog` added to the clippy and test steps of `r0-substrate`. |

## 2. What it protects, and what it does not

**Does:** a changed, deleted, replaced-by-a-link, replaced-by-a-folder or newly planted guarded file is reported with the digest found; a baseline whose signature does not verify is refused before any of it is parsed; the process analysis reports RWX memory SUL did not allocate, a function whose first bytes differ from its file (with the shape of the patch), a thread that starts somewhere it should not, and a section that differs in a quarter or more of its bytes; the threat log refuses to be appended to after its chain breaks; a detection reaches the caller even when the log cannot be written; a log record cannot carry a path, an address or a sentence, because the type has no field for one.

**Does not:**
- **Read a live process, or set `BELOW_NORMAL`, or run on its own thread.** `ProcessProbe` has no real implementation, so the scan has only ever seen made-up snapshots. Nothing sets the watchdog's priority (Ch. 13 and the guards ask for `BELOW_NORMAL`).
- **Decide what the installed product guards, or sign a baseline.** The installer's layout is not fixed and no installer exists; `Baseline::compute` exists and no caller does. Who signs, and where the public key is embedded, belongs to the installer and update chain.
- **Stop someone who can write the log file** from cutting off its end or replacing it whole with a freshly chained copy. The chain makes an edit, a deletion or a reorder inside the log visible; nothing outside the file remembers the last line. An anchor (the hive-mind's copy, or a signed checkpoint) is not built. `sul-hive-mind` does not exist; `ThreatSink` is where it will plug in.
- **Prove the links and the real process behave on Windows.** The container is Linux. The Windows runner (run 45942630461) passed 82 of 82 tests, including the real watcher on `ReadDirectoryChangesW` within one second (`the_real_watcher_reports_a_write_to_a_guarded_file_within_a_second`). But the two tests that plant a link are `cfg(unix)` and a third skips its link step, so "a link is never followed" has only been exercised on Linux; junctions, file locks and a real process have not been exercised anywhere.
- **Take the 25% hollowing threshold (`HOLLOW_PERCENT`) from any measurement.** It is a named starting value.

## 3. Spec mapping

| Spec text | Where | State |
|---|---|---|
| Ch. 12 §2.8 / Ch. 14 §14.13.2 "async OS filesystem hooks monitor binaries, sidecars, manifests against trusted baselines. Baseline computed at install time." | `baseline`, `guard`, `monitor` | logic built; "at install time" waits for an installer |
| §2.8 test: "filesystem hooks detect a modified binary within 1 s of write" | `monitor::tests::the_real_watcher_…_within_a_second` | passes on Linux and on the Windows runner (run 45942630461) |
| Ch. 14 §14.13.3 process tamper: RWX not mapped by SUL, API hooks, foreign threads, DLL hollowing; user-visible AV/EDR allowlist | `scan` (`analyze`, `Allowlist`) | logic built, tested on made-up snapshots; no live probe |
| §2.8 / Ch. 14 §14.14.1: model files verified "on every session start, not just first load" | `Watchdog::session_start`; `Role::ModelFile` is skipped by the periodic pass only | built |
| §14.13.5 hive-mind threat logging, privacy: "hashes, detection reasons, event types — never user file contents, never user paths" | `incident::ThreatRecord`, `threat_log` | built; the hive-mind end does not exist |
| §14.13.4 quarantine, `sul-quarantine restore` | `sul-quarantine` (decision 0012) | not this crate; the caller joins them |
| Failure taxonomy: `E_WATCHDOG_TAMPER` (letter E), `E_WATCHDOG_HOOK_DETECTED` and `E_WATCHDOG_RWX_REGION` (no letter) | `code` and a test that reads `SUL-LEDGER/failures.md` | matches (D-152: the row's letter, not an inferred one) |
| §14.13.1 Tier 1 "Ed25519 Minisign" | `sul_trust::verify_ed25519` (raw signature) | signature check built; Minisign format, watermarks and key custody are not |

## 4. What the owner decided and what was left to Claude

Decided by the owner: the order (D-156 q4) and, earlier, the standing direction of 2026-11-22 (technical choices left to Claude go to performance and optimization, consistent with full functionality).

Left to Claude under that direction, and why:
- **Report only, never act.** Quarantining a file that is in use, or disabling a feature, needs knowledge only the caller has; a watchdog that acts on its own could break a running game.
- **Event-driven, not polling.** An idle watchdog costs no CPU and so can sit at `BELOW_NORMAL` without touching a game's frame time (HANDOFF section 3). A periodic full check (default 10 minutes) is the backstop for events the system lost, and skips model files (gigabytes); the session-start check and the change events cover those.
- **`notify`, default features off.** One maintained crate instead of hand-written `ReadDirectoryChangesW`, which would need `unsafe` and break `forbid(unsafe_code)`.
- **Debounce and overflow = "check everything".** A write arrives as many events and a full event buffer loses some; both are handled by design, not left to hope.
- **A baseline is signed with Ed25519 and checked before it is read** (strict check). A baseline found on disk is never trusted unsigned; `from_json_unsigned` exists for one computed a moment ago.
- **Links are never followed**, for a guarded file or any folder above it, so a junction swapped in for a folder cannot make a tampered file look like the original.
- **A hash-chained log**, each line quoting the SHA-256 of the line before, and a sink that refuses to append after a break.

## 5. Verification (Linux container, rustc 1.97, 2026-11-26)

| Check | Result |
|---|---|
| `cargo fmt --all --check`, `cargo check --workspace --locked`, clippy with `-D warnings` on the `r0-substrate` set and on the `core` set | clean |
| `cargo test -p sul-watchdog --locked` (alone) | 84 passed, 0 failed |
| `cargo test -p sul-trust`, `-p xtask`, `-p sul-quarantine`, `-p sul-ipc-types`, `-p sul-ring`, `-p sul-wire`, `-p sul-metrics` (each alone) | all pass (59 + 7, 94, 59 + 1, 12, 17, 12, 1) |
| `cargo xtask config-check`, `electron-audit`, `ac-regression`, `release-check --release R0`, `ipc-check` | all exit 0; `unsafe-code` reports 7 crates including `sul-watchdog`; `network-surfaces` reports `sul-watchdog: 64 crates` and no network crate |
| Deliberate-bug checks, one break at a time, on the repository tree with every file backed up and compared byte for byte afterwards | **first pass 42 of 52 CAUGHT, 10 MISSED; after closing the gaps, 51 of 52 CAUGHT and 1 equivalent mutant** |

What the 10 misses were, and what closed them (all found 2026-11-26):

| Break | Why it was missed | Closed by |
|---|---|---|
| A known module's thread trusted even when it starts in non-executable memory | the test covered executable memory only | `a_thread_in_a_known_module_but_in_memory_that_is_not_executable_is_still_foreign` |
| An unlisted link followed and hashed | no test planted a link | `a_new_link_of_a_guarded_kind_is_unlisted_with_no_digest_and_is_never_hashed` (Unix; creating a link on Windows needs a privilege CI may not give) |
| A read of a file counted as a change | the outcome is the same (the check finds the file clean); only the cost differs | a direct test of what each kind of event becomes |
| The system's overflow flag ignored; a watcher error swallowed | no test produced either | one test of both (both must ask for a full check) |
| A locked file not retried | no way to hold a lock here | the retry policy pulled out into `retry_once_if_unreadable` (behaviour unchanged) and tested with a counting closure |
| The log's line number unchecked | the hash chain already catches every line that another line follows; only the **last** line is vouched for by its number alone | `a_changed_line_number_on_the_last_line_is_noticed…` |
| The log record without the file's digest | nothing asserted it, and the digest is the point of the log (§14.13.5) | one test on `ThreatRecord` and an assertion at `Watchdog` level |
| A repeated baseline path accepted | the "a repeat" case was appended at the end, so the sort check refused it anyway | a repeat beside its original |

The equivalent mutant: removing the explicit `is_symlink` check in `guard::walk_chain` changes no outcome, because `symlink_metadata` never reports a link as a file or a folder and the next two checks already refuse it. It is kept as a statement of intent and marked as such in the code. (I reasoned the same holds for junctions on Windows; that was not run.)

Also found and fixed before any of the above: a test in `monitor.rs` that did not compile (a variable renamed halfway), a stale test in `guard.rs` that expected the answer from before `walk_chain` existed (a plain file where a folder should be is `NotAFile`, confirmed, not `Unreadable`, suspicious; the module's own documentation says so), and `Watchdog` itself, the glue that decides what reaches the log and what reaches the caller, had **no tests at all** (9 added).

**Windows CI (read 2026-11-27, run 45942630461, all four jobs success):** 82 `sul-watchdog` tests passed on Windows (84 here; two `cfg(unix)` link tests do not run there), including the real-watcher test on `ReadDirectoryChangesW`; clippy `-D warnings` and the xtask gates passed.

**Not run:** Miri and Kani (not installed here); anything that touches a real Windows process, a real junction or symlink, or a real file lock.

## 6. Open points, and what the next cut needs

1. **Two of the four process detections have no failure code (owner question).** Ch. 12 §2.8 and Ch. 14 §14.13.3 list four detections (RWX regions, API hooks, foreign threads, DLL hollowing) but the ledger names three codes. Foreign threads and hollowing are logged with a `reason` and no code. *Option A (recommended):* add `E_WATCHDOG_FOREIGN_THREAD` and `E_WATCHDOG_IMAGE_HOLLOWING` to `SUL-LEDGER/failures.md` and Ch. 12 §2.8 so every detection has a code a support bundle can show; both are detections like the other two, so no recovery mode letter. *Option B:* leave them code-less. The code supports either; this is a ledger amendment, so it is the owner's call.
2. **The live probe will need `unsafe` (Windows memory queries), and `sul-watchdog` is `forbid(unsafe_code)`.** My call, flagged here and not blocking: put the Windows probe and the `BELOW_NORMAL` thread in their own crate (working name `sul-watchdog-win`) behind `ProcessProbe`, the same split `sul-wire` uses for its Win32 shared-memory adapter, so Miri and Kani keep verifying the pure analysis. It will need its own decision when built (D-153 says a crate leaves the list only by a recorded decision; this one never joins it) and it is where the `process-memory access` boundary rule of `cargo xtask ac-regression` (today "NOT checked, no crate for it exists yet") gets armed.
3. **What the installed product guards, and who signs the baseline,** wait for the installer and the update channel.
4. **`HOLLOW_PERCENT` (25) and the 10-minute backstop** are starting values; measure before trusting them.
5. **A signed checkpoint or the hive-mind's copy** is the only thing that would make the log's end tamper-evident.
