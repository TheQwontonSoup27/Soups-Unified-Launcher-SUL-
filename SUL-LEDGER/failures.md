SUL-LEDGER/failures.md



Reading rule. Every error code maps to exactly one recovery mode. The deterministic recovery matrix is exhausted before escalating to code-generation (Mode F). Every entry has: Code · Mode (A–F) · Trigger · Recovery. Mode F classification is per-entry; entries whose mode is "informational" are noted.



Mode definitions. A = LLM inference timeout (retry once, halve context, retry, CPU-only retry, mark tier degraded). B = invalid structured output (retry 3× stricter schema, fall back to unstructured text). C = IPC channel death (recreate channel, replay last message, 3× fail → degraded). D = model file corruption (SHA-256 verify against signed manifest; mismatch → quarantine lifecycle). E = Rust panic in supervised subsystem thread (capture minidump; per-process handling; 3× repeat → degraded). F = cascading persistent failure (escalates to Coder; bounded repair scope; hardcoded zero write to trust primitives). A config error that aborts startup, and a build failure (Kani invariant, Miri UB, merge mismatch, net tripwire, write-site violation), are \*\*not\*\* Modes. They are startup aborts or build failures. The Mode column reads "— (startup abort)" or "— (build failure)" for those entries. Mode F is reserved for a runtime failure that repeats across sessions and escalates to Coder repair.



Modes A–E are runtime recovery. Mode F is assisted repair.



Base CoreError (Part 1 §1.16)



Code	Mode	Trigger	Recovery



E\_CFG	— (startup abort)	Any config validator failure	Startup aborts. No silent degradation.

E\_UNKNOWN\_ARCH	E	GGUF general.architecture not recognized	Refuse to load tier; log; user notified.

E\_KV\_ASYMMETRIC\_FA	E	K/V asymmetric when FA on	Refuse to start. Never silently degrade — silent quality degradation from asymmetric FA is worse than refusing to start.

