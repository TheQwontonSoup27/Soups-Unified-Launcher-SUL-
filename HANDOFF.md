# HANDOFF: the owner and the environment

Written 2026-11-09 for a Claude with no memory of this project; trimmed 2026-11-20 (the owner asked for the stale parts to go and the owner and environment notes to stay). Where this file disagrees with the new spec, the new spec wins. Where it disagrees with the owner's latest message, the message wins. Anything marked "old plan" may be superseded: say so and ask rather than assuming. What has been done, and what comes next, is in `docs/SESSION_STATE.md`, not here.

## 0. Read order, and what to skip

1. `docs/SESSION_STATE.md` (the log the owner assigned: check it first on every run).
2. `CLAUDE.md` (engineering rules and the R0 build order).
3. This file.
4. Only the parts of the spec the task needs. `CLAUDE.md` forbids loading `SUL-ARCHITECTURE.md` or `SUL-LEDGER-AND-DECISIONS.md` whole: use the Chapter Index and the Section Index.

Do not read these in full unless the task needs them (they are large and the owner is watching token use): `SUL-ARCHITECTURE.md` (1 MB), `SUL-DECISIONS.md` (125 KB), `docs/capability-matrix/README.md` (29 KB), `docs/references.md` (26 KB). Grep them. `docs/decisions/0001` to `0009` are the digest of why things are as they are.

## 1. The old build, in brief

- The V5.1 build is a Tauri 2 shell (`src-tauri`), seven Rust crates that used to be one (`sul-core`, `sul-trust`, `sul-quarantine`, `sul-launcher`, `sul-builds`, `sul-mod`, `sul-app`; split on 2026-11-21 and 2026-11-23, `docs/decisions/0010-sul-core-split.md` and `0011-trust-split.md`), an empty `crates/sul-net` (the one place network code may live) and a React 19 / TypeScript UI (`ui`, Vite). It covers the old spec's Module 1 (library, builds, folders, launch, catalogue) and items 2.1 to 2.4 (mods, profiles, deploy, purge, undo, recipes, patches, external tools), plus the Steam attach and launch layer, the compatibility matrix, Discord presence, safe file operations, a log redactor, a Security and Trust page, density settings and backgrounds.
- It is being merged into the new design, not discarded: the new design wins where they disagree, and nothing goes into a `legacy/` folder. The decision is in `docs/SESSION_STATE.md`; the module-by-module plan is `docs/decisions/0009-sul-core-merge-map.md`.
- **No adapter exists for any real game**, so Deploy, recipe runs and Play are refused for every real game. Everything was tested with made-up folders and fake launchers. Nothing was ever run in a real Tauri window, with a real game, or against a real disk. Windows CI runs the tests only; it does not open the app.
- The last commit of the old build is `131ca0c`, kept on the branch `archive/legacy-pre-ground-up-2026-11-09` (it still has `PROJECT_SPEC.md`, `PROJECT_STATUS.md`, `ROADMAP.md` and the old `CLAUDE.md`, which is also in `docs/legacy/`). `main` is at `cc29fcb`, one commit later; nothing of the rebuild is on `main`. The point before the Step 1.5 consolidation is `9ece116`.
- Where facts are recorded: `docs/decisions/` (calls and owner answers), `docs/capability-matrix/README.md` (what is labelled how, with evidence), `docs/investigations/` (Steam, audio modding, SteamCMD, Security and Trust), `docs/references.md` (links), `docs/design-references/` (the UI images).

## 2. The owner, and working with them

- Works on Windows in PowerShell with Rust, Visual Studio Desktop Development tools and Node 22 (`docs/SETUP-WINDOWS.md`). Their clone is a folder named `Claude` in their user folder; ask for the exact path before giving commands. GitHub account `TheQwontonSoup27`.
- Claude runs in a Linux container, not on the owner's machine. It can build and test the Rust and UI code there, but it cannot run the Windows app or see a window. The only Windows check is CI (see section 5). Say plainly when something was only tested in the container.
- The owner watches usage limits. When a limit resets they sometimes send the same long message again. If the work in it is already done, reply with one line saying so and carry on.
- They review documents with another AI (DeepSeek) and want a short plain-words description beside every link or item so it can follow without effort. Reports should be plain too: what exists, what was tested and how, what was not.
- They want a stop-and-report at the points they set ("implement 1 to 2.4 and stop at 3, then report back"). Stop there; do not start the next step. Consult them on conflicts rather than going silent.
- They want the project spec worked through step by step, one small vertical slice at a time, not everything at once.
- A log is assigned: `docs/SESSION_STATE.md`. Read it at the start of every run and update it at the end of each milestone (standing preference since 2026-10-24).
- Merging: nothing goes into `main` without their explicit go-ahead, and tests pass first ("test what hasn't been tested; on success merge with main and test again"). On 2026-11-09 they said "you have full permission for everything, including a merge" in reply to a merge question; it was read as permission for that merge only. If a broad "full permission" could cover something destructive or hard to undo, ask. Pushing the branch `rebuild/r0-bootstrap` is approved; `main` is not.
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
- **Network code** goes in the separate `sul-net` crate (approved). Since 2026-11-20 the `cargo xtask ac-regression` network guard (`xtask/src/network.rs`, Ch. 16 §16.15.14) fails if a network crate reaches the Windows graph of any shipped package other than `sul-net`, if tokio's `net` or a Windows networking feature is on, or if Rust source outside `sul-net` names a networking API. It replaced the old Node script. It is not proof that nothing in the app touches a network (it does not look inside WebView2, std or Steam).
- **Portable mode:** keep it (`portable.marker`); the owner wants it later for another portable version of the app (2026-11-19).
- **Build manifest:** v1 JSON with forward-only migrations. The legacy app identifier `app.soups.unifiedlauncher` is kept. Capability labels are the five in the capability matrix, layered on the old tick matrix.
- **Owner answers on the old modules:** the audio tool is `RaidingForPants/hd2-audio-modder` (SUL only registers it as an external tool and may rewrite the wrapper); a build with mods deployed stays locked against moving until a purge; the launch warning wording was approved as written; Helldivers 2 version 7.1.1 isolation is a separate adapter track, not a prerequisite for the base launcher.
- **Do not decide the old spec's "Section 7" open items silently.** Ask.
- **V5.1 directions** (the watchdog as file watcher, `sul_pack_governor`, webhooks over reqwest, local speech under a 500 MB VRAM cap, an egui overlay over a shared-memory ring, two-tier verification, two AIs adding to the known-harmful hash list) were checked in `docs/decisions/0006`. The new spec governs them now; none is built except where decisions 0003 to 0008 say so.

