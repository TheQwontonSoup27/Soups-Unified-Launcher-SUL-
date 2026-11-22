SUL-LEDGER/guards.md



Reading rule. Every AST guard and network tripwire. Enforced by cargo xtask ac-regression on every PR. Categorized by the owning subsystem. The guard is a build-time check; a PR that violates any guard fails CI.



Guards are grouped by the three tiers they defend: (1) structural invariants — a code change cannot remove the guard; (2) architectural invariants — the guard catches a regression against a design decision; (3) hygiene — the guard catches a style violation that would let a bug in.



Structural invariants



These are the code-level invariants. A refactor that removes one fails CI.



Electron exclusion



No electron, electron-builder, or @electron/\* in any manifest.

No nodeIntegration, contextBridge, webPreferences, BrowserWindow, BrowserView, <webview> in any file.

No asar anywhere.

No bundled Node.js runtime.

No Electron sidecar.

Gate: electron-audit (one of the three CI gates).



Tauri capability audit



No "csp": null in tauri.conf.json.

No withGlobalTauri: true in production.

No wildcard fs scopes.

No wildcard window capabilities.

No dangerousDisableAssetCspModification: true.

No shell:allow-execute granted to any window.

Gate: tauri-capability-audit (one of the three CI gates).



WebView runtime audit



WebView2 runtime floor ≥ 156.0.4314.58 (Nov 2026).

Tauri version floor ≥ 2.11.5.

plugin-updater ≥ 2.12.0.

plugin-dialog and plugin-http pinned to fixed versions.

Gate: webview-runtime-audit (one of the three CI gates).



WebView config guards



\[webview] isolation\_pattern must be true in shipped config.

\[webview] enhanced\_security\_mode must be "strict" in shipped config.

\[webview] runtime\_floor must be ≥ the current Microsoft security baseline.



Vault guards



sul-vault-core must not import any HTTP client.

sul-vault-core must not expose the DEK outside the vault thread.

sul-vault-core must not write plaintext to disk at any point (fuzz test at every write boundary).

sul-face-auth and sul-voice-auth must not store plaintext embeddings.

\[vault] extra\_paths must only be extended via OS file picker (AST check against config edit path).



App auth guards



sul-auth-app must not expose a bypass path in release builds.

\[auth] enabled = false must be a Tier 1 event.

\[auth] require\_password = false with no other factor enabled must be rejected (E\_CFG).
[auth.profiles] isolation guard: Profile A must never have read access to Profile B's vault key or `game_metadata`.



Voice cloning refusal



sul-voice-fx must not expose any voice-cloning entry point (the API surface is checked, not just the config).

\[audio.voice\_fx] voice\_clone must remain "refused" in every shipped config.

sul-voicestudio must not expose a "clone specific person" code path (AST check on the API surface — hard invariant).

sul-voicestudio must not import an HTTP client.

sul-voicestudio's character voice models must be loaded from signed manifests with per-model license identifiers.

sul-heretic runs at build time only, not at runtime — no runtime code path imports heretic.

The abliterated GGUFs follow the standard model-file signing chain.



Anti-cheat structural refusals



sul-fg must not be present in VK\_INSTANCE\_LAYERS for any anti\_cheat.kernel\_level == true game.

sul-fg does not import ReadProcessMemory.

sul-fg does not hook IDXGISwapChain::Present (it hooks vkQueuePresentKHR, which is the Vulkan present path).

\[frame\_gen] enabled = true with any per-game frame\_gen.engine != "off" and anti\_cheat.kernel\_level = true → rejected with E\_CFG.

\[frame\_gen] base\_fps\_gate < 25 → rejected (safety floor).

sul-broadcast-video must not import ReadProcessMemory.

sul-broadcast-video must not hook IDXGISwapChain::Present.

sul-broadcast must not bypass the encoder admission controller.

\[broadcast.encoder] admission\_control must be true in shipped config.

\[broadcast.video] capture\_backend = "game\_hook" must be refused on kernel-AC titles (AST check against game\_registry.json).