E\_K2\_ARCH	E	K2-Horizon attempted on --features fallback-llama-cpp-2 (which doesn't support K2)	Reload with previous known-good model; do not retry the bad architecture. Log the specific GGUF general.architecture value. Shipping path uses mainline llama.cpp b11454+; only fires when the fallback feature is enabled.

E\_EAGLE\_DRAFT	E	EAGLE-3 draft load fails at Deep load	Fall back to LiLiCorr; continue with reduced throughput.

E\_OCR\_DIMENSION	E	Image dimension mismatch reaching backend	Preprocessing wrapper should have normalized it; log and skip the frame (do not crash).

E\_OCR\_MODEL	D	OCR model file hash mismatch	Quarantine lifecycle.

E\_VLM\_BACKEND	E	VLM backend load fails	Disable scene understanding for the session.

E\_VLM\_QUERY\_COMPOUND	E	Compound query reaches backend	sul-agent should have decomposed it; log and process first instruction only, then queue the rest.

E\_PRO\_INTENT	E	sul-prosody crash	Respawn with default\_safe\_intent = "assess" and log.

E\_TOOL\_UNFORMABLE	B	Verification escalation exhausted; tool call cannot be formatted	User sees: "I can't format this call reliably. Here's what I have — {partial}. You may want to do this manually."

E\_IPC\_CHANNEL\_DEAD	C	IPC channel dead	Recreate channel, replay last message, 3× fail → degraded.

E\_INFER\_TIMEOUT	A	LLM inference timeout	Retry once, halve context, retry, CPU-only retry, mark degraded.

E\_INFER\_INVALID\_OUTPUT	B	Invalid structured output from LLM	Retry 3× stricter schema, fall back to unstructured text.

E\_MODEL\_FILE\_CORRUPT	D	Model file corrupt	Quarantine lifecycle; prompt redownload.

E\_LORA\_MERGE\_MISMATCH	— (build fail)	cargo xtask verify-lora-merge fails cosine similarity >0.998 or output-token equality >98%	Build fails.

E\_PACK\_SIG\_INVALID	— (load refuse)	Pack Ed25519 signature mismatch	Refuse to load pack.

E\_PACK\_SCHEMA\_INVALID	— (load refuse)	Pack schema validation fails	Refuse to load pack.

E\_PACK\_VERSION\_MISMATCH	C	Pack version mismatch (newer than spec)	Migration path; if migration fails, refuse to load pack.

E\_HIVE\_FETCH\_FAILED	C	Hive fetch failed	Cache-when-fresh, network-when-stale; rate-limited to 5/session.

E\_HIVE\_SIG\_INVALID	D	Hive pack signature invalid	Refuse to load; log.

E\_TRUST\_PRIMITIVE\_WRITE	F	Write attempt to a trust primitive file	Fatal. The Coder is not allowed to touch trust primitives; a write attempt is a violation of the write restrictions.

E\_WATCHDOG\_TAMPER	E	Trust primitive file modified	Mode D quarantine lifecycle; hive threat log.

E\_QUARANTINE\_RESTORE\_FAILED	C	Quarantined file restore fails	Log; quarantined file retained.

E\_SAVE\_VERSION\_MISMATCH	— (refuse)	Shadow version ≠ current save version	Refuse to restore; user must resolve version mismatch.

E\_OVERLAY\_INIT\_FAILED	E	External DComp window init fails	Log; overlay disabled for session.

E\_WGC\_CAPTURE\_FAILED	C	Screenshot ring capture fails	Screenshot ring degraded; overlay still functional.

E\_AUDIO\_DEVICE\_LOST	D	WASAPI device invalidated	Re-query, re-decide, resume.

E\_CLIP\_ENCODE\_FAILED	C	Clip encode failed	Log; clip not produced.

E\_EMU\_CORE\_LOAD\_FAILED	D	libretro core .dll fails to load	Report to user; Gambatte fallback.

E\_MOD\_INSTALL\_FAILED	— (refuse)	Staged package fails integrity or signature check	Package quarantined; install refused.

E\_OPTIMIZE\_AC\_BLOCKED	B	anti\_cheat.kernel\_level = true on target game	Pack type refused; sul-optimize not run for this title.

E\_NET\_TRIPWIRE	— (build fail)	Undeclared network access detected	Build fails (ac-regression).

E\_WRITE\_SITES\_VIOLATION	— (build fail)	AST guard finds a write site in a forbidden module	Build fails (ac-regression).

E\_MINHOOK\_ON\_AC\_TITLE	B	Flip-model overlay attempted on kernel\_level: true	E\_REFUSAL\_POLICY — refusal, flip-model path refused.

E\_RUNTIME\_AC\_DETECTED	B	Kernel-level anti-cheat detected at launch	E\_REFUSAL\_POLICY — structural refusal.

E\_REFUSAL\_POLICY	B	Structural refusal triggered	Refuse the action; the game/system continues without the refused feature.

E\_KANI\_INVARIANT\_FAIL	— (build failure)	Kani formal verification invariant violated	Fatal for the build; the binary is not shipped.

E\_MIRI\_UB	— (build failure)	Miri detected undefined behavior	Fatal for the build; the binary is not shipped.

E\_RELEASE\_FEATURE\_MISMATCH	C	Feature toggle enabled from a later release class than the installed binary	Recoverable config error — the feature is disabled and a warning is emitted. If the user reinstalls the correct release class, the feature becomes available.

E\_RELEASE\_UPDATE\_SKIP	B	Update manifest jumps the release ladder	The update is refused; the currently-installed version continues to run.



Vault (Part 2 §2.13)



Code	Mode	Trigger	Recovery



E\_VAULT\_UNLOCK\_FAILED	informational	Auth factors fail or TPM unavailable	Vault remains locked; session continues without vault access.

E\_VAULT\_TAMPER\_DETECTED	E	Header/manifest signature mismatch	Vault refuses to open entirely; whole vault to quarantine; session continues without vault access; user notified; hive threat log.

E\_VAULT\_HARDWARE\_ROOT\_UNAVAILABLE	informational	TPM + DPAPI both unavailable	Allow passphrase-only mode with Tier 1 confirmation; or refuse.

E\_VAULT\_DECOY\_ACCESSED	E	Decoy vault opened	VaultDecoyAccessed telemetry; hive threat log; user notification; optional cooldown on real vault.

E\_VAULT\_RATE\_LIMITED	informational	Too many failed unlock attempts	5 min lock (5 failures); 24 h lock + TPM PIN reset (20 failures).



Failure-injection flags. vt-1 (unlock fail), vt-2 (tamper detected), vt-3 (hardware root unavailable), vt-4 (decoy accessed), vt-5 (rate limited).



Emulation (Part 3 §3.17)



Code	Mode	Trigger	Recovery



E\_GB\_MAPPER\_UNSUPPORTED	B	MBC5 rumble / HuC1 / MBC6 cartridge	Refuse to load; Gambatte fallback offered.

E\_GB\_ACCURACY\_GATE\_FAILED	B	A.90 gate fails on current hardware	Forced to libretro\_gambatte for the session.

E\_GB\_ROM\_CORRUPT	D	Cartridge header parse or ROM-size consistency mismatch	Quarantine lifecycle.

E\_SAVESTATE\_SIZE\_MISMATCH	C	Snapshot size differs from state\_size()	Run-ahead disabled for the affected core.

E\_SAVESTATE\_WORKER\_STALLED	C (persistent → F)	Worker doesn't return within budget	Run-ahead disabled for the session; persistent across sessions → Mode F (Coder reviews worker-drain logic).

E\_SAVESTATE\_ROUND\_TRIP\_MISMATCH	C	Restore produces different state	Slot dropped without affecting emulator.

E\_EMU\_CORE\_LOAD\_FAILED	D	libretro core .dll fails to load	Quarantine lifecycle if the core file is corrupt; otherwise user notification.

E\_OPTIMIZE\_AC\_BLOCKED	B	kernel\_level: true on target game	Pack type refused.

E\_VFS\_HANDLE\_CACHE\_STALE	C	Cached handle no longer valid	Fall back to non-cached passthrough.



Failure-injection flags. ss-1 (size mismatch), gb-1 (mapper unsupported), save-1 (shadow failed), save-2 (version mismatch).



Audio (Part 4 §4.13)



Code	Mode	Trigger	Recovery



E\_DSP\_ARTIFACT\_UNSTABLE	E (persistent → F)	Stability check fails on load	Refuse to load, fall back to AVX2 hand-written chain. Persistent across sessions → Coder review of the DSP artifact stability check.

E\_DSP\_ARTIFACT\_HASH	D	Hash doesn't match signed manifest	Refuse to load, log.

E\_VOICE\_MODEL\_MISSING	D	Model file not present at model\_path	Disable voice isolation, log.

E\_VOICE\_SIMD\_UNSUPPORTED	— (refuse)	No supported SIMD path available	Disable voice isolation, log.

E\_AG\_FIR\_OVERFLOW	C (persistent → F)	Longest active FIR exceeds 1024-sample buffer	Drop longest FIR, log AudioFilterDropped. Persistent across sessions → Coder review of FIR/buffer arithmetic.

E\_AG\_DEVICE\_LOST	D	WASAPI AUDCLNT\_E\_DEVICE\_INVALIDATED	Re-query, re-decide, resume.

E\_AG\_EXCLUSIVE\_BLOCKED	B	Another app using device	Refuse, surface reason.

E\_AG\_HRTF\_OVERRUN	C	DSP p99 >800 µs with HRTF	Disable HRTF, log.

E\_ROUTING\_VIRTUAL\_CABLE\_MISSING	B	Configured cable not detected	Routing matrix not enabled for session; base DSP pipeline continues.

E\_ROUTING\_DEVICE\_IN\_USE	B	Another app using device	Routing matrix not enabled for session; base DSP pipeline continues.

E\_ROUTING\_DRIFT\_EXCEEDED	E (persistent → F)	Sample-level drift over 1 hour	Routing matrix disabled for session (escalates to Mode F across sessions); base DSP continues.

E\_ROUTING\_LIMITER\_OVERFLOW	C	Limiter saturation	Reduce limiter aggressiveness.

E\_VOICE\_FX\_BUDGET\_EXCEEDED	C	Combined DSP p99 > baseline + 50%	Governor disables lowest-priority FX (ring mod → bitcrush → formant → pitch).

E\_VOICE\_FX\_NEURAL\_VC\_TOO\_SLOW	informational	A.88 gate not passed	neural\_vc forced to "off".

E\_VOICE\_CLONE\_REFUSED	informational	Attempt to clone specific real person	Refused — the refusal is the intended behavior.

E\_STT\_MODEL\_NOT\_FOUND	C	Bundled model missing	Fall back to bundled default.

E\_STT\_WHISPER\_INIT\_FAILED	A	whisper.cpp fails to initialize	Retry once with CPU-only; if that fails, disable STT for session.

E\_STT\_AUDIO\_CTX\_INVALID	C	audio\_ctx not in {0, 768, 1500}	Clamp to default and log.

E\_STT\_FLASH\_ATTN\_UNAVAILABLE	C	Backend doesn't support flash\_attn	Fall back to non-flash path — still functional, slower.

E\_STT\_HYBRID\_SPLIT\_UNSUPPORTED	C	GPU not present	Fall back to gpu\_both if GPU present, else cpu\_both.



Failure-injection flags. rt-1 (DSP artifact reload fail), vfx-1 (voice FX budget exceeded).



Music player (Part 5 §5.13)



Code	Mode	Trigger	Recovery



E\_MUSIC\_DEVICE\_CONFLICT	B	Exclusive mode refused (device coordination)	Fall back to shared mode with warning; UI shows reason.

E\_MUSIC\_BIT\_PERFECT\_UNAVAILABLE	informational	Exclusive mode unavailable or DSP active	Badge hidden; shared mode used.

E\_MUSIC\_TAG\_BACKUP\_FAILED	B	Backup copy fails before tag write	Refuse tag write; user must resolve backup failure.

MU-1	B	sul-audio loopback or voice loop active on same device	Music player falls back to shared mode, warning.

MU-2	informational	Exclusive mode unavailable or DSP active	Bit-perfect badge hidden, UI shows reason.



Failure-injection flags. mu-1 (music exclusive mode refused), mu-2 (music bit-perfect path unavailable).



Broadcast (Part 6 §6.13)



Code	Mode	Trigger	Recovery



E\_BROADCAST\_ADMISSION\_REFUSED	B	Over-budget configuration	Refuse to start stream with specific message.

E\_BROADCAST\_USBIP\_ENUM\_FAILED	C	Virtual audio device server fails to enumerate	Fall back to VBAN bridge.

E\_BROADCAST\_BUS\_DRIFT\_EXCEEDED	E	Sample-level drift across buses over 1 hour	Disable routing matrix for session; base DSP pipeline continues.

E\_BROADCAST\_ENCODER\_OVERLOAD	C	Admission controller downgrade exhausted, still over budget	Refuse stream start.

E\_BROADCAST\_AUDIO\_UNDERRUN	C	Audio callback missed deadline	Governor adjusts buffer size / drops FX.



Failure-injection flags. br-1 (broadcast admission controller refusal), br-2 (broadcast USB/IP device enumeration failure), br-3 (broadcast audio bus drift exceeded).



Overlay (Part 7 §7.10)



Code	Mode	Trigger	Recovery



E\_OVERLAY\_INIT\_FAILED	E	External DComp window init fails	Log; overlay disabled for session.

E\_WGC\_CAPTURE\_FAILED	C	Screenshot ring capture fails	Screenshot ring degraded; overlay still functional.

E\_MINHOOK\_ON\_AC\_TITLE	B	Flip-model overlay attempted on kernel\_level: true	E\_REFUSAL\_POLICY — refusal, flip-model path refused.

E\_WIRE\_RING\_NOT\_MAPPED	C	Ring mapping fails at startup	Fall back to Tauri IPC path for tokens.

E\_WIRE\_FRAME\_CORRUPT	C	Frame corruption detected	Drop frame, log, continue.

E\_WIRE\_BUFFER\_DETACHED	C	ArrayBuffer detached	Remap ring via WireRemapRing control message.

E\_RDP\_WGC\_FALLBACK	C	RDP session detected	Screenshot ring becomes request-scoped at 2s / 5 Hz.



Failure-injection flags. None at the crate level (the fallback path itself is the recovery).



Vision and search (Part 8 §8.10)



Code	Mode	Trigger	Recovery



E\_OCR\_DIMENSION	E	Image dimension mismatch reaching backend	Log and skip frame (preprocessing should have caught).

E\_OCR\_MODEL	D	Model file hash mismatch	Quarantine lifecycle.

E\_VLM\_BACKEND	E	Backend load fails	Disable scene understanding for session.

E\_VLM\_QUERY\_COMPOUND	E	Compound query reaches backend	Process first instruction only, queue rest (sul-agent should have decomposed).

E\_SEARCH\_PIPELINE\_DEGRADED	C	Any pipeline component unavailable	Fall back to direct fetch with truncated context.

E\_SEARCH\_NO\_RESULTS	C	Empty result for critical query	Mark claim unsupported; model abstains.

E\_SEARCH\_CITATION\_UNRESOLVED	C	Citation marker could not be resolved	Strip sentence via citation filter.

E\_VERIFICATION\_AMBIGUOUS	non-fatal annotation	Deterministic verdict could not be established	Mark claim contested; surface both sides.

E\_VERIFICATION\_FALLACY\_DETECTED	non-fatal annotation	Rule-based detector flags a critical claim	Annotate in packet; model abstains or requests clarification.

E\_GITHUB\_RATE\_LIMITED	C	Rate limit hit	Log, return empty result set.

E\_GITHUB\_AUTH\_FAILED	C	API auth token invalid	Disable github\_search for session.



Failure-injection flags. None specific.



Frame generation (Part 9 §9.7)



Code	Mode	Trigger	Recovery



E\_FG\_LAYER\_LOAD\_FAILED	C	vkNegotiateLoaderLayerInterfaceVersion fails	Fall back to win\_fg, log FrameGenFallback.

E\_FG\_DLL\_MISSING	C	Lossless.dll not present or import fails	Fall back to win\_fg.

E\_FG\_VRAM\_RESERVE\_UNAVAILABLE	B	free\_vram below reservation + margin	Refuse FG, FrameGenRejected { InsufficientVram }.

E\_FG\_BASE\_FPS\_BELOW\_THRESHOLD	B	Median <30 fps in 5s or 30s window	Refuse FG, FrameGenRejected { BaseFpsBelowThreshold }.

E\_FG\_NATIVE\_FPS\_UNKNOWN	B	Config reader cannot determine native rate	FrameGenUnavailable { NativeFpsUnknown }.

E\_FG\_FRAME\_CORRUPTED	C	Pixel hash mismatch across consecutive presents	Disable FG for session, log.

E\_FG\_VULKAN\_INCOMPATIBLE	C	No compute-shader capability	Fall back to win\_fg, else refuse.

E\_FG\_AC\_REFUSED	B	Kernel-level anti-cheat detected	E\_REFUSAL\_POLICY — structural refusal.

E\_FG\_UNSAFE\_TITLE\_REFUSED	B	fg\_unsafe flag set on title and not overridden	Refuse FG.



Failure-injection flags. fg-1..fg-7 (per Part 9).



AI (Part 10 §10.24)



KV tier / speculation



Code	Mode	Trigger	Recovery



E\_STAR\_KV\_QUALITY	C	PPL regression >2% vs f16 on A.18 eval	Fall back to GAAC (KV-2).

E\_STAR\_KV\_DECOMPRESS	C	NaN/Inf in decompressed KV	Fall back to GAAC.

E\_STAR\_KV\_FALLBACK\_EXHAUSTED	E	All fallback rungs exhausted	Refuse to load.

E\_STAR\_KV\_UNSUPPORTED\_TREE	— (validator)	Model tree not supported	Refuse (rejected by validator; unreachable in shipping path).

E\_STAR\_KV\_SURGERY\_VERSION\_MISMATCH	D	Model surgery version ≠ runtime version	Refuse to load, prompt re-download.

E\_STAR\_KV\_PAIR\_MISMATCH	D	Sibling uncompressed model not found	Refuse to load.

E\_GAAC\_ROUND\_TRIP	E	Bit-level mismatch	Fall back to uncompressed KV.

E\_KV\_COLD\_READ	E	Cold-tier read returns error	Fall back to warm-tier-only.

E\_KV\_COLD\_READ\_FAILED	C	Cold-tier read fails and warm fallback unavailable	Fall back to hot/warm only.

E\_KV\_ASYMMETRIC\_FA	E	K/V asymmetric when FA on	Refuse to start.

E\_STAR\_KV\_QUALITY (KL@D)	C	KL divergence exceeds f16 baseline	Fall back to GAAC (KV-8).



SP-1	C	UniSpec acceptance <0.3	Fall back to n-gram.

SP-2	C	Phrase head MLP weights corrupt	Fall back to UniSpec.

SP-3	C	ChainFlow integrator diverges	Fall back to LiLiCorr (Deep) / UniSpec (Combat Full).

SP-4	C	EAGLE-3 acceptance <1.8	Fall back to n-gram.

SP-5	C	Draft tokens inconsistent with target logits	Retry without draft for this turn.

SP-6	— (refuse)	Draft tokenizer ≠ target tokenizer	Refuse speculation for tier.

SP-7	C	Named rung not compiled in or model missing	Skip to next rung.

SP-8	— (validator)	Draft model loaded but no --spec-type	Config validator closes at startup (E\_SPEC\_TYPE\_UNSET).

SP-9	— (validator)	Config error	E\_SPEC\_CHAIN\_UNTERMINATED at startup.

SP-10	C	Verify pass exhausts VRAM	Drop to next rung with smaller max draft.

SP-11	C	PyTorch weights corrupt	Fall back to phrase head.

SP-12	— (startup abort)	Config error	E\_MEDUSA\_TEMP\_SCALING\_MISSING at startup (acceptance drops ≥30% without it).

SP-13	— (startup abort)	Config error	E\_MEDUSA\_OWN\_HEAD\_VARIANT at startup (would defeat the general combat ceiling).

SP-14	C	LiLiCorr scorer transformer corrupt	Fall back to UniSpec.



MS-1	— (refuse)	Surgery version mismatch	E\_STAR\_KV\_SURGERY\_VERSION\_MISMATCH — refuse to load.

MS-2	— (refuse)	Sibling uncompressed model not found	E\_STAR\_KV\_PAIR\_MISMATCH — refuse to load.

MS-3	C	Post-surgery A.18 gate fail	Fall back to GAAC (model's sibling is usable uncompressed).

MS-4	N/A	MBZUAI-IFM fork lacks STAR-KV support	Obsolete — fork was deleted after b11454 mainline merge.



RT-1	D	InternViT attention layers 6–12 not accessible	Fall back to full-token baseline for session; E\_RETRACE\_UNAVAILABLE.

RT-2	C	Dormant buffer exceeds capacity	Drop oldest dormant tokens; E\_RETRACE\_DORMANT\_OVERFLOW.

RT-3	C	Dormant restore at backtracking layer fails	Fall back to full-token baseline for that pass; E\_RETRACE\_BACKTRACK\_INJECTION. Persistent → Mode E (disable ReTrace for session).

RT-4	E	Pre-created contexts unavailable, or --reasoning-budget fails to flip	E\_COMBAT\_PROFILE\_INVALID — router disabled for session; session falls back to Instruct-only if Instruct context available; if no context available, Combat Full is dropped entirely for session.



MO-1	C	Thinking OOM or unavailable when selected; Instruct takes over	CombatRouterFallback logged; instruct\_fallback\_rate\_pct tracked; session continues in Instruct for that turn.

MO-2	informational	User forced Instruct; destructive action detected; router overrode to Thinking	E\_DESTRUCTIVE\_INSTRUCT\_OVERRIDE logged; CombatProfileDenied telemetry emitted; user sees profile override notice; cannot be disabled without destructive\_action\_always\_thinking = false (Tier 1).

MO-3	informational	router\_enabled: false and destructive\_action\_always\_thinking = true; destructive action detected in fixed Instruct profile	Safety invariant still applies: destructive action escalates to Thinking even without the router; logged as MO-2.

MO-4	C	Deterministic signals conflict, small-LLM fallback fails	Default to Instruct; log E\_COMBAT\_PROFILE\_AMBIGUOUS.

MO-5	— (validator)	\[ai.combat] profile = "conversation" with \[net.search] backend = "disabled"	E\_CFG at startup — Conversation profile depends on search.



CP-1	— (gate fail)	A.101 fail (HumanEval <60)	Coding profile stays research; Deep is the only coding path.



QT-1	C	SPEED-Q 4-bit regression vs A.63 gates	Fall back to llama.cpp Q4\_K\_S.

QT-2	C	SPEED-Q 2-bit >2% absolute regression vs A.63 gates	Fall back to SPEED-Q 4-bit.

QT-3	C	APEX mixed-precision GGUF fails A.63	Fall back to SPEED-Q 4-bit uniform.



KC-1	C	Selected backend fails PPL + KL@D gates	Fall back to STAR-KV/GAAC per trait fallback order.

KC-2	C	PalQuant identity test fails	Fall back to GAAC (lossless), which passes.



SD-1	— (dormant)	Draft cache ≤32 MB	Wrapper not engaged.

SD-2	C	Worker thread doesn't refresh in time	Drop to RIFT-stat (static), log.



TP-1	C	Cross-encoder fails ranking/end-to-end gates	Fall back to no-PRM baseline.

TP-2	C	Cross-encoder weights corrupt	Fall back to no-PRM baseline.

TP-3	C	Cache hit rate below 20% (unexpected)	Disable cache, log E\_TOOLPRM\_CACHE\_THRASH.



E\_COMBAT\_PROFILE\_INVALID	E	Contexts unavailable / budget flip fails	See RT-4.

E\_COMBAT\_ROUTER\_INIT\_FAILED	C	Router fails to initialize at startup	Session runs fixed profile (default: Instruct); overlay shows warning chip on CombatProfileIndicator.

E\_DESTRUCTIVE\_INSTRUCT\_OVERRIDE	informational	User forced Instruct; destructive action detected	Logged as warning, not error.

E\_DESTRUCTIVE\_PROTECTION\_DISABLED	informational	destructive\_action\_always\_thinking = false	Logged as warning.

E\_QUANT\_SPEED\_Q\_FAILED	C	A.96 fail	Fall back to llama.cpp Q4\_K\_S.

E\_QUANT\_APEX\_PLANNER\_FAILED	C	QT-3	Fall back to SPEED-Q 4-bit uniform.

E\_QUANT\_GGUF\_NOT\_K\_QUANT	C	Custom quant format detected	Fall back to llama.cpp Q4\_K\_S.

E\_KV\_COMPRESS\_BACKEND\_UNAVAILABLE	C	KC-1	Fall back to STAR-KV/GAAC.

E\_KV\_COMPRESS\_IDENTITY\_FAILED	C	KC-2	Fall back to GAAC.

E\_RIFT\_REFRESH\_STALLED	C	SD-2	Drop to RIFT-stat.

E\_TOOLPRM\_LOAD\_FAILED	C	TP-2	Fall back to no-PRM baseline.

E\_TOOLPRM\_CACHE\_THRASH	C	TP-3	Disable cache.

E\_MEDUSA\_TEMP\_SCALING\_MISSING	— (startup abort)	SP-12	Config-error; abort startup.

E\_MEDUSA\_OWN\_HEAD\_VARIANT	— (startup abort)	SP-13	Config-error; abort startup.

E\_COMBAT\_PROFILE\_AMBIGUOUS	C	MO-4	Default to Instruct.

E\_SPEC\_TYPE\_UNSET	— (validator)	SP-8	Config validator closes at startup.

E\_SPEC\_CHAIN\_UNTERMINATED	— (validator)	SP-9	Config error at startup.



Failure-injection flags. kv-1, kv-8, kc-1, kc-2, sp-2, sp-11, sp-12, sp-13, sp-14, mo-1, mo-2, mo-4, rt-4, qt-1, qt-2, qt-3, sd-1, sd-2, tp-1, tp-2, tp-3.



Mode F cascading escalation



Repeated E\_DSP\_ARTIFACT\_UNSTABLE across sessions → Coder proposes repair diff. ReTrace or search pipeline cascade → Coder proposes repair diff. Persistent E\_OODA\_INCOMPLETE across sessions → Coder reviews prompt scaffold logic. Persistent E\_TSCG\_COMPILE\_FAILED on specific schemas → Coder reviews TSCG compiler. Persistent E\_QUANT\_SPEED\_Q\_FAILED → Coder reviews the APEX planner or SPEED-Q wrap. Persistent E\_KV\_COMPRESS\_BACKEND\_UNAVAILABLE → Coder reviews the backend A.97 quality gates. Persistent E\_TOOLPRM\_CACHE\_THRASH → Coder reviews the score-cache key design. Persistent E\_MEDUSA\_OWN\_HEAD\_VARIANT misconfiguration → Coder reviews the config (a Mode F here means a validator bypass). Repeated E\_COMBAT\_ROUTER\_INIT\_FAILED across sessions → Coder reviews router initialization and pre-created context allocation. Persistent instruct\_fallback\_rate\_pct above tolerance → Coder reviews the router's fallback handling for Thinking OOM. Recurring E\_COMBAT\_PROFILE\_INVALID → Coder reviews VRAM reservation and context sizing for Combat Full's four profiles.



Telemetry, diagnostics, crash (Part 12)



Code	Mode	Trigger	Recovery



E\_METRICS\_RING\_WRAP	C	Slot state machine detects torn write during wrap	Drop in-flight slot, log, continue.

E\_METRICS\_ROLLUP\_FAILED	C	SQLite rollup write fails	Log, retry next interval.

E\_METRICS\_QUERY\_TIMEOUT	C	mmap direct query exceeds budget	Fall back to SQLite rollup query.

E\_CRASH\_DUMP\_WRITE\_FAILED	—	MiniDumpWriteDump fails (permissions, disk full)	Log to event log; process continues / terminates per its panic mode.

E\_DIAG\_CHECK\_TIMEOUT	—	Any check exceeds its budget	Log, continue with next check; check marked "unknown" in DiagnosticsPanel.

E\_SAVE\_SHADOW\_FAILED	C	Filesystem snapshot fails (permissions, disk full)	Log; save continues without shadow.

E\_SAVE\_RESTORE\_FAILED	—	Snapshot restore fails	Log; original save unaffected; user notified.

E\_QUARANTINE\_RESTORE\_FAILED	C	Quarantined file restore fails	Log; quarantined file retained.

E\_RDP\_WGC\_FALLBACK	C	RDP session detected	Screenshot ring becomes request-scoped at 2s / 5 Hz.

E\_SCREENSHOT\_RING\_FULL	C	Producer outpaces consumer	Drop oldest frame.

E\_WATCHDOG\_TAMPER	E	Trust primitive file modified	Mode D quarantine lifecycle; hive threat log.

E\_WATCHDOG\_HOOK\_DETECTED	—	API hook in SUL's address space	Log, notify, optional auto-disable flip-model.

E\_WATCHDOG\_RWX\_REGION	—	RWX region not mapped by SUL	Log, notify.

E\_DIAG\_BUNDLE\_WRITE\_FAILED	—	Zip write fails (disk full)	Log; user notified; partial bundle offered.



Hardware (Part 13)



Code	Mode	Trigger	Recovery



E\_OS\_TUNING\_SCRIPT\_FAILED	—	Script returns non-zero (permissions, missing registry key)	Log; revert partial changes; user notified.

E\_OS\_TUNING\_REVERT\_FAILED	—	Revert script cannot restore original state	Log; user notified; .revert.json retained for manual recovery.

E\_ALLOC\_LARGE\_PAGE\_UNAVAILABLE	C	VirtualAlloc fails with large-page flag	Fall back to 4 KB pages; log; continue.

E\_ALLOC\_OUT\_OF\_MEMORY	—	Allocation fails	Refuse to load tier; user notified.



Safety and watchdog (Part 14 §14.18)



Code	Mode	Trigger	Recovery



E\_TRUST\_PRIMITIVE\_WRITE	F	Write attempt to a trust primitive file	Fatal — the Coder is not allowed to touch trust primitives.

E\_WATCHDOG\_TAMPER	E	Trust primitive file modified	Quarantine + hive threat log.

E\_QUARANTINE\_RESTORE\_FAILED	C	Quarantined file restore fails	Log; quarantined file retained.

E\_REFUSAL\_POLICY	B	Structural refusal triggered	Refuse the action; the game/system continues without the refused feature.

E\_KANI\_INVARIANT\_FAIL	— (build failure)	Kani formal verification invariant violated	Fatal for the build; the binary is not shipped.

E\_MIRI\_UB	— (build failure)	Miri detected UB	Fatal for the build; the binary is not shipped.

E\_RELEASE\_FEATURE\_MISMATCH	C	Feature toggle enabled in a release class where it does not exist	Recoverable config error — the feature is disabled and a warning is emitted.

E\_RELEASE\_UPDATE\_SKIP	B	Update attempted to skip a release class	The update is refused; the currently-installed version continues to run.



Handoff features (Part 15 §15.17)



Code	Mode	Trigger	Recovery



E\_CSI\_NODE\_DISCONNECTED	C	ESP32 node unreachable	Presence detection disabled; retain last known vitals with timestamp.

E\_CSI\_MODEL\_LOAD\_FAILED	C (→ D)	Edge inference model corrupt	Fall back to host-side processing; Mode D quarantine on the model file.

E\_BCI\_CALIBRATION\_FAILED	informational	Signal quality too poor	Prompt for recalibration; disable BCI for session.

E\_BCI\_MODEL\_LOAD\_FAILED	D	brain2qwerty model corrupt	Fall back to no-BCI operation; Mode D quarantine on the model file.

E\_GEV\_FEED\_UNAVAILABLE	C	Feed timeout	Use cached tiles; retry with backoff.

E\_GEV\_GEOLOCATION\_MODEL\_UNAVAILABLE	D	geo-sleuth model missing	Disable geolocation; retain GEV visualization.

E\_SECURITY\_TOOL\_CONTAINER\_FAILED	B	Docker container fails to start	Report error; do not execute scan.

E\_SECURITY\_TOOL\_GPU\_UNAVAILABLE	B	GPU required, none available	Refuse cracking; suggest CPU alternative.

E\_VOICESTUDIO\_MODEL\_MISSING	C	TTS or character voice model missing	Fall back to Piper TTS.

E\_VOICESTUDIO\_CLONE\_REFUSED	informational	Clone specific real person attempt	Refused — the refusal is the intended behavior.

E\_HERETIC\_QUALITY\_GATE\_FAILED	— (gate fail)	Abliterated model regresses >2%	Do not ship; toggle hidden.

E\_LOCAL\_UNCENSORED\_LICENSE\_INVALID	— (gate fail)	Model license incompatible	Refuse to load.

E\_LOCAL\_UNCENSORED\_QUALITY\_GATE\_FAILED	— (gate fail)	Model regresses >2%	Refuse to load.

E\_BROWSER\_NAV\_FAILED	C	Page load timeout	Retry once, then report failure.

E\_BROWSER\_CDP\_DISCONNECTED	C	Browser daemon crash	Restart browser daemon.

E\_BROWSER\_SANDBOX\_ESCAPED	C (severe)	Sandbox violation detected	Kill browser; quarantine profile.

E\_RPPG\_NO\_FACE	C	Face detector fails	Skip frame, retry.

E\_RPPG\_COMPUTE\_FAILED	C	GPU compute shader fails	Fall back to CPU path.



Failure-injection flags. csi-1, csi-2, bci-1, bci-2, gev-1, gev-2, sec-1, sec-2, vs-1, vs-2.



Phase 3 failure codes (Ch. 11 §2.19 / Ch. 15 §15.6 / Ch. 5 §3.9 / Ch. 4 §2.5)



Code	Mode	Trigger	Recovery



E\_HIGGS\_AUDIO\_INIT\_FAILED	C	Higgs Audio engine fails to initialize	Fall back to Piper (Mode C).

E\_JELLYFIN\_UNREACHABLE	C	Jellyfin server unreachable	Fall back to local library\_paths; warning (Mode C).

E\_JELLYFIN\_AUTH\_FAILED	C	Jellyfin API key invalid	Disable Jellyfin source for session; local paths continue (Mode C).

E\_NEXTCLOUD\_SYNC\_FAILED	C	Nextcloud sync fails	Fall back to git backend; warning (Mode C).

E\_LIBRETRANSLATE\_UNAVAILABLE	C	LibreTranslate unavailable	Fall back to whisper.cpp translation (Mode C).

E\_LOCALSEND\_TRANSFER\_FAILED	C	LocalSend transfer fails	Report failure; no state change (Mode C).

E\_CONTAINER\_TOOL\_FAILED	B	Container tool (GIMP, Stirling PDF, ArchiveBox, etc.) fails	Report error; do not execute (Mode B).

E\_WASM\_ISOLATION\_FAILED	B	WASM sandbox cannot isolate the tool	Refuse to run the tool in WASM; fall back to Docker (Mode B).

E\_PEXELS\_RATE\_LIMITED	C	Pexels rate limit hit	Return empty result set; log (Mode C).

E\_HIBP\_RATE\_LIMITED	C	HaveIBeenPwned rate limit hit	Return empty result set; log (Mode C).

E\_VIRUSTOTAL\_RATE\_LIMITED	C	VirusTotal rate limit hit	Return empty result set; log (Mode C).

E\_TINEYE\_RATE\_LIMITED	C	TinEye rate limit hit	Return empty result set; log (Mode C).

E\_NINITE\_INSTALL\_FAILED	B	Ninite install fails	Report error; user notified (Mode B).

E\_MAXUN\_SCRAPE\_FAILED	C	Maxun scrape fails	Report error; fall back to web\_fetch (Mode C).



Optimization, Agent, Media & Sensing Extensions (Phase 4 — Handoff Integration)

Code	Mode	Trigger	Recovery

E_KERNEL_AUTOTUNE_FAILED	C	Magnitude `--backend-autotune` probe fails or kernel cache corrupt	Fall back to default `llama.cpp` kernels; log warning.

E_PPTX_GENERATE_FAILED	C	`pptx_generate` container fails or native shape validation <95%	Fall back to Markdown export in `%APPDATA%\SUL\exports\`.

E_FILE_CONVERTER_FAILED	C	`filee` offline converter fails on unsupported MIME/corrupt input	Pass raw file or report unsupported format to user.

E_POSE_BACKEND_UNAVAILABLE	C	Multi-cam `skellytracker`/`RTMPose` ONNX backend unavailable	Fall back to no-pose input; retain other sensing inputs.

E_SPYWARE_SCAN_IOC_UNAVAILABLE	C	Zero-Breach IOC signature bundle missing or corrupt	Fall back to signature-only `sul-watchdog` checks.

E_SPYWARE_SCAN_POSITIVE	informational	Zero-Breach offline IOC scan detects suspicious indicator	Report finding to user and log to `security_history.json`; never auto-delete any file.

E_WIFI_AUDIT_NO_ADAPTER	B	No compatible monitor-mode wireless NIC detected for `wifi_audit`	Refuse scan; surface hardware requirement.


Mode F escalation summary



Every Mode A–E code above is a candidate for Mode F when it repeats across sessions. Startup aborts and build failures are not candidates for Mode F — they are terminal for the run or the build. The escalation is: capture the pattern → Coder proposes a repair diff → user reviews and approves each diff individually. Hardcoded zero write to trust primitives. This is the self-healing watchdog's only path to code-generation, and it is bounded.
