# SUL-LEDGER/releases.md: the release plan (not written yet)

**Status: placeholder (D-150, 2026-11-22).** The release plan this file is meant to hold has not been written.

What belongs here (SUL-BRIEF.md section 3, `guards.md` "Release-plan guards", D-080, D-081): the `[release.R0..R4]` blocks (`version`, `name`, `reference_hardware`, `release_blocking_gates`, `components` / `components_added`, `features_off`, `tier1_events` / `tier1_events_added`) and the `[reference_config.R0..R3]` blocks.

Until it is written:

- The release ladder (R0 to R4) is in `SUL-BRIEF.md` section 3, and the 21 Tier 1-core event ids are in `SUL-BRIEF.md` section 6.8. `cargo xtask release-check` reads them there.
- `cargo xtask release-check` says in its own output that the `[release.R<n>]` / `[reference_config.R<n>]` checks, component resolution, the superset invariant and the Tier 1 events added by R1 to R3 are not checked.

History: until D-150 this file was a plain-text copy of `SUL-BRIEF.md` (the same text, slightly older). It held nothing the brief does not, so the copy was removed. If a flat upload ever brings that copy back, do not restore it.