Coder trust primitive restrictions



sul-trust, sul-quarantine, sul-watchdog not touched by Coder PRs without Tier 1 approval modal (Gate A extension).

Coder worktree has no write access to %APPDATA%/SUL/trust/ (filesystem permission).



Security toolkit structural refusals



sul-security-toolkit's container must be enforced (AST check that no tool runs outside the container).

sul-security-toolkit's hashcat/nuclei/mitmproxy wrappers must not import wasapi or ReadProcessMemory.

sul-security-toolkit must refuse to run any tool against a target the user has not explicitly authorized (a signed consent file must exist).

sul-security-toolkit's scan output must be flagged data\_untrusted in the session packet (AST check on the packet-injection path).
sul-security-toolkit `wifi_audit` (`aircrack-ng`) must run as an external process (never statically linked, GPL boundary) and requires a signed consent file.
`security_audit` multi-stage diff verification must enforce independent verifier isolation — no agent instance may verify its own finding.



Handoff feature guards



sul-csi must not import ReadProcessMemory or any game memory access.

sul-csi's edge firmware must not accept unsigned models (ESP32 secure boot enforced).

sul-csi must not import an HTTP client (local USB/LAN only).

sul-csi's global sensor fusion path (when \[features] vision.global\_sensor\_fusion = true) must emit a user-visible modal listing every sensor being fused before the first session.

sul-bci must not import ReadProcessMemory.

sul-bci's real-time thread must be pinned and must not block on allocations (custom global allocator panic test).

sul-bci must not import an HTTP client (USB only).

sul-bci's decoded text must be flagged data\_untrusted in the session packet (AST check on the packet-injection path).

sul-gev must not import ReadProcessMemory.

sul-gev's ADS-B/AIS/satellite feed parsing must use only declared network surfaces.

sul-gev must not write to any feed.

sul-gev must not import any game-relevant memory access.

sul-gev's geolocation runs entirely locally — no image is uploaded.



Additional handoff / Phase 3 guards



sul-security-toolkit's WASM path must not import host filesystem or network APIs (AST check on the WASM module).

sul-security-toolkit's container tools must not mount the host filesystem rw (AST check on the container config).

sul-music's Jellyfin path must not transmit user library paths outside the configured server URL (AST check).

sul-packs' Nextcloud path must not transmit pack contents beyond the configured server URL (AST check).

sul-net's translate path must not transmit user text beyond the configured backend (AST check).

sul-net's LocalSend path must not transmit files beyond the LAN destination (AST check on destination parsing).

sul-tts's Higgs Audio path must not import an HTTP client (Higgs Audio runs locally; AST check).

sul-voicestudio's VoxCPM path must not expose a "clone specific person" code path (same AST guard as VoiceStudio).



Network tripwires



Every outbound network surface is declared. The ac-regression check fails on undeclared network access. Current declared surfaces:



Tauri IPC — custom protocol, not HTTP (explicitly allowed as a declared network surface).

RustXNG MCP server — out-of-process, https://localhost:<port>.

GitHub search API — https://api.github.com.

Hive-mind fetch — https://raw.githubusercontent.com/sul-hive-mind/.

Auto-updater — Ed25519-signed artifacts.

Sentry — optional, user-configurable, default local-only.

GEV public feeds — ADS-B/AIS/satellite public endpoints, read-only; the specific URL list is in \[gev] feed\_urls in config.toml and is user-verifiable.

Security toolkit target — the user's chosen target URL, declared at scan time.



Phase 3 additions (declared, user-configured, local-first):



Jellyfin (user-configured local server URL).

Nextcloud (user-configured local server URL).

LibreTranslate (user-configured local or self-hosted URL).

Pexels API (api.pexels.com).

HaveIBeenPwned API (haveibeenpwned.com).

VirusTotal API (virustotal.com).

TinEye API (tineye.com).

Ninite (ninite.com).

Maxun (user-configured local or container).

LocalSend (LAN only).



Owner additions (D-149, 2026-11-21; declared before any code calls them):



