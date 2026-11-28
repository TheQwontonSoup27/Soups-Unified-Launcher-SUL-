SUL-DECISIONS.md
The append-only rule. This file is a decision log. Entries are D-NNNN, sequential. Once written, an entry is never edited or deleted. When a decision is superseded, the new entry cites the old one; the old entry gains a superseded\_by field but its body is unchanged. When a decision is rejected, the entry records the rejection and its rationale; it is not removed. When a reader wants to know why the spec looks the way it does, they read this file — not the Architecture Book. The Architecture Book describes current state; this file describes how we got here.

The archaeology rule. Every claim in the pre-rewrite spec of the form "was referenced as §11.24 — body written here", "the original §13.6 was titled…", "merged from §3.4 and §11.9", "the pre-consolidation design had Combat Low", "\[CUT]", "\[HISTORICAL]", "OBSOLETE — fork deleted" is now a D-NNNN entry below. The Architecture Book cites D-NNNN; it does not restate the archaeology.

The status vocabulary. Every entry has one of the following statuses:

decided — the decision is in force.

superseded — a later decision replaced this one; the body is preserved for provenance.

rejected — the option was considered and rejected; the rejection is in force.

parked — out of scope for the shipping release; may re-open if the condition that parked it changes.

merged — the content described was absorbed into another location; the entry records where.

archived — retained for historical record only; no operational effect.

stub — a placeholder pending content; no shipping obligation.

Part 1 — Model ladder and tier consolidation
D-001 · One product, multiple releases (the Blender model)
Status: decided
Context: The spec grew to describe a game launcher, an AI assistant, an emulator frontend, a music player, a broadcast engine, a vault, a frame-generation layer, and a handoff feature bundle. The question was whether this was one product or several.
Decision: One product. One installer, one config schema, one update channel, one brand, one compatibility matrix, one binary. Releases (R0–R4) are capability milestones, not fragments. Every release is a strict superset of the previous one. This is the same model Blender, Steam, VS Code, OBS, DaVinci Resolve, Krita, and every JetBrains IDE use. The user never perceives fragmentation because the identity, installer, config, and update channel never change.
Consequence: The release ladder (releases.md) is the only schedule in the spec. Every feature is a gated component of the product; nothing is cut. See D-077 (nothing is cut, everything is gated) and D-078 (release-ladder superset invariant).

