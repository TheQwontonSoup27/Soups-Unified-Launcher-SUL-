SUL-LEDGER/baselines.md



Reading rule. Every entry carries a status field, and it is the honest provenance of the number:



measured — measured on the reference rig with the shipping binary; CI gates on regression.



target — the aspirational target the spec aims for; not yet measured. CI does not gate until the first measurement lands.



placeholder — a stand-in pending a specific gate (A.18, A.54, A.63, A.96, A.97, A.100); the entry exists so the schema and audit trail are stable.



gated — the entry belongs to a feature behind a gate that has not passed; the number is a paper-cited target that becomes target when the gate opens.



CI fails only on a measured entry that regresses beyond tolerance. The release audit (cargo xtask baseline-audit) produces a per-release ledger of which entries are measured for that release. A spec with 50 measured entries and 200 honestly-tagged targets is more useful than one with 500 numbers of unknown provenance.



Each entry has the form value = median / p99 where applicable, tolerance\_pct, and note. Where a section has only one value (e.g., a binary gate), the median column holds the value and p99 is —.



1\. Cross-cutting



\[overlay] — Part 7



Entry	median	p99	higher\_is\_better	status	note



frame\_time\_p99\_ms	0.8	1.0	no	target	p99 overlay frame time <1 ms



flip\_model\_latency\_ms	2.5	3.0	no	target	opt-in per-game; not available on kernel-AC titles



dcomp\_latency\_ms	4	5	no	target	default path; works on all games including kernel-AC titles



Owner: overlay module. Combat Full status indicator and broadcast compositor both read frame\_time\_p99\_ms as a cross-module dependency.



\[ipc] — Part 1



Owned by the IPC ring buffer. sul-ring and sul-wire both consume it.



\[telemetry] — Part 12



See Part 12 §12.11 below; canonical home is \[telemetry] in §12.11.



\[webview] — Part 1



Entry	median	p99	higher\_is\_better	status	note



ui\_frame\_p99\_ms\_esm\_on	8	12	no	target	Strict ESM enabled



ui\_frame\_p99\_ms\_esm\_off	7.5	11.5	no	target	Strict ESM disabled



ipc\_latency\_isolation\_on\_us	200	500	no	target	Isolation Pattern enabled



ipc\_latency\_isolation\_off\_us	150	400	no	target	Isolation Pattern disabled



\[sustained] — Part 13



Canonical home is Part 13 §13.15 (hardware tier discipline); Part 10 §10.23 references it.



Entry	median	p99	higher\_is\_better	status	note



deep\_q4\_tok\_per\_sec\_after\_10min	30	25	yes	target	10-min continuous inference



deep\_q5\_tok\_per\_sec\_after\_10min	25	20	yes	target	10-min continuous inference



\[wire] — Part 7



Entry	median	p99	higher\_is\_better	status	note



tauri\_binary\_encode\_ns	358	500 (inferred)	no	target	tolerance\_pct = 30



tauri\_binary\_decode\_ns	433	600 (inferred)	no	target	tolerance\_pct = 30



token\_ring\_ui\_frame\_ms	4	8	no	target	tolerance\_pct = 25



ring\_map\_latency\_ms	5	10	no	target	tolerance\_pct = 20



2\. Vault (\[vault]) — Part 2 §2.11



Entry	median	p99	higher\_is\_better	status	note



read\_mb\_s	500	800	yes	target	AEAD + IOCP pipeline, disk-bound



write\_mb\_s	500	800	yes	target	AEAD + IOCP pipeline, disk-bound



unlock\_ms	1500	3000	no	target	DPAPI + TPM + password + biometrics



peak\_rss\_at\_20gb\_mb	80	100	no	target	streaming chunk-by-chunk, never whole file



Benchmarks (xtask vault-bench):



vault-bench.py::throughput — read/write throughput. Produces read\_mb\_s, write\_mb\_s.



vault-bench.py::unlock — unlock latency. Produces unlock\_ms.



vault-bench.py::rss\_20gb — 20 GB stream peak RSS. Produces peak\_rss\_at\_20gb\_mb.



vault-bench.py::adversarial — 20+ AEAD adversarial cases from §2.3. Any case that decrypts tampered data is a hard fail.



vault-bench.py::fuzz\_crash — fuzz at every write boundary; asserts zero plaintext leak.



vault-bench.py::key\_rotation — key rotation works without bulk re-encryption (vault file size unchanged).



3\. Emulation — Part 3 §3.13



\[emulation.run\_ahead]



Entry	median	p99	higher\_is\_better	status	note



input\_to\_display\_ms\_reduction	16	32	yes	target	tolerance\_pct = 20



run\_ahead\_1\_ms	3.2	4.0	no	target	run\_ahead = 1 per-frame cost



run\_ahead\_2\_ms	6.2	7.5	no	target	run\_ahead = 2 per-frame cost; marginal on reference rig



snapshot\_restore\_us	137	200	no	target	snapshot + restore at run\_ahead = 2; 2.2% of total; canonical metric for savestate ring



\[emulation.latency]



Entry	median	p99	higher\_is\_better	status	note



