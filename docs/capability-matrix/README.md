# Capability matrix

Rule: nothing is "Supported and tested" without a test date and an evidence pointer
(enforced by `sul-core::capability::CapabilityEntry::validate` and mirrored in `ui/src/capabilities.ts`).

| Feature | Game | Build | OS | Adapter | Status | Test date | Evidence | Limitations |
|---|---|---|---|---|---|---|---|---|
| Build isolation | Helldivers 2 | 7.1.1 | Windows | none yet | Not investigated | – | – | Separate adapter track |
| Offline launch | Helldivers 2 | 7.1.1 | Windows | none yet | Not investigated | – | – | Feasibility unknown. No bypass of Steam auth, DRM or anti-cheat |
| Manifest scan/verify (generic) | any | – | Windows | sul-core 0.1.0 | Experimental | 2026-10-22 | CI run 39880270552: `cargo test --workspace` passed on windows-latest | Unit/integration tests only; no real game install tested |
| Theme presets | – | – | browser | ui 0.1.0 | Partially supported | 2026-10-22 | `ui` tests passed in Linux container (13/13) and CI run 39880270552 (windows-latest) | Desktop wrapper compiles in CI (`cargo check`); never launched |
