# 0015 — Live-code attestation and the watchdog's other failure codes (`sul-watchdog`, R0 layer 3, cut 3b)

Status: **built and verified in the Linux container on 2026-11-27 on branch `rebuild/r0-bootstrap` (commit `e38f367`); Windows CI green (run 46090494777); not merged into `main`.** This is the owner's two answers of 2026-11-27 to the open points of decision 0014 section 6: "1. do whatever is best for security, performance and speed" and "2. maybe we could signature new live code as new live code so the watchdog still verifies it". Nothing here reads a live Windows process, and nothing holds a private key: it is logic that is correct for the inputs it was given, not protection. D-158 and D-159 are the entries in `SUL-DECISIONS.md`.

In plain words: before this, the watchdog could only judge code that existed on the day the baseline was signed. Anything SUL makes later (a hot update, a mod adapter built on this machine, generated code, a hook trampoline) was either reported as foreign or, if the caller said "that is mine", **skipped**, which is worse: whoever overwrote a skipped trampoline was invisible. Now new code is *signed as live code* by the right key, the watchdog checks the signature and the rules for it, and from then on it **keeps looking at that code against a digest it did not choose**. The same step added the two failure codes that decision 0014 left open, plus two for live code, so every detection the watchdog makes has a stable code.

## 1. What changed

| Where | What |
|---|---|
| `crates/sul-watchdog/src/live_code.rs` (new) | The record (`LiveCodeRecord`, canonical JSON, signed with Ed25519), the two ledgers (`LiveCodeLedger`: persistent for files, session for memory), the key roles (`TrustRoots`, `TrustedProducers`), every refusal (`Refusal`, 19 closed reasons), the `LiveCodeSigner` trait (the vault implements it, in another process, at layer 4) and the `RegionDigester` trait (the Windows adapter implements it). 26 tests. |
| `crates/sul-watchdog/src/code.rs` | `WatchdogCode` grows from three to seven: `ForeignThread`, `ImageHollowing`, `LiveCodeModified`, `LiveCodeRefused`. A test reads `SUL-LEDGER/failures.md` and fails if any watchdog row disagrees with the enum, **or** if the ledger has a watchdog code the enum lacks (both directions). |
| `crates/sul-watchdog/src/scan.rs` | Attested regions replace the old "own RWX" skip list: each scan asks the probe to hash every registered range, compares with the signed digest, and only a **verified** region is accounted for (and shown to the person with the record that allows it). A changed region is `E_WATCHDOG_LIVE_CODE_MODIFIED` (confirmed) and loses its cover; an unreadable or undigested one is suspicious, never a pass. A thread may start inside verified live code and not inside changed live code. |
| `crates/sul-watchdog/src/monitor.rs` | `Monitor::rebase(new_baseline)` and the **announce-first** rule (section 3). |
| `crates/sul-watchdog/src/lib.rs` | `Watchdog::with_live_code`, `admit_file`, `admit_region`, `retire`, `live_code`; expiry is handled on every `pump`, `session_start` and scan. A refusal is a detection (returned and logged), not an error of the call. |
| `crates/sul-watchdog/src/incident.rs`, `threat_log.rs` | A refusal becomes an incident and a log record whose only extra field is the closed reason (`why`); the field is absent otherwise, so existing log lines serialise byte-for-byte as before (a test pins an old line, because the hash chain must not move). |
| `SUL-LEDGER/failures.md`, `SUL-ARCHITECTURE.md` | Four rows added to the failure ledger and to the taxonomy in Ch. 12 §2.8; the §2.8 process-tamper paragraph, §14.13.3 and the §14.13.5 code list say what D-159 added. |
| `Cargo.toml` (crate), `Cargo.lock` | `blake3` as a **dev**-dependency only (the tests need a real digest). The crate computes no BLAKE3 in production code; the digester supplied by the Windows adapter does. |

Size: the crate is now 7,656 lines with 135 tests (decision 0014: 4,287 lines, 84 tests).

## 2. The design, and why each part is there

**Four purposes, three keys, and a key never signs outside its own purposes.**

| Purpose | Scope | Signed by | May do |
|---|---|---|---|
| `HotUpdate` | persistent (files) | release key | add or replace any file, including ones the baseline lists |
| `LocalBuild` | persistent (files) | device key | add a **new** file (sidecar, mod, adapter, model file); never replace a guarded one, never claim a binary, manifest or trust-primitive role |
| `JitCode` | session (memory) | session key | one in-memory region |
| `HookTrampoline` | session (memory) | session key | one in-memory region (the MinHook opt-in, Ch. 7 §2.1) |