dolphin\_ms	16	20	no	target	Vulkan/D3D12; varies with Synchronize GPU thread setting



pcsx2\_ms	20	25	no	target	improves with Vulkan renderer; hardware mode preferred



rpcs3\_ms	25	35	no	target	CPU-heavy; varies with game and hardware



duckstation\_ms	18	22	no	target	Vulkan-first



\[emulation.libretro]



Entry	median	p99	higher\_is\_better	status	note



frame\_delta	-1	-2	no	target	sul-emu-libretro vs generic frontend; negative = fewer frames of latency



cpu\_pct\_delta	-20	-15	no	target	15–25% CPU savings



\[emulation.ruffle]



Entry	median	p99	higher\_is\_better	status	note



heap\_peak\_mb	300	500	no	target	ruffle\_core native AVM2 heap; exceeds 500 MB → policy refusal, not browser WASM limit



\[emulation.gb]



Entry	median	p99	higher\_is\_better	status	note



frame\_time\_ms	2.0	2.5	no	target	sul-gb on reference rig at 60 fps



allocations\_per\_frame	0	—	no	target	zero allocations in hot loop



savestate\_size\_kb	32	32	no	target	\~32 KB snapshot



savestate\_snapshot\_us	15	20	no	target	sul-gb SavestateSource impl



savestate\_restore\_us	15	20	no	target	sul-gb SavestateSource impl



\[emulation.savestate]



Entry	median	p99	higher\_is\_better	status	note



ps2\_snapshot\_us	400	500	no	target	\~35 MB state; libretro shim path



ps2\_restore\_us	400	500	no	target	\~35 MB state; libretro shim path



allocations\_hot\_path	0	—	no	target	zero allocation in snapshot / restore / run-ahead orchestration



changed\_block\_compression\_ratio	3.0	4.0	yes	target	zstd on changed blocks vs full snapshot



\[vfs] — Part 3 §3.12



Entry	median	p99	higher\_is\_better	status	note



open\_latency\_ns	500	1000	no	target	tolerance\_pct = 20



frametime\_impact\_pct	0.5	0.5	no	target	tolerance\_pct = 10; open-heavy workloads



Benchmarks (xtask emu-bench, gb-bench, savestate-bench, ra-bench): listed in Architecture Ch. 3 §3.16.



4\. Audio — Part 4 §4.14



\[audio.dsp]



Entry	median	p99	higher\_is\_better	status	note



callback\_allocations	0	—	no	target	zero allocations in callback



filter\_chain\_p99\_us	800	1000	no	target	tolerance\_pct = 20



artifact\_load\_ms	30	50	no	target	tolerance\_pct = 20



\[audio.voice]



Entry	median	p99	higher\_is\_better	status	note



isolation\_roundtrip\_us	2500	3000	no	target	tolerance\_pct = 20



isolation\_allocations	0	—	no	target	zero allocations in callback



wer\_improvement\_pct	5	10	yes	target	tolerance\_pct = 20



\[audio.routing]



Entry	median	p99	higher\_is\_better	status	note



matrix\_mix\_ms	0.5	1.0	no	gated	6 inputs × 4 buses, gain-only; four-bus with limiters is 3–5 ms; A.87 gate



limiter\_p99\_us	3000	5000	no	gated	four limiters at 48 kHz / 512-sample buffer on AVX2; A.87 gate



hardware\_bus\_roundtrip\_ms	12	20	no	gated	within 5 ms of native WASAPI period at 512 samples



virtual\_bus\_roundtrip\_ms	20	40	no	gated	VB-Cable native; typically reported 10–30 ms per hop



bus\_drift\_per\_hour	0	0	no	gated	hard gate: zero sample-level drift across buses over 1 hour



\[audio.voice\_fx]



Entry	median	p99	higher\_is\_better	status	note



combined\_p99\_us	6000	8000	no	gated	pitch + formant + ring mod at 48 kHz / 512-sample on AVX2; A.88 gate



stt\_wer\_delta\_pct	0	2	no	gated	voice FX must not regress STT WER for the A.85 voice loop



voice\_loop\_delta\_ms	0	50	no	gated	voice FX must not regress A.85 <500 ms end-to-end



preset\_perceptual\_agreement\_pct	0.80	0.90	yes	gated	10 listeners, ≥80% agreement that the preset matches its name



\[audio.governor]



Entry	median	p99	higher\_is\_better	status	note



decision\_loop\_ms	5	15	no	target	tolerance\_pct = 50



underrun\_events\_per\_100\_sessions	0	0	no	target	



fir\_drop\_events\_per\_100\_sessions	0	2	no	target	tolerance\_pct = 50



exclusive\_mode\_recommendation\_accuracy\_pct	95	100	yes	target	



hrtf\_auto\_disable\_correct\_pct	100	100	yes	target	



\[stt]



Entry	median	p99	higher\_is\_better	status	note



end\_to\_end\_latency\_ms	150	400	no	target	WhisperPipe-pattern target; 89 ms median measured on WhisperPipe reference



wer\_pct	4.0	6.0	no	target	measured against offline Whisper baseline; wrapper target is <2% absolute delta