Steam Web API (api.steampowered.com) — public player data keyed by the user's SteamID64, read-only; any API key is user-supplied and kept out of source control and logs.

Discord webhook (discord.com) — the clipping engine's social/Discord toggle only; outbound posts only; the webhook URL is a secret and is masked in logs.



Nothing else. The agent, the search pipeline, the vault, the STT pipeline, the audio pipeline, the music player, the broadcast engine, the emulator pipeline, CSI, BCI, and VoiceStudio do not have their own HTTP clients.



Release-plan guards



releases.md must exist at the workspace root.

Every \[release.R<n>] block must have a \[reference\_config.R<n>] block.

Every component listed in a \[release.R<n>].components array must resolve to a real crate or a real \[features.\*] toggle.

Every A.x gate referenced must exist in gates.md.

The release-ladder superset invariant must hold (no component is removed between consecutive release classes).

Every Tier 1 event in releases.md must be either Tier 1-core or assigned to a specific release class.

The installed %APPDATA%/SUL/release.toml must match the binary's embedded release class (both must agree).



Feature toggle guards



No core feature may be togglable.

No structural refusal may be togglable.

\[performance] toggles must default on.

Gated features must default off until their gate passes.



Architectural invariants



These catch a regression against a design decision. A PR that violates one needs a design discussion, not just a code fix.



Base checks



No wasapi import in sul-dsp/sul-voice outside sul-audio.

sul-wire does not import an HTTP client.

sul-wire does not bypass sul-ipc's try\_push.

sul-voice model file is SHA-256 verified.

No wasapi import in sul-net/sul-agent outside sul-audio.

sul-agent does not import an HTTP client.

ReTrace does not bypass STAR-KV backpressure.

Search pipeline does not bypass sul-ipc try\_push.
`adblock-rust` filter engine must use FlatBuffers serialization (resident memory ≤50 MB).

sul-ai-native ReTrace paths do not import ReadProcessMemory.

OODA-Tool does not bypass sul-ipc backpressure.

InternVL3-2B captures via WGC/DXGI only.

sul-audio-governor does not import wasapi directly (uses sul-audio).

sul-audio-governor does not raise max\_buffer\_samples above the config ceiling.



Multi-profile guards



sul-vlm must not reference qwen35\_08b (removed backend).

sul-agent::combat\_profile must not import an HTTP client.

sul-agent::combat\_profile must not import wasapi.

The router's destructive\_action\_thinking config must be sourced only from \[ai.combat] and per-game combat blocks (no ad-hoc path).

\[ai.combat\_full.thinking] context must be strictly greater than \[ai.combat\_full.instruct] context (AST check against config.toml).

\[ai.combat\_full.conversation] context must be strictly greater than \[ai.combat\_full.thinking] context.

\[ai.speculative] must not contain combat\_instruct\_chain / combat\_thinking\_chain / combat\_conversation\_chain / combat\_coding\_chain keys (single combat\_chain only).

Medusa temperature scaling must be enabled.

Medusa own-LM-head variant must not be present.



Audio / STT / emulation guards



sul-audio must not import an HTTP client.