D-002 · Combat tier consolidation: one checkpoint, four profiles
Status: decided
Context: The pre-consolidation design had three combat tiers (Combat Low, Combat Mid, Combat Full), three coding/general tiers (Coding Low, Butler, Deep Extended), and two vision layers (Combat Full's InternViT and Combat-Vision-Lite). Each had different base models, different quants, different VRAM footprints. The tier ladder had become a fragmentation of the product, not a simplification of it.
Decision: Combat Full is the only combat checkpoint. It runs in one of four inference profiles — Instruct 2K, Thinking 4K, Conversation 16K, Coding 8K (research) — selected per turn by a router. All four profiles share the same weights (InternVL3-2B Q4\_K\_S) and the same llama\_context; the profile flip is a runtime parameter change (--reasoning-budget and --ctx-size), not a model reload. Deep is the only coding checkpoint (K2-Horizon-7B Q5\_K\_M). Scene understanding is a capability layer (sul-vlm), not a tier.
Consequence: The tier table in Ch. 10 §2.1 reflects this. Every other chapter that referenced Combat Low, Combat Mid, Butler, Coding Low, or Deep Extended now references either Combat Full (with a profile) or Deep. The router (sul-agent::combat\_profile) is the mechanism that makes four profiles feel like one model. See D-003 through D-008 for the per-model cuts.

D-003 · Hammer2.1-3B cut (Combat Full fine-tuning replaces it)
Status: decided (with live trigger)
Context: Hammer2.1-3B was a specialist execution model with BFCL 81.42 non-live / 95.00 live. It was the strongest tool-calling model in the tier ladder. It was also execution-only and required a swap-in — it had no vision, no dialogue, no planning capability.
Decision: Cut Hammer. Replace it with a LoRA-tuned Combat Full (InternVL3-2B) trained via function masking + xLAM corpus + ToolPRM. The target is not Hammer-grade specialist precision — it is a generalist VLM that approaches Hammer's tool-calling capability while keeping vision and dialogue in one checkpoint. The A.63 gate (per-profile BFCL ≥65 non-live on Instruct and Thinking, ≥95% router accuracy, 100% destructive-action recall) is the release gate.
Live trigger: If A.63 returns <65 non-live on any profile, this rejection is automatically re-evaluated. Hammer can return as a swap-in if the fine-tuning does not clear the gate. The releases.md R0 components list does not include Hammer; if A.63 fails, an R0 revision adds it.
Rationale: The \[CUT] annotation in R.4 (Hammer2.1-3B entry) preserves Hammer as the specialist benchmark Combat Full is measured against, not as a shipping model. See D-036 (Hammer as execution booster is rejected — same decision from the opposite direction).

D-004 · Butler (Qwen3-4B-Instruct-2507) cut
Status: decided
Context: Butler was the conversational tier — a 4B general model with strong dialogue and no tool calling. It existed because the pre-consolidation design had no way to give Combat Full a dialogue mode.
Decision: Cut Butler. Combat Full's Conversation profile (16K context) replaces it. The Conversation profile is the same checkpoint as Instruct and Thinking; it just runs with a larger context window and a bounded reasoning budget (256), which lets it do open-ended dialogue without changing the model.
Consequence: The \[CUT] annotation in R.4 (Butler entry) preserves the model as historical context. The useCombatMode.ts hook (old Butler-era Instruct/Thinking/Auto) is removed; useCombatFullStatus.ts replaces it. See D-010 (indicator rename chain).

D-005 · Coding Low (Qwen2.5-Coder-3B-Instruct) cut
Status: decided
Context: Coding Low was a 3B code-specialized model. It existed to give the pre-consolidation design a coding tier smaller than Deep.
Decision: Cut Coding Low. Deep is the only coding checkpoint. Combat Full's Coding profile (8K, research-gated) is the only other code path, and it ships only if A.101 passes (HumanEval ≥60). No 3B specialist fallback exists.
Consequence: The \[CUT] annotation in R.4 preserves the model. The distillation pipeline (D-021 / Ch. 10 §2.21) references this cut as the reason it no longer targets a 3B Qwen coder. See D-006 (Deep Extended cut).

D-006 · Deep Extended (Qwen2.5-7B-Instruct + PrismML) cut
Status: decided
Context: Deep Extended was a PrismML-patched Qwen2.5-7B that enabled 262K context fitting in 7.7 GB with q4\_0 K/V. It existed to give the pre-consolidation design a larger-context coding tier.
Decision: Cut Deep Extended. Deep (K2-Horizon-7B) covers agentic coding at a 32K ceiling. The 262K-context capability is not a shipped deployment — it was a research demonstration that the PrismML patches enabled. 512K context on 24 GB+ cards remains A.13 research (Ch. 10 §10.26).
Consequence: The PrismML fork is deleted from the vendored submodule list (see D-011). The \[CUT] annotation in R.4 preserves the model. The "PrismML patches enabled 262K context fitting in 7.7 GB with q4\_0 K/V" claim is preserved as historical context in the reference entry — it is not a shipping capability.

D-007 · Combat-Vision-Lite (Qwen3.5-0.8B) cut
Status: decided
Context: Combat-Vision-Lite was a 0.8B vision model for the deprecated combat path. Under the three-tier design it existed as a small vision tier for scenarios where Combat Full was too expensive.
Decision: Cut Combat-Vision-Lite. Combat Full's InternViT encoder handles all gameplay vision. sul-vlm's no-game backends (MiniCPM-V, LFM2.5-VL, jina-vlm, Qwen2.5-VL) handle no-game scene queries. These are two distinct vision paths, not three.
Consequence: The qwen35\_08b.rs backend is removed from the sul-vlm source tree. Qwen35\_08B is removed from the VlmBackend enum in Ch. 1 §3.2. The AST guard verifies sul-vlm never references qwen35\_08b (Ch. 16 §16.15.2). The \[CUT] annotation in R.4 preserves the model. vision\_bench.py::qwen35\_08b is marked DEPRECATED (Ch. 10 §10.23) — retained for historical comparison but not run by CI.

D-008 · Qwen3-0.6B as Combat-Exec rejected (A.69)
Status: rejected
Context: Qwen3-0.6B was evaluated as a tiny execution specialist for Combat-Exec — a sub-1B model that would handle tool formatting at near-zero VRAM cost.
Decision: Rejected. Action score 0.700 is too low for execution. Not in shipping release.
Consequence: A.69 in gates.md records the rejection. The pre-consolidation Combat-Exec tier does not exist; the Instruct profile handles fast execution at 2K context.

D-009 · Mode enum deprecated; Tier + CombatProfile replaces it
Status: decided
Context: The Mode enum (CombatMinimal, CombatLite, CombatExec, CombatVisionLite, CodingLow, Butler, Deep, DeepExtended) carried tier-like names that no longer correspond to a single checkpoint. New code should not use it.
Decision: Deprecate Mode. Retain it in the enum for backward compatibility with the SwitchMode control message (Ch. 1 §3.2). New code uses Tier (CombatFull, Deep) + CombatProfile (Instruct, Thinking, Conversation, Coding). A profile switch is a runtime parameter change, not a tier load/unload. The SetCombatProfile control message replaces SwitchMode.
Consequence: The SwitchMode variant is marked DEPRECATED — use SetCombatProfile in Ch. 1 §3.2. ModelLoaded / ModelEvicted telemetry fires once for Tier::CombatFull, not per profile. EscalationTriggered still uses Tier for from/to — a Combat Full → Deep escalation fires it; Combat Full Instruct ↔ Thinking ↔ Conversation ↔ Coding switches do not.

D-010 · CombatVariantIndicator → CombatModeIndicator → CombatProfileIndicator rename chain
Status: decided
Context: Three UI components have shared the "what combat state is active" role across spec revisions. Each rename corresponded to a design change.
Decision: Final name is CombatProfileIndicator. It shows one of five states: Instruct, Thinking, Conversation, Coding, Auto. When Auto, it shows the router's last decision with confidence. Manual override displays a lock icon. Router disabled displays a warning chip. The old CombatVariantIndicator (showing Combat Low / Combat Mid / Combat Full) and CombatModeIndicator (showing Instruct / Thinking / Auto) are removed — they no longer correspond to valid states. CombatFullStatusIndicator (detailed diagnostic view, Ch. 10 §10.15) and CombatProfileIndicator (compact chip) both consume CombatProfileSelected.
Consequence: useCombatVariant.ts and useCombatMode.ts are removed from the module chapters. useCombatFullStatus.ts replaces them. The old CombatModeIndicator API is removed in Ch. 1 §3.5. The \[HISTORICAL] note in R.4 is preserved for the reference chain.

Part 2 — Fork deletions and external dependency rejections
D-011 · PrismML fork deleted
Status: decided
Context: PrismML-Eng/llama.cpp was a llama.cpp fork with PrismML patches. It was the only consumer of Deep Extended's 262K context.
Decision: Delete the PrismML fork from the vendored submodule list. PrismML is not in the shipping build. Deep Extended is cut (D-006); the 262K-context capability is not a shipped deployment.
Consequence: PrismML-Eng/llama.cpp is marked OBSOLETE in R.4 and retained for provenance only. The sul-ai-native vendor/llama.cpp tree is mainline only (b12207+). There is no llama.cpp-k2 fork, no PrismML cherry-picks, no dual-build complexity. See D-012 (MBZUAI-IFM fork deletion) — the same discipline applies to the K2 fork.

D-012 · MBZUAI-IFM fork deleted after b11454 mainline merge
Status: decided (obsoletes A.25 / A.53)
Context: The MBZUAI-IFM fork was the pinned llama.cpp source for K2-Horizon support before it landed in mainline. A.25 tracked "STAR-KV on MBZUAI-IFM fork" — the fork may not support STAR-KV decompression. A.53 was an alias.
Decision: Delete the MBZUAI-IFM fork. K2-Horizon support landed in mainline llama.cpp b11454 (PR #29535). Deep runs on mainline llama.cpp b12207+ (which satisfies b11454). A.25 and A.53 are marked OBSOLETE in gates.md. E\_STAR\_KV\_UNSUPPORTED\_TREE is retained in the code for future fork scenarios but is unreachable in the shipping path.
Consequence: The single-mainline-tree architecture is preserved. MS-4 (MBZUAI-IFM fork lacks STAR-KV support) is obsolete and marked so in failures.md. The \[OBSOLETE — fork deleted] annotations are preserved in R.4 for provenance.

D-013 · lsfg\_layer shipping against MIT branch only
Status: decided
Context: The upstream PancakeTAS/lsfg-vk layer is MIT through v1.0 and GPL-3.0 after v2.0.0-dev. Vendoring the GPL branch requires the vendored code to remain GPL — incompatible with SUL's MIT/Apache license.
Decision: lsfg\_layer ships against the MIT branch only. If the GPL branch is the only available branch at some future point, lsfg\_layer is gated off permanently. The shipping release of sul-fg uses lsfg\_native (user-supplied Lossless.dll) + win\_fg (clean-room). lsfg\_layer is gated shipping (A.72) — vendoring the MIT branch is legal, vendoring the GPL branch is not.
Consequence: The licensing boundary is documented in Ch. 9 §2.10 and Ch. 16 §16.2.2. The lsfg\_layer backend is listed in R.18 with the exact license transition preserved. GPL is rejected on the same grounds as D-014 (OBS as library). See also D-027 (Smelter/Isocast rejection).

D-014 · libobs-rs (OBS as library) rejected
Status: rejected
Context: libobs-rs wraps the real libobs and could have provided a proven streaming stack.
Decision: Rejected. libobs-rs is GPL-3.0, incompatible with SUL's shipping licence, and does not work on macOS. The broadcast engine is clean-room (Ch. 6 §2.2.1).
Consequence: The broadcast engine's clean-room constraint (Ch. 6 §2.2.1) is enforced by the licensing boundary in Ch. 16 §16.2.8. See D-027 (Smelter and Isocast rejection — same clean-room constraint).

D-015 · Smelter as a bundled dependency rejected
Status: rejected
Context: Smelter (Software Mansion) is a production-grade wgpu live compositor with the clearest agent-drivable API in existence.
Decision: Rejected as a bundled dependency. Smelter's licence forbids bundling it into a distributed app without a deal starting at $1,000/month. The scene-graph API shape is borrowed as reference; the implementation is clean-room.
Consequence: The broadcast compositor (Ch. 6 §2.4.2) is a clean-room implementation of the same View / Rescaler / Crop / Color / Effect node tree. Borrowing the shape of an API is not copyrightable. The rejection is documented in Ch. 6 §2.18 (explicit rejections). See D-013 and D-014 for the same licensing discipline.

Part 3 — Handoff feature reconciliation
D-016 · Handoff feature gate renumbering: A.114–A.135 → A.136–A.152
Status: decided (documentation reconciliation)
Context: The handoff proposal (dated 2026-11-11) introduced sensing, security, and voice-synthesis features with gate numbers A.114–A.135. Those numbers collided with existing gates in the shipping spec (A.114 = app-level auth, A.117 = compatibility validator, etc.).
Decision: Renumber the handoff features to A.136–A.152. The migration table is recorded in gates.md (search for "Handoff Gate Migration Table"). The handoff's own gate numbers are preserved in the historical document but do not appear in the shipping spec's gate ledger. A.153 records the reconciliation. All handoff references in Ch. 15 that carry a "(handoff)" suffix are authoritative for interpreting the mapping.
Migration table (from gates.md):
Handoff gate	Final gate	Feature	Release class
A.114 (handoff)	A.136	WiFi CSI Sensing \& Vital Signs	R3
A.115 (handoff)	A.137	God's Eye View (GEV) \& Geolocation	R3
A.116 (handoff)	A.138	Remote Photoplethysmography (rPPG)	R3
A.117 (handoff)	A.139	Brain2Qwerty Integration (BCI)	R3
A.118 (handoff)	A.140	Browser Automation (browser-use, Crawl4AI)	R3
A.119 (handoff)	A.141	Shannon Autonomous Pentester	R3
A.120 (handoff)	A.142	Hashcat, Nuclei, Mitmproxy	R3
A.121 (handoff)	A.143	VoiceStudio	R3
A.122 (handoff)	A.144	Heretic (Uncensored Models)	R3
A.123 (handoff)	A.145	Local Uncensored Models	R3
A.124 (handoff)	A.146	400B Model Distillation	R4
A.125 (handoff)	A.147	Global Sensor Fusion	R3
A.126 (handoff)	A.148	Predictive Analytics \& Simulation	R3
A.127 (handoff)	A.149	Personal Health Dashboard	R3
A.128 (handoff)	A.150	Agentic Code Review \& Binary Audit	R3
A.129 (handoff)	A.151	Automated Vulnerability Remediation	R3
A.130 (handoff)	A.152	Legal \& Ethical Framework	R3 (prerequisite)
Consequence: No functional change — this is a documentation reconciliation. The handoff's A.118 (browser automation) influences \[features] agent.web\_browsing and the browse\_web tool; its gate is A.140. The handoff's A.119–A.124 (security, VoiceStudio, uncensored models) are gated by A.141–A.145. The handoff's A.125–A.130 (expansions) are gated by A.147–A.152. See gates.md for the full mapping.

D-017 · Voice cloning structural refusal (VoiceStudio clarification, A.155)
Status: decided
Context: The handoff's VoiceStudio feature includes a clone\_voice(audio\_sample) tool. The pre-existing spec's structural refusal (Ch. 16 §2.5) prohibits voice cloning of a specific real person. The question was whether the tool's existence undermines the refusal.
Decision: The clone\_voice tool accepts only character voice preset IDs from the vendored catalog (trained on licensed voice-actor data with explicit commercial-use rights). Any attempt to pass a raw audio sample of a specific real person is refused with E\_VOICESTUDIO\_CLONE\_REFUSED. The API surface has no parameter that could accept a target person. This is enforced by the AST guard (Ch. 16 §16.15) — any PR that introduces a "target person" parameter is rejected regardless of its other merits.
Consequence: A.155 in gates.md records the resolution. The refusal is not a config toggle; it is a code invariant. The \[audio.voice\_fx] voice\_clone field is permanently "refused"; any attempt to change it is a Tier 1 event followed by E\_CFG, forced back to "refused". The voicestudio:attempt\_clone\_specific\_person Tier 1 event lives in R0's refusal:voice\_clone\_attempt (Tier 1-core). See D-073 (voice cloning as code invariant).

D-018 · Uncensored model safety invariant (A.154)
Status: decided
Context: The handoff's uncensored-models feature (\[features] ai.uncensored = true and \[features] ai.local\_uncensored = true) loads abliterated or third-party uncensored models. The question was whether these models bypass the safety invariant (destructive-action → Thinking), the Verifier's tool-call check, or the structural refusals.
Decision: The safety layer sits below the model, not inside it. The destructive-action safety invariant, the Verifier, the structural refusals, and the persona constraints are all enforced at the agent/router level. An abliterated model does not bypass any of them. This is a documentation clarification — the mechanism was already correct; the handoff's uncensored-model feature is safe to add because the safety layer sits below the model.
Consequence: A.154 in gates.md records the decision. The router's deterministic signal evaluation runs on the model's output regardless of which model emitted it. The persona pack's constraints ("warn before destructive operations", "strict tool-calling") are in the always-loaded context and are not removed by abliteration. See D-066 (safety layer sits below the model) and D-017 (voice cloning is a code invariant, not a model-weight property).

D-019 · A.152 promoted from A.2 to A.1
Status: decided
Context: A.152 (Legal and Ethical Framework) was originally in A.2 (gated shipping components). It is the prerequisite for every R3 handoff feature.
Decision: Promote A.152 to A.1 (release-blocking gates). R3 ships when A.152 is written, reviewed, and the R3 features have been audited against it. A.152 is not a feature — it is a condition for shipping the R3 features. The framework defines consent requirements, data retention policies, encryption requirements, user control, legal boundaries, and ethical boundaries.
Consequence: gates.md A.1 bucket grows from 6 to 7 (the original 5, plus A.117 promoted per D-083, plus A.152). A.2 shrinks from 62 to 60 (the same two promotions). The releases.md R3 release\_blocking\_gates array includes A.152 as the first entry.

D-020 · A.117 (compatibility validator) promoted from A.2 to A.1
Status: decided
Context: A.117 was originally in A.2 as "ship when cargo xtask config-check runs the full compatibility matrix at every launch." The validator is what makes the unified-product/multi-release model tractable at monorepo scale — without it, the combinatorics of "every feature is a toggle, every release is a superset" do not scale.
Decision: Promote A.117 to A.1 (release-blocking). The compatibility validator is what makes the coexistence matrix real. It runs at every launch, catches every explicit conflict in Ch. 1 §5, resolves resolvable conflicts with a warning dialog, raises E\_CFG for unresolvable conflicts, and produces the coverage metric (validator rule #34).
Consequence: gates.md A.1 bucket grows from 5 to 7 (with A.152). The releases.md R0 release\_blocking\_gates array includes A.117. See D-077 (nothing is cut) and D-078 (release-ladder superset invariant) for the design context this validator enables.

Part 4 — Scope and non-goals decisions
D-021 · Linux, macOS, ARM ports parked (A.65)
Status: parked
Context: A Linux port is a real option — sul-core, sul-unpack, and sul-ipc are portable in principle.
Decision: Parked. The model ladder is Windows-first because K2-Horizon's mainline llama.cpp support, EAGLE-3's draft path, and the OCR/VLM pipelines are all validated on Windows CUDA first. A Linux port requires re-validating all of them. macOS arm64 is a separate port. Original roadmap pointers (Linux x86\_64 alpha, Linux stable, macOS arm64) are preserved in the Brief as reference intent only — not obligations.
Consequence: A.65 in gates.md records the parking. The Brief's §5.1 (Non-Goals) lists Linux/macOS/ARM under "out of scope for the shipping release." Full technical requirements for re-opening are preserved in A.65 (no scoping work is lost).

D-022 · Mobile companion (SUL-Mobile) parked (A.65)
Status: parked
Context: A read-only companion for stats, game library browsing, and mod install approval via Tauri would extend the platform to mobile.
Decision: Parked. Tauri 2 mobile bindings are stable enough; the deferral is scope, not capability. The mobile companion is not in the shipping release.
Consequence: A.65 in gates.md covers both the desktop port and the mobile companion under one parking entry. The Brief's §5.1 lists it.

D-023 · First-party game suite (Showcase Hub) parked (A.67)
Status: parked
Context: A dedicated Showcase Hub for first-party SUL games, modding-friendly by design, with signed-manifest metadata ingestion.
Decision: Parked. Conditional on a first-party game existing, which it does not. Re-open automatically if a first-party SUL game is ever produced. The technical description is preserved in Ch. 3 §2.21.
Consequence: A.67 in gates.md. The Brief's §5.1 lists it.

D-024 · Non-emulation cross-OS remote session streaming deferred
Status: parked
Context: Streaming the full desktop overlay (not just the emulated game) was a reference-intent feature in §1.18.
Decision: Deferred indefinitely. Share-Play is R1 for emulation (A.66); the desktop-overlay streaming variant is deferred. This is documented in Ch. 1 §1.18 and the Brief §5.1. In the release ladder, only the emulation variant has a release class (R1).
Consequence: The launcher.share\_play feature toggle ships with the emulation scope. The non-emulation streaming variant stays in the Brief's §5.1 (Non-Goals).

D-025 · Custom kernel-mode virtual audio driver rejected
Status: rejected
Context: Voicemeeter ships a kernel-mode virtual audio driver. SUL could have done the same for its broadcast engine's B buses.
Decision: Rejected. Ch. 16 §2.5 prohibits kernel-mode interaction of any kind. The shipping path is USB/IP userspace virtual audio devices (Ch. 6 §2.9). The USB/IP server runs in userspace; Windows enumerates standard USB audio devices; no kernel driver, no WHQL, no kernel attack surface.
Consequence: The \[broadcast.audio.virtual\_device] backend = "usbip" config default. SynchronousAudioRouter is listed as fallback only (see D-026). A.89 records the "user-mode PortCls/APO driver replacing VB-Cable" as long-term research — re-evaluate only if the user-facing cost of installing VB-Cable becomes a measured barrier to adoption. See D-030 (virtual camera driver rejected — same discipline).

D-026 · SynchronousAudioRouter as fallback only
Status: rejected (as shipping path)
Context: SynchronousAudioRouter is a Windows audio driver that synchronises all virtual streams to the physical interface clock, reducing drift and buffer underruns. It is a kernel driver.
Decision: Fallback only. Not the shipping path. If USB/IP has a problem Microsoft will not fix, SynchronousAudioRouter is the emergency fallback — but it violates Ch. 16 §2.5, so it is not in the shipping release.
Consequence: The broadcast engine's \[broadcast.audio.virtual\_device] has backend = "usbip" as default and "vban\_bridge" as fallback (Ch. 6 §2.9). The SynchronousAudioRouter reference is preserved in Ch. 6 §2.18 (explicit rejections). See D-025.

D-027 · Smelter and Isocast as dependencies rejected (clean-room)
Status: rejected
Context: Smelter (proprietary, $1k/month) and Isocast (MIT/Apache, macOS/Linux only, no Windows lane) are the two serious Rust projects adjacent to the broadcast compositor goal.
Decision: Both rejected as dependencies. Smelter's licence forbids bundling; Isocast has no Windows lane. Both are reference material only. The scene-graph API shape and the performance analysis are borrowed; the implementation is clean-room.
Consequence: The compositor (Ch. 6 §2.4.2) implements the same View / Rescaler / Crop / Color / Effect tree clean-room. The admission controller (Ch. 6 §2.4.4) implements the pixel-budget analysis clean-room. The ISO-on-one-clock model (Ch. 6 §2.4.9) is clean-room. See D-013, D-014, D-015 for the same licensing discipline.

D-028 · Game-hook capture refused on kernel-AC titles
Status: rejected (structural refusal)
Context: The game-capture hook (broadcast.capture.game\_hook) injects into the game process. It is the only way to capture fullscreen-exclusive games that WGC cannot see.
Decision: Refused on anti\_cheat.kernel\_level: true titles — the layer is not loaded, the env var is not set, and any attempt to force-enable is refused with E\_REFUSAL\_POLICY. Structural, not policy. WGC is the default for every game (external to the game process, does not trip anti-cheat injection detection).
Consequence: Ch. 16 §2.4 (structural refusals). The \[broadcast.video] capture\_backend = "game\_hook" validator rule B-62 in Ch. 6 §2.7. The AST guard in Ch. 16 §16.15.8. See D-037 (frame generation on kernel-AC titles rejected — same structural refusal discipline).

D-029 · Encoder presets without admission control rejected
Status: rejected
Context: OBS's default NVENC preset P5, tune hq, multipass qres, look-ahead enabled, 2 B-frames, adaptive quantization — with no admission control — is what makes OBS drop frames on fast GPUs.
Decision: The admission controller is not optional. Disabling it is a Tier 1 event with a warning. The encoder admission controller (A.126) is the single largest optimization in the broadcast engine (Ch. 6 §2.4.4). Without it, over-budget configurations silently stutter; with it, the engine refuses with a specific message and a downgrade order.
Consequence: \[broadcast.encoder] admission\_control = true in shipped config. Validator rule B-61 (Ch. 6 §2.7). The AST guard verifies the admission controller is not bypassed (Ch. 16 §16.15.8). broadcast:disable\_admission\_control is a Tier 1 event (R1).

D-030 · Virtual camera driver rejected (USB/IP is the shipping path)
Status: rejected
Context: A virtual camera device for Zoom/Teams/Discord consumption would typically be a kernel driver.
Decision: Rejected. The virtual camera uses the same USB/IP mechanism as the virtual audio devices — no kernel driver. Windows enumerates it as a standard USB video device.
Consequence: Ch. 6 §2.18 (explicit rejections). The \[broadcast.audio.virtual\_device] count = 4 includes "B1-B3 + virtual camera" in Ch. 6 §2.7. Same discipline as D-025.

Part 5 — Rejection ledger from Appendix A (A.3 / A.4)
D-031 · MoE / FreeToken rejected (A.7)
Status: rejected
Context: FreeToken dynamically splits memory misses between CPU and PCIe based on bandwidth, achieving 39.3 tok/s on 8 GB GPU for 35B MoE. arXiv:2608.16157.
Decision: Rejected. FreeToken is a MoE-specific offload technique. MoE was rejected as a tier option because it does not fit SUL's tier budgets without trading system RAM for a model class SUL does not need: at SUL's budgets, MoE needs 2–4× total parameters to match dense quality; a small-total MoE (OLMoE-1B-7B class, \~6.9B total) is 31% slower than same-active dense on 8 GB with 2.1× energy and VRAM at the ceiling; a 35B-class MoE that hits 32+ tok/s requires trading \~30 GB system RAM for VRAM and does not fit Combat Full's 3 GB general combat ceiling (Phase 2) at all. FreeToken has no application without a MoE tier.
Consequence: A.7 in gates.md (bucket A.3). Not inherent inferiority of MoE — tier-budget mismatch. The rejection is at the tier-budget level, not at the technique level.

D-032 · Symmetric q4\_0 KV with Hadamard rotation rejected (A.17)
Status: rejected
Context: Whether mainline llama.cpp's native KV quant with Hadamard rotation (PR #21038) makes q4\_0/q4\_0 viable at +7.6% PPL on 12 GB+ cards for Deep Q5\_K\_M at 8K.
Decision: Rejected. Target hardware mismatch — reference rig is 8 GB; A.17 targets 12 GB+ cards. STAR-KV already covers the potential benefit (up to 20× vs q4\_0's 2×). No independent value on 8 GB.
Consequence: A.17 in gates.md (bucket A.3). The rejection is at the reference-hardware level; it does not say q4\_0 KV is bad, it says it has no independent value on SUL's envelope.

D-033 · Multi-model ensembling for combat rejected (A.68) — design anchor
Status: rejected
Context: Whether multiple small models should be ensembled for improved combat judgment/execution.
Decision: Rejected. Violates the 3 GB general combat ceiling (Phase 2). The multi-profile Combat Full design is one model with four inference profiles, selected by a router per turn — routing, not ensembling. Two or more distinct models running simultaneously would exceed the general combat ceiling. This entry is the design anchor for the Combat Full multi-profile migration. It is cited in Ch. 10 §2.1 as the authoritative reference for why Combat Low, Combat Mid, Butler, Coding Low, and Deep Extended were cut and why the multi-profile design is not an ensemble.
Consequence: A.68 in gates.md (bucket A.3). No change required to this entry — it was already correct; the rest of the spec was migrated to match it. See D-002 (combat tier consolidation).

D-034 · Qwen3-0.6B as Combat-Exec rejected (A.69)
Status: rejected
Context: Same as D-008 above.
Decision: Rejected. Action score 0.700 is too low for execution. Not in shipping release.
Consequence: A.69 in gates.md (bucket A.3).

D-035 · 64K context for combat tiers rejected (A.70)
Status: rejected
Context: Whether Combat Full (all profiles) should run at 64K context.
Decision: Rejected. Cannot fit on 8 GB. Combat Full's 2K (Instruct), 4K (Thinking), 16K (Conversation), and 8K (Coding) contexts are deliberate accuracy-equivalent windows sized to their workload classes, not compromise-fit choices. Extending to 64K would require \~6–7 GiB of KV cache in FP16 alone (before weights or vision tokens), exceeding the 8 GB budget on the reference rig. The pre-consolidation rationale cited Combat Low's accuracy collapse after 16K — that reasoning is retired with Combat Low (cut), but the VRAM-fit argument for Combat Full is unchanged and sufficient on its own.
Consequence: A.70 in gates.md (bucket A.3). The Brief's §5.1 (Non-Goals) lists 64K context under explicit rejections. See D-002.

D-036 · Hammer2.1-3B as execution booster rejected (A.71)
Status: rejected
Context: Whether Hammer should be reintroduced as a swap-in execution specialist.
Decision: Rejected. Replaced by Combat Full fine-tuning (Ch. 10 §10.14). Combat Full already has vision + dialogue + tool calling in one checkpoint; Hammer was execution-only and required a swap-in. Live trigger: if A.63 returns <65 non-live on any profile, this rejection is automatically re-evaluated.
Consequence: A.71 in gates.md (bucket A.3). See D-003 (Hammer cut — same decision from the opposite direction).

D-037 · Frame generation on kernel\_level: true titles rejected (A.77)
Status: rejected (structural refusal)
Context: Whether sul-fg should be allowed to run on games with kernel-level anti-cheat.
Decision: Rejected. Structural violation of Ch. 16 §16.3.1. The layer would need to be present in a process protected by nProtect GameGuard / EAC / BattlEye / Vanguard. The anti-cheat detection surface is exactly what Ch. 16 §2.2 exists to avoid. Not a gate — a rejection.
Consequence: A.77 in gates.md (bucket A.3). The frame\_gen.enabled = true with any per-game frame\_gen.engine != "off" and anti\_cheat.kernel\_level = true validator rule A-6 in Ch. 9 §2.4. AST guard in Ch. 16 §16.15.8. See D-028 (game-hook capture refused on kernel-AC titles — same structural refusal discipline).

D-038 · DFlash on non-Butler tiers obsolete (A.19)
Status: rejected (obsolete by D-004)
Context: DFlash was a block-diffusion draft loader that was in Butler's speculation chain.
Decision: Obsolete. Butler was cut (D-004). DFlash is not in any shipping chain. Retained for historical comparison. If a future tier re-enables DFlash, this entry is restored. EAGLE-3 is also retained as a recognized-but-unused rung name for future re-evaluation.
Consequence: A.19 in gates.md (bucket A.4, obsolete). The speculative.rs module recognizes dflash and eagle3 as rung names but neither is in any shipping chain (Ch. 10 §2.21). \[ai.speculative.dflash] baseline is marked DEPRECATED (Ch. 10 §10.23). \[ai.speculative.eagle3] baseline is marked deprecated (historical reference).

D-039 · OScaR INT2 parked (A.1)
Status: parked
Context: OScaR uses Canalized Rotation + Omni-Token Scaling for near-lossless INT2 KV quantization with 5.3× memory reduction, 3.0× decode speedup, 4.1× throughput increase vs BF16 FlashDecoding-v2 (ZunhaiSu/OScaR-KV-Quant).
Decision: Parked. STAR-KV (up to 20×) supersedes OScaR (5.3×). Unpark only if A.18 fails and GAAC is insufficient.
Consequence: A.1 in gates.md (bucket A.3). The reference is preserved in R.4.

D-040 · ZenDNN on AMD CPUs archived (A.51)
Status: archived
Context: ZenDNN achieves up to 4.5× prompt processing speedup for llama.cpp on AMD EPYC CPUs.
Decision: Archived. Reference rig (Ryzen 5 5600X) is Zen 3; ZenDNN's largest gains are on EPYC server CPUs. Not applicable. Re-purposed as a record of a future module direction: sandbox / malware-testing module for suspicious file triage, based on Windows Sandbox .wsb config generation + host read-only sharing + network disabled as default secure configuration. Decoupled from ZenDNN. Not in shipping release.
Consequence: A.51 in gates.md (bucket A.3). The reference is preserved in R.4.

Part 6 — Section-merge archaeology
D-041 · §3.4 + §11.9 process architecture merged
Status: merged into Ch. 1 §2.6
Context: The pre-consolidation spec had a process architecture section in §3.4 and a duplicate in §11.9. The §11.9 version explicitly self-disclaimed as a duplicate; the §3.4 version was the operative one.
Decision: Merge both into Ch. 1 §2.6 (Process architecture). Unique items from §11.9 preserved: sul-agent Combat Full profile router runs in sul.exe's agent thread (same thread as REWOO/ReAct orchestrator); sul-audio-governor runs in sul-audio's process; sul-voice-fx runs in sul-dsp/sul-audio's process; sul-stt runs in sul-audio's process; sul-gb runs inside the emulator process spawned by sul-emu; sul-savestate is a library; sul-fg runs as a Vulkan implicit layer in the game's process; sul-quant is build-time/load-time; sul-kv-compress and sul-spec-draft run inside sul-ai-native; sul-toolprm runs in sul.exe's agent thread; sul-vault-core/sul-face-auth/sul-voice-auth run inside sul.exe's dedicated vault thread; sul-music.exe, sul-broadcast.exe, sul-csi.exe, sul-bci.exe, sul-voicestudio.exe are separate processes.
Consequence: The §11.9 duplicate is deleted. Ch. 1 §2.6 is authoritative.

D-042 · §11.11g to §10.13 (sul-agent) merge
Status: merged into Ch. 10 §2.13
Context: §11.11g contained the sul-agent REWOO/ReAct hybrid, OODA-Tool, TSCG, and planner\_split content. Under the pre-consolidation spec it lived in the AI Tier table (§11.11g), but the router (Ch. 10 §2.12), the OODA-Tool scaffold (Ch. 10 §2.13), and the fine-tuning recipe (Ch. 10 §2.14) cross-reference it heavily.
Decision: Move §11.11g content into Ch. 10 §2.13 (sul-agent). The routing flow, prosody gate, planner/verifier/executor/solver architecture, OODA-Tool state scaffold, TSCG schema compilation, planner\_split module, verification escalation path, task-type classifier, planner tier selection, and all tests are preserved there.
Consequence: Ch. 10 §2.13 is authoritative. The sul-agent crate row in Ch. 1 §11.5 references Ch. 10 §2.13.

D-043 · §11.11n to failures.md merge
Status: merged into SUL-LEDGER/failures.md
Context: §11.11n was the failure taxonomy with the Mode A–F classifications. It was fragmented — some entries in Ch. 1 §1.16 (base CoreError), some in each module chapter, some in §11.11n.
Decision: Move the full failure taxonomy to SUL-LEDGER/failures.md. Every error code, every Mode A–F classification, every failure-injection flag lives there. The Architecture Book chapters cite failures.md for the failure codes they emit; they do not restate the mode classification.
Consequence: failures.md is authoritative. Ch. 1 §3.4 preserves the base CoreError enum for the reader's reference and cross-links to failures.md.

D-044 · §11.20 baselines merged
Status: merged into SUL-LEDGER/baselines.md
Context: §11.20 was the master baseline framework (the \[overlay], \[ipc], \[telemetry], \[webview], \[sustained] cross-cutting baselines). Individual baselines lived in each module chapter.
Decision: Move all baselines to SUL-LEDGER/baselines.md. Every entry, every status tag (measured / target / placeholder / gated), every tolerance. Module chapters cite the ledger entry; they do not restate the baseline. The canonical home for \[sustained] is Ch. 13 §2.14; Ch. 10 §10.23 references it.
Consequence: baselines.md is authoritative. The cross-cutting baselines in Ch. 1 §1.12 preserve the framework (the status tag definitions, the CI gating rule) for the reader's reference.

D-045 · §13.6 mixed deferred-features table split per module
Status: merged into per-module sections
Context: §13.6 was a single mixed table of "Additional deferred features" — some AI, some audio, some vision/search, some sul-wire.
Decision: Split the table per module.
AI rows → Ch. 10 §10.25 (deferred AI features).
Audio rows → Ch. 4 §4.15 (deferred audio rows).
Emulation rows → Ch. 3 §2.18 (deferred emulation rows) — the original §13.6 had none, and the table's content was all AI, audio, vision, and sul-wire.
Vision/search rows → Ch. 8 §2.13 (deferred rows) — the original §13.6's vision/search rows are preserved in Ch. 8 §2.11 (gate entries).
sul-wire row → Ch. 7 §2.9 (gate entries).
Consequence: Each module chapter has its own deferred-features subsection. The §13.6 unified table is deleted.

D-046 · §12.1 Tier 1 model dual usage
Status: decided
Context: The pre-consolidation §12.1 sometimes referred to "Tier 1 model" (the Data Pack Evolution Tier 1 — Assisted Migration Drafting) and sometimes to "Tier 1 event" (the config approval modal). The two are unrelated.
Decision: Keep both usages, distinguished by context: "Tier 1 event" = config approval; "Data Pack Evolution Tier 1" = migration drafting. Ch. 14 §2.1 documents this terminology distinction. The Tier 1 event classification principle is in Ch. 14 §2.1; the full Tier 1 event list per release class is in releases.md.
Consequence: No functional change. The terminology is clarified in Ch. 14 §2.1.

D-047 · §11.24 / §11.25 feature classification / compatibility matrix "was referenced as §X — body written here"
Status: merged
Context: §11.24 (feature classification) and §11.25 (compatibility matrix) were referenced by many sections that predated their writing. When they were written, the spec noted "was referenced as §11.24 — body written here."
Decision: Feature classification → Ch. 1 §2.4. Compatibility matrix → Ch. 1 §5. Both are authoritative in their new locations. The old "was referenced as §11.24" / "§11.25" note is removed from the spec body — it lives in D-047 for provenance.
Consequence: Ch. 1 §2.4 (feature classification) and Ch. 1 §5 (compatibility matrix) are the canonical locations. The validator rules that referenced §11.24 / §11.25 now reference Ch. 1 §4.2 / §5.

D-048 · §11.11s sul-wire contract merged
Status: merged into Ch. 7 §2.5
Context: §11.11s was the sul-wire crate contract — binary framing protocol, ring map, ArrayBuffer lifetime, backpressure compliance, failure-injection.
Decision: Move §11.11s into Ch. 7 §2.5 (sul-wire). The full contract is preserved there: architecture, config, baselines, failure taxonomy, control message, telemetry, AST guards, test requirements, failure-injection, gate A.50.
Consequence: Ch. 7 §2.5 is authoritative. The sul-wire crate row in Ch. 1 §11.5 references Ch. 7 §2.5.

D-049 · §11.11y FG performance contract merged
Status: merged into Ch. 9 §2.5
Context: §11.11y was the frame generation performance contract — per-frame CPU/GPU overhead at 2× and 4×, VRAM reservation accuracy, frame corruption rate, base-fps gate accuracy, anti-cheat refusal.
Decision: Move §11.11y into Ch. 9 §2.5 (Frame generation baselines and benchmarks). The full performance contract is preserved.
Consequence: Ch. 9 §2.5 is authoritative for FG performance. The FG crate row in Ch. 1 §11.5 references Ch. 9.

D-050 · §13.5 model-ladder table split
Status: merged into Ch. 10 §10.25
Context: §13.5 was a model-ladder table with entries like "K2-Horizon 512K context — Research", "Combat Full Coding profile (A.101) — Gated shipping", "Cross-family LoRA transfer (Trans-LoRA) — Research", "Multi-model ensembling for combat — Rejected", "Qwen3-0.6B as Combat-Exec — Rejected", "Combat-Vision (full Qwen2.5-VL-3B tier) — Removed", "DSpark drafter for Combat Full — Deferred".
Decision: Move the entire §13.5 model-ladder table into Ch. 10 §10.25. Every row is preserved verbatim. Release-ladder classification: Combat Full Coding profile is R3 (gated A.101); all other entries are R4 (research only) or A.3 (rejected / removed).
Consequence: Ch. 10 §10.25 is authoritative.

D-051 · §13.7 gate entries preserved
Status: merged into gates.md
Context: §13.7 was a gate-heavy table with entries like "A.90 sul-gb Game Boy / GBC core — Gated shipping", "A.92 Vault encrypted storage — Hard release gate", "A.123 Musicolet feature parity — Shipping gate", etc.
Decision: Move all §13.7 gate entries into SUL-LEDGER/gates.md. The prose entries are preserved as gate criteria; the harness, fallback, and status fields are added. The prose versions in the module chapters cite the gates.md entry by ID.
Consequence: gates.md is authoritative for gate criteria. See also D-016 (handoff gate renumbering).

D-052 · §17.2 / §17.5 threat-model cross-references
Status: merged into Ch. 16 §16.4.2 / §16.4.5
Context: §17.2 (model file tampering) and §17.5 (prompt injection) had detailed threat model entries in the pre-consolidation spec's threat-model chapter.
Decision: Merge into Ch. 16 §16.4.2 (Model file tampering) and Ch. 16 §16.4.5 (Prompt injection via model outputs). Also referenced from Ch. 10 §10.27 (AI AST guards and threat model summary) and Ch. 11 §2.21 (Pack tampering).
Consequence: Ch. 16 §16.4 is authoritative.

D-053 · §1.2 app auth pointer
Status: merged
Context: §1.2 was "App-Level Authentication (Pointer)" — a cross-reference from the platform chapter to the vault chapter.
Decision: Keep the pointer as Ch. 1 §2.3. It references Ch. 2 §2.1 for the full auth spec. No content change — the pointer structure was correct.
Consequence: Ch. 1 §2.3 is a pointer. Ch. 2 §2.1 is authoritative.

D-054 · §11.22 anti-tampering reference
Status: merged
Context: §11.22 was referenced from the original §1 as the "anti-tampering layers" for the vault. There was no separate §11.22 in the pre-consolidation spec — the content lived in the vault chapter's §2.10.
Decision: Delete the §11.22 reference. The anti-tampering layers (decoy vaults + rate limiting + anti-debug + process hardening + memory protection) are documented in Ch. 2 §2.7 (Vault active defense) and Ch. 2 §2.10 (Vault anti-tampering layers during operation). There is no separate §11.22 — the content is there.
Consequence: The §11.22 reference is removed. Ch. 2 §2.7 and §2.10 are authoritative.

D-055 · §14.1 Tier 1 event classification
Status: merged
Context: §14.1 was "Tier 1 Events — Classification Principle". It described the classification rule but duplicated the actual event list.
Decision: Keep §14.1 as the classification principle only. The full event list per release class is in releases.md. §14.1 references releases.md for the event list; it does not restate it.
Consequence: Ch. 14 §2.1 is the classification principle. releases.md is the event list. See also D-046 (terminology distinction).

Part 7 — Profile design and router decisions
D-056 · Router deterministic-first with small-LLM fallback
Status: decided
Context: The router selects Combat Full's inference profile per turn. It could be a small LLM call on every turn (latency cost) or deterministic signal evaluation (fast, but may be wrong on ambiguous signals).
Decision: Deterministic-first, small-LLM fallback. The router evaluates 10 prioritized signals deterministically (destructive-action detection, user override, prosody intent, task-type classifier, tool verification failures, no-game conversation, default). Only if the deterministic signals conflict or are ambiguous does it invoke a small classifier prompt on Combat Full Instruct (fast path) to make the choice. Target latency for the two-call case: <600 ms cold / <150 ms warm.
Consequence: Ch. 10 §2.12 is authoritative. Router tests in Ch. 10 §2.12 include a proptest for determinism (1000 iterations, no nondeterministic classification). See D-033 (multi-model ensembling rejected — the router is routing, not ensembling).

D-057 · Safety invariant (destructive → Thinking) as a hard gate
Status: decided
Context: A destructive action (file deletion, config overwrite, process launch, save-restore, mod install) should always route to Thinking profile, even if the user has manually set Instruct.
Decision: The safety invariant is signal priority 1. The router intercepts every turn before tool emission and scans for destructive-action indicators deterministically (regex + schema-match — no LLM involvement). If detected and destructive\_action\_always\_thinking = true (default), the router selects Thinking profile regardless of user override. The user sees "Profile overridden to Thinking for destructive action — Instruct profile ignored" in the overlay (CombatProfileIndicator shows a lock icon with this reason). This cannot be disabled by user profile selection, by the router's default, or by any priority-5+ signal. The only way to disable it is the per-game destructive\_action\_always\_thinking = false flag, which is a Tier 1 event and requires the OS-level approval modal.
Consequence: Ch. 10 §2.12 is authoritative. Router test: destructive-action detection recall = 100% on 100 scripted triggers (any miss is a hard fail). E\_DESTRUCTIVE\_INSTRUCT\_OVERRIDE is informational (logged as warning, not error). E\_DESTRUCTIVE\_PROTECTION\_DISABLED is informational (logged as warning when destructive\_action\_always\_thinking = false). The \[CUT]-era Combat-Confirm sub-role (Ch. 4 §2.6) still applies when Thinking is unavailable. See D-018 (safety layer sits below the model).

D-058 · combat\_chain single chain for all profiles
Status: decided
Context: Speculation decoding chains for Combat Full could have been per-profile (Instruct gets a different chain than Thinking). Per-profile chains were proposed in an earlier draft.
Decision: Combat Full has exactly one chain: combat\_chain = \["medusa", "phrase", "unispec", "ngram", "none"]. All four profiles share it. The target model is the same; the drafter predicts the same target's next tokens regardless of profile. There is no per-profile chain. Per-profile chains (combat\_instruct\_chain / combat\_thinking\_chain / combat\_conversation\_chain / combat\_coding\_chain) are rejected with E\_CFG.
Consequence: Validator rule #9 in Ch. 1 §4.2. AST guard in Ch. 16 §16.15.2. Ch. 10 §2.4 and §2.21 are authoritative.

D-059 · ReTrace as profile-agnostic shipping requirement
Status: decided
Context: ReTrace (visual token compression) is what makes Combat Full fit under the general 3 GB combat ceiling (Phase 2). The question was whether each profile needed its own ReTrace configuration.
Decision: ReTrace compression is the same for all Combat Full profiles. One saliency computation, one dormant buffer, one backtracking injection for all four profiles. The dormant buffer is sized for Conversation profile's 16K context (operational maximum) so all lower profiles fit with headroom (\~30–40 MB allocated of \~40 MB for Conversation, \~20 MB used for Thinking, \~10 MB used for Instruct, \~15 MB used for Coding). No profile-specific ReTrace configuration.
Consequence: Ch. 10 §2.24 is authoritative. ReTrace is a shipping blocker (A.54). Without it, Combat Full is \~1.55–1.80 GB (profile-dependent) — the general 3 GB combat ceiling (Phase 2) does not accommodate 1.55–1.80 GB plus OCR plus ToolPRM plus any other co-resident model; the shipping calculation uses the ReTrace-on figures.

D-060 · ToolPRM cross-encoder restores PRM on HD2
Status: decided
Context: ToolRM Qwen3-4B generative backbone is 2.3–2.5 GB Q4. Combined with Combat Full's 1.3–1.45 GB, it exceeds the general 3 GB combat ceiling (Phase 2) by \~2 GB. The spec previously structurally disabled ToolPRM on HD2.
Decision: Distill to a cross-encoder at \~100–300M params (\~60–180 MB Q4). Distilled from ToolRM-4B teacher on ToolPref-Pairwise-30K + SUL traces. Combat Full (SPEED-Q 4-bit) + cross-encoder + OCR = \~1.53–1.65 GB total — comfortably inside 3 GB with \~1.35 GB headroom. The 4B teacher remains ED-only. This restores PRM rescoring on HD2 that was previously structurally disabled.
Consequence: A.100 in gates.md. Ch. 10 §2.23 is authoritative. \[ai.combat\_full] toolprm\_model default is "sul\_toolprm\_enc". See D-002 (Combat Full VRAM figures).

D-061 · jarvis\_combat\_full\_tools.lora mode-tagged training
Status: decided
Context: One LoRA adapter has to serve four profiles. If the adapter is trained on single-profile-only data, it would break the multi-profile behavior silently (Combat Full Thinking might lose reasoning capability, or Instruct might emit thinking tokens, or Conversation might emit tool-call JSON).
Decision: Train the adapter on mode-tagged data. Every training example is labeled instruct, thinking, conversation, or coding. The merged model responds differently based on a mode prefix in the system prompt (controlled by --reasoning-budget at inference time). Profile-specific loss weighting: Thinking examples get 1.5× weight, Instruct 1.0×, Conversation 1.2×, Coding 1.0×. Router-aware ToolPRM training: the cross-encoder reward head scores not just intra-call decisions but also the profile-conditional quality of the call (training-time only — the router's deterministic signals are unchanged). Negative-example suites: 500 pairs per failure mode (Instruct-mode-response-with-thinking-tokens; Thinking-mode-response-without-reasoning; Conversation-mode-response-with-tool-call-JSON; Coding-mode-response-with-conversational-fluff), labeled as negative examples.
Consequence: Ch. 10 §2.14 is authoritative. verify-lora-merge CI job (cosine similarity >0.998, output-token equality >98%). Per-profile BFCL and router accuracy gates. Mode-tagged data hash is a separate field from the adapter's binary hash — tampering with the training data configuration is caught even if the binary hash is unchanged (Ch. 10 §10.24, Ch. 14 §2.14).

D-062 · Pre-created contexts for four profiles
Status: decided
Context: Each Combat Full profile has a different context length (2K / 4K / 16K / 8K). Pre-allocating all four contexts avoids a re-allocation on every profile switch.
Decision: Pre-create all four contexts at model load. Instruct (2K), Thinking (4K), Conversation (16K, lazy no-game), Coding (8K, research). Verify all exist on the loaded llama\_context. Verify --reasoning-budget toggles correctly between 0 (Instruct/Coding), 256 (Conversation), and the configured Thinking budget (bounded positive or -1 unbounded). If Thinking-only fails: Instruct-only for the session with a warning (router bypassed). If Conversation-only fails: no-game turns run Instruct. If Coding-only fails: Coding stays research. If all four fail: Combat Full is dropped entirely with E\_COMBAT\_PROFILE\_INVALID at the fatal severity.
Consequence: Ch. 1 §2.7 Step 3.7 and Ch. 10 §2.11 are authoritative. Step 3.7 runs before Step 4.5 (DSP artifact load) because Combat Full failure is more user-impactful than DSP artifact failure.

D-063 · Profile switch is runtime parameter change, not model reload
Status: decided
Context: Switching profiles could reload the model (safe but slow) or flip runtime parameters on the same context (fast but requires the context to support it).
Decision: Profile switch is a runtime parameter change on the same llama\_context. Target latency <50 ms. ModelLoaded / ModelEvicted telemetry fires once for Tier::CombatFull, not per profile. CombatProfileSelected is the profile-switch event. EscalationTriggered fires for Combat Full → Deep (a tier change) but not for Combat Full ↔ profile switches.
Consequence: Ch. 1 §3.2 (IPC enums) and Ch. 10 §2.12 are authoritative. Validator rule #14 warns at startup if \[ai.combat] profile = "instruct" (or any fixed profile) is combined with destructive\_action\_always\_thinking = true — the user's fixed profile will be overridden for destructive actions.

D-064 · --reasoning-budget is profile-dependent
Status: decided
Context: The reasoning-budget flag controls how many thinking tokens the model emits. Each profile has a different workload: execution (fast), planning (deliberate), dialogue (bounded), code (fast).
Decision: Instruct and Coding use 0 (no thinking tokens — execution fast path). Thinking uses a bounded positive budget. The exact budget is subject to the multi-profile handoff open question: likely 512–1024 tokens bounded, not unbounded, to preserve latency. Until finalized, reasoning\_budget = -1 (unbounded) is specified in Ch. 1 §4.1's \[ai.combat\_full.thinking] block, with the note that the first A.63 run records the chosen budget based on measured latency. Conversation uses 256.
Consequence: Ch. 10 §2.15 is authoritative. Validator rules #11 and #13 in Ch. 1 §4.2 (Thinking and Conversation must have positive budget). Sampling temperatures differ per profile (Ch. 10 §2.15).

D-065 · Router fallback rate tracked, not silent
Status: decided
Context: If the router selects Thinking but Thinking OOMs (transient VRAM spike), the router falls back to Instruct for that turn. This could be silent (bad — the user thinks Thinking is working) or logged (fast — the user knows).
Decision: Logged. CombatRouterFallback telemetry event emitted. Rate tracked in \[ai.combat\_router] instruct\_fallback\_rate\_pct. The overlay shows the fallback in CombatFullStatusIndicator. If the router itself fails to initialize, E\_COMBAT\_ROUTER\_INIT\_FAILED fires and the session runs in fixed profile with a warning chip on CombatProfileIndicator. Fixed-profile operation is a documented degraded state, not silent.
Consequence: Ch. 10 §2.12 and §2.22 are authoritative. \[ai.combat\_router] instruct\_fallback\_rate\_pct baseline. Router test: fallback fires correctly with scripted VRAM-spike workload.

Part 8 — Threat model decisions

D-066 · Safety layer sits below the model

Status: decided (also A.154)

Context: If an abliterated or third-party uncensored model is loaded, does it bypass the safety invariant?

Decision: No. The destructive-action safety invariant (destructive → Thinking), the Verifier's tool-call check, the structural refusals (Ch. 16 §2.4), and the persona constraints are all enforced at the agent/router level, not at the model level. An abliterated model does not bypass any of them. The router's deterministic signal evaluation runs on the model's output regardless of which model emitted it. This is a documentation clarification — the mechanism was already correct. The safety layer sits below the model, not inside it.

Consequence: A.154 in gates.md. Ch. 16 §16.4.16 (Handoff feature threat model). See D-018.



D-067 · Electron exclusion (structural invariant)

Status: decided

Context: Whether SUL should use Electron as its desktop framework.

Decision: No. Zero Electron components. No electron in any manifest; no contextBridge/nodeIntegration/webPreferences/BrowserWindow/BrowserView/&lt;webview&gt;; no ASAR; no bundled Node.js runtime; no Electron sidecar. Structural — enforced by cargo xtask electron-audit.

The exclusion means six attack surfaces are absent by construction: no Electron code, so no Node.js RCE escalation path; no contextBridge to bypass; no ELECTRON\_RUN\_AS\_NODE fuse to pop; no bundled Chromium to run its own CVE treadmill against; no ASAR to tamper with; no Node.js runtime to compromise. This is a real reduction in the binary's attack surface.

It does not make SUL secure on its own. XSS, capability misconfiguration, npm supply-chain exposure, and WebView2 zero-days remain. Those are closed by the Tauri capability audit, CSP nonces, Isolation Pattern, Strict ESM, de-elevation, the three CI gates, the WebView hardening config, and the vault's anti-tampering layers — as defense-in-depth even if WebView is compromised.

Consequence: Ch. 1 §2.2 is authoritative. electron-audit is one of the three CI gates (Ch. 16 §16.15.13). Tauri 2 is the shipping framework.



D-068 · WebView hardening (Isolation Pattern + Strict ESM + runtime floor)

Status: decided

Context: Tauri's default security posture is good but not sufficient. SUL needs explicit hardening.

Decision: Isolation Pattern true, Strict ESM "strict", runtime floor ≥ 156.0.4314.58 (Nov 2026 Microsoft security baseline). These are Tier 1 to disable. The three CI gates — electron-audit, tauri-capability-audit, webview-runtime-audit — run on every PR. The capability surface is locked to the minimum: no "csp": null, no withGlobalTauri: true in production, no wildcard fs scopes, no wildcard window capabilities, no dangerousDisableAssetCspModification: true, no shell:allow-execute. De-elevated sul.exe at standard user privilege.

Consequence: Ch. 1 §4 (Config) and Ch. 16 §16.4.4 (WebView XSS→RCE). The \[webview] config block in Ch. 1 §4.1. Runtime floor bumped on each Microsoft security release. The residual is WebView2 zero-days, which no architecture eliminates.



D-069 · De-elevated sul.exe

Status: decided

Context: Running the WebView host as Administrator would let an XSS→RCE attack escalate to full system access.

Decision: sul.exe runs at standard user privilege, not Administrator. Microsoft's own WebView2 guidance recommends this; it limits the damage an XSS→RCE attack can do. Admin operations (vault unlock, driver interactions) go through a separate elevated process, not the WebView2-hosting process.

Consequence: Ch. 16 §16.4.4. The vault runs as a dedicated thread in sul.exe, not a separate process (Ch. 1 §2.6); the WebView2 renderer process is a child process with its own memory space, so a WebView compromise cannot read vault memory. This is the honest limitation documented in Ch. 2 §2.17 and Ch. 16 §16.4.8.



D-070 · Three CI gates

Status: decided

Context: The Tauri/WebView security posture depends on the capability configuration staying correct across every PR.

Decision: Three CI gates run on every PR:

electron-audit — fails on electron, electron-builder, @electron/\*, nodeIntegration, contextBridge, webPreferences, BrowserWindow, BrowserView, &lt;webview&gt;, asar, ELECTRON\_RUN\_AS\_NODE.

tauri-capability-audit — wraps tauri-doctor + tauri-audit. Fails on "csp": null, withGlobalTauri: true in production, wildcard fs scopes, wildcard window capabilities, dangerousDisableAssetCspModification: true, any shell:allow-execute.

webview-runtime-audit — WebView2 runtime floor (Nov 2026: 156.0.4314.58), Tauri floor >= 2.11.5, plugin-updater >= 2.12.0, fixed plugin-dialog/plugin-http. Bumped on each Microsoft security release.

Consequence: Ch. 16 §16.15.13 is authoritative. CI fails on any PR that weakens the WebView capability surface. This is what makes the Electron exclusion's benefit real — the capability audit closes the XSS→RCE surface that the Electron exclusion does not itself close.



D-071 · Vault honest limitation (DEK readable if process compromised)

Status: decided

Context: The vault's AEAD construction, TPM sealing, DPAPI binding, and rate limiting make the vault file useless to an attacker who does not have the user's credentials. They do not protect the DEK against a fully compromised host process that reads it while the vault is unlocked.

Decision: Document the limitation honestly. "A compromised SUL process can read the DEK while the vault is unlocked. The defenses make this compromise harder (anti-debug, process hardening, de-elevation) but do not eliminate it. The vault is designed to protect the vault file at rest, not to protect the DEK against a fully compromised host process. This is a fundamental limitation of user-space encryption."

Consequence: Ch. 2 §2.17 and Ch. 16 §16.4.8 preserve the exact wording. The defense is layered, not absolute. Memory protection while unlocked (mlock/VirtualLock, secrecy/redoubt, secmem-alloc), anti-debug (cynapse + astraguard + anti-dbg), process hardening (win\_mitigations), and de-elevated sul.exe raise the bar. They do not eliminate the risk.



D-072 · Structural refusals as code, not policy

Status: decided

Context: A policy refusal ("don't do this") is weak — it can be bypassed by a config edit, a code change, or a compromise. A code refusal ("this code path returns E\_REFUSAL\_POLICY") is stronger.

Decision: Every structural refusal is a code path, not a config toggle. The AST guard verifies the refusal at build time — a refactor that removes a refusal fails CI.

The list of structural refusals (from Ch. 16 §2.4):

Voice cloning of a specific real person.

Frame generation on kernel-level anti-cheat titles.

Mod loading on mods\_allowed\_online: false games.

Per-process ETW on kernel\_level: true titles.

Kernel-mode interaction of any kind.

Bypassing anti-cheat hypervisor detection.

Broadcast game-capture hook on kernel-AC titles.

Security toolkit runs without a signed consent file.

Voice cloning of a specific real person (VoiceStudio) — a redundant restatement to make sure the API-surface refusal is checked.

Uncensored models bypassing the safety layer — they don't; the safety layer sits below the model (D-066).

Consequence: Ch. 16 §2.4 is authoritative. The AST guards in Ch. 16 §16.15 enforce them at build time. No structural refusal may be togglable (Ch. 16 §16.15.10). See D-073 (voice cloning is a code invariant).



D-073 · Voice cloning refusal is a code invariant

Status: decided

Context: The clone\_voice(audio\_sample) tool exists. Could it be misused?

Decision: The API surface has no parameter that could accept a target person. The tool accepts only character voice preset IDs from the vendored catalog (trained on licensed voice-actor data). Any attempt to pass a raw audio sample of a specific real person is refused with E\_VOICESTUDIO\_CLONE\_REFUSED. This is enforced by the AST guard — any PR that introduces a "target person" parameter is rejected regardless of its other merits.

Consequence: A.155 in gates.md. Ch. 15 §2.7 (VoiceStudio) and Ch. 16 §16.4.16 preserve the exact wording. The \[audio.voice\_fx] voice\_clone field is permanently "refused". The voicestudio:attempt\_clone\_specific\_person Tier 1 event lives in R0's refusal:voice\_clone\_attempt (Tier 1-core). The refusal is not a config toggle — it is a code invariant. See D-017.



D-074 · Kernel-AC structural refusals

Status: decided

Context: On kernel-level anti-cheat titles (HD2 with nProtect GameGuard, EAC, BattlEye, Vanguard), some SUL features would be detected by the anti-cheat driver.

Decision: Six features are structurally refused on kernel\_level: true titles: frame generation, flip-model overlay, VFS, ETW per-process, game-hook capture, sul-optimize. The AST guard verifies the refusal at build time. External DComp overlay is the default (works on every game including kernel-AC titles — no injection into the game process). WGC capture is the default for broadcast (external to the game process, does not trip anti-cheat detection).

Consequence: Ch. 16 §2.2 and §2.4 are authoritative. The compat matrix (Ch. 1 §5.5) surfaces each refusal. game\_registry.json's anti-cheat block is hash-verified against an Ed25519-signed baseline in sul-trust at every launch (Ch. 16 §2.2). See D-028 and D-037.



D-075 · Anti-cheat blocks hash-verified at every launch

Status: decided

Context: game\_registry.json declares per-game anti-cheat posture (provider, kernel\_level, overlay mode, etc.). If the file is compromised, the refusal logic could be weakened.

Decision: Anti-cheat blocks are hash-verified against an Ed25519-signed baseline in sul-trust at every launch. A mismatch triggers a hardcoded safe-default. This prevents an attacker who compromised the game\_registry.json file from weakening SUL's anti-cheat posture.

Consequence: Ch. 3 §2.24 (Game registry) and Ch. 16 §2.2 (Integrity requirement) are authoritative. game\_registry.json carries an Ed25519 signature (Ch. 14 §2.14). The registry itself is signed; the anti-cheat blocks within it are hash-verified against a separate signed baseline.



Part 9 — Release ladder decisions

D-076 · One product, multiple releases (recap)

Status: decided

See: D-001. Brief §2 (The One-Product Rule).



D-077 · Nothing is cut, everything is gated

Status: decided

Context: The spec grew to describe a dozen products. The temptation was to cut features that did not fit.

Decision: Nothing is cut. Every feature stays in the spec. The release ladder is used to sequence features by the gate that unlocks them, not to remove them. This is the same model Blender, Steam, VS Code, OBS, DaVinci Resolve, Krita, and every JetBrains IDE use.

The one honest exception (per the rewrite's Diagnosis in Part 1): the scope of what a solo developer can ship on the current timeline is different. But the spec's own release ladder does not promise a timeline — it says "ships when this gate passes." The refactor's ledger has a status field; features stay in the spec; the schedule becomes "when the gates pass." That is the honest version of "nothing is cut."

Consequence: Brief §3 (The Release Ladder). releases.md is the source of truth for "what ships in which release." Every \[features.\*] toggle is assigned to exactly one release class (validator rule #29). No component is removed between consecutive release classes (release-ladder superset invariant).



D-078 · Release-ladder superset invariant

Status: decided

Context: If R1 removes a component from R0, or R2 removes a component from R1, the "one product, multiple releases" model breaks.

Decision: Every release is a strict superset of the previous one. No component or feature is removed from an earlier release class. This is enforced by cargo xtask release-audit and by the release-ladder validator rule #29–#34 (Ch. 1 §4.2).

Consequence: releases.md \[release.R0].components is a subset of \[release.R1].components\_added, which is a subset of \[release.R2].components\_added, etc. The AST guard fails on any PR that removes a component from an earlier release class. See D-079 (E\_RELEASE\_UPDATE\_SKIP).



D-079 · E\_RELEASE\_UPDATE\_SKIP (release-ladder enforcement on update)

Status: decided

Context: An update package declares the release class it installs. If the updater installs a package that jumps from R0 to R2 (skipping R1), the R1 features in R2's enable set were not validated on the R0-installed config.

Decision: The updater refuses to install a package whose declared release class is not the immediate successor or a later successor of the currently-installed release class. No skipping from R0 to R2. Mismatch is E\_RELEASE\_UPDATE\_SKIP and the update is refused. In practice, the update channel serves the latest minor within the current major; the release-class guard is a defense against a corrupted or tampered update manifest that jumps the ladder.

Consequence: Ch. 14 §2.5.2 (Auto-updater — Release-ladder enforcement). releases.md's release-ladder enforcement section. See D-080 (reference config per release).



D-080 · Reference config per release

Status: decided

Context: Testing every toggle combination is infeasible. But some combinations must work for the release to ship.

Decision: Every release has a reference config — the tested set for that release. The compatibility validator guarantees these combinations work. Anything outside a release's reference config is warned about (or refused if over budget). This is NOT a feature cut; it is the tested ground truth.

Reference configs:

R0: launcher + vault + one AI tier. sul.exe, sul-ai.exe, sul-ipc.exe, sul-vision.exe (OCR only). Combat Full Instruct, no Deep, no ReTrace, no music, no broadcast, no emulation. VRAM estimate \~1.5 GB (Combat Full Instruct with ReTrace + OCR). The 3 GB combat ceiling (Brief §7.5) accommodates both this and the no-ReTrace case (\~1.77 GB) if A.54 has not yet passed at R0 ship time.

R1: mainstream features. sul.exe, sul-ai.exe, sul-ipc.exe, sul-vision.exe, sul-music.exe, sul-broadcast.exe. Combat Full Thinking, no Deep, no ReTrace, music player on shared-mode, broadcast 1440p60 2 ISOs, emulation libretro + sul-gb run\_ahead = 1, full search pipeline. VRAM estimate \~1.87 GB (Combat Full + broadcast 1080p + music player CPU + OCR). Fits under the 2 GB general substrate ceiling and well under the 3 GB combat ceiling.

R2: deep stack. Same processes + sul-coder.exe (on-demand). Combat Full Thinking, Deep loaded with 2–4 layer offload, ReTrace active, STAR-KV KV, music player on, broadcast 1440p60 2 ISOs, FG off, emulation libretro + sul-gb + sul-savestate run\_ahead = 2. VRAM estimate \~8.9 GB (Combat Full + broadcast 1440p + Deep Q5\_K\_M + q8\_0 KV). R2's reference config therefore records Deep as not co-resident with the game: Deep is loaded on demand (no-game) and evicted before game launch. The "Deep offload layers: 2–4" is retained for the no-game case only. If the user wants Deep co-resident with a game, the Tier 1 confirmation is required (Brief §7.5).

R3: sensing + security on 12 GB+ hardware. Same + sul-csi.exe, sul-bci.exe, sul-voicestudio.exe. Combat Full Thinking, Deep loaded, ReTrace active, STAR-KV KV, music player on, broadcast 1440p60 2 ISOs, CSI on (two ESP32 nodes), BCI on, VoiceStudio on (character voices only), security toolkit off by default. VRAM estimate \~8.5+ GB (Combat Full + Deep no-game + broadcast 1440p + sensing/security modules). The R3 reference config assumes 12 GB+ hardware per its own reference override; the 3 GB combat ceiling is not binding at 12 GB (3 GB = 25%). The R3 hardware baseline already accepts the exception.

Consequence: releases.md §"Per-release reference configurations." See D-081 (coverage metric).



D-081 · Coverage metric

Status: decided

Context: Testing every toggle combination is infeasible. But a release should not ship without knowing how much of the toggle space is actually tested.

Decision: cargo xtask config-check computes a coverage metric — the fraction of the toggle space actually tested by the release's reference configuration. A coverage metric below the release's declared minimum is a warning, not an error. The metric is written to \[compat] log and surfaced in Diagnostics.

Consequence: Ch. 1 §4.2 validator rule #34. The \[compat] config block in Ch. 1 §4.1. The coverage metric is what makes the combinatorics tractable at monorepo scale — it tells the release manager how much trust to place in the reference config.



D-082 · A.26 release-blocking

Status: decided

Context: The mainline llama.cpp bump (b12207+) adds new features and could regress on any tier.

Decision: A.26 is release-blocking. cargo xtask baseline-audit runs the full baselines regression. If any tier regresses beyond tolerance, revert to b11513 and file upstream issue. Otherwise, ship b12207+. b12207 already satisfies b11454 (K2-Horizon merge); the pin remains b12207+.

Consequence: A.26 is in A.1 (release-blocking) in gates.md. Ch. 10 §2.2 (Single llama.cpp build). Submodule update cadence is quarterly, gated on full baselines regression. This is why the single-mainline-tree architecture is stable — every bump is gated by the full regression.



D-083 · A.117 (compatibility validator) promoted to A.1

Status: decided (also D-020)

Context: The compatibility validator is what makes the "every feature is a toggle, every release is a superset" model tractable.

Decision: A.117 is release-blocking. Without it, the combinatorics of the release ladder do not scale. Coverage metric per release (D-081) is what makes the reference configs verifiable.

Consequence: A.117 in A.1. releases.md R0 release\_blocking\_gates includes A.117.



D-084 · A.152 (legal and ethical framework) promoted to A.1

Status: decided (also D-019)

Context: A.152 is the prerequisite for every R3 handoff feature.

Decision: A.152 is release-blocking for R3. R3 ships when A.152 is written, reviewed, and the R3 features have been audited against it.

Consequence: A.152 in A.1. releases.md R3 release\_blocking\_gates includes A.152 as the first entry.



D-085 · Tier 1-core list in R0

Status: decided

Context: Some Tier 1 events are dangerous regardless of release class — they disable a safety invariant, weaken authentication, weaken WebView hardening, or attempt a structural refusal. These should fire regardless of which release the user is on.

Decision: The 20 Tier 1-core events are the tier1\_events array in R0 of releases.md. They fire regardless of release. Everything else is Tier 1 by release class.

The 20 Tier 1-core events:

vault:disable\_auth\_factor, vault:disable\_anti\_debug, vault:disable\_decoy, vault:change\_kdf\_profile, vault:change\_aead\_cipher, vault:enable\_searchable\_index, vault:add\_extra\_path\_via\_config, auth:disable, auth:disable\_factor, auth:change\_reauth\_for\_vault, auth:disable\_anti\_debug, auth:disable\_process\_hardening, auth:disable\_memory\_lock, invariant:disable\_destructive\_action\_always\_thinking, invariant:enable\_uncensored\_model, webview:disable\_isolation\_pattern, webview:disable\_strict\_esm, webview:lower\_runtime\_floor, webview:lower\_tauri\_version\_floor, webview:grant\_shell\_allow\_execute, refusal:voice\_clone\_attempt.

Consequence: releases.md R0 tier1\_events. Ch. 14 §2.1 is authoritative for the classification principle. No Tier 1-core event may be re-classified into a later release class (validator rule #33).



Part 10 — Reference preservation and rewrite archaeology

D-086 · Permanent preservation rule for R.1–R.27

Status: decided

Context: The reference list (R.1–R.27) is a provenance record. Silently pruning it would lose the audit trail for every dependency, model, and inspiration SUL ever considered.

Decision: Every reference in R.1–R.27 is preserved permanently. No reference is removed for any reason — not for model cuts, not for feature removals, not for deprecated paths, not for scope changes, not for redundancy cleanup — unless the user explicitly instructs its removal in writing. Cut models remain listed with \[CUT] or \[HISTORICAL] annotations beside them. Deprecated features remain listed. A future editor must not silently prune the reference list. The list is a provenance record, not a "currently active" list.

Consequence: Ch. 17's header. See D-087 through D-091 for the annotation vocabulary. (R.28 was added later; the preservation rule extends to it.)



D-087 · \[CUT] annotations preserved

Status: decided

Context: Cut models (Lucy, Qwen3.5-2B, Qwen3.5-0.8B, Hammer2.1-3B, Qwen3-4B, Qwen2.5-Coder-3B, Qwen2.5-7B+PrismML) are no longer in the shipping release.

Decision: Preserve the references with \[CUT] annotations. The model remains in R.4 with a note explaining why it was cut and where its capability went. The \[CUT] annotation is not a removal — it is a provenance record.

Consequence: R.4 preserves the cut models. The Architecture Book's Ch. 10 §2.1 tier table does not list them as shipping models. See D-003 through D-008 for the individual cuts.



D-088 · \[HISTORICAL] annotations preserved

Status: decided

Context: Some models (Qwen2.5 base, Qwen3-1.7B quant study) are historical context — they were benchmarks or studies that informed a decision that was later reversed.

Decision: Preserve the references with \[HISTORICAL] annotations. They remain in R.4 as context, not as shipping dependencies.

Consequence: R.4.



D-089 · OBSOLETE references retained

Status: decided

Context: MBZUAI-IFM/llama.cpp, PrismML-Eng/llama.cpp, sudoingX/local-ai-on-your-gaming-gpu are OBSOLETE — deleted forks or superseded capabilities.

Decision: Retain in R.4 for provenance only. The \[OBSOLETE] annotation explains why they are no longer in the shipping path. The reference is not removed (see D-086).

Consequence: R.4's "OBSOLETE" subsection.



D-090 · Deprecated baselines retained

Status: decided

Context: \[ai.combat\_lite], \[ai.combat\_exec], \[ai.combat\_minimal], \[ai.combat\_vision\_lite], \[ai.coding\_low], \[ai.butler], \[ai.combat\_low], \[ai.combat\_mid], \[ai.deep\_extended] are baseline sections for cut tiers.

Decision: Retain for historical comparison where they have value. \[ai.coding\_low], \[ai.butler], \[ai.combat\_low], \[ai.combat\_mid], \[ai.deep\_extended] are removed entirely — those tiers are cut and their baselines have no historical-comparison value.

Consequence: baselines.md §9 marks the retained deprecated sections DEPRECATED and does not remove the removed ones. See D-087.



D-091 · Deprecated benchmark scripts retained

Status: decided

Context: bench\_ai.py::combat\_lite\_judgment, bench\_ai.py::combat\_lite\_q4\_hifi\_ppl, bench\_ai.py::combat\_exec\_bfcl, vision\_bench.py::qwen35\_08b, bench\_ai.py::butler\_eagle3, bench\_sustained.py::butler measured models that no longer ship.

Decision: Retain in the repo for historical-comparison access but do not run by CI. They do not gate the shipping release. They are not part of cargo xtask ci.

Consequence: Ch. 10 §10.23 documents the deprecated scripts. CI does not fail on their results. See D-087.



D-092 · "The original §13.6 was titled…" removed from spec body

Status: merged

Context: The pre-consolidation spec had archaeology in the body — "the original §13.6 was titled 'Additional deferred features' and was a mixed table."

Decision: Remove this archaeology from the spec body. It lives here (D-045) for provenance. The Architecture Book describes current state; this log describes how we got here.

Consequence: No "was titled" notes in the Architecture Book. D-045 records the merge.



D-093 · "Was referenced as §11.24 — body written here" moved here

Status: merged

Context: Similar archaeology — "the original spec's §11.24 was referenced by many sections that predated its writing. When it was written, the spec noted 'was referenced as §11.24 — body written here.'"

Decision: Remove from spec body. Lives here (D-047). The Architecture Book cites the current location; this log records the archaeology.

Consequence: No "was referenced as" notes in the Architecture Book. D-047 records the merge.



D-094 · "The pre-consolidation design had X" narrative removed

Status: merged

Context: The pre-consolidation design had three combat tiers, three coding/general tiers, two vision layers. The spec described each cut in narrative form as archaeology.

Decision: Remove from spec body. Lives here (D-002 and its downstream decisions D-003 through D-008). The Architecture Book describes the current design (one Combat Full with four profiles); it does not narrate the pre-consolidation design.

Consequence: Ch. 10 §2.1 describes the current tier table only. D-002 records the consolidation.



D-095 · Migration tables consolidated here

Status: merged

Context: The pre-consolidation spec had multiple migration tables (A.114–A.135 → A.136–A.152, the batch migration from §X to Ch. Y, the tier consolidation, the mode deprecation). Some tables were in module chapters; some were in appendix A; some were scattered.

Decision: Consolidate migration tables in this log. The handoff gate migration table is in D-016. The tier consolidation is in D-002 and its downstream decisions. The batch migration from the pre-rewrite spec is documented by the four-artifact split — see the Brief's §10 "What Changed in This Rewrite."

Consequence: No migration tables in the Architecture Book. This log is the provenance for every migration.



D-096 · Fork deletion rationale archived

Status: merged

Context: PrismML-Eng/llama.cpp and MBZUAI-IFM/llama.cpp were deleted. The rationale was scattered.

Decision: Archive in D-011 and D-012. The rationale is preserved. The single-mainline-tree architecture is the reason — no fork, no dual-build complexity. A future fork re-evaluation would need to justify itself against the single-tree baseline.

Consequence: Ch. 10 §2.2 (Single llama.cpp build).



D-097 · Cut-model rationale archived

Status: merged

Context: Every cut model has a rationale.

Decision: Archive in D-003 through D-008. Combat Low, Combat Mid, Butler, Coding Low, Deep Extended, Hammer, Combat-Vision-Lite, Qwen3-0.6B Combat-Exec.

Consequence: No cut-model rationale in the Architecture Book. This log is the provenance.



D-098 · A.5 stubs preserved but marked as stubs

Status: decided

Context: The 18 A.5 entries are TODO: stubs — placeholders for missing draft content. They are not real gates.

Decision: Preserve them in gates.md (bucket A.5) but mark clearly as stubs. No content obligation; no shipping dependency; no date; no owner. When a stub is filled, it moves to A.1, A.2, A.3, or A.4 depending on its classification.

Consequence: gates.md A.5 bucket has 18 entries, all status = "stub". The Brief's §3 release ladder does not reference them. They are not in releases.md's component lists.



D-099 · Release plan as capability ladder (not a schedule)

Status: decided

Context: The release ladder (R0–R4) could be read as a delivery schedule (R0 ships in v0.1, R1 in v0.2, etc.) or as a capability classification (a feature ships when its gate passes; the release class determines which release's reference config was validated for it).

Decision: Capability ladder. No dates. No owners. No promises. A feature ships when its gate passes; the release class is a classification of which gate outcome enables it, not a delivery promise.

Consequence: Brief §3. releases.md's \[release.R&lt;n&gt;] blocks have version, name, reference\_hardware, release\_blocking\_gates, components, features\_off, tier1\_events. None of them have a date or an owner. The R4 block explicitly has shipping\_obligation = false, has\_date = false, has\_owner = false.



D-100 · The rewrite itself

Status: decided

Context: The pre-rewrite spec was \~100,000 words in a single document. It was unpublishable — no one could onboard from it, and the archaeology was drowning the current design.

Decision: Split into four artifacts:

Brief (SUL-BRIEF.md) — front door, identity, scope, non-goals, invariants, hardware envelope, reader's guide.

Architecture Book (SUL-ARCHITECTURE/Ch. 01–17) — one chapter per subsystem, fixed template.

Ledger (SUL-LEDGER/gates.md, baselines.md, failures.md, guards.md, releases.md) — machine-readable source of truth for gates, baselines, failure modes, AST guards, release mapping.

Decision Log (SUL-DECISIONS.md, this file) — append-only archaeology.

Nothing was cut. Every mechanism, every number, every gate, every failure code, every AST guard, every config key, every rejection rationale survives in one of the four artifacts. The container changed; the content did not.

The test for whether the rewrite worked: a new engineer reads the Brief in an hour, picks a subsystem, reads its Architecture chapter in an hour, and reads the Gate Ledger entries for that subsystem without needing to open anything else. If that's true, the core problem is fixed and nothing was cut.

Consequence: This file exists. README.md is the navigation. The Brief's §10 "What Changed in This Rewrite" maps old spec sections to new artifacts.



D-101 · A.1 scope is per release class

Status: decided



Context: gates.md defined A.1 as "release-blocking gates" and listed gates from R0, R2, and R3 in the same bucket. This made it read as if R0 could not ship until R2 and R3 gates passed.



Decision: A gate in A.1 is release-blocking for the release class named in its `release` field. A gate assigned to R2 does not block R0. `release-blocking: yes` means "the release class named in `release` cannot ship if this gate fails." A.2 remains gated shipping components: the feature does not ship until the gate passes, but the release can ship without it (the feature is listed in `features\_off`).



Consequence: A.1 grows to 12 gates (A.18, A.26, A.54, A.63-Instruct, A.63-Common, A.63-Thinking, A.63-Conversation, A.63-Router, A.92, A.114, A.117, A.152). Each release's blocking-gate list is authoritative in releases.md.



D-102 · A.63 split into per-profile gates

Status: decided (supersedes the single-gate reading of D-003)



Context: A.63 was one gate that bundled R0 (Instruct BFCL), R1 (router accuracy, destructive-action recall, Thinking BFCL), R2 (Conversation quality), and R3 (Coding HumanEval). This made R0's gate depend on R2 and R3 features.



Decision: Split A.63 into:

\- A.63-Instruct (R0): Instruct BFCL ≥65, Instruct destructive-action warning recall 100%.

\- A.63-Common (R0): shared training data and recipe gate; must pass before any profile gate.

\- A.63-Thinking (R1): Thinking BFCL ≥65, multi-turn, reasoning-token budget.

\- A.63-Conversation (R2): Conversation quality, multi-turn, KV reuse.

\- A.63-Router (R1): router accuracy ≥95%, destructive-action recall 100%, profile-switch latency, determinism, fallback.

A.101 remains the Coding profile gate (R3).



Consequence: D-003's reference to "the A.63 gate (per-profile BFCL ≥65 non-live on Instruct and Thinking, ≥95% router accuracy, 100% destructive-action recall)" is read through this split. R0's blocking gates are A.63-Instruct and A.63-Common; R1 adds A.63-Thinking and A.63-Router; R2 adds A.63-Conversation. The Hammer swap-in (A.71) remains the fallback if A.63-Common or A.63-Instruct fails.



D-103 · ReTrace is R2+; R0 uses non-ReTrace VRAM

Status: decided



Context: Ch. 10 §10.24 described ReTrace as a "shipping requirement" for Combat Full, but releases.md R0 lists ai.retrace as off and A.54 is R2.



Decision: ReTrace is a shipping requirement from R0.1 onward (as soon as A.54 passes). R0 ships without ReTrace at \~1.67–1.77 GB. R0.1 adds ReTrace and drops to \~1.5 GB. The ReTrace-on figures (\~1.30–1.45 GB) apply from R0.1 onward. Under Phase 2's general 3 GB combat ceiling, all configurations fit.



Consequence: HD2's 2.5 GB ceiling (retired in Phase 2) is replaced by the general 3 GB combat ceiling. Ch. 10 §2.1, §2.10, and §10.24 are patched accordingly.



D-104 · Markdown is the canonical ledger format

Status: decided



Context: The Brief and guards.md referenced gates.toml / baselines.toml / failures.toml / guards.toml / releases.toml, while the repo layout and the actual files are .md.



Decision: Markdown is canonical. A generated TOML mirror is a future build step. CI parses the Markdown tables. All .toml references to the ledger are replaced with .md. `release.toml` (the installed manifest at %APPDATA%/SUL/release.toml) and `VERSIONS.toml` (submodule pins) are different files and are not affected.



Consequence: Brief §3 and §9.2, guards.md, and all architecture chapters are patched. README already used .md; only the "six artifacts" typo is fixed.



D-105 · Third-party driver policy

Status: decided



Context: Ch. 16 §16.5 said "kernel-level interaction of any kind" is structurally refused, but Share-Play (A.66) depends on ViGEmBus, audio routing (A.87) depends on VB-Cable, and broadcast virtual audio devices (A.129) depend on USB/IP. All three are user-installed third-party drivers.



Decision: SUL ships zero kernel-mode components. It does not bundle, sign, load, or install a driver. It may depend on a user-installed third-party driver; it detects it at startup and tells the user which driver is missing. The "no kernel-mode interaction" invariant means "SUL does not ship or load kernel code," not "SUL refuses to work with a driver the user installed." Ch. 16 §16.5.1 is added.



Consequence: The structural-refusal list and Ch. 14 §2.16 are patched to point at §16.5.1. The anti-cheat story remains: SUL does not add a kernel attack surface; it does not claim the user's machine has none.



D-106 · Startup abort and build failure are not Modes

Status: decided



Context: failures.md used "Mode F at startup" for config errors and labeled Kani/Miri build failures as Mode F. Mode F is defined as cascading persistent runtime failure that escalates to Coder repair.



Decision: A config error that aborts startup, and a build failure (Kani, Miri, merge mismatch, net tripwire, write-site violation), are not Modes. The Mode column reads "— (startup abort)" or "— (build failure)." E\_INFER\_INVALID\_OUTPUT is Mode B, not Mode A.



Consequence: failures.md is patched. The Mode F escalation summary is clarified.



D-107 · RustXNG license verification gate (A.161)

Status: decided



Context: references.md records RustXNG as "likely AGPL-3.0 — verify before shipping." RustXNG is in the R1 search pipeline, not R3.



Decision: Add A.161 (A.2, R1, non-blocking). Confirm that integrating RustXNG as a separate local process (MCP over localhost, not linked into sul.exe) is compatible with SUL's MIT/Apache license. If AGPL-3.0 and legal review rejects the separate-process basis, the search pipeline falls back to direct\_fetch (already specified as the E\_SEARCH\_PIPELINE\_DEGRADED path). The search pipeline ships either way.



Consequence: A.161 is added to gates.md A.2. releases.md R1's search pipeline line records the fallback. The declared network surface list (Ch. 16 §16.15.14) is unchanged if RustXNG ships; if direct\_fetch becomes the default, the RustXNG MCP surface is removed.



D-108 · A.92 moved from A.2 to A.1

Status: decided



Context: A.92 (vault) was in A.2 but releases.md treated it as release-blocking for R0.



Decision: Move A.92 to A.1. It is release-blocking for R0. The vault remains opt-in at runtime (`\[vault] enabled = false` by default), but the component ships and must be verified.



Consequence: A.1 grows; A.2 count is unchanged (A.92 removed, A.161 added).



D-109 · HD2 VRAM: R0/R1 (no ReTrace) vs R2+ (ReTrace)

Status: decided



Context: The HD2 VRAM calculation in Ch. 1 §5.3, Ch. 3 §3.24, and Ch. 10 §2.1/§2.10/§10.24 assumed ReTrace and ToolPRM were always active. They are not — both are gated.



Decision: R0 HD2: Combat Full Instruct \~1.65–1.75 GB + OCR \~21 MB = \~1.67–1.77 GB. R0.1 HD2 (once A.54 passes): \~1.5 GB. R1 HD2: Combat Full Thinking \~1.70–1.80 GB + OCR \~21 MB = \~1.72–1.82 GB. R2+ HD2: Combat Full \~1.3–1.45 GB + ToolPRM \~60–180 MB + OCR \~21 MB = \~1.53–1.65 GB. All fit under Phase 2's general 3 GB ceiling.



Consequence: The three chapters are patched.



D-110 · Ledger counts reconciled

Status: decided



Context: Brief §9.2 said A.1=7, A.2=60, A.3=17, A.4=40; gates.md said A.1=7, A.2=80, A.3=18, A.4=39.



Decision: gates.md is canonical. After this correction pass: A.1=12, A.2=80, A.3=18, A.4=39, A.5=18. Brief §9.2 is patched to match.



D-111 · VRAM posture — minimize, don't trade

Status: decided



Context: The user's design intent is "SOTA quality/speed/accuracy at the lowest VRAM possible, with no tradeoffs between those dimensions." This is a stronger principle than "fit under a ceiling." It makes every optimization mandatory and turns each gate into a "no-trade threshold."



Decision: Rule 1 of the VRAM posture (Brief §7.5): every VRAM-saving optimization is mandatory. ReTrace, STAR-KV/GAAC, SPEED-Q/APEX, the full speculation chain, ToolPRM, and quantization all ship as soon as their gates pass. No feature ships that trades quality, speed, accuracy, or performance for VRAM savings. The design always targets the lowest VRAM at which every gate still passes.



Consequence: The individual feature gates (A.54, A.96, A.97, A.99, A.100) become "no-trade thresholds" — the feature ships at the setting that minimizes VRAM while passing the gate. The aggregate ceiling (Rule 2) is a cap, not a target.



D-112 · Layered VRAM ceiling; HD2's 2.5 GB retired

Status: decided



Context: The spec had a HD2-specific 2.5 GB ceiling. Kernel-AC does not impose a VRAM ceiling (it scans process memory, not VRAM); the 2.5 GB was conservative, not structural.



Decision: The VRAM budget is layered (Brief §7.5): idle ≤500 MB; game running with combat AI off ≤2 GB (25% of 8 GB); game running with combat AI on ≤3 GB (37.5% of 8 GB) — the general combat ceiling for all games; Deep / broadcast 4K / FG are explicit Tier 1 exceptions. HD2's 2.5 GB special case is retired.



Consequence: Ch. 1 §5.3, Ch. 3 §3.24, Ch. 10 §2.1/§2.10/§10.24, and releases.md reference configs are patched. The compatibility validator (A.117) gains rule #35 (Ch. 1 §4.2) enforcing the layered ceiling.



D-113 · 500 MB idle target

Status: decided



Context: The user confirmed 500 MB as the acceptable idle VRAM. "Less is better as long as it doesn't trade anything else."



Decision: Idle VRAM target is ≤500 MB. The design minimizes below 500 MB if possible without trading quality/speed/accuracy.



Consequence: Baseline `\[vram.idle] footprint\_mb` = { median = 350, p99 = 500 }. Ch. 1 rule #35 enforces.



D-114 · Combat AI ceiling is a cap, not a target

Status: decided



Context: The user said "minimize, don't trade" and "i still would prefer the application taking half of my vram." Combat Full is the primary exception, but the 3 GB ceiling is a cap, not a target.



Decision: Combat AI takes the lowest VRAM that keeps every gate passing. It does not spend headroom just because the ceiling is 3 GB. The R2 reference config's Combat Full (\~1.5 GB with ReTrace, ToolPRM, full spec chain) is well under the ceiling because the design minimized, not because the ceiling forced it.



Consequence: Baseline `\[vram.game\_combat] footprint\_mb` = { median = 1500, p99 = 3000, note = "cap, not target" }.



D-115 · Deep / broadcast 4K / FG are explicit exceptions

Status: decided



Context: Deep Coder, 4K broadcast, and frame generation are clearly high-VRAM. The user wants them available but not silently loaded with a game.



Decision: Deep, broadcast at 4K, and FG are explicit Tier 1 exceptions. They are refused by default with a game running; the user must accept a Tier 1 confirmation to enable them with a game. Deep is for no-game coding; broadcast at 1080p/1440p fits within the general 2 GB substrate ceiling and is not an exception.



D-116 · OSIRIS AI as GEV layer taxonomy reference

Status: decided



Context: OSIRIS AI (osirisai.live) is a live geospatial-intelligence platform whose layer list is almost exactly what A.137 describes.



Decision: GEV adopts OSIRIS AI's layer taxonomy as a reference for the layer set, the renderer's visual target, and the per-layer toggle structure. OSIRIS is a reference, not a dependency. sul-gev remains a clean-room wgpu renderer.



Consequence: Ch. 15 §2.3 gains the layer set and renderer target. A.137 criterion extended: "render 10,000+ geospatial objects at 60 fps across at least 8 of the OSIRIS layer classes."



D-117 · Picarta as geolocation comparison

Status: decided



Context: Picarta.ai is an image geolocation service. Direct peer to Oldcircle/geo-sleuth (already in spec).



Decision: If Picarta's model is usable (open weights or self-hostable API) and beats geo-sleuth by ≥5 percentage points on the A.137 eval set, it replaces geo-sleuth as the default. Otherwise, geo-sleuth remains.



Consequence: A.137 comparison clause. Ch. 15 §2.3 gains the reference.



D-118 · Higgs Audio TTS engine (A.162)

Status: decided



Context: Higgs Audio (boson.ai/higgs-audio) is a recent open-source TTS system.



Decision: Add `\[tts] engine = "higgs\_audio"` as a third option alongside Piper (default) and Kokoro. Gated by A.162. Must meet A.88 and A.85 latency gates and respect the idle VRAM ceiling.



Consequence: A.162 gate. Ch. 4 §2.5 gains the Higgs Audio section. If A.162 fails, Higgs Audio stays a \[DEPRECATED] evaluation.



D-119 · VoxCPM character-voice engine (A.143 comparison)

Status: decided



Context: VoxCPM (voxcpm.net) is a TTS + voice-cloning system. Peer to VoiceStudio (A.143).



Decision: Evaluate VoxCPM against VoiceStudio on the A.143 character-voice-similarity and latency gates. The better engine is the default. Structural refusal applies unchanged — VoxCPM is a character-voice engine, not a bypass.



Consequence: Ch. 15 §2.7 gains the VoxCPM comparison. A.143 gate unchanged; comparison is part of the evaluation.



D-120 · MCP servers are the canonical source of new agent tools

Status: decided



Context: The tool registry (Ch. 11 §2.19) is a fixed list. MCP servers (punkpeye/awesome-mcp-servers) are a concrete external source of new tools.



Decision: MCP servers are the canonical source of new agent tools. Each tool adopted from the list requires: a tool-registry entry, a network-surface declaration, a license check, and (if destructive) a Tier 1 event. The mechanism is generic; specific servers are adopted as needed.



Consequence: Ch. 11 §2.19 gains the MCP mechanism subsection. Ch. 17 gains R.28 (MCP servers).



D-121 · OpenHands as agentic coding reference

Status: decided



Context: OpenHands is a well-known open-source agentic coding system. Deep Coder's design should be compared against it.



Decision: OpenHands provides a reference for planner/executor decomposition, tool registry design, and multi-turn memory. sul-agent adopts the lessons (planner/executor split, deterministic verification) but not the architecture. OpenHands is a reference, not a dependency.



Consequence: Ch. 10 §10.13 gains the comparison subsection. Ch. 17 R.4 gains the link.



D-122 · DeepSeek models and eval-harness reference

Status: decided



Context: DeepSeek models are candidates for the Deep Coder checkpoint. deepseek-harness is a reference for eval-harness design.



Decision: Evaluate DeepSeek-V4 series against K2-Horizon-7B on the A.108 (post-training benchmark lift) and A.110 (PalQuant KV) gates. If a DeepSeek model wins by ≥3 points on the relevant gate, it replaces K2-Horizon. The A.63 eval harness takes design reference from deepseek-harness.



Consequence: Ch. 10 §2.1 gains the model candidates note. Ch. 17 R.4 gains both links.



D-123 · SearXNG provenance (informs A.161)

Status: decided



Context: RustXNG is based on SearXNG. SearXNG is AGPL-3.0; RustXNG inherits that risk.



Decision: SearXNG provenance is recorded in Ch. 8 §2.5. A.161 verifies the license compatibility of the separate-process integration.



Consequence: Ch. 8 §2.5 gains the provenance note. Ch. 17 R.4 gains the link.



D-124 · WASM isolation for security toolkit (A.163)

Status: decided



Context: Docker is the security toolkit's current isolation primitive. WASM is lighter and might fit better on the reference rig.



Decision: WASM isolation (via Wasmtime or WASM Micro Runtime) ships as an additional option, gated by A.163. Docker remains the default. WASM is an additional isolation layer, not a replacement.



Consequence: Ch. 15 §2.6 gains the WASM section. A.163 gate. Ch. 17 R.10 gains the link.



D-125 · CrowdSec-style threat intelligence pattern

Status: decided



Context: CrowdSec provides a reference for behavior-based detection plus community threat data.



Decision: sul-watchdog adopts the pattern. SUL's version is more conservative: the threat log contains file hashes and event types, never file contents; sharing is opt-in via hive-mind.



Consequence: Ch. 12 §2.8 gains the pattern note. Ch. 17 R.10 gains the link.



D-126 · Containerized image/PDF tools (A.170)

Status: decided



Context: docker-gimp and Stirling PDF are containerizable. They fit the security-toolkit's container-isolation pattern.



Decision: `edit\_image` (GIMP) and `edit\_pdf` / `merge\_pdf` / `split\_pdf` (Stirling PDF) ship as agent tools, gated by A.170. Container isolation matches the security toolkit: no network, read-only filesystem except input.



Consequence: Ch. 11 §2.19 gains the tools. Ch. 15 §2.6 gains the container references. A.170 gate.



D-127 · Jellyfin as local library source (revises music player non-goal)

Status: decided



Context: The music player is local-first. Jellyfin is self-hosted, so it is also local-first. Commercial streaming services remain rejected.



Decision: Jellyfin is accepted as an optional `\[music.library.jellyfin]` source. The music player non-goal is revised from "streaming service integration" to "commercial streaming service integration." Self-hosted libraries (Jellyfin, Nextcloud) are accepted.



Consequence: Ch. 5 §3.1 gains the Jellyfin source. A.164 gate. Ch. 16 §16.15.14 gains the Jellyfin network surface.



D-128 · Nextcloud pack sync backend (A.165)

Status: decided



Context: Pack sync uses git. Some users may prefer Nextcloud.



Decision: `\[packs.sync] backend = "nextcloud"` is an optional backend alongside git. Gated by A.165. Same local-first posture as Jellyfin.



Consequence: Ch. 11 §2.15 gains the Nextcloud backend. A.165 gate. Ch. 16 §16.15.14 gains the Nextcloud network surface.



D-129 · LibreTranslate backend (A.166)

Status: decided



Context: Translation currently uses whisper.cpp. LibreTranslate is a self-hostable alternative.



Decision: `\[translation] backend = "libretranslate"` is an optional backend alongside whisper.cpp. Gated by A.166. Local-first.



Consequence: Ch. 11 §2.19 gains the translate tool. A.166 gate. Ch. 16 §16.15.14 gains the LibreTranslate network surface.



D-130 · Additional agent tools (A.167–A.171, A.174–A.178)

Status: decided



Context: A number of SPEC links expand the agent's tool set: ArchiveBox, Changedetection.io, LocalSend, Maxun, Pexels, Ninite, VirusTotal, HaveIBeenPwned, TinEye.



Decision: Each ships as a gated agent tool via its own gate (A.167, A.168, A.169, A.171, A.174, A.175, A.176, A.177, A.178). All are A.2 (non-blocking). All require network-surface declarations and license checks.



Consequence: Ch. 11 §2.19 gains the tools. Ch. 15 §2.6 gains the container tools. Ch. 16 §16.15.14 gains the network surfaces. Ch. 17 gains the references.



D-131 · Vaultwarden auth UX reference

Status: decided



Context: Vaultwarden is a lightweight password manager with mature auth UX.



Decision: The auth modal's reauth intervals, session token handling, and auto-lock behavior take UX reference from Vaultwarden. SUL does not integrate Vaultwarden; the patterns are borrowed.



Consequence: Ch. 2 §2.1 gains the UX reference note. Ch. 17 R.10 gains the link.



D-132 · HD2 2.5 GB ceiling retirement sweep (consistency)

Status: decided



Context: The Phase 2 VRAM posture (D-111, D-112) retired HD2's 2.5 GB special-case ceiling in favor of a general 3 GB combat ceiling for all games. The Phase 2 patches updated Brief §7.2, Ch. 01 §5.3, Ch. 03 §3.24, Ch. 10 §2.1 / §2.10 / §10.24, releases.md reference configs, baselines.md §13, and gates.md A.117. Two prose locations still carried 2.5 GB HD2-ceiling language that Phase 2 missed: Ch. 14 §2.4 (the Hive-Mind compatibility registry model matrix — the `sul\_toolprm\_enc` and `ToolRM Qwen3-4B` rows) and Ch. 16 §3.2 (the co-residence rules paragraph).



Decision: The residual 2.5 GB language is retired in both locations.



Ch. 14 §2.4 — the `sul\_toolprm\_enc` row previously read "fits HD2 ceiling with Combat Full (SPEED-Q 4-bit) + OCR at \~1.53–1.65 GB total"; it now reads "fits the general 3 GB combat ceiling with Combat Full (SPEED-Q 4-bit) + OCR at \~1.53–1.65 GB total." The `ToolRM Qwen3-4B` row previously read "exceeds HD2 ceiling by \~2 GB"; it now reads "exceeds the general 3 GB combat ceiling by \~0.8–1 GB" — the arithmetic check that 2.3–2.5 GB (ToolRM-4B Q4) + \~1.5 GB (Combat Full) ≈ 3.8–4.0 GB is \~0.8–1.0 GB over a 3 GB ceiling, not \~2 GB (which was the excess over the retired 2.5 GB HD2 ceiling). The remaining rows in the matrix reference specific VRAM figures, not ceilings, and are unchanged. The K2-Horizon-7B "Does not fit during HD2" note is still accurate under the general 3 GB ceiling (6.46 GB Q5\_K\_M weights + KV exceeds 3 GB regardless of ceiling) and is unchanged.



Ch. 16 §3.2 — the co-residence rules paragraph previously read "Combat Full (all four profiles) fits under the 2.5 GB HD2 ceiling with ReTrace active"; it now reads "Combat Full (all four profiles) fits under the general 3 GB combat ceiling (Phase 2) with ReTrace active." The rest of the paragraph (the 1.53–1.65 GB total, the Deep does-not-fit note, the ED co-residence arithmetic, the old three-tier obsolescence note) is unchanged — those figures were already correct under either ceiling.



In both locations, the correction is recorded in situ with a bracketed "\[Correction D-132 note …]" so a future reader can trace the change without cross-referencing this entry.



Consequence: The HD2-specific 2.5 GB ceiling no longer appears anywhere in the spec except as historical reference inside D-103 / D-109 / D-112 / D-132 (which is where it belongs — the append-only rule requires the archaeology to be preserved). All shipping-path prose now uses the general 3 GB combat ceiling per D-112.



D-133 · "Mode F at startup" → "— (startup abort)" (consistency)

Status: decided



Context: Phase 1 Patch 8 / D-106 reclassified config-error startup aborts and build failures out of the Mode taxonomy. Failures.md now reads "— (startup abort)" or "— (build failure)" for those entries instead of "Mode F at startup." Ch. 14 §2.11's Mode E example list still carried the pre-D-106 language in one bullet: "E\_MEDUSA\_TEMP\_SCALING\_MISSING / E\_MEDUSA\_OWN\_HEAD\_VARIANT → abort startup with config error (Mode F)."



Decision: The parenthetical is changed from "(Mode F)" to "(— startup abort)" to match failures.md and D-106. The preceding "abort startup with config error" language is unchanged — it already correctly described the behavior; only the Mode-taxonomy label is wrong. A bracketed "\[Correction D-133 note …]" is recorded in §2.11 in situ.



Consequence: Ch. 14 §2.11 is now consistent with failures.md and with D-106. No other location in the spec uses "Mode F at startup" for a config-error startup abort or for a build failure — the sweep was already complete elsewhere in Phase 1 Patch 8.



D-134 · Ch. 11 §2.14 cross-reference audit (consistency)

Status: decided (audit result — no textual change)



Context: During the D-132 / D-133 consistency review, Ch. 11 §2.14 was flagged as a possible residual inconsistency: it begins a paragraph "§12.1 Tier 1 model (now releases.md): Coder AI migration-drafting runs as GitHub Actions workflow." The concern was that "§12.1 Tier 1 model" was a pre-rewrite section reference (the Data Pack Evolution Tier 1 — Assisted Migration Drafting) which D-046 maps to the terminology distinction in Ch. 14 §2.1, and that the pointer might now be stale.



Audit result: The pointer is accurate. The paragraph already carries the "(now releases.md)" forward-pointer — it does not claim that §12.1 still exists as a live section reference; it explicitly acknowledges the renumbering. D-046 records the terminology distinction (Data Pack Evolution Tier 1 ≠ Tier 1 event) and this is exactly the pattern used here: the migration-drafting runs as a GitHub Actions workflow, its \*trigger\* is the Dual-Gate Safety Protocol (Ch. 14 §2.3), and its \*output\* is a PR against sul-hive-mind. None of that is affected by the Tier 1 reclassification.



Decision: No textual change to Ch. 11 §2.14. The pointer is preserved as-is. This entry records the audit so that a future editor does not "fix" the pointer by removing it (which would lose the archaeology trail) or by removing the "(now releases.md)" qualifier (which would misrepresent the provenance).



Consequence: Ch. 11 §2.14 is unchanged. The append-only rule preserves this decision as the record of why the pointer was left in place rather than rewritten.




D-135 · Companion read-only health export vs. mobile companion non-goal
Status: decided
Context: PocketPal AI demonstrates fully offline local GGUF + TTS on mobile, and R3's health dashboard / sensing features naturally benefit from mobile viewing.
Decision: The mobile companion (SUL-Mobile) remains a non-goal (D-022 / A.65). However, an opt-in desktop-initiated export (`[companion]`, gated Tier 1) that writes a read-only `health_trends.sulpack` for external offline mobile viewers is permitted and is not a mobile companion product.
Consequence: Brief §5.1 and Ch. 15 gain the `[companion]` read-only export boundary.

D-136 · Vault and sulpack cloud sync deferred (Ente architecture reference)
Status: parked
Context: Ente demonstrates a zero-knowledge client-encrypted presigned S3 upload architecture where the server stores only ciphertext and encrypted metadata.
Decision: Cloud library sync remains rejected for shipping releases. If cross-device vault or `.sulpack` sync is ever revisited, it will follow Ente's client-side encryption + presigned S3 blob pattern with master keys never leaving the device.
Consequence: Ch. 2 §2.9 and Ch. 11 §2.15 record the research-only `[vault.sync]` and `[sulpack.sync]` architectural pointers.

D-137 · Reclassify A.63-Thinking, A.63-Conversation, and A.63-Router from A.1 to A.2
Status: decided (supersedes A.1 bucket assignment in D-101 / D-102)
Context: `gates.md` previously placed `A.63-Thinking`, `A.63-Conversation`, and `A.63-Router` in A.1 (`Blocking: yes`), while their own fallback clauses explicitly stated that R1/R2 can ship with those profiles turned off (`features_off`).
Decision: Move `A.63-Thinking`, `A.63-Conversation`, and `A.63-Router` from A.1 to A.2 (`Blocking: no`). Only `A.63-Common` and `A.63-Instruct` remain release-blocking for R0.
Consequence: A.1 contains 9 release-blocking gates; `gates.md` and `releases.md` are now internally consistent.

D-138 · Phase 4 handoff gate renumbering (A.155–A.167 → A.179–A.195)
Status: decided
Context: The Phase 4 expansion handoff proposed gate IDs `A.155–A.167`, which collided with existing gates `A.155–A.178` in `gates.md`.
Decision: Renumber all Phase 4 gates into the unused `A.179–A.195` range:
- `A.155 (handoff)` → `A.179` (Colibri MoE tiered streaming, R4 research)
- `A.156 (handoff)` → `A.180` (HyperFrames offline HTML→MP4, R2)
- `A.157 (handoff)` → `A.181` (`pptx_generate` editable PPTX skill, R2)
- `A.158 (handoff)` → `A.182` (`sul-fg` post-process FX DAG, R2)
- `A.159 (handoff)` → `A.183` (`security_audit` multi-stage Coder diff audit, R2)
- `A.160 (handoff)` → `A.184` (`wifi_audit` external aircrack-ng scan, R3)
- `A.161 (handoff)` → `A.185` (`claim_audit` source-grounded fact-check tool, R2)
- `A.162 (handoff)` → `A.186` (`agent.memory` SQLite FTS5 + vector recall, R2)
- `A.163 (handoff)` → `A.187` (`entity_ledger.custom_types` JSON Schema entities, R2)
- `A.164 (handoff)` → `A.188` (`agent.user_tasks` SQLite task table, R1)
- `A.165 (handoff)` → `A.189` (`reading_mode` EPUB/MOBI Conversation injection, R2)
- `A.166 (handoff)` → `A.190` (`filee` offline format converter, R1)
- `A.167 (handoff)` → `A.191` (`auth.profiles` local multi-user profile isolation, R1)
- `A.192` (Magnitude hardware kernel autotune, R2)
- `A.193` (Zero-Breach offline spyware IOC scan, R3)
- `A.194` (Multi-camera `skellytracker`/`RTMPose` pose fallback, R3)
- `A.195` (`camofox-browser` isolated fallback fetch, R2)
Consequence: Zero gate ID collisions across `gates.md`, `releases.md`, and `SUL-ARCHITECTURE.md`.

D-139 · Node.js / Electron exclusion preserved for Phase 4 sidecar tools
Status: decided
Context: HyperFrames, `ppt-master`, and `camofox-browser` use Node.js/browser tooling, while Ch. 1 §2.2 structurally forbids bundling Node.js or Electron in `sul.exe`.
Decision: None of these tools may bundle Node.js into `sul.exe` or run as Electron sidecars. They must run strictly inside isolated containers or as optional user-installed external processes, enforced by `guards.md` and `electron-audit`.
Consequence: Zero Node.js/Electron attack surface added to the shipping binary.

D-140 · Architectural-reference-only rejections (FinceptTerminal, Flutter, Twenty, OpenToonz C++)
Status: rejected (as runtime dependencies)
Context: FinceptTerminal (AGPL-3.0), Flutter (second UI framework), Twenty (full SaaS CRM stack), and OpenToonz C++ (binary bloat) were evaluated alongside the Phase 4 handoff.
Decision: Reject all four as direct runtime dependencies. Adopt only their architectural patterns (FinceptTerminal multi-panel layout, Twenty metadata-driven Entity Ledger schema, OpenToonz pure-Rust FX DAG & XSheet timeline model). Record Flutter in Ch. 17 solely as a historical note on the parked mobile non-goal.
Consequence: Single Tauri 2 + React/TS UI stack and clean MIT/Apache licensing preserved.

D-141 · Ch. 01 §11.3 compiler-rule and Miri/Kani boundary clarifications
Status: decided
Context: Ch. 01 §11.3 referenced `thiserror + #[help("...")]` and `#[likely]`/`#[unlikely]`, and mandated Miri/Kani on crates calling Win32/C++ FFI.
Decision: Clarify that `#[help]` diagnostics come from `miette` alongside `thiserror` error enums, `#[likely]`/`#[unlikely]` use a stable Rust helper (`likely_stable` / `cold_path`), and Win32/C++ FFI calls sit behind thin OS adapter traits so Miri/Kani verify pure-Rust ring, nonce, and state-machine logic without choking on OS syscalls.
Consequence: Prevents compiler and CI-harness false positives during R0 implementation.

D-142 · Tier 1-core count reconciliation (20 → 21)
Status: decided
Context: D-085, `README.md`, and `SUL-BRIEF.md` referred to "20 Tier 1-core events," whereas the canonical `tier1_events` array in `releases.md` R0 and `SUL-BRIEF.md §6.8` contains 21 events.
Decision: Record the exact count as 21 without altering D-085's historical body (append-only rule).
Consequence: `README.md` and `SUL-BRIEF.md` updated to 21.

Closing

This closes the SUL-DECISIONS.md reissue. Entry count: D-001 through D-142. Statuses as recorded:

\- decided: the majority

\- decided (supersedes …): D-102

\- decided (also A.xxx): D-066, D-083, D-084, D-018-adjacent

\- decided (audit result — no textual change): D-134

\- decided (with live trigger): D-003

\- rejected: D-008, D-014, D-015, D-025, D-026, D-027, D-028, D-029, D-030, D-031, D-032, D-033, D-034, D-035, D-036, D-037, D-038

\- parked: D-021, D-022, D-023, D-024, D-039

\- archived: D-040

\- merged: D-041 through D-055, D-092 through D-097

\- decided (consistency): D-132, D-133



The append-only rule applies to this file. Any future correction is a new D-NNNN, not an edit to an existing one.


### D-143 — Application-wide runtime and build performance optimizations (2026-11-18)
- **Status**: decided
- **Scope**: Cross-cutting (`Ch. 01`, `Ch. 02`, `Ch. 04`, `Ch. 05`, `Ch. 06`, `Ch. 07`, `Ch. 11`, `Ch. 12`, `Ch. 13`)
- **Decision**: Adopt `mimalloc` (`#[global_allocator]`), 64-byte cache-line-padded wait-free SPSC rings (`sul-wire` / `sul-ring`), Windows 11 EcoQoS opt-out for real-time audio/broadcast/overlay threads, unified SQLite WAL/`mmap_size`/`prepare_cached` + batched telemetry commits, `FILE_FLAG_OVERLAPPED` + `PrefetchVirtualMemory` + `blake3`/`zstd`-dict `.sulpack` loading, parallel WebView2 environment pre-warm, and `target-cpu=x86-64-v3` with `[profile.dev.package."*"] opt-level = 2` and `[profile.release-ci]` (`lto = "thin"`).
- **Rationale**: Improves latency, frame pacing, cold-start time, and memory efficiency across every subsystem without cutting any feature or adding kernel-mode code.


### D-144 — Ground-up and open-source-backed second performance sweep (2026-11-18)
- **Status**: decided
- **Scope**: Cross-cutting (`Ch. 01`, `Ch. 04`, `Ch. 08`, `Ch. 10`, `Ch. 11`, `Ch. 12`, `Ch. 13`, `Ch. 17`)
- **Decision**: Wire `simsimd` (`R.8`), `aho-corasick` + `memchr` (`R.8`) single-pass DFA prefiltering, `parking_lot` (`R.8`) locks, a double-buffered `mmap` frame-sharing slot (`mmap-sync` / `shm-rings` pattern, `R.8`/`R.14`) for `sul-vision.exe`, a ground-up page-locked (`cudaHostAlloc`) async DMA staging ring for `sul-kv-tier` / ReTrace, and pre-allocated per-turn/per-frame scratch arenas. Verified full license and runtime compatibility across `R.1`–`R.32` (MIT/Apache-2.0 in `sul.exe`; AGPL/GPL/Node tools isolated out-of-process or in containers).
- **Rationale**: Extracts maximum throughput and minimum latency from already-referenced open-source crates and ground-up primitives with zero feature cuts or licensing conflicts.


### D-145 — Zero-copy GPU capture, completion I/O, exclusive audio, prefix PagedAttention, and Windows PGO alignment (2026-11-18)
- **Status**: decided
- **Scope**: Cross-cutting (`Ch. 01`, `Ch. 04`, `Ch. 05`, `Ch. 06`, `Ch. 08`, `Ch. 10`, `Ch. 13`, `Ch. 17`)
- **Decision**: Lock in `windows-capture` + `cudarc` / `NVIDIA/cuda-rust` D3D11-CUDA/NVENC zero-copy VRAM capture (keeping `D-144` `mmap` as non-NVIDIA fallback inside the single CIG-aware CUDA context), `compio` (Windows IOCP completion I/O) for `.sulpack`/VFS/save-shadow disk ops, `rkyv` 0.8 `bytecheck` zero-copy `sul-wire` IPC (`A.22b`), `mistral.rs`-pattern PagedAttention + copy-on-write prefix caching (`A.4`, `A.19b`), `wasapi-rs` exclusive mode + `rubato` SIMD resampling (`A.119`–`A.122`), and `cargo-pgo` + FatLTO + `/OPT:REF,ICF` for Windows PE `sul.exe` (replacing Linux-only LLVM BOLT per `llvm/llvm-project#72205`).
- **Rationale**: Eliminates PCIe frame copies, thread-pool file I/O overhead, IPC deserialization, redundant prefix prefill, and Windows audio mixer jitter while fixing a Linux-only build assumption.

### D-146 — Ground-up Rust/CUDA kernels, redb VFS/cache, lz4_flex, kitoken, FX DAG, and hybrid memory (2026-11-18)
- **Status**: decided
- **Scope**: Cross-cutting (`Ch. 03`, `Ch. 08`, `Ch. 09`, `Ch. 10`, `Ch. 11`, `Ch. 13`, `Ch. 17`)
- **Decision**: Lock in stable-Rust `NVIDIA/cuda-rust` (`cutile-rs`) kernels for `sul-vision` preprocessing, `A.105` STAR-KV dequant-attend, and `A.43` `zstd_gpu` inside the single CIG-aware CUDA context; `redb` ACID zero-copy B-tree for `A.41` VFS writeback journal, `A.40` pack migration, and `A.46`/`A.58b` `blake3`-keyed search cache; `lz4_flex` (`>= 0.13.0`, `safe-decode` enabled) for warm-RAM KV/frame swap; `kitoken` for local token counting; clean-room `wgpu`/SPIR-V `A.182` FX DAG; and SQLite `FTS5` + `simsimd` hybrid memory (`A.186`).
- **Rationale**: Resolves `A.5` infrastructure stubs (`A.40`, `A.41`, `A.46`, `A.58b`) and delivers ground-up, MIT/Apache-2.0-compatible performance and quality across every subsystem.

### D-147 — Master handoff legitimacy audit, Windows/Ampere rectifications, and R0 validator/router lock-in (2026-11-18)
- **Status**: decided
- **Scope**: Cross-cutting (`Ch. 01`, `Ch. 02`, `Ch. 07`, `Ch. 10`, `Ch. 11`, `Ch. 13`, `Ch. 17`, `CLAUDE.md`, `Cargo.toml`, `.cargo/config.toml`, `xtask`)
- **Decision**: Audit and apply the master handoff after rectifying platform and architectural errors: (1) add `core_affinity = "0.8.3"` + `crossbeam-epoch = "0.9.21"` for core-pinned lock-free runtime threads; (2) remove Linux-only `io-uring` from workspace `compio` features so `compio` targets Windows IOCP; (3) enable Windows Control Flow Guard (`-C control-flow-guard`) in `.cargo/config.toml` instead of Linux/nightly `-Z sanitizer=cfi`; (4) use Win32 `CreateFileMappingW`/`MapViewOfFile` section security descriptors instead of POSIX `fcntl` FD seals; (5) restrict FlashAttention-3 to Hopper SM 9.0+ and keep FlashAttention-2 on RTX 3060 Ti Ampere SM 8.6; (6) preserve `.sulpack` signed data packs (`Ch. 11`), `Q4_K_S` Combat Full (`Ch. 10`, not BitNet b1.58), TPM+DPAPI+XChaCha20-Poly1305 vault (`Ch. 02`, not VBS enclaves), and 21 Tier 1 OS-approval events (`releases.md`); (7) upgrade `xtask` with R0 Build Order Validator checks (Edition 2024/MSRV 1.85, zero-`electron`, zero-`io-uring`, layer boundaries, release parity), add zero-allocation critical-path & temporal hysteresis (`§2.12.1`) to Combat Full router, and add `docs/SESSION_STATE.md` tracking to `CLAUDE.md`.
- **Rationale**: Integrates every valid handoff capability while preventing Linux-only build failures, hardware mismatches, and silent architectural drift.

### D-148 — Whole-chart module performance sweep and R0 substrate crate scaffolding (2026-11-18)
- **Status**: decided
- **Scope**: Cross-cutting (`Ch. 01`–`Ch. 17`, `Cargo.toml`, `CLAUDE.md`, `crates/sul-ring`, `crates/sul-wire`, `crates/sul-trust`, `crates/sul-metrics`)
- **Decision**: Add `arc-swap = "1.9.5"`, `rustc-hash = "2.1.3"`, `bumpalo = "3.20.3"`, and `smallvec = "1.16.5"` to workspace dependencies; apply module-by-module optimizations across `Ch. 01`–`Ch. 16` (`§13.5.8`) including wait-free config hot-reload, `DComp` overlay dirty-rect damage tracking, `redb` group commits, and zero-alloc audio callbacks; and scaffold R0 substrate crates `sul-ring`, `sul-wire`, `sul-trust`, and `sul-metrics` in `sul-r0-scaffolds.txt` wired into workspace `Cargo.toml`.
- **Rationale**: Gives Claude a compilable 6-crate R0 workspace on first run while extending zero-cut performance optimizations to every module in the architecture.

### D-149 — Declare the Steam Web API and a Discord webhook as network surfaces (2026-11-21)
- **Status**: decided
- **Scope**: `Ch. 16` (§16.15.14), `SUL-LEDGER/guards.md` (Network tripwires), `SUL-LEDGER/releases.md`, `SUL-BRIEF.md`, `xtask/src/network.rs`, `crates/sul-net`
- **Decision**: The owner added two surfaces to the declared network list before any code calls them: (1) the Steam Web API (`api.steampowered.com`), public player data keyed by the user's SteamID64, read-only, with any API key user-supplied and kept out of source control and logs; (2) a Discord webhook (`discord.com`) for the clipping engine's social/Discord toggle only, outbound posts only, with the webhook URL treated as a secret and masked in logs. Both are carried over from the V5.1 build's owner decisions (`docs/decisions/0003`, `0004`). Both live behind the one declared package `sul-net`; the `ac-regression` network guard lists them under that package.
- **Rationale**: The `ac-regression` network guard fails on any surface that is not declared. Declaring these two first means the old Steam and Discord work can be merged into the new design without an undeclared-surface failure, and keeps the rule "declare before you call" intact. Nothing calls either surface yet, and `sul-net` stays empty.

### D-150 — Remove the duplicate copy of the brief from SUL-LEDGER/releases.md (2026-11-22)
- **Status**: decided
- **Scope**: `SUL-LEDGER/releases.md`, `xtask/src/release.rs` (`cargo xtask release-check`), `crates/sul-ipc-types` (comments)
- **Decision**: The owner chose to remove the duplicate. A line-by-line comparison (Markdown escapes, tabs and case normalised) found every one of the 337 non-blank lines of `SUL-LEDGER/releases.md` in `SUL-BRIEF.md`: 333 word for word; 3 where the brief is the newer text (its R1 and R2 ladder rows move A.63-Thinking, A.63-Router and A.63-Conversation out of the prerequisite gates into a note, and one line differs by a space); and 1 line, the Tauri stack sentence, that the brief carries with more text after it. The "own text" at the end of the file (the reading guide and the "What Changed in This Rewrite" table) is the brief's sections 9 and 10. Nothing unique was lost. The file is now a short placeholder for the release plan the spec describes and nobody has written (the `[release.R0..R4]` and `[reference_config.R0..R3]` blocks). Until that plan exists, `cargo xtask release-check` reads the release ladder from `SUL-BRIEF.md` section 3 and the 21 Tier 1-core event ids from section 6.8. It ignores gate ids written inside parentheses in a ladder row, which are notes and not prerequisites: reading "A.2 gated shipping components" as the gate A.2 would pass by accident, because `gates.md` has a gate with that id. The number of prerequisite gates it finds is therefore 47, not 50.
- **Rationale**: Two copies of one text drift apart, and this copy was already behind the brief. The spec's prose that names `releases.md` as the single source of truth for what ships in which release describes the intended design and is left as written. When the real plan arrives it replaces the placeholder, the gate is pointed at it, and a new entry supersedes this one.

### D-151 — The ring slot header is 64 bytes; the Ch. 7 §2.3 payload formula is corrected (2026-11-22)
- **Status**: decided
- **Scope**: `Ch. 7` §2.3 (slot layout), `crates/sul-ipc-types` (`SLOT_HEADER_BYTES`), `crates/sul-ring`
- **Decision**: The slot header is 64 bytes, one cache line, and the payload is `SLOT_SIZE - 64`. The fields of the §2.3 `Slot` struct add up to exactly 64 bytes (`state` 1 plus 7 padding, `seq` 8, `timestamp` 8, `len` 4, `_pad: [u8; 36]`), so the formula `SLOT_SIZE - 128` was the error: it left the struct 64 bytes shorter than the slot. The owner left the choice to Claude with the criterion "the most performant and optimal choice that allows full functionality". The code already followed the field list; the formula in §2.3 and the code comments changed. The wire format and the golden bytes did not.
- **Rationale**: A 128-byte header spends a second cache line per slot on padding: a 256-byte slot would carry 128 payload bytes instead of 192, and the small Telemetry and Token messages that dominate the rings would pay for it in memory traffic and cache footprint. The four header fields use 28 bytes, leaving 36 spare for later fields, so nothing the spec asks for is lost.
- **Not decided**: whether slot sizes should be multiples of 128, so that a slot header never shares a 128-byte pair of cache lines with the previous slot's last payload line (crossbeam pads to 128 on x86-64 for the same reason). Unmeasured; settle it with a benchmark when `sul-ipc` picks its slot sizes (R0 layer 3).

### D-152 — The FailureMode letters follow SUL-LEDGER/failures.md (2026-11-22)
- **Status**: decided
- **Scope**: `crates/sul-ipc-types` (`FailureMode` documentation), `SUL-LEDGER/failures.md` ("Mode definitions"), `Ch. 1` §3.4
- **Decision**: The owner decided the ledger wins. A = LLM inference timeout, B = invalid structured output, C = IPC channel death, D = model-file corruption, E = Rust panic in a supervised subsystem thread, F = cascading persistent failure, each with the recovery procedure `failures.md` gives. The one-line meanings the scaffold carried ("retry with backoff", "degrade", "hard refusal"...) are retired: a recovery step belongs to a Mode's procedure, not to the meaning of its letter. The enum keeps its variants and discriminants (`ModeA` to `ModeF`, 0 to 5), so the wire format and the golden bytes do not change; only the documentation does. A startup abort, a build failure and an "informational" entry are not Modes and have no variant.
- **Noted, not decided**: the ledger disagrees with itself about E. Its definition is a Rust panic, but 24 of its 203 error-code rows are Mode E and none has a panic as its trigger (they are refusals and session-long degradations, such as `E_UNKNOWN_ARCH` and `E_VLM_BACKEND`), and `Ch. 1` §3.4 calls E "persistent". C is likewise used for any recoverable component failure (81 rows), not only IPC channel death. So code reports the letter in the row of the error code, never one inferred from a definition, and the owner is asked which text to amend.
- **Rationale**: The ledger is the stated source of truth for the failure taxonomy (`README.md`, source-of-truth rule), and the scaffold's comments had no source. Keeping the discriminants means no migration.

### D-153 — Guard that the substrate crates keep #![forbid(unsafe_code)] (2026-11-22)
- **Status**: decided
- **Scope**: `xtask` (`cargo xtask ac-regression` and `pipeline`), `crates/sul-ipc-types`, `crates/sul-ring`, `crates/sul-wire`, `crates/sul-trust`, `crates/sul-metrics`
- **Decision**: The owner asked for a guard in `cargo xtask ac-regression` that fails when a crate on its list no longer has `#![forbid(unsafe_code)]` at the top of its `lib.rs`. The list holds the five substrate crates that have it today. It is an addition: `guards.md` has no such guard. A new crate joins the list in the commit that creates it, unless it must contain `unsafe` code; leaving the list needs a recorded decision.
- **Rationale**: Alignment and the absence of undefined behaviour in `sul-ring` and `sul-wire` rest on the tests, on Miri (not yet run) and on this attribute (D-141, D-147). Nothing noticed if the attribute was deleted.

### D-154 — `manifest` and `trust/*` leave `sul-core`: the manifest is in `sul-trust`, the quarantine in `sul-quarantine`, the package scan in `sul-mod` (2026-11-23)
- **Status**: decided
- **Scope**: `crates/sul-core`, `crates/sul-trust`, `crates/sul-quarantine` (new), `crates/sul-mod`, `crates/sul-builds`, `crates/sul-app`, `src-tauri`, `xtask` (`forbid(unsafe_code)` list), `.github/workflows/ci.yml`, `docs/decisions/0011-trust-split.md`
- **Decision**: The SHA-256 build manifest, the findings every check reports, the integrity checks, the act-without-asking rule (`policy`) and webhook-address checking live in `sul-trust`, directly above `sul-core`. The quarantine lives in a new crate, `sul-quarantine`, which depends on `sul-core` and `sul-trust`. The mod package scan lives in `sul-mod` and reports in `sul-trust`'s `Finding` type. The owner approved the split on 2026-11-21 and, after the cycle in the first placement (the manifest in `sul-builds` would have sat above the integrity and quarantine code that uses it) was put to them, chose on 2026-11-22 to put the manifest in `sul-trust` with the integrity code. Code and tests moved with `git mv`; only imports, three `Finding` visibilities (`confirmed`, `suspicious`, `extension`) and the tests that needed two crates changed (the policy-and-quarantine test to `sul-quarantine/tests`, the manifest and `hash_file` tests to `sul-trust/tests`). `sul-quarantine` joins the `forbid(unsafe_code)` list (D-153) in the commit that creates it. The quarantine is **not** encrypted in this entry: Ch. 14 §14.13.4 asks for an encrypted quarantine, and the owner's design (a fresh random key per quarantined file, stored beside its record, behind a key-source trait that the vault supplies at R0 layer 4) is the next cut.
- **Rationale**: `sul-core` is the substrate every crate stands on and should hold nothing that knows about builds, mods or trust. The spec treats SHA-256 baseline manifests as the Tier 2 check (Ch. 14 §14.13.1) and the quarantine as its own crate (`sul-quarantine`, Ch. 14 §14.13.4, CLAUDE.md R0 build order), and `sul-trust` already existed as the scaffold's home for manifest verification. One-way dependencies (`sul-core` <- `sul-trust` <- `sul-quarantine`) keep the layers acyclic.

### D-155 — The quarantine seals each copy: XChaCha20-Poly1305 (STREAM), a fresh random key per file, the keys behind a key-source trait (2026-11-24)
- **Status**: decided
- **Scope**: `crates/sul-quarantine` (`seal`, `keys`, `quarantine`), `Cargo.toml` and `Cargo.lock` (`zeroize` added; `chacha20poly1305` and `sha2` were already workspace dependencies), `crates/sul-core/tests/write_sites.rs`, `docs/decisions/0012-quarantine-sealing.md`
- **Decision**: Ch. 14 §14.13.4 asks for "copy → verify → record → verify → remove into encrypted quarantine". A quarantined copy is now sealed. The design is the owner's answer of 2026-11-22: a fresh random key for every quarantined file, kept beside its record, behind a key-source trait (`KeySource`) that the vault supplies at R0 layer 4. The choices the owner left to Claude (standing direction of 2026-11-22: performance and optimization, consistent with full functionality) are: XChaCha20-Poly1305, the vault's cipher (Ch. 2 §2.3), in the `aead::stream` STREAM-BE32 construction, in 64 KiB chunks, with a 32-byte header authenticated as associated data of every chunk, so that a changed byte, a changed header, a reordered, repeated, removed or appended chunk and a stream cut short at a chunk boundary all fail to open; "verify" is opening the sealed copy and hashing the plaintext as it comes out, in one pass and with no second copy of the file on disk, so a copy that does not decrypt to exactly the original is never trusted; record version 2 names the cipher, and version 1 records (copies made before this entry, stored as they were) are still listed and restored as they were; a key that cannot be had is reported as missing, damaged or locked and is never mistaken for a copy that does not match, and recovery does not decide a case while the key is unavailable; key material is wiped when dropped; a key file is created with create-new and removed only with its item. The format is this crate's own and is documented in `seal.rs`; it is **not** the vault's file layout, which Ch. 2 §2.3 describes differently (nonces from a base nonce XORed with the chunk index, no chunk size). Nothing depends on the two matching, because the vault takes custody of the keys through `KeySource`, not through the format. The default key source keeps the key in `key.bin` in the item's own folder, so the sealed copy is neutralised (it cannot run, cannot be scanned as the program it was, and comes back byte for byte) but is **not** kept secret from anyone who can read the quarantine folder; that protection arrives with the vault, and nothing in the code or the docs claims it before then.
- **Rationale**: The spec asks for an encrypted quarantine and the owner chose the key design; an authenticated construction was needed so that tampering with a stored copy is noticed and not merely hashed after the fact, and STREAM's final-chunk flag is the truncation defence Ch. 2 §2.3 already asks of the vault. A fresh key per file keeps one leaked key from opening any other copy. The pinned format values were checked against an independent implementation (`crates/sul-quarantine/tests/golden/seal_reference.py`: HChaCha20 written from the XChaCha20 draft and checked against the draft's own test vector, the `cryptography` package's ChaCha20-Poly1305, and the chunking written from the format description), so the pinned bytes are the documented format and not just what the code produced. Open, not decided here: Ch. 2 §2.3 pairs XChaCha20-Poly1305 with a 12-byte base nonce XORed with a chunk index, but XChaCha20 takes a 24-byte nonce, so that text cannot be implemented as written; it is raised for R0 layer 4, when the vault is built.

### D-156 — The owner's answers on decision 0013: plan the scoped daily digest, an own fetch client in `sul-net`, a measured-only notation; continue the R0 order (2026-11-25)
- **Status**: decided
- **Scope**: `docs/decisions/0013-nosignups-web-lookup-digest-archive-notation.md` (section 9), `docs/SESSION_STATE.md`, `CLAUDE.md` (the cited range). No spec chapter, crate or config is changed by this entry.
- **Decision**: Four answers to decision 0013, section 8. (Q1) The daily digest (P5) is planned as three layers: a watch list of the sources Jarvis already ingests (Ch. 14 §2.6) plus the owner's additions and the patch notes of the owner's games; owner-chosen openly licensed topics; and the pages the AI itself looked up. It is built in CI and delivered through the existing hive fetch. "The whole internet every day" is not planned (scale and licences, 0013 §5.1 and §5.2). (Q2) The fetch-side token savers of P3 (asking sites for Markdown first, conditional requests) live in a small from-scratch Rust fetch client in `sul-net`, declared as a new network surface. The declaration is written, as its own decision and an amendment of Ch. 16 §16.15.14, when that client is built; it must at least require https only, refuse private, loopback and link-local addresses, limit redirects, cap the size of a response, allow only listed content types, set timeouts, honour `robots.txt` and per-host rate limits, and send no cookies or credentials. Until then nothing calls it, and the spec's rule that only RustXNG makes HTTP requests for search stands. (Q3) The notation (P2, SDN) is measured-only: it is switched on per model profile only if SUL's own eval shows accuracy at least equal to JSON with fewer tokens; stored packs stay JSON. (Q4) The R0 build order continues with `sul-watchdog`, then `sul-crash`, where the share-safe diagnostic bundles (P6) are built; stop and report after each.
- **Not decided by this entry**: P1, P3 (a), (d) and (e), and P4 stay proposals (they were not asked). The archive container, the daily size budget and the release placement of P3 to P5 are asked before any R2 work starts. The Wikipedia text licence is unconfirmed.
- **Rationale**: The owner chose the recommended option on all four questions. The digest is bounded by what bandwidth, storage and licences allow. An own client keeps fetching in the one crate where network code may live and avoids the AGPL duties of a RustXNG fork. Measured-only protects small models from a format they have not seen. The R0 order is the dependency order in `CLAUDE.md`.

### D-157 — `sul-watchdog`: the integrity watchdog's logic, a signed baseline, an event-driven file monitor, a log that shows tampering; `sul_trust::verify_ed25519` (2026-11-26)
- **Status**: decided (the order is the owner's, D-156 question 4; the technical choices were left to Claude under the standing direction of 2026-11-22: performance and optimization, consistent with full functionality). Two points are put to the owner in decision 0014, section 6; neither blocks.
- **Scope**: `crates/sul-watchdog` (new), `crates/sul-trust/src/lib.rs` (`verify_ed25519`), `Cargo.toml` and `Cargo.lock` (`sul-watchdog` joins the workspace; `notify` 9 with default features off is the one new third-party dependency), `xtask/src/unsafe_code.rs` and `xtask/src/main.rs` (`sul-watchdog` joins the `forbid(unsafe_code)` list; the summary text no longer hard-codes a crate count), `.github/workflows/ci.yml` (clippy and tests of `r0-substrate`), `docs/decisions/0014-sul-watchdog.md`
- **Decision**: Ch. 12 §2.8 and Ch. 14 §14.13.2 to §14.13.5 are built as logic with no live Windows process behind it. A baseline lists every guarded file with its digest and is signed with Ed25519 and checked, strictly, before any of it is read; a baseline found on disk is never trusted unsigned. A guard compares disk to baseline in one shot and never follows a link, for the file or any folder above it. A monitor does the same driven by file-change events, with a debounce, "check everything" on an overflow or a watcher error, and a periodic backstop (default 10 minutes) that skips model files; model files are verified at every session start (Ch. 14 §14.14.1). A scan analyses a snapshot of SUL's own process (RWX memory SUL did not allocate, hooked function starts, foreign threads, replaced images) against a user-visible allowlist. A detection becomes a record that has no field for a path, an address or a sentence, in a log in which each line quotes the SHA-256 of the line before and whose sink refuses to append after a break. The watchdog **reports and never acts**: quarantining a file in use or turning a feature off needs knowledge only the caller has. `sul-trust` gains `verify_ed25519` (RFC 8032, `verify_strict`: refuses weak keys and small-order signatures that the plain check accepts); the Minisign file format, the binary watermarks and key custody are not built. The crate keeps `#![forbid(unsafe_code)]`; the live-process probe and the `BELOW_NORMAL` thread need Win32 calls and will go in a separate crate behind `ProcessProbe` (working name `sul-watchdog-win`) under its own decision, the same split `sul-wire` uses for its shared-memory adapter.
- **Not decided here**: whether to add `E_WATCHDOG_FOREIGN_THREAD` and `E_WATCHDOG_IMAGE_HOLLOWING` to `SUL-LEDGER/failures.md` (two of the four detections have no failure code; this is a ledger amendment and so the owner's call); what the installed product guards and who signs the baseline (they wait for the installer and the update channel); the value of `HOLLOW_PERCENT` (25, a named starting value, to be measured).
- **Rationale**: Event-driven monitoring costs nothing while idle, so the watchdog can run at `BELOW_NORMAL` without touching a game's frame time (HANDOFF section 3). `notify` with its default features off, on the Windows target, brings `windows-sys`, `walkdir` and small helpers and no `mio` or socket crate, so the network guard passes it (`sul-watchdog: 64 crates`) and hand-written `ReadDirectoryChangesW` (which would need `unsafe`) is avoided. A baseline that is trusted unsigned would let the thing it guards rewrite it. The tests prove the logic for the inputs given, not that SUL is protected: nothing here has read a real process, a real junction or a real file lock, and the watcher ran on Linux's `inotify`, not Windows' `ReadDirectoryChangesW` (the Windows CI result is the first check of that). Deliberate-bug checks: first pass 42 of 52 caught, after closing the gaps 51 of 52 caught plus one equivalent mutant (decision 0014, section 5).

### D-158 — Watchdog failure codes: the two missing process detections get codes, and the Windows probe lives in its own crate (2026-11-27)
- **Status**: decided. The owner's answer of 2026-11-27 to decision 0014 section 6 was "do whatever is best for security, performance and speed"; the choice is Claude's under that answer and under the standing direction of 2026-11-22.
- **Scope**: `SUL-LEDGER/failures.md` (two rows), `SUL-ARCHITECTURE.md` (Ch. 12 §2.8 taxonomy, §14.13.5 code list), `crates/sul-watchdog/src/code.rs`, `docs/decisions/0015-live-code-attestation.md`.
- **Decision**: Ch. 12 §2.8 and Ch. 14 §14.13.3 list four process detections (RWX regions, API hooks, foreign threads, DLL hollowing) and the ledger named three codes. `E_WATCHDOG_FOREIGN_THREAD` and `E_WATCHDOG_IMAGE_HOLLOWING` are added to the ledger and to the Ch. 12 §2.8 table, with no recovery-mode letter (the ledger column reads "—", as for the other detections: nothing is retried or restarted). `WatchdogCode` has a code for every detection, and a test reads the ledger in both directions (a code in the ledger the enum lacks fails, a row that disagrees on letter or trigger fails). The live-process probe, the digester and the `BELOW_NORMAL` thread are confirmed as a separate crate, `sul-watchdog-win`, behind `ProcessProbe` and `RegionDigester`; it is built under its own decision and never joins the `forbid(unsafe_code)` list (D-153), and it is where the `process-memory access` boundary rule of `cargo xtask ac-regression` gets armed.
- **Not decided here**: the recovery text for each code beyond "log, notify" (the caller's), and `HOLLOW_PERCENT` (25, to be measured).
- **Rationale**: A detection with no stable code cannot be searched for in the hive-mind log or shown the same way twice, and a code costs nothing at run time. Keeping the Win32 calls out of `sul-watchdog` keeps `unsafe` small and separately reviewed and lets Miri and Kani keep verifying the analysis.

### D-159 — Live-code attestation: new code is signed as live code, so the watchdog verifies it instead of flagging or skipping it (2026-11-27)
- **Status**: decided in principle by the owner ("maybe we could signature new live code as new live code so the watchdog still verifies it", 2026-11-27); the design is Claude's under the standing direction of 2026-11-22. Open points for the owner are in decision 0015, section 6; none blocks.
- **Scope**: `crates/sul-watchdog` (`live_code` new; `scan`, `monitor`, `incident`, `threat_log`, `code`, `lib` changed), `SUL-LEDGER/failures.md` (two rows), `SUL-ARCHITECTURE.md` (Ch. 12 §2.8, §14.13.3, §14.13.5), `crates/sul-watchdog/Cargo.toml` and `Cargo.lock` (`blake3` as a dev-dependency only), `docs/decisions/0015-live-code-attestation.md`.
- **Decision**: Code that exists after the install-time baseline was signed (a hot update, something this machine built, generated code, a hook trampoline) is signed as a live-code record: canonical JSON, Ed25519, checked strictly before anything is read from it. Four purposes and three keys, and a key never signs outside its own purposes: `HotUpdate` (release key; any file), `LocalBuild` (device key; a **new** file only, never a binary, manifest or trust-primitive role, never a file the baseline or a hot update owns), `JitCode` and `HookTrampoline` (session key; one memory region). No key may hold two roles. Files go in a persistent ledger rooted in the SHA-256 of the signed baseline's bytes, with strictly increasing serials (a rollback is refused); memory goes in a session ledger rooted in a random nonce, with unique serials in any order. The producer must be code the **signed baseline** lists, so trust cannot be pulled up by a chain of attestations. For memory the watchdog **reads the bytes itself** (BLAKE3-256, hashed in place by the Windows adapter) and refuses a record whose digest is not what is in memory; every scan then verifies each registered region against the signed digest, and only a verified region is accounted for. Files are merged into an effective baseline that the monitor checks; an update is announced first (`Monitor::rebase`) so the write is no false alarm, with anything other than the awaited state reported at once and a wait that ends after 10 minutes. Expiry is a re-sign deadline: an expired record stops vouching and the file is judged by the signed baseline. Two failure codes: `E_WATCHDOG_LIVE_CODE_MODIFIED` and `E_WATCHDOG_LIVE_CODE_REFUSED`. The signer is a trait the vault implements in another process at layer 4; this crate holds no private key.
- **Not decided here**: where the device and session public keys are anchored (recommended in decision 0015: the vault hands the session key and nonce over its own channel at start-up; the device key is recorded at first run; a layer-4 decision); whether to build a strict mode that reports private executable memory that is not RWX and not attested (proposed, not built); the values of `ANNOUNCE_TTL` (10 minutes) and `CLOCK_SKEW_SECS` (120 s), to be measured; rate-limiting refusals; where the caller stores signed records for replay after a restart (the installer and updater).
- **Rationale**: Before this the caller could only say "that RWX region is mine", and the scan then stopped looking at it: whoever overwrote a declared trampoline was invisible. Signing makes the watchdog keep looking, against a digest it did not choose and a key that is not the one that could be stolen with it. Separate keys per purpose keep a stolen session key from forging an update and a stolen device key from rewriting SUL's binaries. Reading the bytes itself means a well-signed record about memory that is not what was signed is still refused. BLAKE3 for memory because a region is re-read every scan; SHA-256 for files because the baseline and the manifests use it. The tests prove the logic for the inputs given: nothing reads a live process, nothing signs outside tests, and a change made and undone between two scans is not seen. Deliberate-bug checks: 23 of 23 on the security rules caught, after two gaps found by the first pass were closed (decision 0015, section 5).