model\_size\_mb	150	500	no	target	tiny.en \~20 MB quantized, small.en \~500 MB unquantized, q5\_1 \~2.3× speedup



voice\_loop\_e2e\_ms	500	900	no	gated	A.85 target: user stops speaking → first TTS audio out, with Combat Full Instruct as reasoning backend



pipeline\_speedup\_x	10.0	12.0	yes	target	stock whisper.cpp with VAD + silence-strip + stereo-split pipeline vs stock whisper.cpp without; call-centre deployment reference



audio\_reduction\_pct	0.58	0.58	yes	target	58% of audio stripped before inference by VAD + silence removal



short\_clip\_ms	80	200	no	target	2–8 s combat callout with audio\_ctx = 768 vs default 1500; \~2× faster on short clips



flash\_attn\_enabled	1	1	yes	target	binary: flash\_attn must be true; Metal defaults it OFF, so explicit setting is required



\[tts]



Entry	median	p99	higher\_is\_better	status	note



engine	piper	—	—	target	



latency\_ms	150	400	no	target	Piper CPU desktop CPU; Raspberry Pi reference is \~1,220 ms TTFA



rtf	0.15	0.6	no	target	Piper \~0.15 RTF; Kokoro int8 CPU 0.57 English



vram\_mb	0	0	no	target	TTS never runs on the GPU in the shipping voice loop



\[tts] higgs\_audio\_latency\_ms = { median = 150, p99 = 400, higher\_is\_better = false, status = "gated", note = "A.162" }

\[tts] higgs\_audio\_quality\_rtf = { median = 0.15, p99 = 0.6, higher\_is\_better = false, status = "gated", note = "A.162" }



\[prosody] — Part 4 §4.6, Part 10



Entry	median	p99	higher\_is\_better	status	note



feature\_extraction\_latency\_ms	—	2	no	target	p99 <2 ms



classifier\_latency\_ms	—	5	no	target	p99 <5 ms



three\_way\_intent\_accuracy	0.85	—	yes	target	≥85%



Benchmarks (xtask audio-bench, audio-governor-bench, audio-routing-bench, voice-fx-bench, stt-bench, tts-bench): listed in Architecture Ch. 4 §4.14.



5\. Music player (\[music]) — Part 5 §5.11



Entry	median	p99	higher\_is\_better	status	note



cpu\_playing\_pct	1	2	no	target	no DSP



cpu\_eq\_pct	3	4	no	target	EQ active



cpu\_eq\_replaygain\_resampling\_pct	5	6	no	target	EQ + ReplayGain + resampling



buffer\_underruns\_per\_100\_sessions	0	0	no	target	



seek\_latency\_ms	25	50	no	target	



track\_start\_latency\_ms	50	100	no	target	



bit\_perfect\_loopback\_verified	1	1	yes	target	binary: input bytes match device output bytes when no DSP active



gapless\_gap\_ms	0	0	no	target	sample-accurate across FLAC/MP3(LAME)/OGG/WAV



exclusive\_mode\_buffer\_samples	256	256	no	target	≤5.8 ms at 48 kHz



\[music] jellyfin\_index\_100\_tracks\_ms = { median = 500, p99 = 1500, higher\_is\_better = false, status = "gated", note = "A.164" }



Benchmarks (xtask music-bench): listed in Architecture Ch. 5 §5.11.



6\. Broadcast engine (\[broadcast]) — Part 6 §6.11



Entry	median	p99	higher\_is\_better	status	note



compositor\_cpu\_1080p60\_3sources\_pct	4	6	no	target	



compositor\_cpu\_1440p60\_5sources\_pct	9	12	no	target	



compositor\_cpu\_4k60\_6sources\_pct	15	20	no	target	



encode\_cpu\_nvenc\_1080p60\_2isos\_pct	2	3	no	target	



encode\_cpu\_openh264\_1080p60\_pct	25	35	no	target	



end\_to\_end\_latency\_ms	60	80	no	target	WGC → RTMP out



dropped\_frames\_per\_60min	0	0	no	target	



audio\_underruns\_per\_60min	0	0	no	target	



audio\_engine\_cpu\_12ch\_12bus\_pct	6	8	no	target	all strip FX on



voice\_fx\_latency\_ms	8	10	no	target	mic → Stream bus



autotune\_latency\_ms	4	5	no	target	256-sample buffer



routing\_matrix\_change\_latency\_ms	0.5	1	no	target	



scene\_switch\_latency\_ms	12	16	no	target	



iso\_write\_jitter\_ms	1	2	no	target	



memory\_1080p60\_3sources\_mb	250	300	no	target	



gpu\_vram\_1080p60\_mb	350	400	no	target	



gpu\_vram\_4k60\_mb	900	1000	no	target	



usbip\_virtual\_device\_enumeration	1	1	yes	target	binary: Windows enumerates devices without a SUL driver



bus\_drift\_per\_hour	0	0	no	target	hard gate: zero sample-level drift across buses over 1 hour



admission\_controller\_accuracy\_pct	100	100	yes	target	refuses over-budget configs, downgrades ISOs first

