# CLAUDE.md — SUL (Soup's Unified Launcher)

Read this file before writing or editing any code.

## Autonomous Execution & Bootstrap (For Claude)

* **Run all build, TOML, and verification commands yourself.** Never ask the user to run `cargo check`, `cargo test`, `cargo xtask`, or TOML/build checks manually.
* **First-run bootstrap**:
  1. Reconstruct the nested repository layout from the 11 flat uploaded files using the **Flat Upload File Map** below (splitting `README-AND-BRIEF.md` and `SUL-LEDGER-AND-DECISIONS.md` by their `<!-- FILE_START/END -->` markers so all canonical paths resolve).
  2. Immediately run `cargo check --workspace` and `cargo xtask release-check --release R0` on your own, resolve any crate/lockfile/toolchain issues, and then proceed through the R0 Build Order (`1` -> `6`) below—verifying each layer with `cargo check`, `cargo test`, and `cargo xtask` before advancing to the next.
* **Continuous state tracking (`docs/SESSION_STATE.md`)**: Update `docs/SESSION_STATE.md` at the end of each build milestone (recording completed layers, active gate status, and next command) so context transfers cleanly across sessions.

## Flat Upload File Map (11 Files)

When uploaded flat to Claude Projects (11 files total), map them to their canonical repository paths:

* `CLAUDE.md` -> `CLAUDE.md`
* `README-AND-BRIEF.md` -> split into `README.md` + `SUL-BRIEF.md`
* `SUL-ARCHITECTURE.md` -> `SUL-ARCHITECTURE.md` (17 chapters with `Chapter Index`)
* `SUL-LEDGER-AND-DECISIONS.md` -> split into `SUL-LEDGER/gates.md`, `SUL-LEDGER/baselines.md`, `SUL-LEDGER/failures.md`, `SUL-LEDGER/guards.md`, `SUL-LEDGER/releases.md`, and `SUL-DECISIONS.md`
* `Cargo.toml` -> `Cargo.toml`
* `cargo-config.toml` -> `.cargo/config.toml`
* `sul-ipc-types-Cargo.toml` -> `crates/sul-ipc-types/Cargo.toml`
* `sul-ipc-types-lib.rs` -> `crates/sul-ipc-types/src/lib.rs`
* `xtask-Cargo.toml` -> `xtask/Cargo.toml`
* `xtask-main.rs` -> `xtask/src/main.rs`
* `sul-r0-scaffolds.txt` -> split into `crates/sul-ring/*`, `crates/sul-wire/*`, `crates/sul-trust/*`, and `crates/sul-metrics/*`

## Source of Truth & Scoped Reading Rules

1. `README-AND-BRIEF.md` (`README.md` + `SUL-BRIEF.md`) — repo map, identity, one-product rule, release ladder, invariants, hardware envelope.
2. **Chapter-Scoped Reading Rule (Mandatory)**: `SUL-ARCHITECTURE.md` combines `Ch. 01`–`Ch. 17` into one file. **NEVER** load the entire `SUL-ARCHITECTURE.md` into context at once—use the **Chapter Index** at lines 1–25 to read **only** the exact line range for `Ch. 01 — Core Platform` and the specific subsystem chapter you are implementing.
3. **Ledger & Decisions Scoped Reading Rule (Mandatory)**: `SUL-LEDGER-AND-DECISIONS.md` combines all 5 `SUL-LEDGER/*.md` files and `SUL-DECISIONS.md`. **NEVER** ingest it all at once—use its **Section Index** (lines 1–14) to read **only** the exact line range for the ledger (`gates`, `baselines`, `failures`, `guards`, `releases`) or `SUL-DECISIONS.md` (`D-001`–`D-148`, append-only).

## Non-Negotiable Invariants

* **One-product rule**: One binary (`sul.exe`), one config schema, one installer, one update channel. Features are gate-sequenced (`R0`–`R4`), never cut.
* **Zero Electron / Zero Node in `sul.exe`**: No `electron`, no `nodeIntegration`, no Node runtime in the main binary. Node-based sidecars (`HyperFrames`, `pptx_generate`, `camofox-browser`) run only in isolated containers (`D-139`).
* **Zero kernel-mode code**: Never ship, sign, or load kernel drivers.
* **Structural refusals**: Never bypass or weaken `refusal:voice_clone_attempt`, kernel-AC FG/hook refusals, or `sul-trust` / `sul-vault-core` / `sul-auth-app` boundaries.
* **Safety below the model (A.154)**: Destructive-action -> Thinking invariant and Verifier tool-call checks live in the Rust router/agent layer.

## R0 Build Order (v0.1 "the launcher")

Build and verify in strict dependency order before moving to the next layer:

1. `crates/sul-ipc-types` — single source of truth for `rkyv` IPC schemas, `sul-wire` ring layout, `Tier1Event`, `FailureMode`, and `ReleaseClass`.
2. `xtask` — `cargo xtask config-check`, `ac-regression`, `electron-audit`, `bench-telemetry`, `release-check`.
3. Substrate crates: `sul-ring`, `sul-wire`, `sul-metrics`, `sul-trust`, `sul-quarantine`, `sul-watchdog`, `sul-crash`, `sul-compat`, `sul-toggle`, `sul-core`, `sul-ipc`.
4. Auth & Vault: `sul-auth-app` (Win32 modal before WebView, A.114), `sul-vault-core`, `sul-face-auth`, `sul-voice-auth` (A.92).
5. Launcher & Overlay: `sul-unpack`, `steam-library-parsing`, `sul-overlay` (external DComp only).
6. AI & OCR (R0): `sul-ai-native` (Combat Full Instruct only, A.26 / A.63-Common / A.63-Instruct), `sul-ocr` (PP-OCRv5 INT8).

## Verification Commands (Run Autonomously)

* `cargo check --workspace`
* `cargo xtask config-check`
* `cargo xtask electron-audit`
* `cargo xtask ac-regression`
* `cargo xtask release-check --release R0`
* `cargo test -p sul-vault-core --features=failure-injection`
