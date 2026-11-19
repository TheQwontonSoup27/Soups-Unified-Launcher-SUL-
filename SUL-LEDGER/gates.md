SUL-LEDGER/gates.md



Reading rule. A.1 = release-blocking gates. A.2 = gated shipping components (the feature ships only if the gate passes; the release class determines when). A.3 = decided / resolved (no benchmark needed). A.4 = research only (no shipping obligation, no date, no owner). A.5 = stubs awaiting content.



Every gate carries: bucket, release class, status (gated | passed | rejected | parked | obsolete | research | stub | decided | applied | archived), release-blocking (bool), criterion (the operational test), harness (the xtask command(s) that verify), fallback (what happens on failure), rationale (a relative path into SUL-DECISIONS/rationale/).



The rationale file holds the long-form "why." The criterion here is the operational test.



Scope rule for A.1. A gate in A.1 is release-blocking \*\*for the release class named in its `release` field\*\*, not for every release. A gate assigned to R2 does not block R0. A gate assigned to R3 does not block R0 or R1. `release-blocking: yes` means "the release class named in `release` cannot ship if this gate fails." The release ledger (`releases.md`) lists, per release class, exactly the gates that block that release. A feature whose gate is in A.2 is a gated shipping component: the feature does not ship until the gate passes, but the release can ship without it (the feature is listed in `features\_off` for that release).



A.1 — Release-Blocking Gates (9)



A.18 · STAR-KV PPL + KL@D measurement gate



Bucket: A.1 · Release: R2 · Status: gated · Blocking: yes



Harness: kv-bench.py::star\_kv\_ppl, kv-bench.py::star\_kv\_kl\_d · Rationale: A.18



Criterion. Measure STAR-KV PPL delta vs f16 on 1000-prompt eval, AND measure KL divergence at the logit level (KL@D). Must be <2% PPL for STAR-KV to ship as primary, AND KL@D below the measured f16 baseline at target compression ratio. If either fails, fall back to GAAC (bit-for-bit lossless, 1.405×).



PPL alone can pass while attention logits are distorted in ways that matter for the Entity Ledger, tool-call structure, and OODA scaffold fields. TurboQuant+ documentation is explicit: "Methods that are strictly better under MSE are systematically worse under KL@D in deployment. MSE is a broken proxy for KV cache compression quality." A.97 extends the same gate to every sul-kv-compress backend.



Fallback. GAAC (bit-for-bit lossless, 1.405×). STAR-KV remains research until the surgery pass improves.



Notes. Decision criterion: PPL delta <2% on 1000-prompt eval AND KL@D below f16 baseline → STAR-KV ships. Otherwise, GAAC ships and STAR-KV remains research until the surgery pass improves.



A.26 · Mainline llama.cpp b12207+ regression gate



Bucket: A.1 · Release: R0 · Status: gated · Blocking: yes



Harness: cargo xtask baseline-audit (full baselines.md regression) · Rationale: A.26



Criterion. b12207+ adds Zamba2 throughput, unified KV cache, OCP FP8, Vec-LUT, and (via b11454) K2-Horizon support. Gate: full baselines regression.



Fallback. Revert to b11513 and file upstream issue.



Notes. b12207 already satisfies b11454, which contains the K2-Horizon merge. The pin remains b12207+. If any tier regresses beyond tolerance, revert to b11513 and file upstream issue. Otherwise, ship b12207+.



A.54 · ReTrace production measurement gate



Bucket: A.1 · Release: R2 · Status: gated · Blocking: yes



Harness: retrace-bench.py::saliency, retrace-bench.py::dormant\_buffer, retrace-bench.py::retention, retrace-bench.py::benchmark\_retention, retrace-bench.py::prefill\_speedup · Rationale: A.54



Criterion. ReTrace is the only Combat Full configuration; the gate is release-blocking for all four profiles. Measure saliency computation latency, dormant buffer size, backtracking injection stability, benchmark retention on MMB/POPE/SQAI at 11.1% token retention, prefill speedup. Full test requirements in Architecture Ch. 10 §10.16.



Fallback. Ship if retention ≥95% at 11.1% token retention and prefill speedup ≥2.5×. Otherwise, expand pivot set or fall back to higher retention % (30%, 50%, 70%) with proportional VRAM cost. If ReTrace cannot hit 95% retention at any retention level, Combat Full falls back to full-token baseline for the session (VRAM cost +0.3–0.4 GB, all profiles) and ReTrace is re-evaluated.



Notes. Integration with sul-kv-tier: dormant buffer uses STAR-KV compressed paged format; Lynx promotion for dormant tokens when needed; GAAC lossless fallback if STAR-KV quality gate fails. ReTrace is profile-agnostic — one saliency computation, one dormant buffer, one backtracking injection for all four profiles.



A.63-Instruct · Combat Full Instruct fine-tuning measurement gate



Bucket: A.1 · Release: R0 · Status: gated · Blocking: yes



Harness: combat\_full\_bench.py::instruct\_bfcl, router\_bench.py::destructive\_action\_instruct · Rationale: A.63