A stolen session key cannot forge an update; a stolen device key cannot rewrite SUL's own binaries. No key may hold two roles (refused when the ledger is built): one key in two roles would let the weaker role's compromise act as the stronger.

**Two ledgers with different roots.** The persistent one is rooted in the SHA-256 of the signed baseline's bytes, so a record made for one baseline is refused by another and a full update (new baseline) retires old records by construction. Its serial must be strictly increasing, so a rollback to an older signed state is refused. The session one is rooted in a random nonce chosen at start-up, so a record captured in a previous run is refused; its serials must be unique but may arrive in any order, because several signers may register at once.

**The check order is fixed and the signature comes first.** Nothing is read from a record until a trusted key has signed it. Then: the bytes are the one canonical serialisation of the record (no whitespace, field order or extra field can differ, so there is exactly one signed text per record), the schema, the key is the one the purpose requires, the scope, the root, the serial, the clock (not from the future beyond 120 s of skew, not expired, and an expiry that is not after its creation is refused), the producer (a module the *signed baseline* lists as code; a module that was itself attested later is not added, so trust is anchored in the release-signed baseline and cannot be pulled up by a chain of attestations), and the content rules. Nothing changes unless every check passed.

**For memory, the watchdog reads the bytes itself.** The address is deliberately **not** signed (the operating system chooses it at run time). The registrant names it, the watchdog hashes the memory there through the `RegionDigester` and refuses a record whose digest is not what is in memory, however well it is signed. A refusal for that reason does not use up the serial, so the same record is fine once the memory is right.

**Files go into the baseline the monitor checks.** An accepted file record is merged into an *effective baseline* (signed baseline + attested files that have not expired; a replaced entry keeps its artifact code). The monitor compares the disk with that, so an attested file is guarded exactly like an installed one, and the moment the record expires or is retired it is judged by the signed baseline alone (an unattested file is then reported as unlisted; a replaced one as modified). Expiry is a re-sign deadline, not a silent hole.

**Announce-first, so a legitimate update does not raise a false alarm.** The order of events in an update is: sign, tell the watchdog, *then* write the file. `Monitor::rebase` marks each added or replaced path as announced. Until the file is seen matching the new entry, a **missing** file (new path) or the **old content still in place** (replacement) is waited for, not reported. Anything else is reported at once: wrong content, a third content, the old signed-but-superseded version put back after the new one arrived (a rollback), or a replaced file that vanishes. A wait ends after 10 minutes (a named starting value, `ANNOUNCE_TTL`, to be measured). Without this the order "sign, then write" would report every update as tampering for the length of the write.

**Why BLAKE3 for memory and SHA-256 for files.** A region is re-read on every scan, so its hash is the hot path: BLAKE3 is the fastest widely reviewed option and the digester hashes in place with no copy. Files keep SHA-256 because the baseline and the spec's manifests use it. The signature is verified once, at registration; afterwards a region costs one hash pass per scan and the module compares 32 bytes.

**Seven codes, not an "always flag" mode.** `E_WATCHDOG_LIVE_CODE_MODIFIED` (attested code no longer matches; the caller unloads the feature that owns it) and `E_WATCHDOG_LIVE_CODE_REFUSED` (a record failed a check; nothing is registered) are separate because they mean different things to a person: one is "something changed code SUL vouched for", the other is "something asked the watchdog to vouch for code it should not". A refusal is *confirmed* when the signature, the key's purpose or the memory's content was wrong, and only *suspicious* when it was stale, reordered or from another root (a restart does those too).

## 3. Spec mapping

