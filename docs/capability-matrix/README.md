# Capability matrix

Rule: nothing is "Supported and tested" without a test date and an evidence pointer
(enforced by `sul-core::capability::CapabilityEntry::validate` and mirrored in `ui/src/capabilities.ts`).
Per-game rows in the app derive their label from the compatibility verdict plus an evidence claim
(`FeatureState::status`), so a label cannot say more than the evidence does.

Reset on 2026-10-26: the earlier rows cited a GitHub Actions run on the code that Step 1.5 replaced, so that evidence no longer applies.
Evidence below is the Linux-container runs of 2026-10-26 plus GitHub Actions run 40276520399 (commit 04c968f, windows-latest: fmt,
clippy, `cargo test -p sul-core`, UI typecheck/tests/build, Tauri wrapper clippy/tests all passed; per-test output not read).

| Feature | Game | Build | OS | Adapter | Status | Test date | Evidence | Limitations |
|---|---|---|---|---|---|---|---|---|
| Build isolation | Helldivers 2 | 7.1.1 | Windows | none yet | Not investigated | – | – | Separate adapter track |
| Offline launch | Helldivers 2 | 7.1.1 | Windows | none yet | Not investigated | – | – | Feasibility unknown. No bypass of Steam auth, DRM or anti-cheat |
| Registry game scanner (Steam, GOG, Uninstall keys, drive probing) | any | – | Windows | sul-core 0.1.0 | Experimental | 2026-10-25 (owner-reported) | Owner ran the Step 1 archive on Windows: 78 tests passed, 38 games mapped. Merged code: unit tests with fakes pass in the Linux container and in Windows CI (2026-10-26) | Mapping accuracy not checked against a ground-truth list; merged code not yet run by the owner on a real machine |
| Tick-and-compatibility matrix, library index | any | – | any | sul-core 0.1.0 | Experimental | 2026-10-26 | `cargo test --workspace` in the Linux container (139 passed) | Unit tests only; not shown in the UI yet |
| Manifest scan/verify, safe paths, atomic write, isolated copy, journal, logger | any | – | any | sul-core 0.1.0 | Experimental | 2026-10-26 | `cargo test --workspace` in the Linux container | No real game install tested |
| Discord Rich Presence (opt-in) | any | – | Windows | sul-core 0.1.0 | Experimental | 2026-10-26 | Unit tests against a fake transport | Never run against a live Discord client. Small-image asset key only: the payload has no size field |
| Steam launch URL, library guard, build id recorded read-only | Steam games | – | Windows | sul-core 0.1.0 | Experimental | 2026-10-26 | Unit tests with fixtures | Never run against a real Steam client. Headless session (`steam` feature) not tested |
| Themes, colour editor, backgrounds, title bar, splash | – | – | browser preview | ui 0.1.0 | Partially supported | 2026-10-26 | `npm run ui:test` 33 passed, typecheck and build clean (Linux container) | Not yet run inside the real desktop window; video backgrounds untested |
