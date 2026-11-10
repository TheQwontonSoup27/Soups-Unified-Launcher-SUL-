# HANDOFF: read this first

Written 2026-11-09 for a Claude that has no memory of this project. The owner is starting a ground-up build from a new spec that is very different from the old one, and is wiping Claude's memory so sessions stay cheap. This file holds what would otherwise live only in that memory or in earlier chats. Where it disagrees with the new spec, the new spec wins. Where it disagrees with the owner's latest message, the message wins. Anything here marked "old plan" may be superseded: say so and ask rather than assuming.

## 0. Read order, and what to skip

1. This file, then `CLAUDE.md` (41 lines of engineering rules).
2. The new spec. It was not in the repo when this was written. Ask the owner where it is if you cannot find it. `PROJECT_SPEC.md` is the old V5.1 spec (455 lines); if the two conflict, the new one governs. If it is unclear which file is authoritative, ask.
3. Then report back as `CLAUDE.md` says: current state, confirmed facts, unknowns, the smallest useful next step, and a plan for approval before any major architectural change.

Do not read these in full unless the task needs them (they are large and the owner is watching token use): `PROJECT_STATUS.md` (77 KB), `docs/capability-matrix/README.md` (29 KB), `docs/references.md` (26 KB), `PROJECT_SPEC.md` (34 KB). Grep them instead. `docs/decisions/0001` to `0008` are the digest of why things are as they are. Do not explore the old code (about 36,600 lines of Rust, 7,800 of UI source, 10,000 of UI tests) until the plan says what is reused.

## 1. Where things stand (2026-11-09)