sul-voice-fx must not import wasapi directly (uses sul-audio's WASAPI layer).

sul-net must not import wasapi.

sul-stt (in sul-audio) must not import an HTTP client.

sul-stt must read only from the dual-track ring or sul-voice output, never opening a new audio stream.

\[stt] flash\_attn must be explicitly true (a missing field is an error, not a default).

sul-gb must not import wasapi.

sul-gb must not import ReadProcessMemory or any game memory access.

sul-savestate must not call retro\_serialize outside its libretro shim (the shim is the only place the C-ABI is called, and it always writes into a Rust-owned buffer).

sul-savestate's libretro shim must allocate its buffer once at retro\_serialize\_size() and never resize mid-session.

\[emu.savestate] worker\_threads must be 1.



Quant / KV-compress / Spec-draft / ToolPRM guards



sul-quant must emit pure K-quant GGUF (no custom quant format — verified by checking general.file\_type in the emitted GGUF against llama.cpp's K-quant types).

sul-kv-compress must not import wasapi.

sul-spec-draft must not import an HTTP client.

sul-toolprm must not import wasapi.

sul-toolprm must not import ReadProcessMemory.



Music player guards



sul-music-engine must not import an HTTP client.

sul-music must not import ReadProcessMemory.

sul-music-engine WASAPI exclusive path must not bypass the device-coordination check.

\[music.playback] exclusive\_mode = true must not be set without the device-coordination check having run.



Broadcast guards



sul-broadcast-audio must not import an HTTP client.

sul-broadcast-audio virtual device server must not require a kernel driver.
sul-broadcast-video `theatre` Cargo feature must be optional and never enabled in `default`.
No bundled Node.js runtime in `sul.exe` — HyperFrames (`A.180`), `ppt-master` (`A.181`), and `camofox-browser` (`A.195`) must execute only inside isolated containers or external user-installed processes.



Overlay guards



No IDXGISwapChain::Present hooking unless \[features] overlay.flip\_model = true and the game's anti\_cheat.kernel\_level == false.

\[overlay] backend = "dcomp" is the default; flip-model is opt-in per-game.

wire.ring\_view\_enabled == true requires the sul-wire:default Tauri capability.



Telemetry / diagnostics guards



sul-metrics must not import an HTTP client.

sul-metrics must not block the ring producer — rollup runs on its own BELOW\_NORMAL thread.

sul-crash must not import an HTTP client.

sul-crash minidump path is confined to %APPDATA%/SUL/crashes (AST check).

sul-save must not use hardlinks for snapshots (AST check).

sul-save snapshot path is confined to %APPDATA%/SUL/saves (AST check).

sul-watchdog runs at BELOW\_NORMAL priority; never blocks the game or the audio callback.

ETW per-process is opt-in per game; refused on kernel\_level: true titles.



Hardware guards



sul-ai.exe does not create compute-only CUDA contexts (CIG-aware only).

No thread above BELOW\_NORMAL in sul-ai.exe.

No thread below THREAD\_PRIORITY\_TIME\_CRITICAL in sul-audio / sul-dsp / sul-voice / sul-music-engine / sul-broadcast-audio callbacks.

Large-page allocation only via sul-alloc (no direct VirtualAlloc with large-page flag outside sul-alloc).

sul-watchdog runs at BELOW\_NORMAL.

HAGS is measured, not forced (the AST guard verifies that hags\_enabled is not set to true without a recorded measurement).

CIG is enabled unless a warning was logged (the AST guard verifies that cig\_enabled = true is the shipped default and that the warning path exists).



Hygiene guards



These catch a style violation that would let a bug in.



Pack system



Personal packs cannot contain executable code — AST check on pack content: no fn blocks, no script sections, no exec keys.

write\_file / edit\_file / run\_cargo\_test / git\_diff / dispatch\_ci are not in the baseline tool registry — they only exist when a coding pack is loaded.

Coding pack loads require OS modal approval (AST check against the pack-load path).

Unsigned packs refused unless --dev-unsigned (dev only, refused in release builds).

Pack sync is git fetch + fast-forward only; force-push refused.

Hive fetch rate limit is 5/session; AST check on the fetch path.



Reference preservation



Every reference in R.1–R.27 is preserved permanently (per the permanent preservation rule in SUL-ARCHITECTURE/Ch. 17). CI fails on any deletion from the reference list unless the deletion is accompanied by an explicit user instruction recorded in SUL-DECISIONS.md.



Source of truth



releases.md is the single source of truth for "what ships in which release." A generated TOML mirror is a future build step; until then, the Markdown file is canonical and CI parses the Markdown tables. No other file duplicates this information.



gates.md is the single source of truth for gate criteria. No other file restates a criterion.



baselines.md is the single source of truth for baseline numbers. No other file restates a baseline.



failures.md is the single source of truth for error codes and modes. No other file restates a failure mode.



guards.md is the single source of truth for AST guards. No other file restates a guard.