[broadcast] hyperframes_offline_peak_ram_mb	1500	2000	no	gated	A.180; offline HTML-to-MP4 render peak RAM <2 GB



Benchmarks (xtask broadcast-bench): listed in Architecture Ch. 6 §6.11.



7\. Vision and search — Part 8 §8.11



\[vision.ocr]



Entry	median	p99	higher\_is\_better	status	note



frame\_diff\_gate\_us	500	1000	no	target	



roi\_crop\_us	200	500	no	target	



pp\_ocrv5\_end\_to\_end\_ms	45	60	no	target	



ocr\_accuracy\_pct	92	88	yes	target	500 labeled HUD crops



\[vision.vlm]



Entry	median	p99	higher\_is\_better	status	note



minicpm\_v47\_1b\_latency\_ms	1300	1500	no	target	



minicpm\_v47\_1b\_rubric\_pct	0.68	0.65	yes	target	68.2% rubric on SUL game-screenshot eval set



lfm25\_vl\_3b\_latency\_ms	1500	2500	no	gated	A.15 gate



lfm25\_vl\_3b\_screenspot\_v2	80.7	—	yes	gated	A.15 gate



jina\_vlm\_2\_4b\_latency\_ms	1500	2500	no	gated	A.24 gate



jina\_vlm\_2\_4b\_multilingual\_delta\_pts	2	5	yes	gated	A.24 gate; ≥2 points above MiniCPM-V on JA/RU/DE



\[search]



Entry	median	p99	higher\_is\_better	status	note



rustxng\_search\_ms	400	800	no	target	



adblock\_filter\_us	50	150	no	target	



readdown\_extract\_ms	80	200	no	target	



excerpt\_select\_ms	30	80	no	target	



verification\_packet\_ms	100	250	no	target	



total\_pipeline\_ms	660	1500	no	target	



token\_reduction\_pct	0.90	—	yes	target	tolerance\_pct = 10

[net.search] adblock_filter_rss_mb	35	50	no	target	FlatBuffers serialization; ~75% RSS reduction vs legacy list format



\[search.github]



Entry	median	p99	higher\_is\_better	status	note



github\_search\_ms	300	800	no	target	



rate\_limit\_hits\_per\_session	0	2	no	target	



Benchmarks (xtask ocr-bench, vlm-bench, search-bench, github-search-bench, retrace-bench): listed in Architecture Ch. 8 §8.11.



8\. Frame generation — Part 9 §9.6



\[frame\_gen]



Entry	median	p99	higher\_is\_better	status	note



base\_fps\_gate\_median\_fps	30	—	yes	target	refuse FG below this



vram\_reservation\_1080p\_2x\_mb	300	—	no	target	tolerance\_pct = 15



vram\_reservation\_1080p\_4x\_mb	600	—	no	target	tolerance\_pct = 15



frame\_corruption\_events\_per\_10k	0	0	no	target	



latency\_added\_ms\_60fps	16	17	no	target	tolerance\_pct = 10

[frame_gen] post_process_dag_ms	0.5	1.0	no	gated	A.182; pure-Rust post-process FX DAG overhead <1 ms/frame



\[frame\_gen.emulator]



Entry	median	p99	higher\_is\_better	status	note



pcsx2\_native\_fps\_discovery\_accuracy\_pct	100	100	yes	target	



dolphin\_native\_fps\_discovery\_accuracy\_pct	100	100	yes	target	



rpcs3\_native\_fps\_discovery\_accuracy\_pct	100	100	yes	target	



duckstation\_native\_fps\_discovery\_accuracy\_pct	100	100	yes	target	



retroarch\_native\_fps\_discovery\_accuracy\_pct	100	100	yes	target	



emulator\_launch\_with\_fg\_ms	800	1500	no	target	tolerance\_pct = 25



Benchmarks (xtask fg-bench): listed in Architecture Ch. 9 §9.6.



9\. Local AI — Part 10 §10.23



\[ai.deep\_q4]



Entry	median	p99	higher\_is\_better	status	note



vram\_4k\_mb	5500	—	no	placeholder	Q4\_K\_M; 4400 with STAR-KV active (estimated)



\[ai.deep\_q5]



Entry	median	p99	higher\_is\_better	status	note



vram\_4k\_mb	6460	—	no	placeholder	Q5\_K\_M weights; \~6960 total with q8\_0 KV at 4K; fits under 8 GB with \~1 GB headroom



vram\_8k\_mb	6900	—	no	placeholder	Q5\_K\_M + asymmetric q8\_0/q5\_1 KV at 8K



\[ai.transitions]



Entry	median	p99	higher\_is\_better	status	note



deep\_q4\_q5\_swap\_ms	4000	5500	no	target	STAR-KV decompression adds \~1.5s; tolerance\_pct = 20



\[ai.combat\_full]



Entry	median	p99	higher\_is\_better	status	note



mmb	80.8	—	yes	gated	ReTrace required (A.54); tolerance\_pct = 5



pope	89.9	—	yes	gated	ReTrace required (A.54); tolerance\_pct = 5



sqai	95.7	—	yes	gated	ReTrace required (A.54); tolerance\_pct = 5



