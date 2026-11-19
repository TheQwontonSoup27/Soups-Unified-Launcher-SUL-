# Legacy (V5.1 build) pointers

In plain words: before the ground-up rebuild, this repository held an older build (Tauri 2 shell, a Rust core and a React/TypeScript UI) planned by an older spec called V5.1. The new spec (`SUL-BRIEF.md`, `SUL-ARCHITECTURE.md`, `SUL-LEDGER/*`, `SUL-DECISIONS.md`) replaces that spec. The old code is being merged into the new design; where the two disagree, the new design wins.

## What was removed from the working tree, and where it still is

Three old documents were removed because the new spec supersedes them. They are not lost: the whole old build, including these files, is on the GitHub branch `archive/legacy-pre-ground-up-2026-11-09` (commit `131ca0c`).

- `PROJECT_SPEC.md`: the old V5.1 spec (10 feature modules, open decisions in its section 7).
- `PROJECT_STATUS.md`: the long history log of the old build (slices, test counts, measurements).
- `ROADMAP.md`: the old roadmap.

Older documents under `docs/` (decisions `0001` to `0008`, the investigations, the capability matrix) were left exactly as written. Where they name one of the three files above, read that as "the copy on the archive branch".

## What was moved here

- `CLAUDE-v5.1.md`: the old engineering rules (the previous `CLAUDE.md`). The new `CLAUDE.md` at the repository root replaces it. Rules in it that the new spec does not repeat (for example "never perform ambiguous deletion" and "do not claim a test or build that did not happen") are still the owner's standing practice; they are being carried into the merge, not dropped.