## 4. How work was done (keep what still fits)

- **Slices.** Each slice was: core service, then Tauri commands, then UI, then tests, then deliberate-bug checks, then Windows CI, then docs (`docs/SESSION_STATE.md`, the capability matrix, a decision record). A final report listed which slices exist and what is unverified, then asked for the merge go-ahead.
- **Honesty.** Never claim a test, build or result that did not happen. Give exact commands and results; say when something was not run. A gate that is still a placeholder must say so in its own output. Every capability carries one of: Supported and tested, Partially supported, Experimental, Unsupported, Not investigated, with a test date, evidence and limitations.
- **Deliberate-bug ("mutation") checks.** Apply one broken production line, run the tests, record CAUGHT or MISSED, and restore the file (`git checkout -- <file>`). A MISSED result means a test is added, then the check is repeated. Run them one at a time (they share files and the build directory), with a timeout, and in the background for anything past 10 minutes.
- **Commands.** `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings` (and `-p sul-launcher --features steam`); `cargo test --workspace --locked`; `cargo xtask config-check`, `electron-audit`, `ac-regression`, `release-check --release R0`, `ipc-check`; in `ui`: `npx tsc --noEmit -p .`, `npm test`, `npm run build`.
- **CI.** `.github/workflows/ci.yml` runs four Windows jobs on every push: `r0-substrate` (the new crates and the xtask gates), `core`, `ui` and `tauri-shell`. Raw run logs cannot be downloaded; the workflow writes the cause of a failure into job annotations (`.github/scripts/surface-failure.ps1`). Useful: `gh run list --branch <b>`, `gh run view <id> --json jobs`.
- **References.** `docs/references.md` lists every link and named source with a short description and a status (all "Not investigated"). The owner said the links "will absolutely help". One entry (a gist about faking Discord quest progress) is kept only because they asked for every link; it is marked not usable because it would bypass a provider's rules. The owner is handling a separate `references` repository themselves.

## 5. Traps in this environment

- After any context break, run `git status --short` and `ls` the path **before every Write**. A Write that says "updated" instead of "created" overwrote an existing file. This happened three times (a source file and two committed test files); the test files were restored with `git checkout`.
- The container's `origin/<branch>` refs go stale because the clone only tracks `main`. A Stop hook that says "N unpushed commits" may be wrong. Check with `git ls-remote origin refs/heads/<b>` against `git rev-parse HEAD`, then refresh with `git fetch origin +refs/heads/<b>:refs/remotes/origin/<b>`.
- `git merge -F -` cannot read a message from stdin; use a message file. Tags cannot be pushed or created through the API from the sandbox; GitHub GraphQL is unavailable (use the REST API). Branch pushes work.
- **The container's Rust is older than CI's.** The container has rustc 1.97 and cannot download another; the Windows runner uses the current `stable`. A lint that only warns here can be a hard error there (2026-11-20: `miette::bail!` as the last expression of a function failed to compile on the runner; use `Err(miette::miette!(...))` there). A green container run is not a green CI run: read the Windows result before calling a step done.
- A single tool call is limited to 10 minutes: run long jobs with `nohup ... &` and poll.
- A workspace test run can hide a missing dev-dependency feature (feature unification turns it on for everyone). Before reporting a change to crates, build each package's tests alone: `cargo test -p <pkg> --no-run` (found 2026-11-21: `sul-app` failed alone while the workspace run passed).
- Never run two test or mutation jobs at once on the same checkout. `cargo fmt` rewrites layouts, so edit anchors must match the formatted text.
- In the UI tests, always `await` clicks (an un-awaited `act` poisons later tests); leaked timers need a tracked mount and cleanup.
- The Tauri capabilities are least-privilege. Adding any plugin or permission (the dialog plugin for folder pickers, the asset protocol for showing local images) is an owner decision.

## 6. Open owner decisions carried over from the old plan

(a) The first real game adapter and its risk-kind decision, and connecting `trust::policy` to launch. (b) The SteamCMD/depot acquisition investigation (`docs/investigations/steamcmd-acquisition-2026-11-03.md`) and the old spec's items from 2.5 onward, not started. (c) Whether to add the dialog plugin so folders can be picked rather than typed. (d) Whether to grant the asset protocol so artwork can be shown. (Whether the new build reuses the old code is decided: merge, new design wins.)