retrace\_retention\_pct	11.1	—	no	gated	A.54; lower is more compressed; tolerance\_pct = 5



retrace\_benchmark\_retention	1.0	—	yes	gated	tolerance\_pct = 3



retrace\_prefill\_speedup	2.83	—	yes	gated	tolerance\_pct = 15



\[ai.combat\_full\_instruct]



Entry	median	p99	higher\_is\_better	status	note



bfcl\_nonlive	0.68	—	yes	gated	post-fine-tuning target; A.63 gate is ≥0.65; pre-fine-tuning baseline TBD; tolerance\_pct = 15



bfcl\_live	0.88	—	yes	gated	tolerance\_pct = 15



first\_token\_ms\_cold	350	500	no	target	



first\_token\_ms\_warm	90	150	no	target	



vram\_mb	1350	—	no	gated	with ReTrace active (shipping requirement); 1650–1750 without



tool\_call\_latency\_ms	120	250	no	target	

[ai.combat_full.instruct.magnitude] tok_per_sec	—	—	yes	gated	A.192; populated per-hardware by `--backend-autotune`



\[ai.combat\_full\_thinking]



Entry	median	p99	higher\_is\_better	status	note



bfcl\_nonlive	0.68	—	yes	gated	post-fine-tuning target; A.63 gate is ≥0.65; pre-fine-tuning baseline TBD; tolerance\_pct = 15



bfcl\_live	0.88	—	yes	gated	tolerance\_pct = 15



first\_token\_ms\_cold	400	600	no	target	



first\_token\_ms\_warm	110	180	no	target	



vram\_mb	1400	—	no	gated	with ReTrace active; Thinking's 4K context adds \~50 MB over Instruct; 1700–1800 without



multi\_turn\_pct	0.85	—	yes	target	baseline TBD; first A.63 run records; tolerance\_pct = 10



reasoning\_token\_avg	400	800	no	target	mean thinking tokens per turn; bounded 512–1024 after first A.63 measurement



\[ai.combat\_full\_conversation]



Entry	median	p99	higher\_is\_better	status	note



bfcl\_nonlive	0.68	—	yes	gated	inherits from Thinking fine-tune; tolerance\_pct = 15



first\_token\_ms\_cold	450	700	no	target	



first\_token\_ms\_warm	120	200	no	target	



multi\_turn\_pct	0.85	—	yes	target	tolerance\_pct = 10



vram\_mb	1280	—	no	gated	with ReTrace active; 16K context adds \~800 MB KV in q8\_0, compressed to \~40–80 MB with STAR-KV



kv\_reuse\_rate\_pct	90	95	yes	target	Continuum metric; full-history KV cache reuse



thread\_persistence\_across\_launch\_cycle	1	—	yes	target	1 = conversation thread survives game launch and exit



turn\_quality\_rubric\_pct	0.75	—	yes	target	mean rubric ≥75%; A.63 gate; tolerance\_pct = 10



\[ai.combat\_full\_coding]



Entry	median	p99	higher\_is\_better	status	note



humaneval\_pass1	60	—	yes	gated	absolute threshold; base InternVL3-2B is 39.0; no 3B specialist fallback exists; A.101 gate; tolerance\_pct = 10



vram\_8k\_mb	200	—	no	gated	KV at 8K with STAR-KV



first\_token\_ms\_cold	400	600	no	target	



first\_token\_ms\_warm	110	180	no	target	



\[ai.combat\_router]



Entry	median	p99	higher\_is\_better	status	note



mode\_selection\_accuracy\_pct	95	—	yes	target	tolerance\_pct = 5



mode\_switch\_latency\_ms	30	50	no	target	



destructive\_action\_detection\_recall	1.0	—	yes	target	HARD GATE — 100% on 100-trigger test set; any miss fails A.63; tolerance\_pct = 0



instruct\_fallback\_rate\_pct	2	5	no	target	tolerance\_pct = 50



\[ai.combat\_full.toolprm]



Entry	median	p99	higher\_is\_better	status	note



ranking\_accuracy	0.75	0.85	yes	gated	A.100; within 2pp of teacher on TRBench-BFCL; tolerance\_pct = 10



end\_to\_end\_bfcl\_delta	3	5	yes	gated	≥3pp over no-PRM baseline



vram\_mb	100	200	no	target	



forward\_latency\_ms	8	15	no	target	single (context, candidate) pair



cache\_hit\_rate\_pct	40	60	yes	target	



\[ai.speculative.medusa]



Entry	median	p99	higher\_is\_better	status	note



acceptance\_rate	0.55	0.70	yes	target	tolerance\_pct = 20



speedup	2.2	2.8	yes	target	tolerance\_pct = 20



head\_vram\_mb	35	50	no	target	



temperature\_scaling\_enabled	1	—	yes	target	binary: must be 1, acceptance drops ≥30% without it



\[ai.speculative.phrase]



Entry	median	p99	higher\_is\_better	status	note



acceptance\_rate	0.45	0.60	yes	target	tolerance\_pct = 20



speedup	2.0	2.4	yes	target	tolerance\_pct = 20



