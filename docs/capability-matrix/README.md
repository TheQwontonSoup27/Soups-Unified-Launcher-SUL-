# Capability matrix

Rule: nothing is "Supported and tested" without a test date and an evidence pointer
(enforced by `sul-core::capability::CapabilityEntry::validate` and mirrored in `ui/src/capabilities.ts`).

| Feature | Game | Build | OS | Adapter | Status | Test date | Evidence | Limitations |
|---|---|---|---|---|---|---|---|---|
| Build isolation | Helldivers 2 | 7.1.1 | Windows | none yet | Not investigated | – | – | Separate adapter track |
| Offline launch | Helldivers 2 | 7.1.1 | Windows | none yet | Not investigated | – | – | Feasibility unknown. No bypass of Steam auth, DRM or anti-cheat |
| Manifest scan/verify (generic) | any | – | Windows | sul-core 0.1.0 | Experimental | 2026-10-22 | CI run 39761280234: `cargo test --workspace` passed on windows-latest | Unit/integration tests only; no real game install tested |
| Theme presets | – | – | browser | ui 0.1.0 | Partially supported | 2026-10-22 | `ui` tests 13/13 pass (Linux container, Node 22) | Tauri shell not yet built |