- `main` is at `131ca0c`; Windows CI passed on it. The same commit is kept on the branch `archive/legacy-pre-ground-up-2026-11-09` so it cannot be lost if `main` is rebuilt. (A tag was wanted, but the sandbox's GitHub proxy refuses to create tags.) The older point before the Step 1.5 consolidation is the commit `9ece116`, also in `main`'s history.
- All earlier feature branches on GitHub are merged into `main`; nothing was deleted. The owner can prune them.
- Nothing has been decided about whether the new build reuses, ports from or discards the old code. **Do not delete or rewrite old code before the owner approves a plan.**
- The old build is Windows-only: a Tauri 2 shell (`src-tauri`), a Rust core (`crates/sul-core`, plus an empty `crates/sul-net` kept so that any network code has one place to live), and a React 19 / TypeScript UI (`ui`, Vite). It covers the old spec's Module 1 (library, builds, folders, launch, catalogue) and items 2.1 to 2.4 (mods, profiles, deploy, purge, undo, recipes, patches, external tools), with screens and Tauri commands for them, plus earlier work: the Steam attach and launch layer, the compatibility matrix, Discord presence, safe file operations, a log redactor, a Security and Trust page, density settings and backgrounds.
- **No adapter exists for any real game**, so Deploy, recipe runs and Play are refused for every real game. Everything was tested with made-up folders and fake launchers. Nothing was ever run in a real Tauri window, with a real game, or against a real disk. Windows CI runs the tests only; it does not open the app. Test counts at the end: 674 Rust tests (`cargo test --workspace`) and 615 UI tests (`npm test` in `ui`).
- Where each fact is recorded: `docs/decisions/` (calls and owner answers), `docs/capability-matrix/README.md` (what is labelled how, with evidence), `PROJECT_STATUS.md` (long, history), `docs/investigations/` (Steam, audio modding, SteamCMD, Security and Trust).
- `CLAUDE.md` and `PROJECT_SPEC.md` point at `design-references/` at the repo root. The images are really in `docs/design-references/`.

## 2. The owner, and working with them

- Works on Windows in PowerShell with Rust, Visual Studio Desktop Development tools and Node 22 (`docs/SETUP-WINDOWS.md`). Their clone is a folder named `Claude` in their user folder; ask for the exact path before giving commands. GitHub account `TheQwontonSoup27`.
- Claude runs in a Linux container, not on the owner's machine. It can build and test the Rust and UI code there, but it cannot run the Windows app or see a window. The only Windows check is CI (see section 5). Say plainly when something was only tested in the container.
- The owner watches usage limits. When a limit resets they sometimes send the same long message again. If the work in it is already done, reply with one line saying so and carry on.
- They review documents with another AI (DeepSeek) and want a short plain-words description beside every link or item so it can follow without effort. Reports should be plain too: what exists, what was tested and how, what was not.
- They want a stop-and-report at the points they set ("implement 1 to 2.4 and stop at 3, then report back"). Stop there; do not start the next module.
- They want the project spec worked through step by step, one small vertical slice at a time, not everything at once.
- If they have assigned a log for Claude to check on every run, check it every time (standing preference since 2026-10-24). None was assigned as of 2026-11-09.
- Merging: nothing goes into `main` without their explicit go-ahead, and tests pass first ("test what hasn't been tested; on success merge with main and test again"). On 2026-11-09 they said "you have full permission for everything, including a merge" in reply to a merge question; it was read as permission for that merge only. If a broad "full permission" could cover something destructive or hard to undo, ask.
- Commit and push only when asked; the standing slice instructions and explicit go-aheads counted as asking. Commit with explicit file paths, never `git add -A`. End commits with the attribution lines the harness gives.

## 3. Rules the owner set (old plan; ask if the new spec changes any)

Engineering rules are in `CLAUDE.md`. These came from the owner and are mostly recorded in `docs/decisions/` too:

- **Windows only.** Keep no files dedicated to other operating systems. CI is `windows-latest` only (owner decision 2026-10-22).
- **Steam** (decision 0003): never store or manage raw passwords, tokens or credentials. Use steamworks-rs only to attach to the running client's IPC, and only on request. Public player data (playtime, achievements, owned games) by public HTTP using the SteamID64. If the client is closed, launch falls back to `cmd /C start steam://run/<AppID>`.
- **Discord** (decision 0004): lightweight Rich Presence over local IPC only; no chat overlay, friends list or login. One exception (2026-10-29): a webhook destination for the clipping engine's social/Discord toggle, with the webhook URL held in Windows Credential Manager and masked by the log redactor first.
- **UI** (decision 0004): keep the established Section 1 design language and assets; Compact density by default with a global switch and per-module override; "Details" opens inline inspectors, not new windows; the Playtime Tracker is an explicit placeholder; the search box stays wired up.
- **Clipping engine plan:** a "Minimal" mode and a "Full Feature Mode" with toggles (event log watcher, companion editor handshake, multi-track audio stems, social/Discord webhooks). The RAM figures (under 50 MB Minimal, 90 to 140 MB Full) are goals, not limits. The companion editor handshake is a placeholder: no editor or protocol is chosen.
- **Game files** (decision 0006): an edit that risks a game's integrity and an edit that risks getting flagged by Easy Anti-Cheat are two different cases but must get the same action. Games the owner mods themselves are exempt: it becomes a warning the owner can override. A policy for this exists in `trust::policy` but is not connected to launch.
- **Watchdog priority:** above-normal only if it causes no micro stutters in intensive games; below normal until that is measured.
- **Quarantine:** known-harmful alterations are handled automatically, unknown ones manually.
- **Network code** goes in the separate `sul-net` crate (approved). `.github/scripts/check-network-crates.mjs` fails CI if a network-capable crate or feature is compiled into the shipped app (`sul-core`, `sul-tauri`). It is not proof that nothing in the app touches a network.
- **Build manifest:** v1 JSON with forward-only migrations. The legacy app identifier `app.soups.unifiedlauncher` is kept. Capability labels are the five in `CLAUDE.md`, layered on the old tick matrix.
- **Owner answers on the old modules:** the audio tool is `RaidingForPants/hd2-audio-modder` (SUL only registers it as an external tool and may rewrite the wrapper); a build with mods deployed stays locked against moving until a purge; the launch warning wording was approved as written; Helldivers 2 version 7.1.1 isolation is a separate adapter track, not a prerequisite for the base launcher.
- **Do not decide the old spec's "Section 7" open items silently.** Ask.

## 4. Old plan items that may be superseded

From the old spec and early messages: steamworks-rs headless Steam layer; registry game scanner with sandboxed saves; self-updater with rollback; portable mode via a `portable.marker` file; JSON template import/export; sidebar (Game Library, Settings, General Playtime Tracker); per-game hub with Play button and tabs; Direct3D/Vulkan console overlay; a Switch Pro controller preset for Elite Dangerous. From V5.1 (`docs/decisions/0006` has what was checked): `sul-watchdog` as an event-driven file watcher on a background Tokio thread; a `sul_pack_governor` that quiets non-essential diagnostics while gaming; two-tier verification (internal files by Ed25519/Minisign with quarantine; external game and anti-cheat files by read-only SHA-256 manifests); webhooks and online text through reqwest with rustls (which reqwest option was never decided); voice translation, speech-to-text and text-to-speech kept local (whisper.cpp, Piper) under a 500 MB VRAM cap; an egui overlay fed over a shared-memory ring buffer; two named AIs allowed to add to the known-harmful hash list. None of this is built except where decisions 0003 to 0008 say so.

## 5. How work was done (keep what still fits)

- **Slices.** Each slice was: core service, then Tauri commands, then UI, then tests, then deliberate-bug checks, then Windows CI, then docs (`PROJECT_STATUS.md`, `ROADMAP.md`, the capability matrix, a decision record). A final report listed which slices exist and what is unverified, then asked for the merge go-ahead.
- **Honesty.** Never claim a test, build or result that did not happen. Give exact commands and results; say when something was not run. Every capability carries one of: Supported and tested, Partially supported, Experimental, Unsupported, Not investigated, with a test date, evidence and limitations.
- **Deliberate-bug ("mutation") checks.** A script applied one broken production line, ran the tests, recorded CAUGHT or MISSED, and restored the file. A MISSED result meant a test was added, then the check was repeated. They must run one at a time (they share files and the build directory), with a timeout, and in the background for anything past 10 minutes. The scripts lived in a session scratchpad and are gone; the method is what to keep.
- **Commands.** `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings` (and `-p sul-core --features steam`); `cargo test --workspace --locked`; `node .github/scripts/check-network-crates.mjs`; in `ui`: `npx tsc --noEmit -p .`, `npm test`, `npm run build`.
- **CI.** `.github/workflows/ci.yml` runs three Windows jobs, `core`, `ui` and `tauri-shell`, on every push. Raw run logs cannot be downloaded; the workflow writes the cause of a failure into job annotations (`.github/scripts/surface-failure.ps1`). Useful: `gh run list --branch <b>`, `gh run view <id> --json jobs`.
- **References.** `docs/references.md` lists every link and named source with a short description and a status (all "Not investigated"). The owner said the links "will absolutely help". One entry (a gist about faking Discord quest progress) is kept only because they asked for every link; it is marked not usable because it would bypass a provider's rules. The owner is handling a separate `references` repository themselves.

## 6. Traps in this environment

- After any context break, run `git status --short` and `ls` the path **before every Write**. A Write that says "updated" instead of "created" overwrote an existing file. This happened three times (a source file and two committed test files); the test files were restored with `git checkout`.
- The container's `origin/<branch>` refs go stale because the clone only tracks `main`. A Stop hook that says "N unpushed commits" may be wrong. Check with `git ls-remote origin refs/heads/<b>` against `git rev-parse HEAD`, then refresh with `git fetch origin +refs/heads/<b>:refs/remotes/origin/<b>`.
- `git merge -F -` cannot read a message from stdin; use a message file. Tags cannot be pushed or created through the API from the sandbox; GitHub GraphQL is unavailable (use the REST API). Branch pushes work.
- A single tool call is limited to 10 minutes: run long jobs with `nohup ... &` and poll.
- Never run two test or mutation jobs at once on the same checkout. `cargo fmt` rewrites layouts, so edit anchors must match the formatted text.
- In the UI tests, always `await` clicks (an un-awaited `act` poisons later tests); leaked timers need a tracked mount and cleanup.
- The Tauri capabilities are least-privilege. Adding any plugin or permission (the dialog plugin for folder pickers, the asset protocol for showing local images) is an owner decision.

## 7. Open owner decisions at the end of the old plan

(a) The first real game adapter and its risk-kind decision, and connecting `trust::policy` to launch. (b) The spec 1.2 acquisition investigation (SteamCMD/depot; see `docs/investigations/steamcmd-acquisition-2026-11-03.md`) and spec 2.5 onward, not started. (c) Whether to add the dialog plugin so folders can be picked rather than typed. (d) Whether to grant the asset protocol so artwork can be shown. (e) Whether the new build reuses the old code (section 1).

## 8. A first message for the new session

> Read `HANDOFF.md` and `CLAUDE.md`, then the new spec at `<path>`. Do not read `PROJECT_STATUS.md` in full. Report current state, confirmed facts, unknowns, the smallest useful next step, and a plan for approval. Do not change or delete existing code until I approve the plan.