head\_vram\_mb	2	5	no	target	



\[ai.speculative.unispec]



Entry	median	p99	higher\_is\_better	status	note



acceptance\_rate	0.45	0.55	yes	target	tolerance\_pct = 20



speedup	2.0	2.6	yes	target	tolerance\_pct = 20



\[ai.speculative.dflash] — DEPRECATED



Not in any shipping chain (Butler cut). Retained for historical comparison.



Entry	median	p99	higher\_is\_better	status	note



acceptance\_rate	0.65	0.80	yes	placeholder	tolerance\_pct = 20



speedup	2.5	4.9	yes	placeholder	tolerance\_pct = 20



\[ai.speculative.lilicorr]



Entry	median	p99	higher\_is\_better	status	note



acceptance\_rate	0.60	0.75	yes	target	tolerance\_pct = 20



speedup	2.7	3.2	yes	target	tolerance\_pct = 20



\[ai.speculative.chainflow]



Entry	median	p99	higher\_is\_better	status	note



acceptance\_rate	0.55	0.70	yes	target	tolerance\_pct = 20



speedup	1.6	1.9	yes	target	tolerance\_pct = 20



Note. The +39% over EAGLE-3 and +11% over DFlash figures in Part 10 §10.1 are Qwen3.5-specific published benchmarks at matched drafter size. The shipped Deep Coder is K2-Horizon-7B, not Qwen3.5; the shipped aggregate speedup is 1.6–1.9× across K2 workloads, which is lower than DFlash's 2.5–4.9×. ChainFlow ships as Deep Coder's first rung because its K2 workloads sit in the workload class where flow-matching wins, not because it beats DFlash in aggregate.



\[ai.speculative.rift]



Entry	median	p99	higher\_is\_better	status	note



acceptance\_retention\_pct	0.97	—	yes	target	RIFT-dyn at 16K context, from the paper; tolerance\_pct = 5



activation\_threshold\_mb	32	—	no	target	



refresh\_added\_latency\_ms	3	5	no	target	



\[ai.speculative.fallback]



Entry	median	p99	higher\_is\_better	status	note



fast\_fallback\_ms	100	500	no	target	SP-10 (verify batch OOM) — same rung, smaller max draft, no model reload; tolerance\_pct = 20



slow\_fallback\_ms	350	500	no	target	SP-2, SP-3 (rung change) — bounded by draft-model load time (200–500 ms); tolerance\_pct = 20



spec\_chain\_validator\_pass	100	100	yes	target	



\[kv\_tier.star\_kv]



Entry	median	p99	higher\_is\_better	status	note



compression\_ratio	1.0	—	yes	placeholder	placeholder until A.18 measurement



ppl\_delta\_pct	0.0	2.0	no	placeholder	A.18 gate: <2%



kl\_d\_pct	0.0	0.5	no	placeholder	A.18 gate: below f16 baseline at target compression



\[kv\_tier.gaac]



Entry	median	p99	higher\_is\_better	status	note



compression\_ratio	1.405	1.410	yes	target	provenance = gaac\_paper



\[kv\_tier.palquant]



Entry	median	p99	higher\_is\_better	status	note



compression\_ratio	7.42	7.42	yes	gated	on Qwen3-8B; 5.02 on Llama-3.2-3B; A.97 gate



ppl\_delta\_pct	0.5	2.0	no	gated	



kl\_d\_pct	0.3	0.5	no	gated	



identity\_preserved\_pct	0.97	0.95	yes	gated	A.97 gate: ≥0.95



\[kv\_tier.puzzlekv]



Entry	median	p99	higher\_is\_better	status	note



compression\_ratio	5.3	5.3	yes	gated	INT4 path; A.97 gate



ppl\_delta\_pct	1.0	2.0	no	gated	



\[kv\_tier.jolt]



Entry	median	p99	higher\_is\_better	status	note



compression\_ratio	2.5	3.0	yes	gated	A.97 gate



ppl\_delta\_pct	0.1	0.2	no	gated	



\[kv\_tier.turbo3]



Entry	median	p99	higher\_is\_better	status	note



compression\_ratio	4.6	4.6	yes	placeholder	research backend; no A.97 measurement yet



ppl\_delta\_pct	1.0	1.5	no	placeholder	



\[kv\_tier.tcq]



Entry	median	p99	higher\_is\_better	status	note



compression\_ratio	4.0	4.0	yes	placeholder	3.25–4.25 bpv; research backend



ppl\_delta\_pct	-0.05	0.0	no	placeholder	TCQ can IMPROVE PPL through norm-scaling regularisation



\[kv\_tier.fallback]



Entry	median	p99	higher\_is\_better	status	note



time\_to\_fallback\_ms	500	2000	no	target	tolerance\_pct = 20



\[agent.ooda\_tool]



Entry	median	p99	higher\_is\_better	status	note



scaffold\_overhead\_pct	5	—	no	target	Instruct-mode compact scaffold; tolerance\_pct = 50



scaffold\_overhead\_thinking\_pct	15	—	no	target	Thinking-mode verbose scaffold; tolerance\_pct = 50



