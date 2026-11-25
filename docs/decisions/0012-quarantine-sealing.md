# 0012 — The quarantine seals each copy (R0 layer 3, cut 2)

Status: **done on branch `rebuild/r0-bootstrap`, not merged into `main`.** The code and the D-155 entry were written on 2026-11-24 in an earlier session that stopped before the tests were checked, this record written or the log updated; they were verified, finished and recorded on 2026-11-25. It carries out the owner's design of 2026-11-22 (a fresh random key per quarantined file, kept beside its record, behind a key-source trait that the vault supplies at R0 layer 4). The Windows CI result for the commit is recorded in `docs/SESSION_STATE.md` when it has been read. Decision log entry: D-155.

In plain words: a file that SUL sets aside is no longer stored as it was. Its copy is now sealed with a cipher and checked by opening it again, so a changed, cut-short or swapped copy is noticed instead of being restored. Every file gets its own random key. Until the vault exists the key is kept in a file next to the copy, so the seal makes the copy **harmless** (it cannot run, be scanned as the program it was, or be read as text) but does **not** keep it **secret** from anyone who can read the quarantine folder. Nothing in the code or the docs claims more than that.

## 1. What changed

| Where | What |
|---|---|
| `crates/sul-quarantine/src/seal.rs` (new, 570 lines) | The sealed format and nothing else: bytes in, bytes out, no file is touched. XChaCha20-Poly1305 in the `aead::stream` STREAM-BE32 construction, 64 KiB chunks, a 32-byte header that is the associated data of every chunk. `ItemKey` is wiped when dropped, cannot be cloned and never prints itself. |
| `crates/sul-quarantine/src/keys.rs` (new, 217 lines) | The `KeySource` trait (make, return, forget the key of an item) and its default, `KeyFiles`: `key.bin` in the item's own folder, created with create-new (never over another key) and removed only with its item. |
| `crates/sul-quarantine/src/quarantine.rs` (about 600 lines changed) | Quarantining seals the copy and "verify" means opening the sealed copy and hashing what comes out, in one pass, with no second copy on disk. Restoring opens it the same way. Records are version 2 and name the cipher; version 1 records (copies made before this change, stored as they were) are still listed and restored as they were. A key that cannot be had is reported as missing, damaged or locked, never as a copy that "does not match", and recovery does not decide a case while the key is unavailable. |
| `Cargo.toml`, `Cargo.lock`, `crates/sul-quarantine/Cargo.toml` | `chacha20poly1305` gets its `stream` feature here; `zeroize` is new in the workspace; `sha2` (already a workspace dependency) hashes the plaintext as it is opened. No new network or system crate. |
| `crates/sul-core/tests/write_sites.rs` | `keys.rs` is listed as a write site (one create-new, one delete of its own `key.bin`); `seal.rs` and `keys.rs` are in the coverage check; the deletion rule names the extra site. |
| `crates/sul-quarantine/tests/golden/seal_reference.py` (new) | An independent implementation of the format (HChaCha20 written from the XChaCha20 draft and checked against the draft's own test vector, the `cryptography` package's ChaCha20-Poly1305, and the chunking written from the format description). It exists so that the bytes pinned in `seal.rs` are the documented format and not merely what the Rust code produced. It is a reference to run by hand, not part of `cargo test`. |
| 31 new unit tests | `sul-quarantine` went from 28 to 59 unit tests plus its one integration test. |

## 2. What it protects, and what it does not

**Protects:** a changed byte anywhere, a changed header, a reordered, repeated, removed or appended chunk, and a stream cut short at a chunk boundary all fail to open. One leaked key opens one copy, not the others. A copy that does not decrypt to exactly the original is never trusted, at quarantine time or at restore time. Key bytes are wiped when dropped.

**Does not:**
- Keep a secret from anyone who can read the quarantine folder (the default key sits in it). That protection arrives with the vault at R0 layer 4.
- Hide the size of the file (the sealed length shows it to within 16 bytes per chunk).
- Lock key memory against paging, or wipe the cipher's own copy of the key before the cipher is dropped (the cipher wipes it then).
- Say anything about a power cut, a full disk, a locked file or antivirus interference on Windows: none of that was tested (the container is Linux).

## 3. What the owner decided and what was left to Claude

The owner's design (2026-11-22): fresh random key per file, stored beside its record, behind a key-source trait the vault supplies at layer 4. Left to Claude under the standing direction of 2026-11-22 (performance and optimization, consistent with full functionality): the cipher (XChaCha20-Poly1305, the vault's cipher), the construction (STREAM, for its final-chunk flag against truncation), the chunk size (64 KiB: one buffer and one tag per 64 KiB instead of per 4 KiB), and verification in one pass. **Throughput was not measured**: no benchmark exists yet (`cargo xtask bench-telemetry` is not implemented), so the chunk size rests on reasoning, not on a number.

## 4. Verification (Linux container, rustc 1.97, 2026-11-25)

| Check | Result |
|---|---|
| `cargo fmt --all --check`, `cargo check --workspace --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings` | clean |
| `cargo test -p sul-quarantine --locked` (its tests built and run alone) | 59 unit + 1 integration, 0 failed |
| `cargo test --workspace --exclude sul-tauri --locked` | **787 passed, 0 failed** (756 before; the 31 added are the quarantine's) |
| `cargo xtask config-check`, `electron-audit`, `ac-regression`, `release-check --release R0`, `ipc-check`, `pipeline --release R0` | all exit 0; `unsafe-code` still reports `sul-quarantine` among the six crates that keep `#![forbid(unsafe_code)]` |
| Deliberate-bug checks, on a throwaway copy of the tree outside the repository, one break at a time | **7 of 7 CAUGHT** (below) |

| Break | Caught by |
|---|---|
| Final-chunk flag dropped on both the seal and the open side (round trips still pass) | `a_copy_cut_short_or_extended_does_not_open`, and the pinned-format test |
| Header no longer authenticated, on both sides | the pinned-format test **only**. No behavioural test isolates it, because `parse_header` already refuses any non-zero reserved byte and every other header byte feeds the nonce or a format check, so the authentication is defence in depth that the golden bytes pin |
| The post-copy check accepts any digest | `a_file_that_changes_between_hashing_and_copying_is_refused_and_nothing_is_removed` |
| Restore accepts a copy that no longer matches its record | `a_copy_that_opens_but_to_something_else_is_not_trusted` |
| A key file may replace an existing one | `a_key_is_never_made_over_an_existing_one` |
| Keys not random (nothing written into the key) | `two_items_get_two_different_keys` and three more |
| Nonce prefix not fresh | `the_production_path_uses_64_kib_chunks_and_a_fresh_prefix_each_time` |

**Not run:** Miri (not installed here); Windows CI for this commit (pending until read); any Windows-only behaviour.

## 5. Open points

1. **Ch. 2 §2.3 cannot be implemented as written.** It pairs XChaCha20-Poly1305 with a 12-byte base nonce XORed with the chunk index, but XChaCha20 takes a 24-byte nonce (a 12-byte nonce is plain ChaCha20-Poly1305, or AES-GCM). The quarantine does not depend on the vault's layout, so nothing here is blocked. The vault (R0 layer 4) will need a decision: keep XChaCha20 and change the nonce text to a 24-byte nonce built from a prefix and a counter (what this crate does), or keep the 12-byte text and change the cipher. Not decided; it is for the owner when the vault is built.
2. **Chunk size and throughput** are unmeasured (section 3).
3. **Key custody** moves to the vault at layer 4 through `KeySource`; the sealed format does not change when it does.