Ch. 12 §2.8 (process tamper detection; taxonomy table); Ch. 14 §14.13.3 (what the watchdog guards; the four detections and now live code), §14.13.5 (the privacy-safe record; code list); Ch. 7 §2.1 (the MinHook opt-in that makes trampolines exist). Failure ledger: `SUL-LEDGER/failures.md`, four rows, no recovery-mode letter (they are detections, not failures that retry or restart; the ledger's own letter column reads "—" like the three that were there).

## 4. What the owner decided and what was left to Claude

Decided by the owner (2026-11-27): the two answers quoted at the top. Answer 1 delegates the two open choices of decision 0014 section 6 to "security, performance and speed"; answer 2 is the idea of signing new live code.

Left to Claude under that direction and under the standing direction of 2026-11-22 (performance and optimization, consistent with full functionality):
- **Option A for the codes** (add `E_WATCHDOG_FOREIGN_THREAD` and `E_WATCHDOG_IMAGE_HOLLOWING`): a detection with no stable code cannot be searched for in the hive-mind log or shown the same way twice, and it costs nothing at run time.
- **The Windows probe and the `BELOW_NORMAL` thread go in their own crate** (`sul-watchdog-win`), behind `ProcessProbe` and `RegionDigester`, so `sul-watchdog` keeps `#![forbid(unsafe_code)]` and Miri and Kani keep verifying the pure analysis. Confirmed as the better design for security (the `unsafe` surface stays small and separately reviewed) and speed (the analysis is untouched).
- **The key roles, the two ledgers, the check order, announce-first and BLAKE3 for memory** (section 2).

## 5. Verification (Linux container, rustc 1.97, 2026-11-27)

`cargo fmt --all --check`, `cargo check --workspace --locked`, `cargo clippy --workspace --exclude sul-tauri --locked --all-targets -- -D warnings`, `cargo test --workspace --exclude sul-tauri --locked`, and `cargo xtask` `config-check`, `electron-audit`, `ac-regression`, `release-check --release R0`, `ipc-check`: all pass. `sul-watchdog`: 135 tests.

**Deliberate-bug checks on the security rules: 23 of 23 caught.** Each rule was broken on purpose and the tests had to fail: signature required, key matches purpose, canonical text only, root must match, persistent serial strictly rising, session serial unique, clock skew, expiry at the instant, producer must be trusted code, local-build role limit, a device key cannot replace a guarded file, path validation, memory bytes compared, overlap refused, keys distinct, scan compares the live digest, an unreadable region is not a pass, any change not waited for during a replacement, a missing replaced file not waited for, an announcement ends, `admit_file` tells the monitor, the scan merges session regions, expiry rebases. **Two of them were first missed and the gaps were closed:** (1) an expiry exactly at the moment of presentation (the test record was made at the same instant, so another rule caught it); (2) a replaced file that disappears while its update is awaited (it was being treated as "waiting"; now reported missing).

**Windows CI: success, run 46090494777 (commit `e38f367`, all four jobs), read from the `r0-substrate` job's log.** `sul-watchdog` ran 133 of its 135 tests there (the two symlink tests are `cfg(unix)`), including every live-code test; none failed. The tests use made-up memory and test keys, so this shows the logic behaves the same on Windows, nothing more.

**Not run:** Miri and Kani (not installed here); anything that touches a real Windows process, a real file lock, or live code through a real probe or signer.

## 6. What this does not do, and the open points for the owner

Said plainly, because green tests are not protection:

1. **It cannot stand against an attacker who already runs code in SUL's process *and* can ask the signer to sign.** If the signer lived in the same process, so would the key. That is why signing is a trait the **vault implements in another process at layer 4**; nothing in this crate holds a private key, and until the vault exists nothing signs anything outside the tests.
2. **Where the device and session *public* keys are anchored is not decided.** The release public key can be compiled into the signed baseline's trust anchor. The device key (created and held by the vault at first run) and the session key (made per run by the vault) need a path to the watchdog that an attacker cannot rewrite. *Recommended:* the vault hands the watchdog the session public key and the nonce over the vault's own channel to the watchdog at start-up, and the device public key is recorded in the baseline's trust anchor at first run. This is a layer-4 decision; I will bring it when `sul-vault-core` starts.
3. **Change-and-undo between two scans is not seen.** A region is compared when scanned; code changed and put back inside one interval looks clean. The scan interval is the exposure; a shorter interval costs one hash pass per region.
4. **Nothing reads a live process yet.** The probe, the digester and the thread are the next cut (`sul-watchdog-win`); until then the process scan has only ever seen made-up snapshots.
5. **Executable memory that is not RWX is not flagged by the RWX rule.** An attacker who maps memory writable, writes, and flips it to read-and-execute is not an RWX region when scanned. A strict mode that reports every private executable region that is neither inside a loaded image nor attested would close it, at the cost of false positives from anything that legitimately allocates executable memory in SUL's process (graphics drivers, overlays). *Proposed, not built:* it needs the allowlist's real-world data first. Owner's call whether to build it for R0 or later.
6. **Records must be stored and re-presented by the caller after a restart** (the ledgers live in memory), in order, each re-checked. The installer and updater that store them do not exist yet.
7. **A flood of refused records would grow the threat log.** Only code inside SUL's process can present a record, so this needs an attacker who can already do worse, but nothing rate-limits it. Not built.
8. **The 10-minute announce window and the 120-second clock skew** are starting values; measure before trusting them.
9. **What the installed product guards, and who signs the baseline,** still wait for the installer and the update channel (decision 0014 section 6, items 3 and 5).

Decision 0014 section 6 item 1 is **answered** (Option A, with two more codes); item 2 is **confirmed** (`sul-watchdog-win`, to be built under its own decision).