multi\_turn\_accuracy\_gain\_pct	10	—	yes	target	tolerance\_pct = 30



\[agent.tscg]



Entry	median	p99	higher\_is\_better	status	note



schema\_compression\_pct	0.51	—	yes	target	tolerance\_pct = 15



tool\_accuracy\_retention\_pct	0.95	—	yes	target	tolerance\_pct = 5

[agent.memory] recall_latency_ms	25	50	no	gated	A.186; SQLite FTS5 + sulpack vector recall <50 ms

[agent.user_tasks] sqlite_write_ms	2	5	no	gated	A.188; local SQLite task write <5 ms

[ai.combat_full_conversation] reading_mode_inject_ms	60	100	no	gated	A.189; EPUB/MOBI parse + context injection <100 ms



Benchmarks (xtask kv-bench, spec-bench, quant-bench, kv-compress-bench, toolprm-bench, combat-full-bench, router-bench, mode-switch-bench, ooda-bench, tscg-bench, sul-bench ai): listed in Architecture Ch. 10 §10.23.



Deprecated baseline sections. \[ai.combat\_lite], \[ai.combat\_exec], \[ai.combat\_minimal], \[ai.combat\_vision\_lite], \[ai.coding\_low], \[ai.butler], \[ai.combat\_low], \[ai.combat\_mid], \[ai.deep\_extended] are DEPRECATED (tiers cut) but retained for historical comparison where they have value. \[ai.coding\_low], \[ai.butler], \[ai.combat\_low], \[ai.combat\_mid], \[ai.deep\_extended] are removed entirely — those tiers are cut and their baselines have no historical-comparison value.



10\. Telemetry (\[telemetry]) — Part 12 §12.11



Entry	median	p99	higher\_is\_better	status	note



game\_state\_machine\_us	1500	2000	no	target	regex extraction latency per frame



screenshot\_ring\_alloc\_mb	50	60	no	target	\~10 MB on RDP; zero drops at 30 Hz over 10s



event\_ring\_write\_p99\_us	5	10	no	target	single event write



rollup\_p99\_us	50	100	no	target	rollup doesn't block producer



query\_last\_1000\_events\_ms	0.5	1.0	no	target	mmap direct query



hags\_vram\_delta\_mb	0	—	no	target	measured on reference rig at 1440p/165Hz; tolerance\_pct = 100



Benchmarks (xtask bench-telemetry): listed in Architecture Ch. 12 §12.11.



11\. Handoff features — Part 15



\[csi]



Entry	median	p99	higher\_is\_better	status	note



packet\_to\_ui\_latency\_ms	50	100	no	gated	A.136



heart\_rate\_accuracy\_bpm	3	5	no	gated	



breathing\_rate\_accuracy\_bpm	1	2	no	gated	



presence\_detection\_accuracy\_pct	95	98	yes	gated	



edge\_inference\_cpu\_pct	1	2	no	gated	

[sensing.vision_pose] pose_latency_ms	25	40	no	gated	A.194; multi-cam RTMPose ONNX 3D skeleton fallback



\[bci]



Entry	median	p99	higher\_is\_better	status	note



decoding\_accuracy\_pct	90	95	yes	gated	A.139



end\_to\_end\_latency\_ms	200	300	no	gated	neural signal to text



inference\_time\_ms	50	100	no	gated	INT8 model



\[gev]



Entry	median	p99	higher\_is\_better	status	note



object\_count	10000	50000	yes	gated	A.137



fps	60	60	yes	gated	



geolocation\_accuracy\_pct	85	95	yes	gated	



tile\_cache\_hit\_rate\_pct	80	90	yes	gated	



\[rppg]



Entry	median	p99	higher\_is\_better	status	note



heart\_rate\_accuracy\_bpm	3	5	no	gated	A.138; within ±3 bpm vs. contact sensor



cpu\_overhead\_pct	5	5	no	gated	for 720p 30fps stream



\[security\_toolkit]



Entry	median	p99	higher\_is\_better	status	note



hashcat\_speed\_hashes\_per\_sec	1000000	5000000	yes	gated	A.141, A.142



nuclei\_scan\_throughput\_hosts\_per\_min	50	100	yes	gated	



mitmproxy\_interception\_latency\_ms	5	10	no	gated	



shannon\_container\_startup\_ms	2000	5000	no	gated	



\[security\_toolkit] wasm\_overhead\_ms = { median = 20, p99 = 50, higher\_is\_better = false, status = "gated", note = "A.163" }

\[security\_toolkit] gimp\_container\_startup\_ms = { median = 1500, p99 = 3000, higher\_is\_better = false, status = "gated", note = "A.170" }

\[security\_toolkit] stirling\_pdf\_merge\_ms = { median = 200, p99 = 500, higher\_is\_better = false, status = "gated", note = "A.170" }



\[voicestudio]



Entry	median	p99	higher\_is\_better	status	note



tts\_first\_packet\_latency\_ms	110	200	no	gated	A.143



dubbing\_throughput\_seconds\_per\_minute	10	15	yes	gated	



voice\_clone\_similarity\_pct	90	95	yes	gated	character voices only; cloning of specific real persons is structurally refused



