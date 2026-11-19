README.md

SUL (Soup's Unified Launcher) — one product, one installer, one binary, one config schema, one update channel, one brand, one compatibility matrix. Releases are capability milestones on that product. Nothing is cut; features are sequenced by the gate that unlocks them.



What this is

This repository contains the specification for SUL, a local-first desktop platform that unifies a game launcher, an AI assistant, an emulation frontend, a music player, a broadcast engine, a vault, a frame-generation layer, and a sensing/security handoff bundle into one installer. The specification is split into four content artifacts (plus this README), each serving a different reader.



Nothing in this repository is a shipping promise. The spec describes what the product would be if every gate passes. The release ladder classifies features by the gate that unlocks them; it does not promise a timeline. Features are gated on measurements, not dates.



The four content artifacts

Artifact	What it is	Who reads it	Length

SUL-BRIEF.md	The front door. Identity, scope, non-goals, invariants, hardware envelope, reader's guide.	Anyone new to SUL. A new engineer reads this first.	\~20 pages

SUL-ARCHITECTURE.md (Ch. 01–17)	One chapter per subsystem in a single combined file (use the Chapter Index at the top to read only your target chapter). Every mechanism, config key, failure code, fallback chain, benchmark, test requirement.	A new engineer who wants to work on a specific subsystem. An outside reviewer who wants to understand how a subsystem works.	\~200 pages total

SUL-LEDGER.md (combining `gates.md`, `baselines.md`, `failures.md`, `guards.md`, `releases.md` with a Ledger Index)	The machine-readable source of truth. gates.md (every A.x gate), baselines.md (every baseline with status tag), failures.md (every error code with Mode A–F classification), guards.md (every AST guard and network tripwire), releases.md (the full release plan, component lists, Tier 1 events, reference configs).	The build system. CI. The release auditor. Anyone who wants to look up a specific gate or baseline.	\~80 pages total

SUL-DECISIONS.md	The append-only decision log. Every \[CUT] annotation, every migration table, every rejection rationale, every "was referenced as §X — body written here."	Anyone who wants to know why the spec looks the way it does. A future editor who is about to "clean up" something that looks redundant.	\~60 pages

Where to start

If you have 15 minutes: read SUL-BRIEF.md. That's it. It will tell you what SUL is, what it isn't, and what the invariants are.



If you have 2 hours: read the Brief, then read the Architecture chapter for the subsystem you care about. If you want to understand the AI pipeline, read Ch. 10 — Local AI.md. If you want to understand the broadcast engine, read Ch. 06 — Broadcast Engine.md. Each chapter stands alone.



If you're about to write code: read the Brief, then read `SUL-ARCHITECTURE.md` (`Ch. 01 — Core Platform`, via the Chapter Index) (which has the process architecture, IPC, config, startup sequence, compatibility matrix). Then read the chapter for your subsystem. Then read the ledger entries that reference it (gates.md, baselines.md, failures.md, guards.md).



If you're about to make a decision that looks like it contradicts something: read SUL-DECISIONS.md first. Almost everything that looks redundant or wrong is a decision that was made for a reason.



How to find a fact

Suppose you want to know why Combat Full is \~1.30–1.35 GB in Instruct mode with ReTrace active.



The Brief states the number in §7.2 (Reference Hardware and Performance Envelope).



The Architecture Book Ch. 10 §2.1 (the tier table) explains the mechanism: weights (\~1.05 GB Q4\_K\_S) + ReTrace dormant buffer + KV split across profiles.



baselines.md §9 (AI) \[ai.combat\_full\_instruct] vram\_mb gives the value, the status tag (gated), and the gate that must pass (A.54).



gates.md A.54 gives the full gate criterion (retention ≥95% at 11.1% token retention, prefill speedup ≥2.5×, etc.).



SUL-DECISIONS.md D-002 gives the history of why Combat Full is one checkpoint with four profiles rather than a multi-tier ladder.



Every fact in the spec follows this pattern. The Brief states the headline. The Architecture Book explains the mechanism. The Ledger gives the operational data. The Decisions Log gives the history.



The one-product rule, in one paragraph

SUL is one product: one installer, one config schema (forward-compatible migrations only), one update channel, one brand, one compatibility matrix, one binary that contains every component. Releases (R0–R4) are capability milestones on that product, not a fragmentation of it. Every release is a strict superset of the previous one — same binary (feature-gated, not split binaries), same config schema, same installer, same update channel, same brand, same compatibility matrix. A user on any release is on the same product. Upgrading is a config-preserving update. This is the model Blender, Steam, VS Code, OBS, DaVinci Resolve, Krita, and every JetBrains IDE use. The user never perceives fragmentation because the identity, installer, config, and update channel never change.



The release ladder, in one paragraph

R0 (v0.1 "the launcher"): substrate + launcher + vault + one AI tier (Combat Full Instruct) + overlay (external DComp) + OCR + app auth. R1 (v0.2–0.3 "everything mainstream"): music player, broadcast engine core, emulation (libretro + sul-gb + run-ahead), Combat Full Thinking profile, search pipeline, sul-agent, prosody, audio governor, DSP, voice FX, STT, modding suite, VFS, sandbox, Hive fetch. R2 (v0.4–0.5 "the deep technical release"): AI deep stack (Combat Full Conversation, Deep, ReTrace, Medusa, phrase head, ChainFlow, LiLiCorr, SPEED-Q, APEX, KV backends, ToolPRM), frame generation, broadcast full (ISO + USB/IP + agent API), optimize + zstd\_gpu, Coca-Cola, verification caching, HRTF, Deep 8K. R3 (v0.6+ "sensing and security"): CSI, BCI, GEV, rPPG, browser automation, security toolkit, VoiceStudio, uncensored models, health dashboard, binary audit, auto-remediation, global sensor fusion. Prerequisite: A.152 (legal/ethical framework) complete and reviewed before any R3 feature ships. R4 (research only): 400B distillation, FRISM, MapCoder-Lite, QuietPrune/StepPrune, fused STAR-KV kernel, and every A.4 item.



No dates. No owners. No promises. A feature ships when its gate passes.



The non-goals, in one paragraph

Out of scope for the shipping release: Linux, macOS, and ARM ports; mobile companion; first-party game suite; non-emulation cross-OS remote streaming; cloud dependency for core functionality; streaming service integration (music player is local-first); online lyrics fetching; cloud library sync; Android Auto / Android widgets; ad-supported or freemium tiers; DSD native passthrough as a shipping feature (research only, A.125); OBS as a library (libobs-rs, GPL-3.0); Smelter as a bundled dependency (proprietary); Isocast as a dependency (macOS/Linux only); a custom kernel-mode virtual audio driver; SynchronousAudioRouter as the shipping virtual device (kernel driver, fallback only); streaming from OBS's multi-track model; game-hook capture as the default; a virtual camera driver. Structurally refused (code, not config): voice cloning of a specific real person; frame generation on kernel-level anti-cheat titles; mod loading on mods\_allowed\_online: false games; per-process ETW on kernel\_level: true titles; kernel-level interaction of any kind; bypassing anti-cheat hypervisor detection; broadcast game-capture hook on kernel-AC titles; security toolkit runs without a signed consent file; CSI/BCI/GEV surveilling anyone but the user; rPPG on any camera but the user's own; uncensored models bypassing the safety layer.



The eight cross-cutting invariants

Failure taxonomy (Modes A–F). Every failure maps to exactly one recovery mode. The deterministic recovery matrix is exhausted before escalating to code-generation (Mode F). Full taxonomy in failures.md.



Structural refusals. The list above. Enforced at build time by the AST guard; a refactor that removes a refusal fails CI.



Safety layer sits below the model (A.154). The destructive-action safety invariant, Verifier's deterministic tool-call check, structural refusals, and persona constraints are enforced at the agent/router level, not at the model level. An abliterated or third-party uncensored model does not bypass any of them.



Baseline provenance. Every baseline entry in baselines.md carries a status field: measured / target / placeholder / gated. CI fails only on a measured entry that regresses beyond tolerance.



One-installer rule. See the one-product rule above.



AST guards and network tripwires. cargo xtask ac-regression runs on every PR. Full list in guards.md.



Coder trust primitive restrictions. The Coder AI has hardcoded zero write authority over sul-trust, sul-quarantine, sul-watchdog, Ed25519 signing keys, auto-updater manifest, sul-vault-core, sul-face-auth, sul-voice-auth, sul-auth-app. Ch. 14 §2.2.3.



Tier 1 events. Every safety-relevant config change fires an OS-level approval modal. The 21 Tier 1-core events are in releases.md R0. The full per-release list is in releases.md. Ch. 14 §2.1.



Editing rules

The append-only rule. SUL-DECISIONS.md is append-only. Once an entry is written, it is never edited or deleted. When a decision is superseded, append a new entry that cites the old one; the old entry gains a superseded\_by field but its body is unchanged.



The preservation rule. Every reference in Ch. 17 — References.md (R.1–R.27) is preserved permanently. No reference is removed for any reason — not for model cuts, not for feature removals, not for deprecated paths, not for scope changes, not for redundancy cleanup — unless the user explicitly instructs its removal in writing. Cut models remain listed with \[CUT] or \[HISTORICAL] annotations beside them.



The archaeology rule. Every claim in the pre-rewrite spec of the form "was referenced as §11.24 — body written here", "the original §13.6 was titled…", "merged from §3.4 and §11.9", "the pre-consolidation design had Combat Low", "\[CUT]", "\[HISTORICAL]", "OBSOLETE — fork deleted" is now a D-NNNN entry in SUL-DECISIONS.md. The Architecture Book cites D-NNNN; it does not restate the archaeology.



The source-of-truth rule. gates.md is the single source of truth for gate criteria. baselines.md is the single source of truth for baseline numbers. failures.md is the single source of truth for error codes and modes. guards.md is the single source of truth for AST guards. releases.md is the single source of truth for "what ships in which release." No other file duplicates this information. If you find yourself writing a number in the Architecture Book that already exists in the Ledger, link to the Ledger instead.



Repository layout

text

sul-launcher/

├── README.md                     # this file

├── SUL-BRIEF.md                  # the front door

├── SUL-ARCHITECTURE/

│   ├── Ch. 01 — Core Platform.md

│   ├── Ch. 02 — Auth and Vault.md

│   ├── Ch. 03 — Game Library, Virtualization Core, and Mod Toolchain.md

│   ├── Ch. 04 — Audio Pipeline and Voice Stack.md

│   ├── Ch. 05 — Music Player.md

│   ├── Ch. 06 — Broadcast Engine.md

│   ├── Ch. 07 — Overlay Architecture.md

│   ├── Ch. 08 — Vision, Search, RAG, and Multi-Modal Assisted Control.md

│   ├── Ch. 09 — Frame Generation.md

│   ├── Ch. 10 — Local AI.md

│   ├── Ch. 11 — Data Pack Strategy and .sulpack Format.md

│   ├── Ch. 12 — Telemetry, Diagnostics, Crash, Save Shadowing.md

│   ├── Ch. 13 — Hardware-Tailored Integration and Advanced Optimization.md

│   ├── Ch. 14 — Autonomous Hive-Mind and Safety Protocol.md

│   ├── Ch. 15 — Sensing, Security, and Handoff Features.md

│   ├── Ch. 16 — Threat Model, Licensing, Anti-Cheat.md

│   └── Ch. 17 — References.md

├── SUL-LEDGER/

│   ├── gates.md                  # every A.1–A.5 gate

│   ├── baselines.md              # every baseline entry with status tag

│   ├── failures.md               # every error code with Mode A–F classification

│   ├── guards.md                 # every AST guard and network tripwire

│   └── releases.md               # the full release plan

└── SUL-DECISIONS.md              # append-only decision log

Quick reference

Reference rig: RTX 3060 Ti 8 GB / Ryzen 5 5600X / 64 GB DDR4 / Samsung 970 EVO Plus 500 GB NVMe / Seagate 2 TB HDD (secondary library). R3 optionally specifies a 12 GB+ baseline (RTX 4080 12GB+ / Ryzen 7 7700X / 64 GB DDR5).



Stack: Tauri 2 with a Rust core and a React/TypeScript UI. WebView2 with Isolation Pattern + Strict ESM + runtime floor. Target <50 MB idle RAM excluding WebView2, egui, and model weights.



VRAM posture (Brief §7.5): layered ceiling. Idle ≤500 MB; game running with combat AI off ≤2 GB (25% of 8 GB); game running with combat AI on ≤3 GB (37.5% of 8 GB) — general combat ceiling for all games. Deep / broadcast 4K / frame generation are explicit Tier 1 exceptions. Minimize by optimizing, not degrading.



Combat Full VRAM (with ReTrace active, shipping requirement): Instruct \~1.30–1.35 GB, Thinking \~1.35–1.45 GB, Conversation \~1.25–1.30 GB, Coding \~1.15–1.30 GB. All four profiles share the same weights (\~1.05 GB Q4\_K\_S) and differ only in KV cache. HD2's 2.5 GB special case is retired; the general 3 GB combat ceiling (Phase 2) accommodates any profile alone plus sul\_toolprm\_enc plus PP-OCRv5 (\~1.53–1.65 GB total).



Deep VRAM: Q5\_K\_M at 4K + q8\_0 KV = \~6.96 GB. Fits under 8 GB with \~1 GB headroom. Q4\_K\_M available for additional headroom. Q6\_K does not fit.



Release ladder: R0 (launcher + vault + Combat Full Instruct) → R1 (mainstream features) → R2 (deep technical release) → R3 (sensing and security, A.152 prerequisite) → R4 (research only).



Reading order: Brief → Architecture chapter for your subsystem → Ledger entries that reference it → Decisions log for history.


> **Combined-Upload Note:** For single-drag upload under Claude Projects' 20-file limit, `SUL-ARCHITECTURE.md` (17 chapters) and `SUL-LEDGER.md` (`gates.md`, `baselines.md`, `failures.md`, `guards.md`, `releases.md`) are each packaged as one indexed file with line-range indices at the top. Nothing has been cut or condensed.