Criterion. With A.63-Common passed and the Instruct profile trained, measure Instruct BFCL non-live/live on the reference rig. Measure Instruct destructive-action detection recall on 100 scripted destructive triggers. In R0 there is no router and no Thinking profile — the "destructive → Thinking" safety invariant is enforced by the Instruct profile emitting the confirmation warning. (The `destructive\_action\_always\_thinking` setting name is retained for forward compatibility; in R0 it controls whether Instruct emits the warning. In R1+ it controls the router's override to Thinking.)



Both must pass:



Instruct profile BFCL non-live ≥65 (execution accuracy).



Instruct destructive-action warning recall = 100% on 100 scripted destructive triggers (safety invariant — any miss is a hard fail).



Fallback. If either fails, re-evaluate: (a) keep Hammer as swap-in for the Instruct path (A.71's live trigger), (b) re-run training with more data, (c) accept the lower ceiling if destructive-chain scenarios are rare enough. If the Instruct profile cannot pass, R0 ships without Combat Full — the Hammer swap-in is the fallback AI tier, and the R0 component list is revised.



Notes. The pre-fine-tuning InternVL3-2B Instruct BFCL non-live score is recorded in the first run. If pre-fine-tuning is already above 65, the gate is re-read as "confirm the fine-tuned model does not regress below 65."



A.63-Common · Combat Full shared training data and recipe gate



Bucket: A.1 · Release: R0 · Status: gated · Blocking: yes (shared prerequisite for A.63-Instruct / A.63-Thinking / A.63-Conversation / A.63-Router)



Harness: combat\_full\_bench.py::training\_data\_check, cargo xtask verify-lora-merge · Rationale: A.63



Criterion. The training data and recipe shared by all Combat Full profiles must satisfy:



(a) The training dataset is commercially licensed. Two paths, gated independently: the synthetic path (APIGen — verify current repo license against the tag before shipping, public release is CC-BY-4.0; or ToolOrchestrationReward — horizon-llm/ToolOrchestrationReward, verify license), or the licensed-subset path (xLAM filtered to explicitly CC-BY-4.0 or Apache-2.0 splits; the 60k dataset as published is uniformly CC-BY-NC-4.0, so subsets must be verified individually; if none qualify, this path is closed).



(b) Mode-tagged data covers instruct, thinking, conversation, and coding.



(c) The merged GGUF passes `cargo xtask verify-lora-merge` (cosine similarity >0.998, output-token equality >98%).



(d) The negative-example suites are included: 500 pairs per failure mode — Instruct-mode-response-with-thinking-tokens, Thinking-mode-response-without-reasoning, Conversation-mode-response-with-tool-call-JSON, Coding-mode-response-with-conversational-fluff.



Fallback. If the synthetic path cannot reach ≥90% of xLAM-trained BFCL on the held-out SUL tool set, and no licensed subset qualifies, the Combat Full fine-tuning path is closed. The fallback is the Hammer swap-in (A.71's live trigger) for all profiles. This is a hard blocker for R0's Combat Full tier.



Notes. The shared recipe is the load-bearing part of the multi-profile design. If it fails, every profile gate fails together. ToolPref-Pairwise-30K (RioLee — verify license before shipping) is required for the ToolPRM reward-model SFT stage. Cost across all profiles: LoRA training \~$36; function masking script \~2–3 days engineering; ToolPRM distillation training \~$20; profile-tagged data preparation \~1.5 days. Total \~$56 + \~4.5 days engineering time.



A.92 · Vault encrypted storage



Bucket: A.1 · Release: R0 · Status: gated · Blocking: yes



Harness: vault-bench.py::throughput, vault-bench.py::unlock, vault-bench.py::rss\_20gb, vault-bench.py::adversarial, vault-bench.py::fuzz\_crash, vault-bench.py::key\_rotation · Rationale: A.92



Criterion. XChaCha20-Poly1305 AEAD + IOCP pipeline + DPAPI + TPM wrapping + Ed25519 signatures + decoy vaults + rate limiting + anti-debug + memory protection. Ships when: (a) zero plaintext leak on crash mid-write (fuzz at every write boundary), (b) AEAD construction resists truncation/chunk-reorder/chunk-substitution/header-tamper (20+ adversarial cases), (c) key rotation works without re-encrypting the whole vault, (d) throughput ≥500 MB/s read and write, (e) 20 GB file streams with peak RSS <100 MB, (f) one external crypto review of the AEAD construction and nonce derivation before release. (a), (b), and (f) are the minimum for shipping; all six are required for R0 to ship the vault component.



Fallback. Vault does not ship. This is a hard release gate: R0's vault component requires A.92. If A.92 fails, R0 ships with the vault removed from its component list (vault stays `features\_off` until a later release re-attempts), or R0 does not ship. The spec's intent is that R0 does not ship without the vault; the fallback language records the only alternative.



Notes. Full auth spec in Architecture Ch. 2 §2.1. The vault remains opt-in at runtime (`\[vault] enabled = false` by default), but the component is shipped and must be verified.



A.114 · App-level auth



Bucket: A.1 · Release: R0 · Status: gated · Blocking: yes



Harness: cargo test -p sul-auth-app; cargo xtask ac-regression · Rationale: A.114



Criterion. Ship when the auth modal renders before WebView hydration, all configured factors pass in AND-logic, no bypass path exists in release builds (AST guard), reauth intervals and high-risk reauth work, session token is held in secure\_memory and destroyed on close/expiry, enrollment flow forces explicit no-recovery acknowledgment.



Fallback. A failure blocks release of \[auth] enabled = true; the app can ship with \[auth] enabled = false as a Tier 1 event, but the default posture requires auth.



Notes. Full auth spec in Architecture Ch. 2 §2.1.



A.117 · Compatibility validator



Bucket: A.1 · Release: R0 · Status: gated · Blocking: yes



Harness: cargo xtask config-check · Rationale: A.117



Criterion. Ship when cargo xtask config-check runs the full compatibility matrix at every launch, catches every explicit conflict in Architecture Ch. 1 §1.5, resolves resolvable conflicts with a warning dialog, raises E\_CFG for unresolvable conflicts, warns for missing dependencies, warns or refuses for VRAM over-budget against the layered ceiling (Brief §7.5): above the idle ceiling with no game → E\_CFG; above 2 GB with a game and combat AI off → warning; above 3 GB with a game and combat AI on → E\_CFG; Deep / broadcast 4K / FG with a game → Tier 1 confirmation. The 10% tolerance applies within each layer. Logs auto-resolutions to \[compat] and compat.log, and produces a per-release coverage metric (fraction of the toggle space actually tested by the release's reference configuration).



Fallback. A failure blocks release of the compatibility validator. The coverage metric is surfaced in Diagnostics and in the release audit.



Notes. Promoted from A.2 to A.1 per Architecture Ch. 1 §1.6: the compatibility validator is what makes the combinatorics of the unified-product / multi-release model tractable at monorepo scale, so it is not optional.



A.152 · Legal and ethical framework



Bucket: A.1 · Release: R3 · Status: gated · Blocking: yes



Harness: manual review (no automated harness) · Rationale: A.152



Criterion. The section is complete, all features in Architecture Ch. 3 §3.1, Ch. 6 §6.1, and Ch. 15 are reviewed against it, and any features that do not comply are removed or gated further.



Fallback. Failure: the handoff features do not ship. This is a prerequisite for A.137 (GEV), A.147 (global sensor fusion), A.150 (binary audit), A.151 (auto-remediation), and — by extension — every other R3 handoff feature, because the framework defines the consent and retention baseline those features must meet. The framework is not a shipping gate for the existing SUL features (which are already compliant — they don't collect data on anyone but the user).



Notes. A dedicated section defining legal and ethical boundaries for GEV and surveillance features, including strict data retention, encryption, and user consent policies. The section is drafted, reviewed internally, and added to the spec as §20 (Legal and Ethical Framework).



A.2 — Gated Shipping Components (114)



Organized by release class. Every entry is status = gated unless noted. Criterion, harness, and fallback carry the full original text.



R0 — Substrate



A.50 · sul-wire ring view



Bucket: A.2 · Release: R0 · Status: gated



Harness: wire-bench.py::ring\_ui\_frame, wire-bench.py::binary\_frame · Rationale: A.50



Criterion. sul-wire direct ring view for token streaming. \~2 ms UI frame-time reduction during streaming vs Tauri IPC path. If token ring UI frame p99 <8 ms and binary encode/decode beat prior JSON path by ≥20×, ship. Otherwise, keep Tauri IPC path for tokens.



Fallback. Tauri IPC path for tokens.



A.115 · Feature toggle system



Bucket: A.2 · Release: R0 · Status: gated



Harness: cargo xtask config-check · Rationale: A.115



Criterion. Every non-core feature exposed as a toggle in \[features.\*]. Ship if the toggle registry covers every feature in Architecture Ch. 3 §3.1 through Ch. 9 §9.1, defaults match the classification (shipped on, gated off until A.x, opt-in off), core features are not togglable, and structural refusals are not togglable.



Fallback. Toggle system does not ship; app ships with tuned defaults hardcoded.



R1 — Mainstream features

A.63-Thinking · Combat Full Thinking fine-tuning measurement gate



Bucket: A.2 · Release: R1 · Status: gated · Blocking: no



Harness: combat\_full\_bench.py::thinking\_bfcl, combat\_full\_bench.py::thinking\_multi\_turn, combat\_full\_bench.py::thinking\_reasoning\_tokens · Rationale: A.63



Criterion. With A.63-Common passed and the Thinking profile trained, measure: Thinking BFCL non-live ≥65; Thinking multi-turn accuracy ≥ threshold (first run records the threshold); mean reasoning tokens per turn within the bounded budget (512–1024 after the first measurement records the chosen budget). The profile must emit reasoning traces (no thinking-token suppression) and must not leak Instruct-shaped responses.



Fallback. If Thinking BFCL <65, re-evaluate as in A.63-Instruct. If multi-turn or reasoning-token budget fails, tune the profile-specific loss weighting and re-run. Thinking failure does not block R1 — R1 ships with `ai.combat\_full\_thinking` off.



Notes. The Thinking profile is the first R1 AI addition.



A.63-Router · Combat Full router accuracy and destructive-action gate



Bucket: A.2 · Release: R1 · Status: gated · Blocking: no



Harness: router\_bench.py::mode\_selection, router\_bench.py::destructive\_action, router\_bench.py::determinism, mode\_switch\_bench.py::reasoning\_budget\_flip · Rationale: A.63



Criterion. With the router shipped (R1, `sul-agent::combat\_profile`), measure:



Router profile-selection accuracy ≥95% on 200 SUL-shaped turns.



Destructive-action detection recall = 100% on 100 scripted destructive triggers (the router overrides to Thinking regardless of user override — any miss is a hard fail).



Profile-switch latency <50 ms p99.



Router determinism (proptest 1000 iterations, no nondeterministic classification).



Thinking OOM → Instruct fallback fires correctly (CombatRouterFallback telemetry emitted).



Fallback. If router accuracy <95%, the router is disabled by default (fixed-profile operation); R1 ships with the router off (`ai.combat\_router` gated). If destructive-action recall <100%, the router is disabled and the R0 Instruct-generated warning path is retained. A destructive-action recall miss is a hard fail — the router cannot ship with any miss.



Notes. The router is R1 because it ships with `sul-agent`. The destructive-action safety invariant (Ch. 10 §10.12) is enforced by the router in R1+. In R0, the invariant is enforced by the Instruct profile generating the confirmation warning (A.63-Instruct). The `combat:disable\_router` Tier 1 event in R1 exists precisely so the user can fall back to fixed-profile operation.





A.4 · Persistent prompt cache to disk



Bucket: A.2 · Release: R1 · Status: gated



Harness: bench\_ai.py::first\_token\_cold\_warm · Rationale: A.4



Criterion. Persistent prompt cache to disk (%APPDATA%/SUL/cache/prompt-prefix). Saves 2–3s at session start. Gate for first\_token\_us\_warm baselines in Architecture Ch. 10 §10.23.



Fallback. If the cache does not save 2–3s, keep the in-memory-only path.



A.161 · RustXNG license verification



Bucket: A.2 · Release: R1 · Status: gated · Blocking: no



Harness: cargo xtask license-audit · Rationale: A.161



Criterion. Confirm that integrating RustXNG as a separate local process (MCP over localhost, not linked into sul.exe) is compatible with SUL's MIT/Apache license. The reference note records RustXNG as "likely AGPL-3.0 — verify before shipping." If AGPL-3.0, the separate-process architecture is the legal basis for use and must be confirmed by legal review. If legal review rejects it, the search pipeline falls back to `direct\_fetch` (already specified as the E\_SEARCH\_PIPELINE\_DEGRADED path in Architecture Ch. 8 §2.5).



Fallback. `direct\_fetch` with truncated context. The search pipeline ships either way; this gate determines whether RustXNG is the default backend or whether direct\_fetch is the default.



Notes. This gate does not block R1 — the search pipeline has a working fallback. It determines the default backend. If RustXNG is unavailable, `\[net.search] backend = "direct\_fetch"` becomes the shipped default and the RustXNG MCP surface is removed from the declared network surfaces (Ch. 16 §16.15.14).



A.23 · Surya 2.5 evaluation (OCR promotion)



Bucket: A.2 · Release: R1 · Status: gated



Harness: vision-bench.py::surya · Rationale: A.23



Criterion. Surya 2.5 (689M, 85.9 OmniParseBench, 4.5 pages/sec on H100, 0.1% loop rate, Apache 2.0 code / OpenRAIL-M weights) is a direct PP-OCRv5 INT8 competitor. If Surya 2.5 beats PP-OCRv5 by >5% accuracy on HUD set, replace PP-OCRv5 INT8 as primary engine. Otherwise, keep PP-OCRv5 INT8 primary.



Fallback. PP-OCRv5 INT8 remains primary.



A.24 · jina-vlm multilingual quality gate



Bucket: A.2 · Release: R1 · Status: gated



Harness: vision-bench.py::jina · Rationale: A.24



Criterion. jina-vlm ships as backend for multilingual scene queries. Measure quality vs MiniCPM-V on JA/RU/DE queries. If jina-vlm ≥2 points above MiniCPM-V on multilingual SUL eval set, ship. Otherwise, keep MiniCPM-V as default and jina-vlm as opt-in.



Fallback. MiniCPM-V remains default; jina-vlm is opt-in.



A.44 · Run-Ahead gate



Bucket: A.2 · Release: R1 · Status: gated



Harness: emu-bench.py::run\_ahead\_cost, ra-bench.py · Rationale: A.44



Criterion. Per-game run\_ahead\_frames setting (0 = off, 1–2 = 1–2 frames). Implementation in emulator cores (ares, RetroArch, GroovyMAME, Azahar 2127.0). Frontend exposes config + save-state ring. If input-to-display latency reduction is ≥16 ms (median) with no visible artifacting on scripted platformer test, ship. Otherwise, default run\_ahead\_frames = 0.



Fallback. Default run\_ahead\_frames = 0.



Notes. Cost budget and run-ahead vs FG exclusion are documented in A.74 — run-ahead and FG are mutually exclusive per emulator instance by default, and run-ahead at run\_ahead = 2 is marginal on the reference rig (\~6.2 ms per frame, of which \~137 µs is snapshot+restore). The save-state ring that implements run-ahead is sul-savestate (Architecture Ch. 3 §3.6, A.91), not the libretro-native save-state path.



A.66 · Cross-Platform Screen Sharing, Share-Play and Remote Controller (emulation debut)



Bucket: A.2 · Release: R1 · Status: gated



Harness: shareplay-bench.py · Rationale: A.66



Criterion. Emulation debut feature. Share the SUL emulated game to another device (tablet, laptop) with low-latency video + input passthrough. Over QUIC/WebTransport. Remote virtual gamepad passing over QUIC; remote user's gamepad inputs re-injected into the host via ViGEm. Bandwidth-optimized (\~100 bytes/frame). Session authentication via Ed25519 challenge-response. Decision criterion: under 50 ms end-to-end LAN latency; zero input corruption over 1000 sessions; ViGEm re-injection verified. If any gate fails, the feature falls back to host-only emulation (no share). If the gate passes, Share-Play ships as an emulation debut feature.



Fallback. Host-only emulation (no share).



Notes. Implementation paths: (a) integrate open-source starting points — Kyber (QUIC/WebTransport kit), punktfunk (QUIC + ViGEm + SendInput), LibreRemotePlay (P2P), stream-play-server (WebRTC + ViGEm); or (b) build a ground-up QUIC/WebTransport transport layer. Selective reliability (reliable control stream + unreliable video datagrams) targets ≤2 ms added control latency.



A.75 · sul-audio-governor



Bucket: A.2 · Release: R1 · Status: gated



Harness: audio-governor-bench.py::decision\_loop, audio-governor-bench.py::underruns, audio-governor-bench.py::exclusive\_accuracy · Rationale: A.75



Criterion. The arbiter over the Architecture Ch. 4 §4.2 audio pipeline. Ship if the six-decision loop completes in ≤15 ms p99, zero underruns across 100 scripted sessions, exclusive-mode recommendation accuracy ≥95%, and no silent buffer escalations above 1024 samples across the full test matrix.



Fallback. sul-audio-governor does not ship; sul-audio falls back to static config.



A.79 · sul-dsp artifact distribution



Bucket: A.2 · Release: R1 · Status: gated



Harness: audio-bench.py::dsp\_callback, audio-bench.py::dsp\_p99 · Rationale: A.79



Criterion. Each updated DSP artifact must be re-verified against its SHA-256 signed manifest and re-run through the A.2 audio gate. Failure on any re-verification blocks the artifact update but does not block the shipping release (the previous verified artifact continues to be served).



Fallback. Previous verified artifact continues to be served.



A.80 · sul-voice model distribution



Bucket: A.2 · Release: R1 · Status: gated



Harness: audio-bench.py::voice\_roundtrip, audio-bench.py::voice\_wer · Rationale: A.80



Criterion. Each updated voice model must be re-verified against its SHA-256 signed manifest and re-run through the A.2 voice gate. Failure blocks the model update but not the release.



Fallback. Previous verified model continues to be served.



A.81 · STT model distribution



Bucket: A.2 · Release: R1 · Status: gated



Harness: stt-bench.py::end\_to\_end, stt-bench.py::quant · Rationale: A.81



Criterion. Each updated STT model must be re-verified against its SHA-256 signed manifest and re-run through the A.83 gate. Failure blocks the model update but not the release.



Fallback. Previous verified model continues to be served.



A.83 · STT Optimization Pass



Bucket: A.2 · Release: R1 · Status: gated



Harness: stt-bench.py::end\_to\_end, stt-bench.py::quant, stt-bench.py::pipeline\_custom, stt-bench.py::audio\_ctx, stt-bench.py::flash\_attn, stt-bench.py::voice\_loop · Rationale: A.83



Criterion. The whisper.cpp STT path is subject to a three-rung optimization pass with independently measured targets.



Rung 1 — streaming wrapper (WhisperPipe pattern). Hybrid VAD (Silero + energy, coordinated with the dual-track ring's VAD so the two do not double-gate and no silent audio reaches the decoder), overlapping context windows, adaptive processing. Measured baseline for the wrapper on the reference rig: WhisperPipe reports 89 ms median end-to-end latency (142 ms p90), 48% less peak GPU memory, 80.9% lower average GPU utilization, WER within 2% of offline Whisper.



Rung 2 — quantization. ggml-q5\_1 at \~2.3× inference speedup with <2% precision loss; int8 as the alternative rung.



Rung 3 — streaming-native replacement. FastConformer-RNNT / Nemotron-class (ONNX or Candle, int4 k-quant) only if rungs 1–2 cannot hit the target. Reference measurements for rung 3: Nemotron 3 ASR reports 80 ms minimum chunk, first token in 80–160 ms, \~0.79 s end-to-end; the compact ONNX int4 k-quant version reports 8.20% average streaming WER at 0.56 s algorithmic latency and 0.67 GB model size.



Decision criterion. Shipping rungs 1+2 as default requires median end-to-end latency <150 ms and WER within 2% of offline Whisper on the SUL combat-speech eval set. Shipping rung 3 as default requires 0.56 s algorithmic latency at ≤8.5% WER on the same eval set; Whisper remains the fallback. Rung 3 is gated on the crate work (the Architecture Ch. 1 §1.19.5 sul-stt row is added only if rung 3 passes A.83). Whisper.cpp is not removed from the shipping release; it is the fallback for every rung.



Fallback. Rungs 1+2 ship as default; rung 3 is gated on crate work. Whisper.cpp remains the fallback.



Notes. Five pipeline-customization wins stack on every rung and do not require rebuilding whisper.cpp:



VAD-gate at the dual-track ring, strip silence before inference, split stereo channels — a call-centre ASR deployment on stock whisper.cpp + CUDA with this pipeline measured \~10× faster transcription (best case 12× realtime) and 58% audio reduction before inference;



explicit flash\_attn = true — Metal ships with it OFF by default in whisper-rs 0.16, CUDA/CPU benefit from being explicit, zero quality cost;



audio\_ctx = 768 for clips under 8 s — halves the encoder window relative to the 30 s default, \~2× faster for short clips, safe because the audio fits;



hybrid split (GPU encoder, CPU decoder) only if STT is GPU-offloaded — Snapdragon 8 Elite reference measured 16.5% faster than pure CPU, 21.0% faster than pure GPU, and eliminated thermal throttling;



model-tier + quantization selection — tiny.en q5\_1 for callouts, base.en q5\_1 for dialogue, small.en q5\_1 only when translation active.



These stack with the WhisperPipe wrapper. From-scratch rebuilds are explicitly not the performance lever and are ruled out of the shipping release: franken\_whisper (pure Rust, hand-tuned FrankenTorch kernels) is \~2.99× faster than whisper.cpp on large-v3-turbo but is a 12K-line project; candle-whisper (pure Rust, generic Candle kernels) is 1.5–3× slower than whisper.cpp. The Rust language is not the win — the kernels and the pipeline are. The architectural ceiling is Whisper's whole-segment design; the streaming-engine swap (rung 3) is the only lever for sub-second first-token latency.



A.87 · Voicemeeter-style routing matrix



Bucket: A.2 · Release: R1 · Status: gated



Harness: audio-routing-bench.py::matrix\_mix, audio-routing-bench.py::limiter, audio-routing-bench.py::roundtrip\_hardware, audio-routing-bench.py::roundtrip\_virtual, audio-routing-bench.py::drift · Rationale: A.87



Criterion. N input sources × M output buses, per-cell gain, per-bus limiter and meter. Inputs are the sources Architecture Ch. 4 §4.2 already captures; buses are local, stream, monitor, record. Hardware buses bypass virtual cables at the smallest period that fits the FIR; virtual buses use the cable's native period. SUL depends on the user-installed virtual cable (VB-Audio Cable is the de-facto standard; VoiceMeeter's own cable and equivalents are also detected) — SUL does not build a virtual audio driver.



Shipping the routing matrix requires:



(a) measured per-bus round-trip within 5 ms of the native WASAPI period on hardware buses,



(b) virtual bus round-trip within 5 ms of the virtual cable's documented native latency,



(c) zero sample-level drift between buses over 1 hour of continuous operation,



(d) the four-bus with-all-limiters DSP p99 measured under 5 ms on the reference rig's CPU.



Fallback. Failure on any gate blocks the shipping release of A.87 but does not affect the DSP pipeline in Architecture Ch. 4 §4.2. Building a SUL virtual audio driver is explicitly out of scope for A.87; that work is A.4 research.



A.88 · Voicemod-style voice emulation



Bucket: A.2 · Release: R1 · Status: gated



Harness: voice-fx-bench.py::combined, voice-fx-bench.py::wer\_delta, voice-fx-bench.py::voice\_loop\_delta, voice-fx-bench.py::preset\_perceptual · Rationale: A.88



Criterion. Pitch shift, formant shift, character presets, ring mod / bitcrush / light EQ — all classical DSP inside the sul-dsp graph, no GPU. Combined voice-FX DSP costs \~5–8 ms per 512-sample buffer on AVX2. Neural voice conversion (RVC, kNN-VC) is gated under the same A.88 with a separate criterion.



Shipping the classical-DSP features requires:



(a) combined FX DSP p99 under 8 ms on the reference rig at 512-sample / 48 kHz,



(b) no measurable increase in STT WER for the voice loop (A.85) when FX is active,



(c) no measurable increase in total voice-loop end-to-end latency (A.85 target <500 ms),



(d) character presets validated against a scripted perceptual test (10 listeners, ≥80% agreement that the preset matches its name).



Neural voice conversion ships under the same A.88 only if it hits ≤30 ms on CPU with a small model (no GPU offload) and preserves WER; if it cannot, it remains research (A.89).



Fallback. Classical DSP features ship; neural VC stays A.89 research.



Notes. Voice cloning of a specific real person is a non-negotiable refusal — it is not in A.88, not in any gated path, and not in A.4.



A.90 · sul-gb Game Boy / GBC core



Bucket: A.2 · Release: R1 · Status: gated



Harness: gb-bench.py::accuracy, gb-bench.py::frame\_time, gb-bench.py::allocations, gb-bench.py::savestate\_round\_trip · Rationale: A.90



Criterion. Purpose-built Rust-native GB / GBC core, alternative to the libretro Gambatte core. Ships when the accuracy gate passes; Gambatte remains the fallback for any title where sul-gb fails. This is the only emulator in SUL built from scratch — every other system is integrated via sul-emu-libretro or the native adapters.



Shipping sul-gb requires:



(a) Blargg cpu\_instrs 11/11,



(b) Blargg instr\_timing and mem\_timing all pass,



(c) dmg-acid2 and cgb-acid2 pixel-exact,



(d) MBC1/2/3/5 supported with MBC3 RTC accuracy within 1 s over 24 h,



(e) APU spectrum within ±1 dB of gambatte reference on a scripted ROM set across 20 Hz – 20 kHz,



(f) top-50 GB/GBC title compatibility list runs clean,



(g) per-frame time median <2 ms / p99 <2.5 ms on reference rig at 60 fps,



(h) zero allocations per frame in the hot loop,



(i) savestate snapshot + restore <20 µs each.



Fallback. Gambatte remains the default. sul-gb sits alongside it as a user-selectable core.



A.91 · sul-savestate ring



Bucket: A.2 · Release: R1 · Status: gated



Harness: savestate-bench.py::ps2, savestate-bench.py::allocations, savestate-bench.py::compression · Rationale: A.91



Criterion. Rust-native save-state ring and run-ahead subsystem shared by sul-gb (native) and sul-emu-libretro (wrapping retro\_serialize/retro\_unserialize). Ships when correctness and cost gates pass.



Shipping sul-savestate requires:



(a) round-trip byte-identical after 100 iterations on every supported core,



(b) snapshot / restore cost within budget for GB (\~32 KB, <20 µs) and PS2 (\~35 MB, <500 µs),



(c) run-ahead at run\_ahead = 2 snapshot+restore p99 <200 µs,



(d) zero allocations in the hot path,



(e) worker thread never blocks the emu thread on snapshot or restore,



(f) changed-blocks-only compression achieves ≥3× on repeated snapshots.



Fallback. Failure on any of (a)–(f) leaves run-ahead on the libretro-native save-state path (which the C-ABI audit in R.6 documents as having UB risks — the shipping decision is whether to accept those risks or ship the Rust-native ring; the ring is preferred).



A.116 · Performance toggle system



Bucket: A.2 · Release: R1 · Status: gated



Harness: cargo xtask config-check · Rationale: A.116



Criterion. Every optimization exposed in \[performance.\*], all shipped optimizations default on, gated toggles fall back through the fallback chain if the gate hasn't passed. Ship if the performance toggle registry covers every optimization and the music/broadcast optimization groups, and if enabling an incompatible toggle triggers a confirmation modal.



Fallback. Performance toggle UI does not ship; tuned config still ships.



A.119 · Music player bit-perfect path



Bucket: A.2 · Release: R1 · Status: gated



Harness: music-bench.py::bit\_perfect\_loopback · Rationale: A.119



Criterion. Verified with a loopback capture comparing input file bytes to device output bytes when no DSP active.



Fallback. Bit-perfect badge hidden; shared mode used.



A.120 · Music player gapless



Bucket: A.2 · Release: R1 · Status: gated



Harness: music-bench.py::gapless · Rationale: A.120



Criterion. FLAC, MP3 (LAME tag), OGG, WAV. No audible gap across track boundaries on a curated test set. Measured with a sample-accurate capture.



Fallback. Gapless disabled; standard track-end silence.



A.121 · Music player CPU overhead



Bucket: A.2 · Release: R1 · Status: gated



Harness: music-bench.py::cpu\_overhead · Rationale: A.121



Criterion. <1% playing (no DSP), <3% with EQ, <5% with EQ + ReplayGain + resampling.



Fallback. Higher overhead accepted as a known limitation; user is warned.



A.122 · Music player latency



Bucket: A.2 · Release: R1 · Status: gated



Harness: music-bench.py::latency · Rationale: A.122



Criterion. Exclusive mode buffer ≤256 samples (≤5.8 ms at 48 kHz), seek latency <50 ms, track-start latency <100 ms.



Fallback. Higher latency accepted as a known limitation; user is warned.



A.123 · Musicolet feature parity



Bucket: A.2 · Release: R1 · Status: gated



Harness: manual verification against Musicolet feature list · Rationale: A.123



Criterion. Every feature in the table in Architecture Ch. 5 §5.1 present and working.



Fallback. Missing features are marked as missing in the UI.



A.124 · Music player device coordination



Bucket: A.2 · Release: R1 · Status: gated



Harness: music-bench.py::device\_coordination · Rationale: A.124



Criterion. Exclusive mode correctly refused when it would conflict with sul-audio loopback or voice loop; correctly allowed on a different device; correctly falls back to shared mode with warning.



Fallback. Exclusive mode disabled.



A.126 · Encoder admission controller



Bucket: A.2 · Release: R1 · Status: gated



Harness: broadcast-bench.py::admission\_controller · Rationale: A.126



Criterion. Computes pixel budget from actual card throughput, downgrades ISOs first, refuses when over 100%, specific refusal message. Verified on single-NVENC, dual-NVENC, AMF, and QSV cards.



Fallback. Broadcast refuses to start with E\_BROADCAST\_ADMISSION\_REFUSED.



A.127 · Zero-CPU-readback encode path



Bucket: A.2 · Release: R1 · Status: gated



Harness: broadcast-bench.py::zero\_cpu\_readback · Rationale: A.127



Criterion. Compositor texture → encoder surface with no copy\_to\_host. Verified by a GPU trace showing no readback calls in the encode path.



Fallback. Fallback to readback path (higher CPU).



A.128 · Per-destination audio mixes



Bucket: A.2 · Release: R1 · Status: gated



Harness: broadcast-bench.py::per\_destination\_mixes · Rationale: A.128



Criterion. Music on Stream and Record, off VOD; Twitch output receives Stream mix, VOD output receives VOD mix; null-test verifying the two mixes differ by exactly the music content.



Fallback. Single mix for all destinations.



A.130 · Per-application capture



Bucket: A.2 · Release: R1 · Status: gated



Harness: broadcast-bench.py::per\_app\_capture · Rationale: A.130



Criterion. WASAPI process loopback creates a channel per process, pause/resume works, fallback to virtual cable works when process loopback is unavailable.



Fallback. Virtual cable (B1/B2) fallback.



A.133 · ISO recording



Bucket: A.2 · Release: R1 · Status: gated



Harness: broadcast-bench.py::iso\_recording · Rationale: A.133



Criterion. Every video source and every audio bus/channel written to its own file, sample-aligned on one clock, importable into Resolve/Premiere with tracks aligned.



Fallback. ISO recording disabled; program output only.



A.134 · Audio performance



Bucket: A.2 · Release: R1 · Status: gated



Harness: broadcast-bench.py::audio\_engine\_cpu, broadcast-bench.py::audio\_underruns · Rationale: A.134



Criterion. 12 channels × 12 buses with all strip FX on runs at <6% of one core, zero underruns over 60 minutes, allocation-free audio callback.



Fallback. Strip FX downgraded; channels or buses reduced.



A.162 · Higgs Audio TTS engine



Bucket: A.2 · Release: R1 · Status: gated · Blocking: no



Harness: tts-bench.py::higgs\_audio · Rationale: A.162



Criterion. Higgs Audio (boson.ai/higgs-audio) ships as a third `\[tts] engine` option if (a) it meets A.88's voice-loop latency gate (<500 ms end-to-end), (b) it meets A.85's TTS sub-gate (<150 ms Piper-equivalent latency), (c) it runs without GPU offload, or with GPU offload that keeps the total SUL VRAM footprint within the idle ceiling (Brief §7.5).



Fallback. Piper remains the default; Higgs Audio stays a `\[DEPRECATED]` evaluation.



A.164 · Jellyfin music library source



Bucket: A.2 · Release: R1 · Status: gated · Blocking: no



Harness: music-bench.py::jellyfin · Rationale: A.164



Criterion. Jellyfin (jellyfin/jellyfin) ships as an optional `\[music.library.jellyfin]` source if library indexing, playback URL resolution, and search all work for a scripted 100-track library. API key stored in the vault; server URL declared as a local network surface.



Fallback. `\[music.library] library\_paths` remains the only source.



A.165 · Nextcloud pack sync backend



Bucket: A.2 · Release: R1 · Status: gated · Blocking: no



Harness: pack-sync-bench.py::nextcloud · Rationale: A.165



Criterion. Nextcloud (nextcloud/server) ships as an optional `\[packs.sync] backend = "nextcloud"` if git and Nextcloud both round-trip a scripted 20-pack set (pack files, signatures, version metadata) without loss. WebDAV API.



Fallback. git remains the only backend.



A.166 · LibreTranslate backend



Bucket: A.2 · Release: R1 · Status: gated · Blocking: no



Harness: translate-bench.py::libretranslate · Rationale: A.166



Criterion. LibreTranslate (LibreTranslate/LibreTranslate) ships as an optional `\[translation] backend = "libretranslate"` if it matches whisper.cpp's translation quality on a scripted 50-sentence set and runs locally (self-hosted or local process) with ≤200 ms per-sentence latency.



Fallback. whisper.cpp remains the only backend.



A.169 · LocalSend file transfer tool



Bucket: A.2 · Release: R1 · Status: gated · Blocking: no



Harness: localsend-bench.py::roundtrip · Rationale: A.169



Criterion. LocalSend (localsend/localsend) ships as an agent tool `send\_file(path, destination\_device)` if a scripted file transfer round-trips without corruption, the LAN-only network surface is declared (Ch. 16 §16.15.14), and the tool is gated by `\[features] agent.tool.send\_file` + Tier 1.



Fallback. Tool stays off.



A.170 · Containerized image/PDF tools (GIMP + Stirling PDF)



Bucket: A.2 · Release: R1 · Status: gated · Blocking: no



Harness: container-tools-bench.py::gimp, container-tools-bench.py::stirling\_pdf · Rationale: A.170



Criterion. Docker GIMP (linuxserver/docker-gimp) and Stirling PDF (Stirling-Tools/stirling-pdf) ship as agent tools `edit\_image` and `edit\_pdf` / `merge\_pdf` / `split\_pdf` if (a) the container lifecycle matches the existing security-toolkit pattern, (b) the container has no network, (c) the container has read-only filesystem except for the input, (d) the tools complete a scripted operation set (resize, crop, merge, split, convert) with no host access.



Fallback. Tools stay off. The agent falls back to text-only operations.



A.174 · Pexels stock photo tool



Bucket: A.2 · Release: R1 · Status: gated · Blocking: no



Harness: pexels-bench.py::query · Rationale: A.174



Criterion. Pexels (pexels.com) ships as an agent tool `find\_photo(query)` if the API returns usable results for a scripted 20-query set, the license terms permit use, and the network surface is declared (Ch. 16 §16.15.14).



Fallback. Tool stays off.



A.177 · HaveIBeenPwned breach check tool



Bucket: A.2 · Release: R1 · Status: gated · Blocking: no



Harness: hibp-bench.py::check · Rationale: A.177



Criterion. HaveIBeenPwned (haveibeenpwned.com) ships as an agent tool `check\_breach(email)` if the API returns correct results for a scripted 10-email set, the API terms permit use, and the network surface is declared (Ch. 16 §16.15.14).



Fallback. Tool stays off.



A.188 · Agent user tasks SQLite store (handoff A.164)

Bucket: A.2 · Release: R1 · Status: gated · Blocking: no

Harness: agent-bench.py::user_tasks · Rationale: A.188

Criterion. Local SQLite task table (`[agent.user_tasks]`) supports create/complete/list with write latency <5 ms and encrypted `sulpack` notes via Vault AEAD primitives.

Fallback. User tasks disabled; agent answers reminders ephemerally without persistence.

A.190 · Filee offline format conversion (handoff A.166)

Bucket: A.2 · Release: R1 · Status: gated · Blocking: no

Harness: vision-bench.py::file_converter · Rationale: A.190

Criterion. Offline MIME-triggered conversion (`[vision.preprocess] file_converter = "filee"`) achieves ≥90% success rate on HWP and legacy DOC/Office inputs before OCR/VLM ingestion.

Fallback. Unsupported MIME types rejected with a clear user message; native PDF/PNG path unchanged.

A.191 · Local multi-user auth profiles (handoff A.167)

Bucket: A.2 · Release: R1 · Status: gated · Blocking: no

Harness: cargo test -p sul-auth-app -- multi_profile · Rationale: A.191

Criterion. `[auth.profiles]` isolates per-profile vault keys and `game_metadata` locally (no cloud sync). Profile A must never be able to decrypt or read Profile B's vault key or metadata. WebAuthn/passkey UX maps to Windows Hello + AND-logic biometrics.

Fallback. Single-profile auth mode remains default.

R2 — Deep technical release

A.63-Conversation · Combat Full Conversation profile quality gate



Bucket: A.2 · Release: R2 · Status: gated · Blocking: no



Harness: combat\_full\_bench.py::conversation\_quality, combat\_full\_bench.py::conversation\_multi\_turn, combat\_full\_bench.py::conversation\_kv\_reuse · Rationale: A.63



Criterion. Conversation profile turn quality ≥ threshold (first run records the threshold; expected mean rubric ≥75%); Conversation multi-turn accuracy; KV reuse rate ≥90% on full-history reuse. The Conversation profile must not emit tool-call-shaped output for non-tool turns.



Fallback. If Conversation quality < threshold, the profile stays research; R2 ships with `ai.combat\_full\_conversation` off. No VRAM or safety impact.



Notes. Conversation is R2 because it is the no-game open-ended dialogue profile and depends on the search pipeline (R1) and full-history KV reuse (R2).





A.20 · ChainFlow latency gate (Deep)



Bucket: A.2 · Release: R2 · Status: gated



Harness: spec-bench.py::chainflow, mode-switch-bench.py · Rationale: A.20



Criterion. ChainFlow requires 1–2 flow steps. Measure latency overhead vs speedup gain on Deep. If ChainFlow speedup (1.6–1.9× target) holds on reference rig with ≤20% added latency vs LiLiCorr, ship as Deep's first rung. Otherwise, fall back to LiLiCorr as first rung.



Fallback. LiLiCorr as first rung.



A.22 · sul-wire JSON fallback deprecation



Bucket: A.2 · Release: R2 · Status: gated



Harness: wire-bench.py, release audit · Rationale: A.22



Criterion. sul-wire ships with binary-v1 and json fallback. Deprecate json fallback once binary path is stable for two release cycles. If binary path has zero frame-corruption incidents over 2 release cycles, deprecate json fallback. Otherwise, keep both.



Fallback. Keep both (binary-v1 + json).



A.42 · NVENC zero-copy transcode gate



Bucket: A.2 · Release: R2 · Status: gated



Harness: clip-bench.py::nvenc\_zero\_copy · Rationale: A.42



Criterion. CUarray-shared transcode path per NVENC SDK 13.1 eliminates GPU→CPU→GPU round trip. AV1 B-frames up to 31. If transcode throughput improvement is ≥20% vs previous path on reference rig, ship. Otherwise, keep async NVENC encode.



Fallback. Async NVENC encode.



A.43 · zstd\_gpu decompression gate



Bucket: A.2 · Release: R2 · Status: gated



Harness: opt-bench.py::zstd\_gpu · Rationale: A.43



Criterion. DirectStorage 1.4 Zstd GPU decompression shader for 256 KB chunks. If GPU decompression reduces CPU overhead by ≥30% with no VRAM regression, ship. Otherwise, keep CPU Zstd.



Fallback. CPU Zstd.



A.47 · planner\_split gate



Bucket: A.2 · Release: R2 · Status: gated



Harness: agent-bench.py::planner\_split · Rationale: A.47



Criterion. Small-planner / big-executor for Deep. Planner runs on Combat Full Instruct or Combat Full Conversation (depending on load state); executor runs on Deep. \~30% token reduction measured. If planner\_split uses ≥25% fewer tokens than same-tier planning with no quality loss on 50 SUL-shaped agentic tasks, ship. Otherwise, keep same-tier planning.



Fallback. Same-tier planning.



Notes. Planner tier selection is profile-aware: Combat Full Instruct handles 2–3-step planning, Combat Full Thinking handles 4–5-step planning, Combat Full Conversation handles 3–4-step dialogue-with-tools, Combat Full Coding handles 3–4-step code (A.101 gated), Deep handles 5+ step agentic tasks.



A.55 · Coca-Cola latent compression



Bucket: A.2 · Release: R2 · Status: gated



Harness: search-bench.py::coca\_cola · Rationale: A.55



Criterion. Train small compression model on SUL's search-output format. Compresses tool observations into latent tokens instead of text, preserving only decision-critical information. 70–89% token reduction with higher accuracy on web-search QA benchmarks. If 70–89% token reduction with ≥95% answer accuracy retention on the SUL search eval set, ship. Otherwise, keep Exa-style excerpts only.



Fallback. Exa-style excerpts only.



Notes. Dependency: requires the search pipeline (Architecture Ch. 8 §8.6) to be stable and producing consistent output format for training.



A.56 · InternViT W8A8 quantization



Bucket: A.2 · Release: R2 · Status: gated



Harness: vision-bench.py::internvit\_w8a8 · Rationale: A.56



Criterion. InternViT-300M W8A8 quantization via fused CUDA kernels. Complementary to ReTrace: W8A8 reduces the cost of the initial vision encoding pass, ReTrace reduces the cost of storing and attending to encoded tokens. No more than 1% grounding accuracy drop on ScreenSpot. If passes, integrate into clip\_graph\_internvl build path.



Fallback. Keep InternViT at f16.



Notes. Applies to all Combat Full profiles — the vision encoder runs identically for all profiles. Dependency: requires shipping of Combat Full with ReTrace to establish baseline grounding accuracy.



A.58 · Search pipeline verification caching



Bucket: A.2 · Release: R2 · Status: gated



Harness: search-bench.py::verification\_cache · Rationale: A.58



Criterion. Cache verification packets by claim hash. Invalidates on source volatility (TTL by source tier). Decision criterion: >80% cache hit rate on repeated queries; zero stale results from Tier-1 sources (higher TTL strictness for primary sources than for secondary).



Fallback. No verification cache.



Notes. Dependency: requires the search pipeline (Architecture Ch. 8 §8.6) to be stable and producing consistent verification packet schema.



A.59 · OODA-Tool on non-combat tiers



Bucket: A.2 · Release: R2 · Status: gated



Harness: ooda-bench.py::multi\_turn\_gain · Rationale: A.59



Criterion. Apply OODA-Tool scaffold to Deep. Currently applied to all Combat Full profiles. Decision criterion: multi-turn accuracy improvement ≥10% with ≤5% token overhead on Deep task set.



Fallback. OODA-Tool stays combat-only.



Notes. Dependency: requires combat-tier OODA-Tool to be stable and measured.



A.60 · TSCG schema compilation expansion



Bucket: A.2 · Release: R2 · Status: gated



Harness: tscg-bench.py::compression, tscg-bench.py::accuracy\_retention · Rationale: A.60



Criterion. Extend TSCG schema compilation to all tool schemas, not just combat. Currently applied to all Combat Full profiles. Decision criterion: ≥51% compression with ≥95% tool-call accuracy retention across Deep.



Fallback. TSCG stays combat-only.



Notes. Dependency: requires combat-tier TSCG to be stable and measured.



A.61 · WASAPI exclusive-mode auto-detect



Bucket: A.2 · Release: R2 · Status: gated



Harness: audio-governor-bench.py::exclusive\_accuracy · Rationale: A.61



Criterion. Detect whether WASAPI exclusive mode is beneficial based on audio pipeline load. Currently opt-in per session by user. Decision criterion: latency reduction >20% with zero device seizure errors across 100 sessions. If auto-detect would cause seizure events (other applications losing audio), never auto-enable — only recommend.



Fallback. Recommend only; never auto-enable.



Notes. Dependency: requires WASAPI exclusive mode to be stable in the shipping release.



A.62 · ReTrace on sul-vlm scene backends



Bucket: A.2 · Release: R2 · Status: gated



Harness: vision-bench.py::retrace\_on\_vlm · Rationale: A.62



Criterion. Extend ReTrace to sul-vlm MiniCPM-V, LFM2.5-VL, jina-vlm. Currently ReTrace only applies to InternVL3-2B (Combat Full, all profiles). Decision criterion: ≥85% benchmark retention at 20% token retention on each backend's reference benchmark. If passes, integrate.



Fallback. sul-vlm runs at full-token baseline.



Notes. Dependency: requires A.54 (ReTrace on Combat Full) to ship first to establish pattern.



A.64 · Verification packet fallacy detection expansion



Bucket: A.2 · Release: R2 · Status: gated



Harness: search-bench.py::fallacy · Rationale: A.64



Criterion. Extend beyond the five deterministic patterns (false dilemma, appeal to authority, hasty generalization, circular reasoning, temporal inconsistency). The expansion would add: equivocation, straw man, post hoc, ad hominem. Decision criterion: precision ≥90%, recall ≥80% on labeled test set for each new pattern before inclusion. Patterns below threshold are not added.



Fallback. Five-pattern detection only.



Notes. Dependency: requires the base verification packet builder (Architecture Ch. 8 §8.8) to be stable.



A.72 · lsfg\_native backend



Bucket: A.2 · Release: R2 · Status: gated



Harness: fg-bench.py::frame\_corruption, fg-bench.py::latency\_added · Rationale: A.72



Criterion. In-tree Vulkan compute pipeline. vkQueuePresentKHR hook, optical-flow interpolation, SPIR-V compiled from source. Ship if the native backend achieves ≥90% of the vendored lsfg\_layer framerate uplift on the reference rig, zero frame corruption across 10,000 presented frames on each of {ED via DXVK, PCSX2, Dolphin, RetroArch Vulkan}, and full VRAM reservation enforcement. Otherwise, ship lsfg\_layer as the default (if the licensing boundary is resolved) and treat lsfg\_native as a follow-up.



Fallback. lsfg\_layer as default (if licensing resolved); otherwise win\_fg.



Notes. Dependency: Lossless.dll user-supplied.



A.73 · win\_fg clean-room backend



Bucket: A.2 · Release: R2 · Status: gated



Harness: fg-bench.py::frame\_corruption · Rationale: A.73



Criterion. FSR3 optical flow + from-scratch synthesis. No Lossless.dll required. Fully redistributable. Ship if the clean-room backend produces no visible ghosting on a scripted 30 fps → 60 fps test, no proprietary weights are shipped in the binary, and the base-framerate gate is enforced identically to the native backend. Otherwise, hold the release of win\_fg and ship with only the Lossless.dll-dependent backends.



Fallback. Hold win\_fg; ship with only the Lossless.dll-dependent backends.



A.74 · Emulator FG compatibility (per emulator, independently gated)



Bucket: A.2 · Release: R2 · Status: gated



Harness: fg-bench.py::emulator\_native\_fps, fg-bench.py::emulator\_launch · Rationale: A.74



Criterion. Frame generation in each supported emulator's Vulkan instance. Per-emulator gates: PCSX2, Dolphin, RPCS3, DuckStation, RetroArch, Ryujinx forks, Cemu, Flycast, Ares, Azahar. For each emulator, ship if (a) native-fps discovery accuracy is 100% on a 20-title test set, (b) the layer produces no crash or hang on the emulator's Vulkan path across 100 launches, and (c) the run-ahead exclusion fires correctly. Failure on any emulator excludes only that emulator, not the whole feature.



Fallback. Excludes only the failing emulator.



Notes. Run-ahead cost budget (cross-ref A.44). Run-ahead at run\_ahead = 2 costs \~6.2 ms per displayed frame, of which snapshot + restore is \~137 µs — 2.2% of the cost. The rest is the actual emulation of the extra frames. Run-ahead at run\_ahead = 1 is comfortably within budget on the reference rig; run\_ahead = 2 is marginal and should be measured per-emulator before defaulting. The save-state ring is Rust-native, not libretro-bound: the libretro C-ABI boundary has documented UB risks (aliasing violations in retro\_unserialize, data races from unsynchronized worker-thread queries, NUL-termination hazards in disk-control callbacks, repeated-retro\_init() panics), and the Rust-native ring avoids all of them. The ring owns its own buffers, compresses with zstd, and uses a single savestate worker thread with a FIFO job queue — the emu thread keeps only the snapshot and the apply; compression and file I/O are on the worker. This is the PPSSPP async-savestate pattern applied to the emulator-agnostic ring. Measured: a full PSP save state is \~35–70 MB uncompressed; zstd at default level compresses the changed blocks \~3×, so the ring takes about a third of the memory. SUL's ring targets the same shape: the ring is sized to the emulator's snapshot size, not to a fixed slot count.



A.76 · optimize + fg co-scheduling



Bucket: A.2 · Release: R2 · Status: gated



Harness: fg-bench.py::co\_schedule · Rationale: A.76



Criterion. The combined state where texture downscale and frame generation are simultaneously active, with the governor managing VRAM. Ship if optimize + fg produces ≥1.2× effective framerate at ≤same VRAM as native textures with FG off, with no frame corruption, on a 10-game test set (ED, 3 emulators, 6 native single-player titles).



Fallback. optimize only (no fg) or fg only (no optimize).



A.78 · libretro Vulkan frontend for FG



Bucket: A.2 · Release: R2 · Status: gated



Harness: libretro-bench.py::vulkan\_frontend · Rationale: A.78



Criterion. The current sul-emu-libretro frontend uses glMapBufferRange for persistent-mapped texture upload — an OpenGL path. A Vulkan variant of the frontend would let libretro cores (which may or may not be Vulkan-aware themselves) route presentation through Vulkan, so the FG layer could attach. This is a larger change than per-emulator env injection: it requires building a Vulkan rendering path in the libretro frontend, not just setting env vars. Decision criterion: if the Vulkan frontend achieves ≥95% of the current OpenGL frontend's p99 frame time and zero visible artifacts across the full libretro core matrix, integrate and re-evaluate A.74 for the libretro path. Otherwise, retain OpenGL for the frontend and let libretro users who want FG use RetroArch directly (which already has a Vulkan driver).



Fallback. OpenGL frontend; libretro users who want FG use RetroArch directly.



Notes. Dependency: only needed for melonDS and Xenia workarounds and for libretro cores that don't reach Vulkan through RetroArch. melonDS and Xenia are tracked separately in Architecture Ch. 6 §6.9.



A.82 · Profile router training data expansion



Bucket: A.2 · Release: R2 · Status: gated



Harness: router-bench.py::mode\_selection, combat\_full\_bench.py::\* · Rationale: A.82



Criterion. Expanded router training data (SUL-shaped turns) must improve router profile-selection accuracy without regressing any A.63 gate. Ship if router accuracy on 200 SUL-shaped turns improves ≥2 points with no per-profile BFCL regression.



Fallback. Keep current training data.



A.85 · JARVIS voice loop for Combat Full



Bucket: A.2 · Release: R2 · Status: gated



Harness: stt-bench.py::voice\_loop · Rationale: A.85



Criterion. The voice-loop architecture treats STT and TTS as CPU-side satellites with Combat Full owning the GPU. STT: whisper.cpp (per A.83 rungs 1+2), pinned thread affinity, VAD-gated at the dual-track ring. Reasoning: Combat Full Instruct (fast path). TTS: Piper on CPU, zero VRAM, \~150 ms synthesis latency on desktop CPU; Kokoro int8 CPU as the alternative when prosody quality outweighs the last few milliseconds. First-sentence streaming: begin TTS synthesis on the first completed sentence of Combat Full output, flushing at 300 ms max, for a 0.3–0.7 s perceived-latency reduction.



Decision criterion: ship the voice loop when median end-to-end latency (user stops speaking → first TTS audio out) is <500 ms with Combat Full Instruct as the reasoning backend, and WER on the SUL combat-speech eval set remains within 2% of the A.83 baseline.



Fallback. Independent STT (subtitles) and TTS without the loop wiring if the <500 ms gate cannot be met. Sub-components STT (A.83) and TTS (Piper or Kokoro) can ship separately; only the full voice loop is gated on A.85.



Notes. TTS never runs on the GPU in the shipping voice loop — the VRAM belongs to Combat Full, and the measured latency gap between GPU TTS (Breeze TTS 2 on RTX 4060: 220–472 ms first packet) and CPU Piper (\~150 ms on M2-class, \~1,220 ms on Raspberry Pi) is not worth the VRAM cost.



Data flow: mic → dual-track ring (sul-voice isolation if enabled) → STT → Combat Full Instruct → TTS → speaker/stream. No new audio stream; no new VRAM allocation beyond Combat Full's existing footprint.



A.94 · APEX-style mixed-precision GGUF for InternVL3-2B



Bucket: A.2 · Release: R2 · Status: gated



Harness: quant-bench.py::apex\_planner · Rationale: A.94



Criterion. Q6\_K edges / Q5\_K attention / Q4\_K middle FFN, calibrated on SUL tool-call traces via imatrix. Runs on standard llama.cpp (each layer is a standard K-quant type). Gate: A.63 per-profile gates hold. This is part of A.96's APEX planner scope; A.94 is the specific research entry for the APEX-planned quantization variants beyond the standard SPEED-Q 4-bit and 2-bit.



Fallback. SPEED-Q 4-bit uniform.



A.95 · Trellis-coded KV quantization (TCQ) as production tier



Bucket: A.2 · Release: R2 · Status: gated



Harness: kv-compress-bench.py::tcq · Rationale: A.95



Criterion. TCQ combines Fast Walsh-Hadamard rotation with Viterbi-optimal trellis encoding. Measured results show 3.25 bpv produces lower perplexity than uncompressed FP16 KV (5.802 vs 5.805 at 2K) due to norm-scaling regularisation, and 4.25 bpv matches FP16 quality at half the VRAM of q8\_0. Blocker: buun-llama-cpp fork, \~15% Viterbi encode tax, not in mainline. Decision criterion: promote TCQ from research to shipping tier when the mainline merge lands (or when the Viterbi encode tax drops below 5% on the reference rig). Until then, TCQ is a sul-kv-compress research backend gated behind A.97.



Fallback. TCQ remains a research backend in sul-kv-compress.



A.96 · sul-quant SPEED-Q / APEX



Bucket: A.2 · Release: R2 · Status: gated



Harness: quant-bench.py::speed\_q\_4bit, quant-bench.py::speed\_q\_2bit, quant-bench.py::apex\_planner · Rationale: A.96



Criterion. SPEED-Q 4-bit brings InternVL3-2B to 1,454.6 MB (\~63% reduction). SPEED-Q 2-bit brings it to 1,218.7 MB (\~69%, 6× higher accuracy than prior 2-bit). APEX-style layer-wise precision planner outputs a mixed-precision K-quant GGUF that runs on standard llama.cpp (no fork). Ship 4-bit when A.63 per-profile gates hold. Ship 2-bit when A.63 per-profile gates hold within 2% absolute. Ship APEX when A.63 per-profile gates hold on the mixed-precision GGUF and the GGUF loads on unmodified llama.cpp.



Fallback. llama.cpp Q4\_K\_S.



A.97 · sul-kv-compress backends



Bucket: A.2 · Release: R2 · Status: gated



Harness: kv-compress-bench.py::palquant, kv-compress-bench.py::puzzlekv, kv-compress-bench.py::jolt, kv-compress-bench.py::turbo3, kv-compress-bench.py::tcq · Rationale: A.97



Criterion. Each alternative KV compressor (PalQuant, PuzzleKV, JoLT, TurboQuant+ turbo3, TCQ) ships when it holds A.18's PPL delta <2% gate AND KL@D below the measured f16 baseline at its target compression ratio, AND passes the multi-session identity-preservation test (PalQuant's test — critical for the Entity Ledger). STAR-KV and GAAC ship as part of the base stack (subject to A.18).



Fallback. Failure on any gate hides the backend from the user preset; the fallback chain takes over silently but logs E\_KV\_COMPRESS\_BACKEND\_UNAVAILABLE.



A.98 · sul-spec-draft RIFT wrapper



Bucket: A.2 · Release: R2 · Status: gated



Harness: spec-bench.py::rift · Rationale: A.98



Criterion. Ship when DFlash acceptance retention is ≥95% and ChainFlow retention is ≥95% at the configured draft KV budget, and when the wrapper adds ≤5 ms per refresh step. Does not require the paper's 98% figure (that depends on compressibility training SUL is not building). Dormant until draft cache >32 MB.



Fallback. RIFT stays dormant.



A.99 · Medusa-1 + phrase head



Bucket: A.2 · Release: R2 · Status: gated



Harness: spec-bench.py::medusa, spec-bench.py::phrase · Rationale: A.99



Criterion. Medusa-1 (shared-LM-head, frozen backbone) + phrase-level head. Ship when: acceptance rate ≥0.50 median (Medusa) / ≥0.45 median (phrase), +15% tok/s aggregate vs UniSpec-first chain, byte-identical BFCL/router/recall outputs to baseline on 200-prompt test set, head VRAM ≤50 MB (Medusa) / ≤5 MB (phrase), temperature scaling applied, own-LM-head variant rejected by validator.



Fallback. UniSpec-first chain.



A.100 · sul-toolprm cross-encoder



Bucket: A.2 · Release: R2 · Status: gated



Harness: toolprm-bench.py::ranking\_accuracy, toolprm-bench.py::end\_to\_end\_bfcl, toolprm-bench.py::cache\_hit\_rate · Rationale: A.100



Criterion. Ship when:



(a) cross-encoder ranking accuracy on TRBench-BFCL is within 2 pp of the ToolRM-4B teacher's pairwise accuracy,



(b) end-to-end tool-call accuracy on BFCL non-live + live improves ≥3 pp over no-PRM baseline,



(c) cross-encoder VRAM at Q4 ≤200 MB,



(d) forward-pass latency ≤15 ms,



(e) score-cache hit rate ≥40% on a 200-turn session,



(f) coarse-to-fine produces ≥95% of full-step-scoring tool-call accuracy,



(g) on HD2, Combat Full (SPEED-Q 4-bit) + cross-encoder + OCR fits within 3 GB with ≥200 MB headroom.



Fallback. no-PRM baseline (or ToolRM-4B on ED only).



A.106 · Target-BPW Deep GGUF at 5.4–5.6 GiB



Bucket: A.2 · Release: R2 · Status: gated



Harness: quant-bench.py::deep\_target\_bpw · Rationale: A.106



Criterion. sul-quant target-BPW build at 5.5 GiB with SUL imatrix. Ship if ΔPPL is between Q5\_K\_M's +0.9% and Q4\_K\_M's +2.0% (target +1.2–1.4%) and the VRAM saving is ≥0.4 GiB.



Fallback. Q5\_K\_M.



A.107 · Layer-wise APEX Deep GGUF



Bucket: A.2 · Release: R2 · Status: gated



Harness: quant-bench.py::deep\_apex · Rationale: A.107



Criterion. APEX-style layer-wise precision (Q6\_K edges, Q5\_K attention/embeddings, Q4\_K middle FFN) at 5.4–5.6 GiB. Ship if it holds Q5\_K\_M's quality (94.1% top-1) at the reduced size.



Fallback. Target-BPW or Q5\_K\_M.



A.108 · Deep post-training benchmark lift



Bucket: A.2 · Release: R2 · Status: gated



Harness: deep-bench.py::post\_training · Rationale: A.108



Criterion. LoRA SFT + FTPO + RL on agentic coding. Ship if the aggregate benchmark lift is at least +7 points over baseline at unchanged VRAM.



Fallback. Deep stays at baseline quality; optimizations are still shipped (they are quality-first and do not regress).



A.109 · GRASPrune 40–50% Deep checkpoint



Bucket: A.2 · Release: R2 · Status: gated



Harness: deep-bench.py::grasprune · Rationale: A.109



Criterion. If 5.5 GiB still too tight, prune to 50% parameter retention (\~3.0–3.5 GiB) then quantize to \~5.0 GiB. Ship if pruned+quantized quality stays within 2% absolute of the unpruned target-BPW build.



Fallback. Target-BPW.



A.110 · PalQuant KV for Deep



Bucket: A.2 · Release: R2 · Status: gated



Harness: kv-compress-bench.py::palquant\_deep · Rationale: A.110



Criterion. PalQuant at 7.42× on Qwen3-8B. Ship if it passes A.97's PPL + KL@D gates and multi-session identity preservation ≥95% on Deep.



Fallback. STAR-KV/GAAC.



A.111 · Cold/warm router profiles



Bucket: A.2 · Release: R2 · Status: gated



Harness: mode-switch-bench.py::profile\_swap · Rationale: A.111



Criterion. Per-language Deep profiles (Rust/Python/TypeScript/shell/general) with LoRA adapters merged per profile, router mode swapping, context checkpoints across swaps. Ship if profile-switch latency is <200 ms warm and context preservation works (no prompt re-processing).



Fallback. Single merged Deep profile.



A.118 · Deep unified shipping config



Bucket: A.2 · Release: R2 · Status: gated



Harness: cargo xtask baseline-audit · Rationale: A.118



Criterion. The tuned \[performance.deep] config with all optimizations defaulting on. Ship if the full config passes the regression suite and the quality gates.



Fallback. Deep stays at the prior config.



A.129 · USB/IP virtual audio device



Bucket: A.2 · Release: R2 · Status: gated



Harness: broadcast-bench.py::usbip\_enumeration · Rationale: A.129



Criterion. Windows enumerates SUL Virtual Audio B1/B2/B3 as standard USB audio devices, no kernel driver. Verified by pnputil /enum-drivers showing no SUL driver.



Fallback. VBAN bridge.



A.132 · Preview→Program safety



Bucket: A.2 · Release: R2 · Status: gated



Harness: broadcast-bench.py::preview\_program\_safety · Rationale: A.132



Criterion. Agent cannot mutate program scene directly; every mutation goes through preview and take(); preview\_frame() returns the actual preview. Verified by a scripted agent that attempts to bypass preview.



Fallback. Agent API disabled; manual control only.



A.167 · ArchiveBox tool



Bucket: A.2 · Release: R2 · Status: gated · Blocking: no



Harness: archivebox-bench.py::archive · Rationale: A.167



Criterion. ArchiveBox (archivebox/archivebox) ships as agent tool `archive\_page(url)` if a scripted 10-page archive round-trips without corruption, the container lifecycle matches the security-toolkit pattern, and the network surface is declared.



Fallback. Tool stays off.



A.168 · Changedetection.io tool



Bucket: A.2 · Release: R2 · Status: gated · Blocking: no



Harness: changedetection-bench.py::watch · Rationale: A.168



Criterion. Changedetection.io (dgtlmoon/changedetection.io) ships as agent tool `watch\_page(url, condition)` if it correctly detects a scripted 10-page change set, the container lifecycle matches, and the network surface is declared. Feeds A.58's verification cache.



Fallback. Tool stays off.



A.171 · Maxun web scraping tool



Bucket: A.2 · Release: R2 · Status: gated · Blocking: no



Harness: maxun-bench.py::scrape · Rationale: A.171



Criterion. Maxun (getmaxun/maxun) ships as agent tool `scrape\_page(url)` if it produces structured output for a scripted 10-page scrape set and the network surface is declared. Overlaps `web\_fetch` / `readdown`; only adopted if it produces materially better structured output.



Fallback. Tool stays off. `web\_fetch` remains the default.



A.175 · Ninite software installer tool



Bucket: A.2 · Release: R2 · Status: gated · Blocking: no



Harness: ninite-bench.py::install · Rationale: A.175



Criterion. Ninite (ninite.com) ships as agent tool `install\_software(name)` if the scripted installer set runs end-to-end with the user's explicit confirmation and the network surface is declared. Requires Tier 1 per invocation.



Fallback. Tool stays off.



A.176 · VirusTotal file scanning tool



Bucket: A.2 · Release: R2 · Status: gated · Blocking: no



Harness: virustotal-bench.py::scan · Rationale: A.176



Criterion. VirusTotal (virustotal.com) ships as agent tool `scan\_file(path)` if it returns correct verdicts for a scripted 20-file set (10 clean, 10 known-malicious), the API terms permit use, and the network surface is declared. Could also feed the mod-install scanner (Ch. 3 §3.13).



Fallback. Tool stays off. The mod-install path continues with local signature-based detection only.



A.178 · TinEye reverse image search tool



Bucket: A.2 · Release: R2 · Status: gated · Blocking: no



Harness: tineye-bench.py::search · Rationale: A.178



Criterion. TinEye (tineye.com) ships as agent tool `find\_image\_source(image\_path)` if it returns usable results for a scripted 20-image set, the API terms permit use, and the network surface is declared.



Fallback. Tool stays off.



A.180 · HyperFrames offline HTML-to-MP4 rendering (handoff A.156)

Bucket: A.2 · Release: R2 · Status: gated · Blocking: no

Harness: broadcast-bench.py::hyperframes_offline · Rationale: A.180

Criterion. `sul-broadcast` `render_mode = "offline"` invokes HyperFrames inside an isolated container (no bundled Node.js in `sul.exe`) to render deterministic MP4s muxed by the ISO muxer with peak RAM <2 GB on the 8 GB reference rig.

Fallback. Offline HTML rendering disabled; standard hardware/software encoder path used.

A.181 · pptx_generate editable PowerPoint skill (handoff A.157)

Bucket: A.2 · Release: R2 · Status: gated · Blocking: no

Harness: agent-bench.py::pptx_generate · Rationale: A.181

Criterion. `pptx_generate` (invoked by Deep in an isolated container, no bundled Node.js in `sul.exe`) converts PDF/DOCX/Markdown/URLs into editable `.pptx` files under `%APPDATA%\SUL\exports\` with ≥95% native shape preservation.

Fallback. Falls back to Markdown export (`E_PPTX_GENERATE_FAILED`).

A.182 · Frame generation post-process FX DAG (handoff A.158)

Bucket: A.2 · Release: R2 · Status: gated · Blocking: no

Harness: fg-bench.py::post_process_dag · Rationale: A.182

Criterion. Pure-Rust `sul-fg::post_process` DAG (motion blur, DoF, chromatic aberration, lens flare) adds <1 ms/frame overhead on the 8 GB reference rig with zero frame corruption.

Fallback. Post-process DAG disabled; clean interpolated frames presented.

A.183 · Multi-stage Coder security_audit (handoff A.159)

Bucket: A.2 · Release: R2 · Status: gated · Blocking: no

Harness: agent-bench.py::security_audit · Rationale: A.183

Criterion. Four-stage Coder diff audit (secrets → dependency CVEs → `unsafe` review → IPC boundary check) with independent verifier agent enforcement (no agent verifies its own finding) meets precision/recall targets on scripted vulnerable diffs.

Fallback. Standard AST guards (`ac-regression`) and Gate A/B checks remain active.

A.185 · claim_audit source-grounded fact-check tool (handoff A.161)

Bucket: A.2 · Release: R2 · Status: gated · Blocking: no

Harness: search-bench.py::claim_audit · Rationale: A.185

Criterion. `claim_audit` reuses the verification packet builder to output `claim → evidence → confidence` for marketing/item/hardware claims with ≥90% grounding accuracy and zero new model weights.

Fallback. Standard `web_search` verification packet used directly.

A.186 · Long-term agent memory FTS5 + vector recall (handoff A.162)

Bucket: A.2 · Release: R2 · Status: gated · Blocking: no

Harness: agent-bench.py::memory_recall · Rationale: A.186

Criterion. `[agent.memory]` SQLite FTS5 + `sulpack` vector recall retrieves top-k cross-session summaries with address pointers in <50 ms.

Fallback. Session-only working memory and Entity Ledger remain active.

A.187 · Custom Entity Ledger types (handoff A.163)

Bucket: A.2 · Release: R2 · Status: gated · Blocking: no

Harness: agent-bench.py::entity_ledger_custom · Rationale: A.187

Criterion. `[agent.entity_ledger.custom_types]` JSON Schema custom entities keep Ledger serialized size growth <10% over baseline.

Fallback. Fixed Entity Ledger schema only.

A.189 · Conversation reading mode for EPUB/MOBI (handoff A.165)

Bucket: A.2 · Release: R2 · Status: gated · Blocking: no

Harness: combat_full_bench.py::reading_mode · Rationale: A.189

Criterion. `[features.reading_mode]` parses local EPUB/MOBI files (`epub` crate) and injects chapter/selection context + address-pointer anchors into Combat Full Conversation in <100 ms.

Fallback. Reading mode disabled; standard document paste/OCR path used.

A.192 · Magnitude hardware kernel autotune

Bucket: A.2 · Release: R2 · Status: gated · Blocking: no

Harness: kv-bench.py::magnitude_autotune · Rationale: A.192

Criterion. `sul-ai-native --backend-autotune` generates `%APPDATA%\SUL\bench\kernel_cache.json` and `hardware_profile.json`, delivering measurable decode/prefill speedup over default `llama.cpp` kernels with zero output regression and automatic invalidation on GPU/driver change.

Fallback. Default `llama.cpp` kernels (`E_KERNEL_AUTOTUNE_FAILED`).

A.195 · camofox-browser fallback fetch

Bucket: A.2 · Release: R2 · Status: gated · Blocking: no

Harness: search-bench.py::camofox_fallback · Rationale: A.195

Criterion. `[net.search.fallback] browser_backend = "camofox"` runs in an isolated container (no bundled Node.js in `sul.exe`), only on public pages when `direct_fetch` is blocked, with Tier 1 approval.

Fallback. `browser_backend = "none"`; `direct_fetch` / `RustXNG` only.

R3 — Sensing and security (prerequisite: A.152)



A.101 · Combat Full Coding profile



Bucket: A.2 · Release: R3 · Status: gated



Harness: combat\_full\_bench.py::coding\_humaneval · Rationale: A.101



Criterion. The Coding profile ships only if absolute HumanEval ≥60 through the Architecture Ch. 10 §10.6 fine-tuning pipeline. Base InternVL3-2B is at HumanEval 39.0 per the InternVL3 technical report. If A.101 fails, the Coding profile stays research and the only coding capability in the shipping release is Deep (K2-Horizon-7B). No fallback to Qwen2.5-Coder (Coding Low cut).



Fallback. Coding profile stays research; Deep is the only coding path.



A.136 · CSI sensing and vital signs (handoff A.114)



Bucket: A.2 · Release: R3 · Status: gated · Prerequisite: A.152



Harness: csi-bench.py::packet\_to\_ui, csi-bench.py::vitals\_accuracy, csi-bench.py::presence\_accuracy · Rationale: A.136



Criterion. Integrates ruvnet/ruview (WiFi DensePose) for presence, pose, breathing (6–30 BPM), and heart rate (40–120 BPM) detection through walls. 4KB quantized INT8 models run on ESP32 edge nodes for presence detection. Rust-native FFT-based vital sign extraction on the host. Zero-copy data flow via sul-ring. Decision criterion: <50 ms latency from CSI packet to UI update. Through-wall detection accuracy verified against a reference camera.



Fallback. sul-csi stays opt-in (disabled by default); presence/vitals feed not offered.



Notes. Hard gate; the feature does not ship until latency and accuracy criteria are met.



A.137 · God's Eye View and geolocation (handoff A.115)



Bucket: A.2 · Release: R3 · Status: gated · Prerequisite: A.152



Harness: gev-bench.py::render\_fps, gev-bench.py::geolocation\_accuracy · Rationale: A.137



Criterion. sul-gev module for geospatial intelligence. Ingests public feeds (ADS-B, AIS, satellites) similar to bilawalsidhu/gods-eye-view. Integrates Oldcircle/geo-sleuth as an agent skill for image geolocation. WebGPU dynamic streaming at 60 FPS for the 3D globe. Tile-based caching with predictive prefetching. Zero-copy GPU buffers via wgpu. Decision criterion: render 10,000+ geospatial objects at 60 fps across at least 8 of the OSIRIS layer classes. Geolocation from a single image <2 s using local models.



Fallback. sul-gev stays opt-in (disabled by default).



Notes. Feature does not ship until both latency and rendering criteria are met. GEV ingests only public feeds — no private or proprietary surveillance network.



A.138 · rPPG vitals (handoff A.116)



Bucket: A.2 · Release: R3 · Status: gated · Prerequisite: A.152



Harness: vision-bench.py::rppg · Rationale: A.138



Criterion. rPPG pipeline that extracts heart rate and respiratory rate from facial video via sul-vision. GPU-accelerated signal processing (bandpass filtering and ICA in wgpu compute shaders). ROI-specific processing using a lightweight face detector. Adaptive sampling based on subject motion. Output displayed in HealthDashboard and stored encrypted in the vault. Decision criterion: heart rate accuracy within ±3 bpm vs. contact sensor. CPU overhead <5% for 720p 30fps stream.



Fallback. rPPG stays opt-in (disabled by default).



Notes. Does not ship until both accuracy and overhead criteria are met. rPPG runs only on the user's own camera feed — no remote video is processed.



A.139 · BCI integration (handoff A.117)



Bucket: A.2 · Release: R3 · Status: gated · Prerequisite: A.152



Harness: bci-bench.py::decoding\_accuracy, bci-bench.py::end\_to\_end\_latency · Rationale: A.139



Criterion. sul-bci crate wrapping eugenehp/brain2qwerty-rs for MEG/EEG-to-text decoding (Pearson r ≥0.999999 numeric parity to Python). OpenBCI hardware ingestion. Rust signal preprocessing with rustfft. INT8 quantized decoding model on a dedicated real-time thread. Decision criterion: decode a full sentence with >90% accuracy. End-to-end latency (neural signal to text) <200 ms.



Fallback. sul-bci stays opt-in (disabled by default).



Notes. Does not ship until both accuracy and latency criteria are met.



A.140 · Browser automation (handoff A.118)



Bucket: A.2 · Release: R3 · Status: gated · Prerequisite: A.152



Harness: agent-bench.py::web\_browsing · Rationale: A.140



Criterion. Integrate browser-use as an agent skill for headless browser control (direct CDP connection for \~50 ms command latency). Use Crawl4AI for structured data extraction into LLM-ready Markdown (AsyncWebCrawler with CacheMode for optimized concurrency and caching). Persistent browser daemon between commands to eliminate startup cost. Decision criterion: agent completes a 10-step task (e.g., "find price of X on Y site and add to spreadsheet") with >95% success rate.



Fallback. Browser automation stays opt-in (disabled by default).



Notes. Does not ship until the success-rate gate passes. The browser runs in a sandboxed profile with no access to the user's cookies, saved passwords, or browser extensions.



A.141 · Shannon autonomous pentester (handoff A.119)



Bucket: A.2 · Release: R3 · Status: gated · Prerequisite: A.152



Harness: security-bench.py::shannon · Rationale: A.141



Criterion. Integrate KeygraphHQ/shannon as a security skill. SUL manages the lifecycle (launch, monitor, collect report) in an isolated Docker container with enforced CPU/memory limits. Parallel scanning orchestration. Decision criterion: successfully run a pentest against OWASP Juice Shop and produce a report with valid findings.



Fallback. Shannon stays opt-in (disabled by default).



Notes. Does not ship until the OWASP Juice Shop test passes. The user must create a signed consent file before any scan runs.



A.142 · Security toolkit — hashcat, nuclei, mitmproxy (handoff A.120)



Bucket: A.2 · Release: R3 · Status: gated · Prerequisite: A.152



Harness: security-bench.py::hashcat\_speed, security-bench.py::nuclei\_throughput, security-bench.py::mitmproxy\_latency · Rationale: A.142



Criterion. sul-security-toolkit crate wrapping hashcat (GPU password recovery with -O for kernel optimization, -w 3/-w 4 for workload profile), nuclei (vulnerability scanner with -c/-bs/-rl tuning), mitmproxy\_rs (Rust bits for mitmproxy — local redirect mode, \~5× lower latency than Python). Isolated execution in Docker containers with enforced resource limits. Concurrent execution via tokio. Zero-copy interception via mitmproxy\_rs's Rust API. Decision criterion: each tool passes a functional test (crack a known hash, find a known vuln, intercept and log HTTP traffic).



Fallback. Individual tools stay opt-in; the toolkit does not ship until each passes.



Notes. The user must create a signed consent file before any tool runs against a target. The tools run only against the user's authorized targets — never against kernel-AC game processes or arbitrary third-party hosts.



A.143 · VoiceStudio (handoff A.121)



Bucket: A.2 · Release: R3 · Status: gated · Prerequisite: A.152



Harness: voicestudio-bench.py::tts\_latency, voicestudio-bench.py::clone\_similarity · Rationale: A.143



Criterion. sul-voicestudio.exe as a separate process running a local voice cloning, TTS, and dubbing engine (646 languages). Process isolation. Low-latency streaming via MeanVC2 (110 ms end-to-end first-packet latency). VoiceClonePrompt caching (LRU keyed by ref\_path + mtime + ref\_text). Decision criterion: zero-shot voice cloning (character voices only) with >90% similarity. TTS latency <100 ms for first sentence.



Fallback. VoiceStudio stays opt-in (disabled by default).



Notes. Hard invariant: voice cloning of a specific real person is structurally refused. The API surface has no code path that could accept a specific person's voice sample. Character voice models must be trained on commercially-licensed voice-actor data with explicit redistribution rights. This invariant is enforced at the AST guard level — any PR that introduces a code path accepting a "target person" parameter is rejected regardless of its other merits.



A.144 · Heretic uncensored models (handoff A.122)



Bucket: A.2 · Release: R3 · Status: gated · Prerequisite: A.152



Harness: eval-abliterated.py · Rationale: A.144



Criterion. Build-time integration of p-e-w/heretic CLI. Pre-applies abliteration to GGUFs during the release build. Ships abliterated GGUFs alongside the base GGUFs; \[features] ai.uncensored selects which file to load at runtime. Decision criterion: abliterated models retain >98% of original benchmark performance (BFCL, HumanEval) as measured by a full A.63-like eval run.



Fallback. Abliterated GGUFs are not shipped; the toggle remains hidden.



Notes. The safety invariant (destructive-action → Thinking), Verifier, and structural refusals in Architecture Ch. 16 §16.5 are enforced at the agent/router level, not at the model level — an abliterated model does not bypass them.



A.145 · Local uncensored models (handoff A.123)



Bucket: A.2 · Release: R3 · Status: gated · Prerequisite: A.152



Harness: eval-abliterated.py · Rationale: A.145



Criterion. Support for locally-uncensored third-party models as an alternative to abliterated GGUFs. Same quality gate as A.144 — the model must retain >98% of the base model class's benchmark performance, and the model must be from a source with a compatible license (verify per-model).



Fallback. The option is not offered; \[features] ai.local\_uncensored is hidden.



A.149 · Personal health dashboard (handoff A.127)



Bucket: A.2 · Release: R3 · Status: gated · Prerequisite: A.152



Harness: manual + vault integrity tests · Rationale: A.149



Criterion. Expand rPPG/CSI vitals into a comprehensive dashboard tracking stress, sleep (via CSI), and activity over time. All data stored encrypted in the vault. Decision criterion: HealthDashboard UI is complete, all vitals are accurate (per A.136, A.138), and all data is verified to be encrypted in the vault (no plaintext on disk, verified by the vault's own integrity tests).



Fallback. The dashboard does not ship; individual vitals remain available in the overlay.



Notes. Dependency: A.136 (CSI), A.138 (rPPG), A.92 (vault).



A.150 · Agentic code review and binary audit (handoff A.128)



Bucket: A.2 · Release: R3 · Status: gated · Prerequisite: A.152



Harness: security-bench.py::binary\_audit · Rationale: A.150



Criterion. Use Ghidra MCP (bethington/ghidra-mcp, 193+ tools, batch operations for 93% API call reduction) and reverse engineering capabilities to allow the agent to audit third-party binaries (mods, game files) for security issues. Integrates with sul-quarantine — any flagged binary is quarantined automatically. Decision criterion: the agent successfully audits a scripted set of known-malicious binaries and identifies the malicious behavior on ≥90% of them, with zero false positives on the clean binary set.



Fallback. The feature does not ship; sul-quarantine continues to operate on signature-based detection only.



Notes. This feature is opt-in per binary and requires a Tier 1 event to enable.



A.151 · Automated vulnerability remediation (handoff A.129)



Bucket: A.2 · Release: R3 · Status: gated · Prerequisite: A.152



Harness: security-bench.py::auto\_remediation · Rationale: A.151



Criterion. When Shannon or Nuclei finds a vulnerability in a user-built web app, the agent can automatically propose a patch using its coding capabilities (Deep or Combat Full Coding). Decision criterion: the agent produces a working patch on ≥70% of a scripted set of common vulnerability classes (SQLi, XSS, CSRF, path traversal, broken auth).



Fallback. The feature does not ship; the agent continues to report findings without proposing patches.



Notes. Tier 1 event — enabling automatic remediation requires user approval, and each patch proposal requires explicit user acceptance before being applied.



A.163 · WASM isolation for security toolkit



Bucket: A.2 · Release: R3 · Status: gated · Blocking: no



Harness: wasm-bench.py::overhead, wasm-bench.py::coverage · Rationale: A.163



Criterion. WASM isolation (via Wasmtime or wasm-micro-runtime) ships as an additional option to Docker if (a) at least 50% of the security toolkit's tools can run in WASM, (b) the WASM sandbox has ≤50 ms overhead per tool invocation, (c) the AST guard verifies that WASM-executing tools cannot reach the host filesystem or network beyond their declared surface. Docker remains the default.



Fallback. Docker remains the only backend.



A.184 · WiFi security audit tool (handoff A.160)

Bucket: A.2 · Release: R3 · Status: gated · Prerequisite: A.152

Harness: security-bench.py::wifi_audit · Rationale: A.184

Criterion. `[security_toolkit.network] wifi_audit` invokes external `aircrack-ng` (never statically linked, GPL boundary) only with a compatible NIC and signed consent file, reporting weak encryption/WPS/default-credential risks to `security_history.json` without automated attacks.

Fallback. WiFi audit disabled.

A.193 · Zero-Breach offline spyware IOC scan

Bucket: A.2 · Release: R3 · Status: gated · Prerequisite: A.152

Harness: security-bench.py::spyware_scan · Rationale: A.193

Criterion. `[security_toolkit.spyware_scan]` runs offline Zero-Breach-Toolkit IOC signatures locally with zero network upload and never auto-deletes files.

Fallback. `sul-watchdog` baseline integrity checks only (`E_SPYWARE_SCAN_IOC_UNAVAILABLE`).

A.194 · Multi-camera vision pose fallback

Bucket: A.2 · Release: R3 · Status: gated · Prerequisite: A.152

Harness: csi-bench.py::vision_pose · Rationale: A.194

Criterion. `[sensing.vision_pose]` runs `skellytracker` + `RTMPose` ONNX across 2–3 USB cameras as a BCI fallback with <40 ms p99 pose latency.

Fallback. Pose input disabled (`E_POSE_BACKEND_UNAVAILABLE`).

A.3 — Decided / Resolved (18)



Settled. No benchmark needed. The decision is recorded here and in SUL-DECISIONS.md.



A.1 · OScaR INT2 KV cache — parked



OScaR uses Canalized Rotation + Omni-Token Scaling for near-lossless INT2 KV quantization with 5.3× memory reduction, 3.0× decode speedup, 4.1× throughput increase vs BF16 FlashDecoding-v2 (ZunhaiSu/OScaR-KV-Quant). Parked: STAR-KV (up to 20×) supersedes OScaR (5.3×). Unpark only if A.18 fails and GAAC is insufficient. Rationale: A.1.



A.6 · CPU offload of KV cache (-nkvo) — decided



Extreme fallback. Latency penalty \~10–20×. Config-only. Superseded by sul-kv-tier for most cases. Now exposed as a first-class preset in Architecture Ch. 10 §10.20 AI Optimization Presets: offload\_mode = "nkvo" is the extreme-VRAM option. Default is "auto" (sul-kv-tier with STAR-KV/GAAC). "none" disables offload entirely (may OOM on some tiers). The setting is user-selectable; the default is the tuned configuration. Rationale: A.6.



A.7 · FreeToken bandwidth-adaptive offloading — rejected



FreeToken dynamically splits memory misses between CPU and PCIe based on bandwidth, achieving 39.3 tok/s on 8 GB GPU for 35B MoE (arXiv:2608.16157). Rejected: MoE-specific technique; doesn't fit Combat Full's 3 GB general combat ceiling (Phase 2). Not inherent inferiority of MoE — tier-budget mismatch. Rationale: A.7.



A.17 · Symmetric q4\_0 KV with Hadamard rotation — rejected



Whether mainline llama.cpp's native KV quant with Hadamard rotation (PR #21038) makes q4\_0/q4\_0 viable at +7.6% PPL on 12 GB+ cards for Deep Q5\_K\_M at 8K. Rejected: target hardware mismatch — reference rig is 8 GB; A.17 targets 12 GB+ cards. STAR-KV already covers the potential benefit (up to 20× vs q4\_0's 2×). No independent value on 8 GB. Rationale: A.17.



A.25 · STAR-KV on MBZUAI-IFM fork — obsolete



Fork deleted. K2-Horizon support landed in llama.cpp mainline b11454 (PR #29535). The MBZUAI-IFM fork no longer exists in the shipping release. E\_STAR\_KV\_UNSUPPORTED\_TREE is retained in the code for future fork scenarios but is unreachable in the shipping path. Deep runs on mainline llama.cpp b12207+ (which satisfies b11454), with STAR-KV or GAAC per A.18. Rationale: A.25.



A.48 · LlamaWeb memory pattern — applied



LlamaWeb (arXiv 2026) achieves 29–33% less memory and 45–69% higher decode throughput than other browser LLM frameworks. The memory-mapping discipline (keep weights out of model process's resident set, mmap from disk) is already used in sul-ai-native (FILE\_FLAG\_NO\_BUFFERING | FILE\_FLAG\_SEQUENTIAL\_SCAN). Already applied. No action needed. Rationale: A.48.



A.51 · ZenDNN on AMD CPUs — archived



ZenDNN achieves up to 4.5× prompt processing speedup for llama.cpp on AMD EPYC CPUs. Reference rig (Ryzen 5 5600X) is Zen 3; ZenDNN's largest gains are on EPYC server CPUs. Not applicable. Re-purposed as a record of a future module direction: sandbox / malware-testing module for suspicious file triage, based on Windows Sandbox .wsb config generation + host read-only sharing + network disabled as default secure configuration. Decoupled from ZenDNN. Not in shipping release. Rationale: A.51.



A.53 · STAR-KV on MBZUAI-IFM fork (alias) — obsolete



Alias of A.25. Both numbers preserved because the handoff brief references both. Update both simultaneously when A.25 closes. Current state: OBSOLETE — fork deleted after b11454 mainline merge. Rationale: A.53.



A.65 · Multi-OS, Linux, macOS, ARM, Mobile Companion — parked



Port sul-core, sul-unpack, sul-ipc to Linux; re-validate K2-Horizon mainline llama.cpp support, EAGLE-3 draft path, and OCR/VLM pipelines on Linux CUDA. macOS arm64 port. Mobile companion (read-only stats, library browsing, mod install approval). Out of scope for the shipping release per the one-product rule (Architecture Ch. 1 §1.6). Re-open only if a cross-platform release becomes a product decision. Full technical requirements preserved in SUL-DECISIONS.md so no scoping work is lost. Rationale: A.65.



A.67 · SUL First-Party Game Suite and Developer Showcase — parked



Dedicated Showcase Hub for first-party SUL games. Modding-friendly by design. Seamless game library ingestion for first-party titles: direct metadata import (title, cover art, description) without Steam or GOG. Signed manifest from maintainer. Conditional on a first-party game existing, which it does not. Re-open automatically if a first-party SUL game is ever produced. Rationale: A.67.



A.68 · Multi-model ensembling for combat — rejected



Ensemble multiple small models for improved combat judgment/execution. Rejected: violates the 3 GB general combat ceiling (Phase 2). The multi-profile Combat Full design is one model with four inference profiles, selected by a router per turn — routing, not ensembling. Two or more distinct models running simultaneously would exceed the general combat ceiling. This entry is the design anchor for the Combat Full multi-profile migration (Architecture Ch. 10 §10.1, §10.4). Cited in Ch. 10 §10.1 as the authoritative reference for why Combat Low, Combat Mid, Butler, Coding Low, and Deep Extended were cut and why the multi-profile design is not an ensemble. No change required — it was already correct; the rest of the spec was migrated to match it. Rationale: A.68.



A.69 · Qwen3-0.6B as Combat-Exec — rejected



Use Qwen3-0.6B as a tiny execution specialist. Rejected: action score 0.700 is too low for execution. Not in shipping release. Rationale: A.69.



A.70 · 64K context for combat tiers — rejected



64K context for Combat Full (all profiles). Rejected: cannot fit on 8 GB. Combat Full's 2K (Instruct), 4K (Thinking), 16K (Conversation), and 8K (Coding) contexts are deliberate accuracy-equivalent windows sized to their workload classes, not compromise-fit choices. Extending to 64K would require \~6–7 GiB of KV cache in FP16 alone (before weights or vision tokens), exceeding the 8 GB budget on the reference rig. The pre-consolidation rationale cited Combat Low's accuracy collapse after 16K — that reasoning is retired with Combat Low (cut), but the VRAM-fit argument for Combat Full is unchanged and sufficient on its own. Rationale: A.70.



A.71 · Hammer2.1-3B as execution booster — rejected



Reintroduce Hammer as a swap-in execution specialist. Rejected: replaced by Combat Full fine-tuning (Architecture Ch. 10 §10.6). Combat Full already has vision + dialogue + tool calling in one checkpoint; Hammer was execution-only and required a swap-in. Live trigger: if A.63 returns <65 non-live on any profile, this rejection is automatically re-evaluated. Rationale: A.71.



A.77 · Frame generation on kernel\_level: true titles — rejected



Allow sul-fg on games with kernel-level anti-cheat. Rejected: structural violation of Architecture Ch. 16 §16.3.1. The layer would need to be present in a process protected by nProtect GameGuard / EAC / BattlEye / Vanguard. The anti-cheat detection surface is exactly what Ch. 16 §16.3 exists to avoid. Not a gate — a rejection. Rationale: A.77.



A.153 · Handoff feature numbering reconciliation — decided



The handoff proposal (dated 2026-11-11) numbered its feature gates A.114–A.135, which collide with existing gate numbers in the current spec. Resolved: the handoff features are renumbered to A.136–A.152, and the migration table is recorded in SUL-DECISIONS.md. All handoff references in Architecture Ch. 15 that carry a "(handoff)" suffix are authoritative for interpreting the mapping. The handoff's own gate numbers are preserved in the historical document but do not appear in the shipping spec's gate ledger. Rationale: A.153.



A.154 · Uncensored model safety invariant — decided



The user can enable abliterated or third-party uncensored models via \[features] ai.uncensored = true and \[features] ai.local\_uncensored = true (Tier 1 events). Resolved: the safety invariant (destructive-action → Thinking), Verifier's tool-call check, structural refusals in Architecture Ch. 16 §16.5, and the persona constraints in the always-loaded context are enforced at the agent/router level, not at the model level. An abliterated model does not bypass any of them. This is a documentation clarification — the mechanism was already correct; the handoff's uncensored-model feature is safe to add because the safety layer sits below the model, not inside it. Rationale: A.154.



A.155 · Voice cloning structural refusal (VoiceStudio clarification) — decided



The handoff's VoiceStudio feature includes a clone\_voice(audio\_sample) tool. The existing spec's structural refusal in Architecture Ch. 16 §16.5 prohibits voice cloning of a specific real person. Resolved: the clone\_voice tool accepts only character voice preset IDs from the vendored catalog (trained on licensed voice-actor data). Any attempt to pass a raw audio sample of a specific real person is refused with E\_VOICESTUDIO\_CLONE\_REFUSED. The API surface has no parameter that could accept a target person. This is enforced by the AST guard in Architecture Ch. 16 §16.15 — any PR that introduces a "target person" parameter is rejected. The refusal is not a config toggle; it is a code invariant. Rationale: A.155.



A.4 — Research Only (40)



Measurement-gated, no shipping obligation, no date, no owner. All A.4 entries are R4 by definition.



A.2 · Iterative CADENCE round 2 — research



Second CADENCE iteration on remaining disagreements. If round 1 gives 95%+ per-task match, round 2 optional. Requires another \~$36 compute on rented GPU. Rationale: A.2.



A.3 · MTP as benchmark winner — research



Test after sul-ai-native stable. If InternVL3 or K2 GGUF carries MTP heads, startup micro-benchmark tests MTP alongside n-gram and 0.5B draft. MTP and n-gram are alternatives, not additive. If >10% throughput gain on Deep eval → default. Rationale: A.3.



A.5 · Parallel prefill (Green Context) — research



Track llama.cpp CUDA Green Context PR. When merged into mainline, enable for Deep. Expected first-token improvement: 150 ms → \~60–80 ms on long prompts. No VRAM increase. Rationale: A.5.



A.6b · Green Context in-house build — research



TurboPrefill (sergey-automation/TurboPrefill) and CUDA Driver API Green Context primitives (cuDevSmResourceSplitByCount, cuGreenCtxStreamCreate) are both available. PyTorch merged experimental SM partition (PR #198278); FlashInfer has experimental Green Context support (PR #1163). The same in-house build pattern used for buffer-borrow mmproj applies. If TurboPrefill-style acceleration delivers the expected 150 → 60–80 ms TTFT improvement on Deep, adopt it. Otherwise, build a Green Context prefill path directly in sul-ai-native on top of the driver API. Rationale: A.6b.



A.8 · Full packet compression via LoRA baking — research



Train a packet LoRA on compressed-form packets so the schema lives in weights, not context. Positional JSON keys, positional arrays. Saves additional \~250 tokens per session. Why deferred: silent failure mode if eval is lazy. Sequencing: build eval harness with 200 packet-grounded queries, measures uncompressed-format baseline. If compressed-format accuracy ≥93% on that eval, ship compression. Data decides. Rationale: A.8.



A.9 · Trans-LoRA cross-family adapter transfer — research



Whether jarvis\_structured\_k2.lora trained for K2-Horizon-7B can be transferred to InternVL3-2B via Trans-LoRA. If Trans-LoRA achieves ≥90% of a from-scratch adapter's BFCL score on 200 SUL queries, ship the transferred adapter instead of retraining. Otherwise, retrain from scratch (\~$36). Rationale: A.9.



A.10 · MiniCPM-V fine-tuning on SUL HUDs — research



MiniCPM-V 4.7 1B is weak on handwriting and multilingual translation. Fine-tune on SUL-specific HUD data (ED, HD2) to close those gaps without needing PaddleOCR-VL as runtime fallback. If fine-tuned MiniCPM-V achieves ≥75% mean rubric on SUL game-screenshot eval set (up from 68.2%) and matches PaddleOCR-VL on handwriting, ship the fine-tuned model and remove the fallback from shipping path. Otherwise, keep both models. Rationale: A.10.



A.11 · FastV visual token pruning — research



FastV prunes redundant visual tokens in deeper layers of the language model, achieving 45% FLOPs reduction with nearly no performance loss on LLaVA-1.5-13B. Applicability to MiniCPM-V, LFM2.5-VL, InternVL3-2B is unmeasured. If FastV reduces MiniCPM-V prefill latency by >20% with <3% rubric drop on SUL eval set, integrate into sul-vlm. Otherwise, not worth complexity. Rationale: A.11.



A.12 · DSpark drafter for Combat Full — research



DSpark reports 2.35× average on Qwen3-1.7B, but EACL 2026 speculative decoding study says "drafting overhead, rather than draft quality, becomes the primary bottleneck fundamentally limiting acceleration of small models." Contradiction needs resolution. The original A.12 body cited Qwen3-1.7B (Lucy, cut). The Qwen3-1.7B benchmark is historical context for the DSpark overhead question — it is not a target. Under the multi-profile design, DSpark would be evaluated against Combat Full's internals (InternVL3-2B with its 2B parameter class), not a specific cut model. Combat Full's shipping chain is \["medusa", "phrase", "unispec", "ngram", "none"] — single chain shared by all profiles. DSpark would replace UniSpec as fallback rung if it wins. Rationale: A.12.



A.13 · K2-Horizon 512K context on 24 GB — research



Whether K2-Horizon's 512K native context is usable on 24 GB card (RTX 3090/4090) at usable throughput with symmetric q8\_0 KV. If median tok/s ≥15 at 128K context, add "Deep-Ultra" tier for 24 GB cards. Otherwise, Deep ships at 4K–8K default with 32K ceiling. With STAR-KV, 16K becomes plausible on 8 GB and 32K is practical ceiling. 512K still requires 24 GB+. Not required for the release. The block is VRAM, not code, but it can be closed without buying hardware. Options: (1) leave open indefinitely — research-only, no shipping obligation; (2) ingest community benchmark data via Architecture Ch. 14 §14.6 if upstream or another user publishes K2 512K numbers on a 24 GB card; (3) rent a 24 GB cloud instance for a single \~$1–3 benchmark run. Rationale: A.13.



A.14 · Qwen3-4B thinking mode as separate tier — research (closed by Butler cut)



Whether a "General Lite-Think" tier for deliberate reasoning is worth the latency. If quality improvement on config-diff tasks exceeds +8 points with <3× latency, add the tier. Otherwise, not worth UX cost. Combat Full Thinking profile is a separate mechanism (Architecture Ch. 10 §10.4) that adds a reasoning-enabled profile to Combat Full; it does not create a General Lite-Think tier. A.14 is about a General Lite-Think tier specifically — but Butler is cut, so the tier doesn't exist. A.14 is effectively closed by the Butler cut; retained for provenance. Rationale: A.14.



A.15 · LFM2.5-VL-3B as default scene backend — research



ScreenSpot-v2 80.7 (vs Qwen 3.5 4B's 78.5, Gemma-4-E4B's 51.2). If LFM2.5-VL-3B measures <2.5 s latency and ≥75% mean rubric on SUL game-screenshot eval set at 8 GB (transient), promote to default for no-game sessions where free\_vram ≥ 5 GB. Otherwise, MiniCPM-V remains default. Rationale: A.15.



A.16 · PaddleOCR-VL handwriting fallback promotion — research



Whether PaddleOCR-VL accuracy on game HUDs justifies +50 MB bundle size. If PP-OCRv5 INT8's accuracy on handwritten game text (player names, hand-drawn fonts) is <80% and PaddleOCR-VL achieves ≥90% on same set, promote PaddleOCR-VL from optional (paddleocr\_vl\_enabled = false) to default. Rationale: A.16.



A.19 · DFlash on non-Butler tiers — obsolete



DFlash is not in any shipping chain (Butler was the only consumer). Retained for historical comparison. If a future tier re-enables DFlash, this entry is restored. Rationale: A.19.



A.21 · sul-voice multi-mic support — research



Currently sul-voice isolates a single mic. Multi-mic (array) support for spatial isolation is unmeasured. If multi-mic isolation improves WER by ≥3 points vs single-mic on a 200-utterance test set, add support. Otherwise, single-mic remains shipping path. Rationale: A.21.



A.49 · RidgeFill TTFT pattern — research



RidgeFill (arXiv, Nov 2026) achieves 1.25–1.31× faster TTFT at 16K–120K tokens via training-free late-layer cache fill. Not applicable at baseline (Deep is 4K–8K); becomes applicable when Deep's context grows past 32K or when 64K-tier research is revived. If RidgeFill reduces Deep cold TTFT by >20% at 32K, integrate into sul-ai-native. Rationale: A.49.



A.57 · ReTrace + Coca-Cola combined pipeline — research



ReTrace compresses vision tokens; Coca-Cola compresses search excerpts. Combined pipeline targets total context reduction across all model inputs. Total pipeline token cost ≤50% of baseline with ≥95% accuracy across the full combat + search pipeline. Dependency: requires A.54 (ReTrace) and A.55 (Coca-Cola) to ship first. Rationale: A.57.



A.84 · VLM-native audio branch for Combat Full — research



Add an audio encoder (Whisper encoder or smaller FastConformer encoder) → projector → InternVL3-2B language backbone, following the Qolda-AVL and SpeakerLM patterns. Qolda-AVL transfers features from three intermediate Whisper encoder layers into the first three layers of the language model via dedicated projections and residual connections; SpeakerLM reports up to 13.82% absolute cpCER reduction over cascaded SD+ASR baselines. Research only; not a shipping prerequisite and does not gate A.83. If the joint fine-tune preserves vision quality (InternViT performance on the existing SUL game-screenshot eval set) within 2% absolute and hits ≤8.5% WER on the SUL combat-speech eval set, evaluate as a Combat Full Thinking sub-profile or a separate profile-specific checkpoint. Otherwise, keep STT as a separate model. No shipping obligation; no date; no owner. Rationale: A.84.



A.86 · Voice-loop latency budget breakdown — research



Decomposes the A.85 <500 ms end-to-end target across its four terms: STT (WhisperPipe wrapper's 89 ms median is the baseline; rung 3 streaming-native would drop it to \~80 ms first-token), Combat Full Instruct reasoning (tool-call latency baseline 120 ms median / 250 ms p99; the voice loop targets the no-tool-call conversational path, which is shorter — first-token warm 90 ms / cold 350 ms), TTS (Piper \~150 ms on desktop CPU; first-sentence streaming reduces perceived latency by 0.3–0.7 s), plumbing (ring + IPC + scheduler overhead — budgeted at \~50 ms). Measured breakdown on the reference rig with Combat Full Instruct as backend, using the same stt\_bench.py::voice\_loop harness as A.85. If any term exceeds 50% of the budget, that term becomes the primary optimization target for the corresponding research item. Rationale: A.86.



A.89 · SUL virtual audio driver and neural voice conversion — research



Two distinct tracks, both long-term.



(1) SUL virtual audio driver: a user-mode PortCls or APO driver that presents a new WASAPI endpoint without requiring the user to install VB-Cable. The security surface and driver-signing overhead (WHQL attestation) are not appropriate for a Rust consumer application that also lives inside the anti-cheat boundary of Architecture Ch. 16 §16.3. Re-evaluate only if the user-facing cost of installing VB-Cable becomes a measured barrier to adoption, or if Microsoft's user-mode audio driver story changes materially.



(2) Neural voice conversion (RVC, kNN-VC): real-time timbre conversion to a target speaker. Latency is typically 30–100+ ms on GPU and higher on CPU; the ≤30 ms CPU gate in A.88 is a strict filter. Re-evaluate if a small (\~20–50 MB) CPU-resident model can hit ≤30 ms with acceptable quality, or if a GPU-resident variant becomes viable once Combat Full's VRAM footprint drops below 1 GB (currently not the case). Voice cloning of a specific real person remains a non-negotiable refusal regardless of model capability.



Rationale: A.89.



A.93 · Servo embedding as WebView2 alternative — research



Servo 0.1.0 shipped on crates.io; Servo 0.5 performance numbers show 55% canvas performance and 10× text rendering. Tauri Discussion #15235 and copse-dev/tauri-runtime-servo track the integration path. Ship Servo embedding only if ≥95% of WebView2's UI frame-time p99 and full compatibility with the SUL React UI + Monaco + <video>/<audio> stack on the reference rig. Otherwise, WebView2 remains the runtime. Research gate, not a shipping gate — the Electron exclusion and WebView hardening (Architecture Ch. 16 §16.4.4, §16.15) are the shipping-position defenses. Rationale: A.93.



A.102 · FRISM KV-cache offload — research



FRISM (ICML 2026, arXiv:2601.21187) — KV-cache offload with I/O scheduling. No code released as of writing. Track upstream for code release; when available, evaluate against sul-kv-tier's existing cold-tier path. If FRISM's I/O scheduling improves cold-tier throughput by ≥20% vs the current KVSwap-style approach, integrate as a sul-kv-tier optimization. Otherwise, retain the current path. Rationale: A.102.



A.103 · MapCoder-Lite for compact code generation — research



MapCoder-Lite (EACL 2026, code public) — lightweight code-generation approach that could inform the Combat Full Coding profile's training data or the Deep LoRA training recipe. Not a shipping dependency. If MapCoder-Lite's training recipe improves Coding-profile HumanEval by ≥5 points without regressing any other profile, integrate the recipe into Architecture Ch. 10 §10.6. Otherwise, retain the current recipe. Rationale: A.103.



A.104 · QuietPrune / StepPrune / early ViT pruning — research



Three visual token pruning methods that could reduce the vision encoder cost. QuietPrune, StepPrune, and early ViT pruning each target a different stage (input projection, early ViT layers, pre-projector). Not shipped. If any of them reduces InternViT-300M vision encoding latency by ≥20% on Combat Full without measurable accuracy drop on MMB/POPE/SQAI, integrate into the InternVL3 path. Otherwise, retain the current encoder. Complementary to ReTrace (which compresses tokens after encoding; early pruning reduces the encoder's own cost). Rationale: A.104.



A.105 · Fused STAR-KV dequant-and-attend kernel — research



llama.cpp b11980 added a SYCL DPAS flash attention kernel that runs FA directly on q4\_0/q8\_0 KV blocks (31.3 → 54.2 t/s, +73% on B580). STAR-KV's current design reconstructs to f16 before FA — the same inefficiency the DPAS kernel eliminates for q4\_0/q8\_0. A ground-up CUDA kernel that dequantizes STAR-KV's low-rank + 3.2-bit codes inside the attention computation (same shape as DPAS but for STAR-KV's format) would turn STAR-KV from "compression that costs throughput" into "compression that is throughput-neutral or better." Roughly 3–4 weeks for a CUDA developer. Measured throughput within 5% of no-compression FA on the reference rig, with STAR-KV's compression ratio preserved. Failure falls back to the current reconstruction path. Rationale: A.105.



A.112 · FlashSVD low-rank Deep — research



Fuses low-rank projection kernels into attention and FFN, cuts peak activation memory by up to 70.2% with zero accuracy loss vs low-rank baselines. Requires Candle-side or custom runtime. Not a shipping gate. Rationale: A.112.



A.113 · OPD for sub-4-bit Deep — research



On-policy distillation recovers reasoning at sub-4-bit: retention rises from 35% → 70% on MATH-500 and 66% → 91% on HumanEval. Required only if a sub-4-bit target is chosen. Not a shipping gate. Rationale: A.113.



A.125 · Native DSD over DoP — research



Bit-perfect DSD passthrough to a DoP-capable DAC. Research only; not a shipping gate. Rationale: A.125.



A.131 · AI voices — research



Character voices trained on commercially licensed voice-actor data, no zero-shot cloning capability, no "upload a sample" API path, structural refusal of voice cloning. Not a shipping gate. Rationale: A.131.



A.135 · VBAN send/receive — research



Interop with Voicemeeter's VBAN implementation on the LAN. Research gate; not required for shipping if USB/IP is the primary virtual-device path. Rationale: A.135.



A.146 · 400B model distillation (handoff A.124) — research



Distill knowledge from a 400B model into SUL's 2B/7B models using a data-generation → distillation → evaluation pipeline. Use the 400B model to generate a 1M-example instruction-following dataset. Train SUL models via KL divergence and sequence-level KD. Evaluate against SUL benchmarks. If successful, distilled models integrate via the existing model update pipeline (Tier 1 event). No shipping obligation; no date; no owner. Research budget: \~$200–500 in rented compute per iteration. Rationale: A.146.



A.147 · Global sensor fusion (handoff A.125) — research



Fuse data from CSI (A.136), rPPG (A.138), BCI (A.139), geolocation (A.137), audio, and camera feeds into a single, coherent world model in sul-governor. This is the true "God's Eye View" for situational awareness. The fused world model must outperform any individual sensor's state estimation by ≥15% on a benchmark eval set, with ≤2× the CPU overhead of the most expensive individual sensor. Tier 1 event — enabling global sensor fusion fires a modal listing every sensor being fused. No shipping obligation; research only. This is the single most privacy-sensitive toggle in the spec. Rationale: A.147.



A.148 · Predictive analytics and simulation (handoff A.126) — research



Integrate a MiroFish-style multi-agent prediction engine to simulate outcomes of events ("what happens if we deploy this resource here?"). Used for strategic planning and risk assessment in strategy games and simulation contexts. The prediction engine must produce action recommendations that a human evaluator rates as "useful" on ≥70% of test scenarios. No shipping obligation; research only. Rationale: A.148.



A.156 · FSDP / DeepSpeed-style model sharding for Deep — research



Whether a sharded inference path (splitting Deep's weights across VRAM and system RAM, with careful prefetch) outperforms the current disk-backed path (FILE\_FLAG\_NO\_BUFFERING | FILE\_FLAG\_SEQUENTIAL\_SCAN + KV tier). Sharding trades NVMe read latency for DDR4 read latency, which is an order of magnitude lower (\~80 ns vs \~50 µs on random reads, but sequential NVMe is fast enough that the current path is not the bottleneck). If a sharded inference path delivers >25% tok/s improvement on Deep at 4K context on the 8 GB reference rig, evaluate as a replacement for disk-backed. Otherwise, keep the current path. Research only; no date; no owner. Rationale: A.156.



A.157 · Batched tool-call scoring for ToolPRM — research



Current sul-toolprm does coarse-to-fine scoring at end-of-call (top-M scored once, top-N retained). A batched approach would score all candidates in a single forward pass across the batch dimension. If batched scoring improves ToolPRM throughput by >40% with zero accuracy regression on TRBench-BFCL, integrate. Otherwise, keep the current coarse-to-fine. Research only; no date; no owner. Rationale: A.157.



A.158 · Quantized activation cache for Combat Full Instruct — research



KV cache is already quantized (q8\_0). Activations during inference are still f16. A quantized activation cache would reduce peak activation memory during a single forward pass, freeing more VRAM for KV cache or larger context. If quantized activations preserve A.63 per-profile gates on the full eval suite within 1% absolute and reduce peak activation memory by >15%, integrate. Otherwise, keep f16 activations. Research only; no date; no owner. Rationale: A.158.



A.159 · Per-token draft-tree branching for Medusa-1 — research



Medusa-1 currently builds a candidate tree from the top-K tokens at each head position. A per-token branching scheme would expand the tree more aggressively at head positions where the top-1 prediction is uncertain (high entropy) and less aggressively where it's confident. If per-token branching improves acceptance rate by >5 percentage points on the Medusa-1 baseline with <10% additional verify batch cost, integrate. Otherwise, keep the uniform tree. Research only; no date; no owner. Rationale: A.159.



A.160 · Handoff feature cross-interaction research — research



How do CSI, rPPG, BCI, GEV, and VoiceStudio interact when all are enabled simultaneously? Does global sensor fusion actually produce a better world model than the sum of individual sensors? Does the safety invariant hold across all sensor modalities? Does the prompt-injection defense hold when the attacker controls one sensor but not the others? This entry covers the cross-cutting research that only becomes answerable once the individual handoff features ship. No shipping obligation; no date; no owner. If any interaction is found that weakens the safety invariant, that interaction blocks the shipping release of the affected combination until it is resolved. Rationale: A.160.



A.179 · Colibri MoE tiered streaming backend (handoff A.155) — research

Bucket: A.4 · Release: R4 · Status: research · Blocking: no

Criterion. Pure-C Colibri MoE tiered engine (hot experts on GPU, warm in RAM, cold streamed from NVMe) evaluated for R4 400B distillation on ≥25 GB RAM + NVMe systems (`[ai.deep] moe_backend = "colibri"`). Measure minimum sustained tok/s and verify license compatibility before any promotion. No shipping obligation; no date; no owner. Rationale: A.179.

A.5 — Stubs Awaiting Content (18)



Placeholders for missing draft content. Not real entries. Each stub is a TODO marker; the numbering is preserved from prior drafts so cross-references don't break. No content obligation; no shipping dependency; no date; no owner. When a stub is filled, it moves to A.1, A.2, A.3, or A.4 depending on its classification.



A.15b — New OCR engine evaluation. Stub.



A.19b — Speculation chain warm-up budget. Stub.



A.22b — sul-wire json fallback cutover. Resolved by D-145 (`rkyv` 0.8 `bytecheck`-validated shared-memory ring structs replace JSON fallback on all hot IPC paths).



A.33 — CPU-only inference path. Stub.



A.35 — Audio DSP stage insertion. Stub.



A.38 — Multi-device audio routing. Stub.



A.40 — Pack version migration ordering. Resolved by D-146 (`redb` transactional state + atomic `.sulpack` delta migration ordering).



A.41 — VFS writeback ordering. Resolved by D-146 (`redb` ACID writeback journal + `zstd` dict + `blake3` block verification).



A.46 — Search pipeline cache invalidation strategy. Resolved by D-146 (`blake3`-keyed `redb` cache entries with TTL + source-hash invalidation).



A.52b — Direct Storage CPU fallback. Stub.



A.58b — Search cache warm-up. Resolved by D-146 (`compio` async preload of `redb` verification cache at Stage 3 boot).



A.62b — ReTrace on no-game backends. Stub.



A.64b — Fallacy detector training data. Stub.



A.77b — Frame gen on user-mode AC. Stub.



A.79b — sul-dsp artifact distribution. Stub.



A.80b — sul-voice model distribution. Stub.



A.81b — Profile router training data expansion. Stub.



A.82b — Destructive-action detection class expansion. Stub.