Benchmarks (xtask csi-bench, bci-bench, gev-bench, security-bench, voicestudio-bench, vision-bench rPPG subtest): listed in Architecture Ch. 15 §15.18.



12\. The full baseline ledger



Every baseline entry above is a candidate for the release audit (cargo xtask baseline-audit). The audit produces a per-release ledger of which entries are measured for that release. Entries that are target, placeholder, or gated are not gated by CI until the first measurement lands.



The reference configurations in releases.md are the tested ground truth for each release. A coverage metric below the release's declared minimum is a warning, not an error.



13\. VRAM posture (cross-cutting)



New baseline section added by the VRAM posture (Brief §7.5). Each entry is measured against the reference rig (8 GB VRAM).



| Entry | median | p99 | higher\_is\_better | status | note |

|---|---|---|---|---|---|

| `\[vram.idle]` footprint\_mb | 350 | 500 | no | target | No game, no AI. WebView2 + egui + IPC + watchdog + telemetry. |

| `\[vram.game\_substrate]` footprint\_mb | 1500 | 2000 | no | target | Game running, combat AI off. Substrate + OCR + prosody + state machine + music (if on). |

| `\[vram.game\_combat]` footprint\_mb | 1500 | 3000 | no | target | Game running, combat AI on. Substrate + Combat Full with ReTrace + ToolPRM + vision tokens. The 3 GB is a cap; the design minimizes below it. |

| `\[vram.combat\_full\_r0]` footprint\_mb | 1700 | 1900 | no | placeholder | R0 (no ReTrace, no ToolPRM). Combat Full Instruct + OCR. |

| `\[vram.combat\_full\_r2]` footprint\_mb | 1550 | 1700 | no | placeholder | R2+ (ReTrace + ToolPRM + full spec chain). Combat Full + OCR. |

| `\[vram.deep\_coder]` footprint\_mb | 6500 | 7000 | no | placeholder | Deep Q5\_K\_M at 4K. Explicit Tier 1 exception; not co-resident with a game by default. |

| `\[vram.broadcast\_1080]` footprint\_mb | 350 | 400 | no | target | 1080p60, 3 sources. |

| `\[vram.broadcast\_1440]` footprint\_mb | 450 | 550 | no | target | 1440p60, 5 sources. |

| `\[vram.broadcast\_4k]` footprint\_mb | 900 | 1000 | no | target | 4K60, 6 sources. Explicit Tier 1 exception. |



Benchmarks (xtask vram-bench):



\- `vram\_bench.py::idle` — idle footprint.

\- `vram\_bench.py::game\_substrate` — game running, combat AI off.

\- `vram\_bench.py::game\_combat` — game running, combat AI on.

\- `vram\_bench.py::deep\_coder` — Deep loaded, no game.

\- `vram\_bench.py::broadcast` — broadcast at 1080 / 1440 / 4K.



Validation: the compatibility validator (A.117, rule #35 in Ch. 1 §4.2) compares the aggregate against the layered ceiling at every launch. Baseline regressions above the ceiling fail CI.


### §13 Application-Wide Runtime Performance Targets (D-143)

```toml
[perf.runtime_app]
idle_rss_mb = { median = 42, p99 = 49, higher_is_better = false, status = "target" }
sul_wire_spsc_push_ns = { median = 18, p99 = 45, higher_is_better = false, status = "target" }
sqlite_wal_batch_flush_ms = { median = 1.2, p99 = 4.0, higher_is_better = false, status = "target" }
webview2_parallel_prewarm_ms = { median = 180, p99 = 320, higher_is_better = false, status = "target" }
wasapi_ecoqos_disabled_jitter_us = { median = 45, p99 = 120, higher_is_better = false, status = "target" }
```


### §13 Second-Sweep Runtime Performance Targets (D-144)

```toml
[perf.second_sweep]
aho_corasick_scan_us = { median = 45, p99 = 95, higher_is_better = false, status = "target" }
simsimd_cosine_10k_us = { median = 120, p99 = 250, higher_is_better = false, status = "target" }
vision_mmap_frame_copy_us = { median = 0, p99 = 5, higher_is_better = false, status = "target" }
pinned_dma_kv_swap_ms = { median = 1.8, p99 = 4.5, higher_is_better = false, status = "target" }
```

### `[perf.runtime_app_v2]` (D-145, D-146 — Locked Ground-Up Targets)
```toml
[perf.runtime_app_v2]
wgc_d3d11_cuda_zero_copy_us = { median = 80, p99 = 150, higher_is_better = false, status = "target" }
compio_iocp_sulpack_read_mbps = { median = 3200, p99 = 2400, higher_is_better = true, status = "target" }
rkyv_sul_wire_decode_ns = { median = 25, p99 = 60, higher_is_better = false, status = "target" }
wasapi_exclusive_jitter_us = { median = 120, p99 = 350, higher_is_better = false, status = "target" }
redb_vfs_journal_commit_us = { median = 180, p99 = 450, higher_is_better = false, status = "target" }
kitoken_encode_1k_tokens_us = { median = 95, p99 = 160, higher_is_better = false, status = "target" }
```
