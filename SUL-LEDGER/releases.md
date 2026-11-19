SUL-BRIEF.md

SUL (Soup's Unified Launcher) — one product, one installer, one binary, one config schema, one update channel, one brand, one compatibility matrix. Releases are capability milestones on that product. Nothing is cut; features are sequenced by the gate that unlocks them.

This document is the front-door. It states identity, scope, non-goals, invariants, hardware envelope, threat model summary, and reader's guide. Every mechanism, every number, every gate, every failure code, every AST guard, every config key lives in the other three artifacts — this brief cites them, does not restate them.

1\. Identity

Name: SUL — Soup's Unified Launcher.

Platform: Windows 10 1909+ / Windows 11. Reference rig: RTX 3060 Ti 8 GB, Ryzen 5 5600X, 64 GB DDR4, Samsung 970 EVO Plus 500 GB NVMe, Seagate 2 TB HDD for the secondary library. All code paths are driven by the runtime hardware profile — the reference rig is the tuning target, not a hard requirement.

What SUL is: A local-first desktop platform that unifies, in one installer and one binary:

Game launcher — library, launch profiles, per-game anti-cheat posture, save shadowing, modding and VFS (§2 of Architecture).

AI assistant (Jarvis) — one Combat Full checkpoint with four inference profiles (Instruct 2K / Thinking 4K / Conversation 16K / Coding 8K, research-gated), one Deep Coder checkpoint (K2-Horizon-7B), a deterministic profile router, OODA-Tool scaffold, TSCG schema compilation, ReTrace visual-token compression, STAR-KV KV compression, distilled ToolPRM cross-encoder, and a hierarchy of fallbacks (§6 of Architecture).

Emulation — libretro frontend (sul-emu-libretro), native adapters (PCSX2, Dolphin, Ryujinx forks, DuckStation, melonDS, RPCS3, Cemu), a from-scratch Game Boy / GBC core (sul-gb), a Rust-native save-state ring (sul-savestate), run-ahead, and a Vulkan implicit-layer frame generation (sul-fg).

Music player — Musicolet feature parity with bit-perfect WASAPI exclusive output, gapless, ReplayGain, EQ, tag editor, synchronized lyrics, sleep timers, A-B repeat, audio cutter, taskbar controls (§4 of Architecture).

Audio pipeline and voice stack — WASAPI backend, dual-track ring, sul-audio-governor, sul-dsp, sul-voice (cIRM isolation), sul-voice-fx (Voicemod parity), sul-stt (whisper.cpp streaming), sul-tts (Piper/Kokoro/Higgs Audio), prosody + intent gate, audio routing matrix (§3 of Architecture).

Broadcast engine — the OBS/Voicemeeter/Voicemod replacement. Encoder admission controller, per-destination audio mixes, ISO recording on one clock, USB/IP virtual audio devices (no kernel driver), WASAPI process loopback, per-application capture, VBAN, and an agent-native method table (§5 of Architecture).

Overlay — external DComp by default (works on every game including kernel-AC titles), opt-in flip-model per-game on kernel_level: false only. sul-wire binary IPC framing, direct ring view for tokens (§7 of Architecture).

Vault — XChaCha20-Poly1305 AEAD, DPAPI + TPM wrapping, decoy vaults, rate limiting, anti-debug, Ed25519 signatures, AND-logic layered auth (§2 of Architecture).

App-level auth — native Win32 modal before WebView hydration; AND-logic across factors; --dev-auth-bypass refused in release builds.

Vision, search, RAG — sul-ocr (PP-OCRv5 INT8), sul-vlm (MiniCPM-V / LFM2.5-VL / jina-vlm / Qwen2.5-VL), RustXNG search pipeline, verification packet builder, GitHub search, FAISS RAG (§8 of Architecture).

Frame generation — Vulkan implicit layer with three backends (lsfg_native, lsfg_layer, win_fg); structural anti-cheat refusal.

Data pack strategy — .sulpack format; persona / user_profile / coding_context / knowledge_pack / translation_pack / game_db / task_context; Ed25519 Minisign; sectioned on-demand loading; session working memory with Entity Ledger, address pointers, thread-based revisable memory (§11 of Architecture).

Telemetry, diagnostics, crash — sul-metrics ring, SQLite rollup, sul-crash minidump handlers, save shadowing, watchdog (§12 of Architecture).

Hardware integration — CIG GPU scheduling, HAGS measurement, large-page allocation, one-click OS tuning, DirectStorage pre-warm, PGO + BOLT (§13 of Architecture).

Autonomous hive-mind + safety protocol — Jarvis-Coder hierarchy, Dual-Gate safety, SOTA self-improvement pipeline, self-healing watchdog Modes A–F, threat quarantine, hive threat logging (§14 of Architecture).

Sensing / security / handoff (R3, gated on A.152) — CSI sensing, BCI, God's Eye View, rPPG, browser automation, security toolkit (Shannon, hashcat, nuclei, mitmproxy, Ghidra MCP), VoiceStudio, uncensored models (heretic + local), 400B distillation (research), global sensor fusion, predictive analytics, health dashboard, binary audit, auto-remediation (§15 of Architecture).

What SUL is not: No cloud dependency for core functionality. No streaming-service integration (local-first; the music player is local-first, and self-hosted libraries like Jellyfin and Nextcloud are accepted as local-first additions). No ad-supported or freemium tiers. No kernel-mode code of any kind. No Electron. No anti-cheat bypass. No voice cloning of a specific real person. See §5 (Non-Goals) for the full list.

Stack: Tauri 2 (desktop framework) with a Rust core and a React/TypeScript UI. WebView2 runtime with Isolation Pattern + Strict ESM + runtime floor. Target <50 MB idle RAM excluding WebView2, egui, and model weights.

2\. The One-Product Rule

SUL is one product: one installer, one config schema (forward-compatible migrations only), one update channel, one brand, one compatibility matrix, one binary that contains every component.

Releases (R0–R4, §3) are capability milestones on that product — not fragmentation. Every release is a strict superset of the previous one:

Same binary (feature-gated, not split binaries).

Same config schema.

Same installer.

Same update channel.

Same brand.

Same compatibility matrix.

A user on any release is on the same product. Upgrading is a config-preserving update.

This is the model Blender, Steam, VS Code, OBS, DaVinci Resolve, Krita, and every JetBrains IDE use. The user never perceives fragmentation because the identity, installer, config, and update channel never change.

Release-ladder enforcement on update (releases.md, Part 2 of this rewrite): every update package declares the release class of the version it installs (v0.1 = R0, v0.2 = R1, v0.4 = R2, v0.6 = R3). The updater refuses to install a package whose declared release class is not the immediate successor or a later successor of the currently-installed release class — no skipping from R0 to R2, because R1's enabled feature set was not validated on R0's installed config. A mismatch is E_RELEASE_UPDATE_SKIP and the update is refused. This is a defense against a corrupted or tampered update manifest, not the normal path — the normal path serves the latest minor within the current major.

3\. The Release Ladder

The release ladder is the only scheduling mechanism in SUL. No dates. No owners. No promises. A feature ships when its gate passes; the release class is a classification of which gate outcome enables it, not a delivery promise.

Release	Version	Name	Purpose	Prerequisite gates

R0	v0.1	"the launcher"	Substrate + launcher + vault + one AI tier (Combat Full Instruct) + overlay (external DComp) + OCR (PP-OCRv5 INT8) + app auth	A.26, A.63-Instruct, A.63-Common, A.92, A.114, A.117 (all release-blocking for R0)

R1	v0.2–0.3	"everything mainstream"	Music player, broadcast engine core, emulation (libretro + sul-gb + run-ahead), Combat Full Thinking profile, search pipeline, sul-agent, prosody, audio governor, DSP, voice FX, STT, modding suite, VFS, sandbox, Hive fetch	A.63-Thinking, A.63-Router, A.90, A.91, A.119–A.124, A.126–A.128, A.130, A.133, A.134, A.83, plus per-emulator A.74

R2	v0.4–0.5	"the deep technical release"	AI deep stack (Combat Full Conversation, Deep, ReTrace, Medusa, phrase head, ChainFlow, LiLiCorr, SPEED-Q, APEX, KV backends, ToolPRM), frame generation, broadcast full (ISO + USB/IP + agent API), optimize + zstd_gpu, Coca-Cola, verification caching, HRTF, Deep 8K	A.18, A.54, A.63-Conversation, A.96, A.97, A.99, A.100, A.72, A.73, A.74, A.76, A.129, A.132

R3	v0.6+	"sensing and security"	CSI, BCI, GEV, rPPG, browser automation, security toolkit, VoiceStudio, uncensored models, health dashboard, binary audit, auto-remediation, global sensor fusion. Prerequisite: A.152 (legal/ethical framework) complete and reviewed before any R3 feature ships.	A.152 (prerequisite), A.136–A.145, A.149–A.151

R4	n/a	"research only"	400B distillation, FRISM, MapCoder-Lite, QuietPrune/StepPrune, fused STAR-KV kernel, and every A.4 item. Graduates into a future R-release only if its measurement gate passes.	None; no shipping obligation

Every R3 feature is a strict superset of R2. Every release is a strict superset of the previous. This is enforced by E_RELEASE_FEATURE_MISMATCH (feature toggle enabled from a later release class than the installed binary) and E_RELEASE_UPDATE_SKIP (update manifest jumps the release ladder).

The releases.md file (canonical location: SUL-LEDGER/releases.md) is the single source of truth for "what ships in which release." The build system, the installer, the config validator, and the compatibility matrix all read it. It contains:

\[product\] — the one-installer / one-config / one-update-channel / one-compat-matrix invariants.

\[release.R0..R4\] — version, name, reference_hardware, release_blocking_gates, components / components_added, features_off, tier1_events / tier1_events_added.

\[reference_config.R0..R3\] — the tested reference configuration for each release. The compatibility validator guarantees these combinations work. Anything outside a reference config is warned about (or refused if over budget).

Release-plan validation rules (cross-cutting rules #29–#34; now SUL-LEDGER/releases.md validation and cargo xtask config-check):

Every \[features.\*\] toggle is assigned exactly one release class in releases.md.

Every A.2 gate referenced by a components list exists in gates.md.

Every release class has a matching \[reference_config.R<n>\] block.

The installed %APPDATA%/SUL/release.toml matches the binary's embedded release class.

Every Tier 1 event is classified as Tier 1-core (R0) or assigned to a specific release class.

cargo xtask config-check emits a coverage metric — the fraction of the toggle space actually tested by the release's reference configuration. Coverage below the declared minimum is a warning, not an error.

4\. Modular Feature Model

Every feature is classified as exactly one of four classes:

Class	Togglable?	Default	Example

Core	Never — code change, not config change	Always on	Config validator, IPC ring, Ed25519 verification, trust primitives, crash handlers, process watchdog, safety invariant (destructive action → Thinking), persona constraints, anti-cheat structural refusals, destructive-action detection, Entity Ledger, OODA-Tool in the safety path, TSCG in the destructive-action judgment path

Shipped	Yes; on by default	On	Every \[features.\*\] entry whose source is true in its module chapter

Gated	Yes; off by default until A.x passes	Off	Every entry with a gate reference. The gate's measurement criterion (in gates.md) determines when the feature flips to true.

Opt-in	Yes; off by default; safety-critical	Off	Vault (\[vault\] enabled = false), uncensored models (\[features.ai.uncensored\], \[features.ai.local_uncensored\]), global sensor fusion, every handoff feature (\[features.csi.\*\], etc.)

Every non-core feature is a toggle. Every toggle has a documented performance impact. Every toggle lives in \[features.\*\] and is mirrored in the release-gate view (\[features.deep.\*\] for Deep) and the tuning view (\[performance.\*\]).

Safety-critical features are structurally enforced regardless of toggle state. The safety invariant (destructive action → Thinking) is not togglable. The voice-cloning refusal has no toggle. The anti-cheat structural refusals are code, not config.

Tier 1 events. Every safety-relevant config change requires an OS-level approval modal (native Win32, outside the LLM context, prompt-injection-proof). The modal's record is stored in %APPDATA%/SUL/tier1-approvals.json (Ed25519-signed, DPAPI-wrapped) and referenced by the Coder's Gate A verification and by the auto-updater's Tier 1 checks. Disabling the safety invariant, weakening auth, weakening WebView hardening, or attempting a structural refusal is Tier 1-core (R0). Everything else is Tier 1 by release class.

Compatibility matrix. Every \[features.\*\] toggle participates in the master compatibility matrix (SUL-ARCHITECTURE/Ch. 1, §1.5). cargo xtask config-check runs at every launch before model load. It walks the matrix against the enabled feature set for the installed release class (drawn from releases.md \[release.R<n>\].components), auto-disables resolvable conflicts with a warning dialog, raises E_CFG for unresolvable conflicts, warns for missing dependencies, and warns or refuses for VRAM over-budget against the layered ceiling (Brief §7.5). Auto-resolutions are logged to \[compat\] and compat.log. The coverage metric (rule 6 above) is surfaced in Diagnostics.

5\. Non-Goals

The original spec never had a non-goals section. This one extracts and consolidates every explicit refusal, rejection, deferral, and out-of-scope declaration. The non-goals are as important as the goals — they are what makes the scope survivable.

5.1 Out of scope for the shipping release

Linux, macOS, ARM Windows ports. Reference intent only. The model ladder is Windows-first; re-validating K2-Horizon mainline, OCR/VLM pipelines, and the emulator surface on Linux CUDA is a separate project. (§1.18, Appendix A.65)

Mobile companion (SUL-Mobile). Reference intent only. (§1.18, A.65)

Companion read-only export (\[companion\]). Desktop-initiated export of `health_trends.sulpack` for offline mobile viewing (PocketPal-style GGUF summary) is a gated opt-in export, not a mobile companion product (D-135).

First-party game suite. Parked conditional on a first-party SUL game existing. Does not exist. (§3.21, A.67)

Non-emulation cross-OS remote session streaming. Share-Play is R1 for emulation (A.66); the desktop-overlay streaming variant is deferred indefinitely. (§3.20, §1.18)

Cloud dependency for core functionality. Local-first by design. Sentry upload is the only optional network surface that is off by default.

Commercial streaming service integration (music player). Local library only; self-hosted libraries (Jellyfin, Nextcloud) are accepted as local-first additions. (§5.17)

Online lyrics fetching. Embedded ID3/FLAC/MP4 lyrics or .lrc sidecar only. (§5.17)

Cloud library sync (commercial cloud). Nextcloud self-hosted sync is accepted as a local-first addition (§5.17).

Android Auto / Android widgets. Replaced by SMTC + taskbar thumbnail toolbar + overlay mini-player. (§5.17)

Ad-supported or freemium tiers. (§5.17)

DSD native passthrough as a shipping feature. Research only (A.125). (§5.17)

OBS as a library (libobs-rs). GPL-3.0, incompatible with SUL's shipping licence; no macOS. (§6.17)

Smelter as a bundled dependency. Proprietary licence, \$1,000/month floor; reference for scene-graph API shape only, clean-room implementation. (§6.17)

Isocast as a dependency. MIT/Apache, macOS/Linux only, no Windows lane; reference for performance analysis and ISO-on-one-clock model, clean-room implementation. (§6.17)

A custom kernel-mode virtual audio driver. Violates §16.5; USB/IP userspace is the shipping path. (§6.17)

SynchronousAudioRouter as the shipping virtual device. Kernel driver; fallback only. (§6.17)

Streaming from OBS's multi-track model. OBS streams one track; only Twitch supports a VOD track; per-destination mixes must be computed in SUL's engine. (§6.17)

Game-hook capture as the default. WGC is the default because it is external to the game process and does not trip anti-cheat. (§6.17)

A virtual camera driver. Uses the same USB/IP mechanism as the virtual audio devices; no kernel driver. (§6.17)

Encoder presets without admission control. The admission controller is not optional; disabling it is a Tier 1 event with a warning. (§6.17)

Voice cloning of a specific real person. Structural refusal (see §5.2).

5.2 Structurally refused (code, not config)

These refusals return E_REFUSAL_POLICY. They are not togglable. The AST guard verifies each refusal at build time.

Voice cloning of a specific real person. The API surface has no entry point; the config has no toggle; the auto-updater has no path to introduce one. Applies to sul-voice-auth (authentication is allowed — the user authenticates with their own voice) and sul-voicestudio (cloning is refused — clone_voice only accepts character voice preset IDs trained on licensed data). \[audio.voice_fx\] voice_clone is permanently "refused"; any attempt to change it is a Tier 1 event followed by E_CFG, then forced back to "refused".

Frame generation on kernel-level anti-cheat titles. The layer is not loaded, the env var is not set, any attempt to force-enable returns E_REFUSAL_POLICY.

Mod loading on mods_allowed_online: false games. The VFS is not mounted.

Per-process ETW on kernel_level: true titles. The opt-in is refused.

Kernel-level interaction of any kind. SUL never ships, signs, or loads kernel-mode code. It never installs a driver. It never uses a kernel-mode EDR-like primitive. (It may depend on a user-installed third-party driver — see Ch. 16 §16.5.1.)

Bypassing anti-cheat hypervisor detection. sul-sandbox runs Windows Sandbox; it does not hide the VM from hypervisor-detection logic. Games that refuse to run in a VM are refused by SUL.

Broadcast game-capture hook on kernel-AC titles. Same discipline as the overlay flip-model path.

Security toolkit runs without a signed consent file. Any tool invocation without a matching consent file returns E_REFUSAL_POLICY. The security toolkit is opt-in per session and never runs automatically.

CSI sensing, BCI, GEV are entirely local. They cannot surveil another person without the user's explicit configuration.

rPPG runs only on the user's own camera feed. Never on a remote user's video.

Uncensored models bypassing the safety layer. They don't; the safety layer sits below the model (A.154).

5.3 Rejected after evaluation

A.7 MoE / FreeToken offload. MoE-specific technique; doesn't fit Combat Full's 3 GB general combat ceiling (Phase 2) at any useful quality. Not inherent inferiority of MoE — tier-budget mismatch.

A.17 Symmetric q4_0 KV with Hadamard rotation. Target hardware mismatch (12 GB+ cards); STAR-KV already covers the benefit.

A.68 Multi-model ensembling for combat. Violates the 3 GB general combat ceiling. The multi-profile design is one model with four inference profiles selected by a router — routing, not ensembling.

A.69 Qwen3-0.6B as Combat-Exec. Action score 0.700 is too low for execution.

A.70 64K context for Combat Full. Cannot fit on 8 GB — \~6–7 GiB KV cache in FP16 alone. The 2K/4K/16K/8K profile windows are deliberate accuracy-equivalent windows.

A.71 Hammer2.1-3B as execution booster. Replaced by Combat Full fine-tuning. Live trigger: if A.63 returns <65 non-live on any profile, this is automatically re-evaluated.

A.77 Frame generation on kernel_level: true titles. Structural violation of the anti-cheat policy. Not a gate — a rejection.

A.19 DFlash on non-Butler tiers. Butler cut. DFlash not in any shipping chain. Retained for historical comparison.

A.51 ZenDNN on the reference rig. Not applicable to Ryzen 5 5600X; ZenDNN's largest gains are on EPYC server CPUs.

5.4 Deferred indefinitely (A.4 research)

Every A.4 item is R4 with no shipping obligation, no date, no owner. Examples: FRISM (A.102), MapCoder-Lite (A.103), QuietPrune/StepPrune (A.104), fused STAR-KV kernel (A.105), 400B distillation (A.146), global sensor fusion (A.147), predictive analytics (A.148), FSDP-style sharding (A.156), batched ToolPRM scoring (A.157), quantized activations (A.158), per-token Medusa branching (A.159), handoff cross-interaction (A.160). See gates.md A.4 for the full list.

5.5 Licensing boundaries that exclude options

GPL branch of lsfg-vk. lsfg_layer ships against the MIT branch only (MIT through v1.0). Vendoring the GPL branch (after v2.0.0-dev) requires the vendored code to remain GPL — incompatible with SUL's MIT/Apache license. §9.11, §16.2.2.

NDI SDK. grafton-ndi is MIT but the NDI runtime is not redistributable without a NewTek license. SUL uses oximedia-ndi (clean-room) as the default.

LGPL paths as default. soxr and mp3lame-encoder are feature-gated, not in the default binary. The default music player path uses rubato_sinc and FLAC/WAV.

jina-vlm and Surya 2.5 weights. CC-BY-NC-4.0 and OpenRAIL-M respectively. Both are gated shipping components, opt-in only, never on the shipping path by default.

5.6 The R3 prerequisite

A.152 (legal and ethical framework) is a hard prerequisite for every R3 feature. The framework defines consent requirements, data retention policies, encryption requirements, user control (delete / export / revoke consent), legal boundaries (GEV ingests only public feeds; security toolkit runs only against authorized targets), and ethical boundaries. Until A.152 is written, reviewed, and the R3 features are audited against it, no R3 feature ships. This keeps every R3 feature in the spec and makes the framework a condition rather than a stub.

6\. Cross-Cutting Invariants

Eight invariants hold across every subsystem, every module, every release. They are the reason the spec is internally consistent — every other rule cites one of these eight.

6.1 Failure taxonomy (Modes A–F)

Every failure maps to exactly one recovery mode. The deterministic recovery matrix is exhausted before escalating to code-generation (Mode F). Full taxonomy in SUL-LEDGER/failures.md.

Mode	Category	Recovery

A	LLM inference timeout	Retry once, halve context, retry, CPU-only retry, mark tier degraded

B	Invalid structured output	Retry 3× stricter schema, fall back to unstructured text

C	IPC channel death	Recreate channel, replay last message, 3× fail → degraded

D	Model file corruption	SHA-256 verify against signed manifest; mismatch → quarantine lifecycle, prompt redownload

E	Rust panic in supervised subsystem thread	Capture minidump; per-process handling (sul-ai / sul-ipc = kill + relaunch clean; supervised thread in sul.exe = validate shared state against last-known-good before respawn); 3× repeat → degraded

F	Cascading persistent failure	Escalates to Coder. Bounded repair scope: reads minidumps, proposes specific diffs, runs cargo test via Part 11 for >30s, presents each diff individually. Hardcoded zero write to trust primitives.

Modes A–E are runtime recovery. Mode F is assisted repair. Every error code in failures.md carries its mode classification; a Mode F entry that fires is a code-level bug, not a config issue.

6.2 Structural refusals

The list in §5.2. Enforced at build time by the AST guard; a refactor that removes a refusal fails CI.

6.3 Safety layer sits below the model (A.154, DECIDED)

The destructive-action safety invariant (destructive action → Thinking), the Verifier's deterministic tool-call check, the structural refusals in §5.2, and the persona constraints in the always-loaded context are enforced at the agent/router level, not at the model level. An abliterated or third-party uncensored model (A.144, A.145) does not bypass any of them. This is a documentation invariant — the mechanism was already correct; the clarification exists so an uncensored-model toggle cannot be mistaken for a safety-layer toggle.

6.4 Baseline provenance

Every baseline entry in baselines.md carries a status field:

measured — measured on the reference rig with the shipping binary; CI gates on regression.

target — the aspirational target; not yet measured. CI does not gate until the first measurement lands.

placeholder — a stand-in pending a specific gate (A.18, A.54, A.63, A.96, A.97, A.100); the entry exists so the schema and audit trail are stable.

gated — the feature is behind a gate that has not passed; the number is a paper-cited target that becomes target when the gate opens.

CI fails only on a measured entry that regresses beyond tolerance. The release audit (cargo xtask baseline-audit) produces a per-release ledger of which entries are measured for that release. A spec with 50 measured entries and 200 honestly-tagged targets is more useful than one with 500 numbers of unknown provenance.

6.5 One-installer rule

See §2. Enforced by releases.md and by cargo xtask release-audit.

6.6 AST guards and network tripwires

cargo xtask ac-regression runs on every PR and enforces the structural invariants listed in SUL-LEDGER/guards.md. Categories:

Base checks — no wasapi import outside sul-audio, no HTTP client in sul-wire, no bypass of sul-ipc::try_push, no ReadProcessMemory in AI paths, etc.

Multi-profile guards — no per-profile speculation chain, Thinking context > Instruct context, Conversation context > Thinking context, Medusa temperature scaling enabled, no own-LM-head variant.

Audio / STT / emulation guards — no wasapi in sul-voice-fx (uses sul-audio), voice-clone API surface has no entry point, \[stt\] flash_attn explicitly true, sul-gb no wasapi, sul-savestate calls retro_serialize only in its libretro shim.

Quant / KV / Spec / ToolPRM guards — sul-quant emits pure K-quant GGUF, sul-kv-compress no wasapi, sul-toolprm no ReadProcessMemory.

Vault guards — no HTTP client, no DEK escape, no plaintext to disk, no plaintext embeddings, \[vault\] extra_paths only via OS file picker.

WebView guards — isolation_pattern = true, enhanced_security_mode = "strict", runtime floor enforced, electron-audit passes, tauri-capability-audit passes, webview-runtime-audit passes.

Music / broadcast guards — sul-music-engine exclusive path does not bypass device coordination, sul-broadcast does not bypass admission controller, no game-hook capture on kernel-AC titles.

App auth guards — no bypass path in release builds, \[auth\] enabled = false is a Tier 1 event, \[auth\] require_password = false with no other factor is E_CFG.

Feature toggle guards — no core feature togglable, no structural refusal togglable, \[performance\] toggles default on, gated features default off until their gate passes.

Release-plan guards — releases.md exists, every release has a reference config, every component resolves, every A.x gate exists, the superset invariant holds, every Tier 1 event is classified.

Handoff guards — sul-csi no ReadProcessMemory / no unsigned models / no HTTP, sul-bci no ReadProcessMemory / pinned real-time thread / decoded text flagged data_untrusted, sul-gev no ReadProcessMemory / read-only feeds / local geolocation only, sul-security-toolkit container enforced / no ReadProcessMemory / signed consent required / output flagged data_untrusted, sul-voicestudio no "clone specific person" code path / no HTTP / character voice models signed with per-model license identifiers, sul-heretic build-time only.

Network tripwires. Every outbound network surface is declared. Current declared surfaces: Tauri IPC (custom protocol), RustXNG MCP (localhost), GitHub search API (api.github.com), hive-mind fetch (raw.githubusercontent.com), auto-updater, Sentry (optional, off by default), GEV public feeds (ADS-B/AIS/satellite read-only), security toolkit target (declared at scan time), Jellyfin (user-configured local server URL, local network only), Nextcloud (user-configured local server URL, local network only), LibreTranslate (user-configured local or self-hosted URL, local network only), LocalSend (LAN only), Pexels API (api.pexels.com), HaveIBeenPwned API (haveibeenpwned.com), VirusTotal API (virustotal.com), TinEye API (tineye.com), Ninite (ninite.com), Maxun (user-configured local or container). The AST guard fails on any undeclared network access.

Three CI gates run on every PR: electron-audit (no Electron anywhere), tauri-capability-audit (no "csp": null, no withGlobalTauri: true in production, no wildcard scopes, no shell:allow-execute), webview-runtime-audit (WebView2 ≥ 156.0.4314.58, Tauri ≥ 2.11.5, plugin-updater ≥ 2.12.0).

6.7 Coder trust primitive restrictions

The Coder AI (Jarvis-Coder hierarchy) has hardcoded zero write authority over:

sul-trust, sul-quarantine, sul-watchdog (security substrate).

Ed25519 signing keys (%APPDATA%/SUL/trust/).

Auto-updater manifest.

sul-vault-core, sul-face-auth, sul-voice-auth (vault cryptographic material).

sul-auth-app (app-level auth).

A PR that touches any of these is refused by the Gate A check with E_TRUST_PRIMITIVE_WRITE. The user can override the refusal only via a Tier 1 approval modal — a deliberate multi-step action. The trust primitive files are Ed25519-signed; a modified file fails signature verification and triggers Mode D quarantine. The Coder AI's worktree has no write access to %APPDATA%/SUL/trust/ (filesystem permission enforced at worktree creation).

6.8 Tier 1 events

Every safety-relevant config change fires an OS-level approval modal (native Win32, outside the LLM context, prompt-injection-proof). The modal's record is stored in %APPDATA%/SUL/tier1-approvals.json (Ed25519-signed, DPAPI-wrapped) and referenced by the Coder's Gate A verification and by the auto-updater's Tier 1 checks.

Tier 1-core (R0, always fires regardless of release): a setting is Tier 1-core if it disables a safety invariant, weakens authentication, weakens WebView hardening, or attempts a structural refusal. The 21 Tier 1-core events are the tier1_events array in R0 of releases.md:

vault:disable_auth_factor, vault:disable_anti_debug, vault:disable_decoy, vault:change_kdf_profile, vault:change_aead_cipher, vault:enable_searchable_index, vault:add_extra_path_via_config, auth:disable, auth:disable_factor, auth:change_reauth_for_vault, auth:disable_anti_debug, auth:disable_process_hardening, auth:disable_memory_lock, invariant:disable_destructive_action_always_thinking, invariant:enable_uncensored_model, webview:disable_isolation_pattern, webview:disable_strict_esm, webview:lower_runtime_floor, webview:lower_tauri_version_floor, webview:grant_shell_allow_execute, refusal:voice_clone_attempt.

Everything else is Tier 1 by release class. The full per-release list is in releases.md (tier1_events_added for R1, R2, R3). Rationale paragraphs live in SUL-DECISIONS.md under T1-RATIONALE-R1, T1-RATIONALE-R2, T1-RATIONALE-R3.

7\. Reference Hardware and Performance Envelope

7.1 Reference rig

RTX 3060 Ti 8 GB / Ryzen 5 5600X / 64 GB DDR4 (XMP/DOCP active) / Samsung 970 EVO Plus 500 GB NVMe / Seagate 2 TB HDD (secondary library). R3 optionally specifies a 12 GB+ baseline (RTX 4080 12 GB+ / Ryzen 7 7700X / 64 GB DDR5) for its reference configuration. R3 features do not ship on 8 GB hardware by default; the compatibility validator warns (does not refuse) if a user enables R3 sensing features on 8 GB alongside the R2 stack.

7.2 The 8 GB envelope

The hard constraint is 8 GB VRAM. Every component that allocates VRAM is accounted for. Combat Full's four profiles all share the same weights (\~1.05 GB Q4_K_S) and differ only in KV cache:

Component	VRAM (with ReTrace active, shipping requirement)	When active

Combat Full — Instruct	\~1.30–1.35 GB	Game running, fast execution

Combat Full — Thinking	\~1.35–1.45 GB	Game running, planning / destructive-action

Combat Full — Conversation	\~1.25–1.30 GB	No-game open-ended dialogue

Combat Full — Coding (research)	\~1.15–1.30 GB	No-game code work (A.101 gated)

Deep Q5_K_M	\~6.46 GB weights, \~6.96 GB with q8_0 KV at 4K	ED and coding only

sul_toolprm_enc (distilled cross-encoder)	\~60–180 MB Q4	Function-calling turns

PP-OCRv5 INT8	\~21 MB	During gameplay

Frame gen (at 1080p 2×)	\~300 MB reserved	Per-game, kernel-AC refused

Broadcast (1080p60, 3 sources)	\~350 MB	When streaming

Broadcast (4K60, 6 sources)	\~900 MB	When streaming 4K

R0 case (no ReTrace, no ToolPRM): Combat Full Instruct \~1.65–1.75 GB + OCR \~21 MB = \~1.67–1.77 GB. Fits under the 3 GB general combat ceiling (Phase 2) with \~1.23 GB headroom.

R0.1 case (ReTrace passed, ToolPRM not yet): Combat Full \~1.30–1.35 GB + OCR \~21 MB = \~1.32–1.37 GB. Fits under 3 GB with \~1.63 GB headroom.

R2+ case (ReTrace + ToolPRM + full spec chain): Combat Full \~1.3–1.45 GB + ToolPRM \~60–180 MB + OCR \~21 MB = \~1.53–1.65 GB. Fits under 3 GB with \~1.35 GB headroom.

HD2 Combat Full VRAM calculation (Phase 2 retires the 2.5 GB HD2-specific ceiling in favor of a general 3 GB combat ceiling for all games): Combat Full (\~1.3–1.45 GB with ReTrace) + sul_toolprm_enc (\~60–180 MB) + PP-OCRv5 (\~21 MB) = \~1.53–1.65 GB. Comfortably inside 3 GB with \~1.2 GB headroom. No co-residence calculation is needed — Combat Full is the only combat model, and all profiles share the same weights. The router flips profiles in <50 ms without a model reload.

ED Combat Full VRAM calculation (no kernel-AC): Combat Full (\~1.3–1.45 GB with ReTrace) can co-exist with Deep (\~6.46 GB Q5_K_M with STAR-KV fits at 4K) only if 2–4 layers of Deep are offloaded (\~7.1 GB total, tight but fits). Without offload, 1.45 + 6.46 = 7.91 GB > 8 GB, so E_CFG. Under the VRAM posture, Deep + game co-residence requires a Tier 1 confirmation; the default posture refuses it. The old three-tier ED co-residence calculation (\~5.2 GB for Combat Low + Combat Mid + Combat Full) is obsolete — Combat Full is \~1.3–1.45 GB, Deep is the only other checkpoint.

Deep + game co-residence is an explicit exception. Deep is for no-game coding. If the user invokes Deep with a game running, the compatibility validator raises a Tier 1 confirmation; accepting it records the exception. The R2 reference config therefore records Deep as not co-resident with the game: Deep is loaded on demand (no-game) and evicted before game launch. The "Deep offload layers: 2–4" is retained for the no-game case only.

7.3 Sustained performance targets

Full numbers in baselines.md. The envelope:

Deep Q4: median 30 tok/s / p99 25 tok/s after 10 minutes continuous inference (catches thermal throttling). target.

Deep Q5: median 25 tok/s / p99 20 tok/s after 10 minutes. target.

Audio: 48 kHz / 512-sample default; governor never silently escalates above 1024 samples. DSP p99 ≤8 ms at 512/48k on AVX2.

Broadcast: 1080p60 / 3 sources <4% of one core, <250 MiB RSS, <350 MiB VRAM. 4K60 / 6 sources <15% CPU, <900 MiB VRAM.

Music: <1% CPU no DSP, <3% with EQ, <5% with EQ + ReplayGain + resampling.

Frame gen: <1.5 ms CPU overhead at 2×, <2.5 ms at 4×.

Vault: ≥500 MB/s read/write median, p99 800 MB/s; 20 GB streams with peak RSS <100 MB; unlock median 1500 ms, p99 3000 ms.

Overlay: p99 frame time <1 ms (target).

Vision: PP-OCRv5 INT8 end-to-end p99 <60 ms per changed frame, <1 ms per unchanged frame.

Search: full pipeline p50 \~660 ms, p99 \~1500 ms.

Most of these are target or placeholder. A handful are measured. The point is not to pretend the envelope is achieved — it is to know which numbers are real and which are wishes.

7.4 Quality-first posture

Where performance and quality trade off, SUL chooses quality on the reference rig:

Deep Q5_K_M (6.02 GiB, 94.14% top-1, +0.915% ΔPPL) is the default; Q4_K_M (5.21 GiB, 92.12% top-1, +2.04% ΔPPL) is available for headroom. JQ5_K (same size, 17.1% lower mean KLD) is preferred if the mainline PRs merge.

TTS on CPU (Piper \~150 ms on desktop CPU) rather than GPU — the VRAM belongs to Combat Full, and the measured latency gap (Breeze TTS 2 on RTX 4060: 220–472 ms first packet vs CPU Piper \~150 ms) is not worth the VRAM cost.

ReTrace is a shipping requirement for Combat Full (A.54). Without it, Combat Full is \~1.65–1.80 GB and the general 3 GB combat ceiling (Phase 2) does not accommodate 1.55–1.80 GB plus OCR plus ToolPRM plus any other co-resident model; the shipping calculation uses the ReTrace-on figures. R0 ships Combat Full Instruct without ReTrace at \~1.65–1.75 GB; R0.1 adds ReTrace when A.54 passes and drops to \~1.5 GB; R2+ ships with ReTrace, ToolPRM, and the full spec chain. All configurations fit the 3 GB general combat ceiling (Phase 2).

7.5 VRAM posture — minimize, don't trade

SUL is a sub-OS. It runs alongside a game the way Steam does, and it must not crowd the game out of VRAM. The posture has three rules:

Rule 1 — Minimize by optimizing, not by degrading. Every VRAM-saving optimization is mandatory: ReTrace (A.54), STAR-KV / GAAC (A.18), SPEED-Q / APEX (A.96), the full speculation chain (A.99), ToolPRM cross-encoder (A.100), quantization, aggressive but lossless token compression. No feature ships that trades quality, speed, accuracy, or performance for VRAM savings. The design always targets the lowest VRAM at which every gate still passes.

Rule 2 — Layered ceiling. The VRAM budget is layered by what is loaded:

| State | Ceiling | What's loaded |

|---|---|---|

| Idle (no game, no AI) | ≤500 MB | UI substrate, IPC, watchdog, telemetry, vault locked |

| Game running, combat AI off | ≤2 GB (25% of 8 GB) | Idle substrate + OCR + screenshot ring + prosody + game-state machine |

| Game running, combat AI on | ≤3 GB (37.5% of 8 GB) — general combat ceiling | The 2 GB substrate, plus the combat AI stack (Combat Full + ReTrace + ToolPRM + vision tokens) |

| Deep Coder / broadcast 4K / frame generation | Explicit high-VRAM exception | User-invoked. Refused by default with a game running unless the user accepts a Tier 1 confirmation. |

The 3 GB combat ceiling applies to all games, including kernel-AC titles. Kernel-AC does not impose a VRAM ceiling — it scans process memory, not VRAM. HD2's old 2.5 GB special case was conservative, not structural, and is retired.

Rule 3 — Ceiling is a cap, not a target. Combat AI takes the lowest VRAM that keeps every gate passing. It does not spend headroom just because the ceiling is 3 GB. The R2 reference config's Combat Full (\~1.5 GB with ReTrace, ToolPRM, and the full spec chain) is well under the 3 GB ceiling because the design minimized, not because the ceiling forced it.

Measured against the ceiling. The compatibility validator (A.117) checks the layered ceiling at every launch. Above the idle ceiling with no game: E_CFG. Above 2 GB with a game and combat AI off: warning. Above 3 GB with combat AI on: E_CFG. Deep / broadcast / FG with a game running: Tier 1 confirmation required.

The trade rule. If a gate (A.63-Instruct, A.63-Thinking, A.63-Router, A.99, A.100, A.54, A.96) fails at the current VRAM setting, the config validator surfaces the trade and asks the user to choose which dimension to trade. It does not silently degrade.

8\. Threat Model in Brief

Full threat model in SUL-ARCHITECTURE/Ch. 16.

8.1 What SUL defends against

Vault file theft — XChaCha20-Poly1305 encrypted; DEK wrapped by TPM + DPAPI + user's password (via HKDF-derived KEK); all three must be defeated simultaneously. TPM 2.0 sealing ties to the machine; DPAPI ties to the user account; layered auth (password + Windows Hello + optional face/voice) is AND-logic. No password recovery. Decoy vaults + rate limiting (5 fails → 5 min lock, 20 fails → 24 h lock + TPM PIN reset) + anti-debug (cynapse + astraguard + anti-dbg).

Vault file tampering — Ed25519 signatures on header, manifest, user-marked files; SHA-256/BLAKE3 hashes in the signed manifest; tampered file → quarantine; tampered header/manifest → vault refuses to open entirely; unknown file → treated as potentially injected, quarantined.

Model file tampering — SHA-256 against signed manifest on every session start (not just first load); Mode D quarantine on mismatch.

Pack tampering — Ed25519 Minisign on every load; unsigned or wrong-signed refused; persona pack signed by user's own key; hive packs signed by maintainer key.

WebView XSS → RCE — Electron exclusion removes the Node.js RCE path by construction; tauri-capability-audit, CSP nonces, Isolation Pattern, Strict ESM, de-elevation, webview-runtime-audit.

Prompt injection via model outputs — persona constraints in always-loaded context, destructive-action safety invariant, deterministic Verifier, TSCG compilation, OODA-Tool grounding.

Unauthorized network exfiltration — every outbound surface declared; ac-regression fails on undeclared network access.

Kernel-AC interaction — structural refusals (§5.2); external DComp overlay by default; flip-model per-game on kernel_level: false only.

Unauthorized security tooling — signed consent file required; container isolation; data_untrusted flag on scan output.

Voice cloning of a specific real person — structural refusal; API surface has no entry point; AST guard rejects any PR that introduces one.

8.2 What SUL explicitly does not defend against

These are stated in the architecture as limitations, not hidden:

A fully compromised host process that reads the DEK from memory while the vault is unlocked. The defenses (mlock, secrecy/redoubt, secmem-alloc, anti-debug, process hardening, de-elevation) make this harder but do not eliminate it. "This is a fundamental limitation of user-space encryption."

A malicious OS that fakes TPM sealing. DPAPI's user-account binding is orthogonal and would still need to be defeated, but the OS-level trust assumption is explicit.

An attacker with the user's password, Windows Hello credentials, and physical access to the machine.

WebView2 zero-days. The residual after everything else. "No architecture eliminates \[WebView2 zero-days\]." Mitigated by de-elevation, Isolation Pattern, and the vault being a separate subsystem.

Hardware side channels (Meltdown-class, Spectre-class, GPU-side). OS/microcode responsibility.

Driver-level compromise. SUL does not run kernel-mode code; a compromised driver is out of scope and every application on the machine is affected, not just SUL.

8.3 The safety layer sits below the model

See §6.3. This is the crucial invariant for uncensored models and for prompt injection. An abliterated model (A.144) or a locally-uncensored model (A.145) cannot bypass the destructive-action safety invariant, the Verifier's tool-call check, the structural refusals, or the persona constraints. The safety layer is below the model, not inside it.

9\. Reader's Guide to the Other Artifacts

9.1 SUL-ARCHITECTURE/ — one chapter per subsystem, fixed template

Each chapter has the same sections, in the same order:

Purpose — one sentence.

Architecture — module structure, data flow, process model.

Contract — the crate's public API and what it promises.

Config — every \[module.\*\] block, verbatim.

Failure taxonomy — every error code with its recovery mode (cites SUL-LEDGER/failures.md).

Fallback chain — ordered fallbacks with success criteria per rung.

Gates — every A.x gate that gates this subsystem (cites SUL-LEDGER/gates.md).

Baselines — every baseline entry (cites SUL-LEDGER/baselines.md).

AST guards — every guard (cites SUL-LEDGER/guards.md).

Benchmarks — which xtask commands measure this subsystem.

Test requirements — Miri, Kani, loom, failure-injection, integration.

Cross-references — the other chapters this one depends on.

How to trace a fact: e.g., "Why is Combat Full \~1.30–1.35 GB?"

The Brief states the number (§7.2).

The Architecture Ch. 10 — Local AI explains the mechanism (weights + ReTrace + KV split across profiles).

baselines.md \[ai.combat_full_instruct\] gives the value, status (gated), and the gate that must pass (A.54).

gates.md A.54 gives the full criterion.

SUL-DECISIONS.md D-0007 (hypothetically) gives the history of why Combat Full is one checkpoint with four profiles rather than a multi-tier ladder.

9.2 SUL-LEDGER/ — machine-readable

gates.md — A.1 (release-blocking, 9), A.2 (gated shipping components, 114), A.3 (decided/resolved, 18), A.4 (research, 40), A.5 (stubs, 18). Every gate carries id, bucket, title, release_class, release_blocking, criterion, harness, fallback, status, rationale_ref.

baselines.md — every baseline entry with status (measured / target / placeholder / gated), value, tolerance, note, provenance where applicable.

failures.md — every error code with its recovery mode (A–F) and its owning subsystem.

guards.md — every AST guard, categorized.

releases.md — the full release plan (R0–R4), the reference configurations, the per-release components / features_off / tier1_events.

How to read a gate entry: the criterion is the operational test. The harness is the xtask command that runs it. The fallback is what happens on failure. The rationale_ref points to a rationale/A.x.md file with the long-form explanation. The status field is gated (gate hasn't passed), passed (gate passed, feature ships), or rejected (gate is a rejection, not a gate).

9.3 SUL-DECISIONS.md — append-only

Every "was referenced as §11.24," every "the original §13.6 was titled," every migration table, every \[CUT\] annotation, every rejection rationale, every "the pre-consolidation design had Combat Low" note. Append-only. Never prune. When a decision is superseded, append the superseding decision; do not edit the old one.

How to read a decision entry: D-NNNN (sequential), title, status (decided / superseded / rejected / parked), context, decision, consequence. When a decision is superseded, the new entry cites the old one; the old one gains a superseded_by field but its body is unchanged.

The archaeology rule: every claim in the old spec's "was referenced as" / "the original §X" / "merged from" prose is now a D-NNNN entry. The Architecture Book cites D-NNNN; it does not restate the archaeology.

10\. What Changed in This Rewrite

Nothing was cut. Every mechanism, every number, every gate, every failure code, every AST guard, every config key, every rejection rationale survives in one of the four artifacts. The container changed; the content did not.

Where the old spec's sections went:

Old spec	New artifact

Part 1 §1.1 (Core Platform Architecture)	Brief §1–§2; Architecture Ch. 1

Part 1 §1.2 (App-level auth pointer)	Brief §1; Architecture Ch. 2

Part 1 §1.3 (Feature classification)	Brief §4; Architecture Ch. 1

Part 1 §1.4 (Performance optimizations)	Brief §4; Architecture Ch. 13

Part 1 §1.5 (Master compatibility matrix)	Architecture Ch. 1

Part 1 §1.6 (Release scope, ladder, releases.md)	Brief §3; releases.md; Architecture Ch. 1

Part 1 §1.7 (Dynamic token-limit management)	Architecture Ch. 10

Part 1 §1.8 (Process architecture)	Architecture Ch. 1

Part 1 §1.9 (Startup sequence)	Architecture Ch. 1

Part 1 §1.10 (Precedence rules)	Architecture Ch. 1

Part 1 §1.11 (Measurement methodology)	Architecture Ch. 1; baselines.md

Part 1 §1.12 (Baselines framework)	Brief §6.4; baselines.md

Part 1 §1.13 (Config structure)	Architecture Ch. 1; gates.md validation rules

Part 1 §1.14 (IPC enum definitions)	Architecture Ch. 1

Part 1 §1.15 (UI component catalog)	Architecture Ch. 1

Part 1 §1.16 (CoreError enum)	Architecture Ch. 1; failures.md

Part 1 §1.17 (Diagnostics bundle)	Architecture Ch. 12

Part 1 §1.18 (Deferred platform scope)	Brief §5.1

Part 1 §1.19 (Development workflow)	Architecture Ch. 1; SUL-DECISIONS.md

Part 2 (Auth and vault)	Brief §1, §6.7, §8; Architecture Ch. 2

Part 3 (Game library, virtualization, mod toolchain)	Architecture Ch. 3

Part 4 (Audio pipeline and voice stack)	Architecture Ch. 4

Part 5 (Music player)	Architecture Ch. 5

Part 6 (Broadcast engine)	Architecture Ch. 6

Part 7 (Overlay)	Architecture Ch. 7

Part 8 (Vision, search, RAG)	Architecture Ch. 8

Part 9 (Frame generation)	Architecture Ch. 9

Part 10 (Local AI)	Architecture Ch. 10

Part 11 (Data pack strategy)	Architecture Ch. 11

Part 12 (Telemetry, diagnostics, crash)	Architecture Ch. 12

Part 13 (Hardware integration)	Architecture Ch. 13

Part 14 (Autonomous hive-mind and safety)	Architecture Ch. 14

Part 15 (Sensing, security, handoff)	Architecture Ch. 15

Part 16 (Threat model, licensing, anti-cheat)	Brief §8; Architecture Ch. 16

Part 17 (References)	SUL-ARCHITECTURE/Ch. 17

Appendix A (Gates, decided, research)	SUL-LEDGER/gates.md; SUL-DECISIONS.md

What's new in the Brief that the old spec didn't have explicitly:

A Non-Goals section (§5) consolidating every refusal, rejection, deferral, and licensing boundary from across the spec.

A Reader's Guide (§9) explaining how to trace a fact across the four artifacts.

A hardware envelope section (§7) that says explicitly which numbers are target / placeholder / measured, including the layered VRAM posture (§7.5).

What's gone from the spec that was never supposed to be in it:

The "was referenced as §11.24 — body written here" archaeology (moved to DECISIONS.md).

The "the original §13.6 was titled…" migration prose (moved to DECISIONS.md).

The \[CUT\] and \[HISTORICAL\] annotations (moved to DECISIONS.md).

The A.5 stubs (preserved in gates.md A.5 but marked clearly as stubs, not gates).

Nothing else was removed.
