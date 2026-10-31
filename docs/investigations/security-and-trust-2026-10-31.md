# Security & Trust investigation (2026-10-30 to 2026-10-31)

Scope (owner's task, 2026-10-30): every item on `Settings > Security & Trust` that was labelled "Not investigated", plus the unverified network
and credential statements on that page. For each: audit, then build or verify what needs no new decision, then test, then say plainly what is still unconfirmed.

Branch: `feature/security-trust-investigation` (on top of `feature/section-2-security-trust-pane`; neither is merged into `main`).
Environment: Linux container, cargo 1.97, Node 22.23. Nothing here was run on Windows by hand; the Windows results are the CI run named in `PROJECT_STATUS.md`.

## What the page said before, and what it says now

| Row | Before | Now | Why that label |
|---|---|---|---|
| Package scanning | Not investigated ("Not built") | Experimental | The code exists and is tested; the app does not call it, so the page says "In the code, not in the app" and states the limits |
| Quarantine | Not investigated ("Not built") | Experimental | Same |
| Integrity checks | Not investigated ("Not built") | Experimental | Same |
| Automated Social/Discord Webhooks | Not investigated | Not investigated | Only the address check was built; sending and storing need decisions the owner has not made |
| Discord Rich Presence | Experimental | Experimental | Unchanged; still never run against a live Discord client |
| Headline | Not investigated | Not investigated | Weakest label on the page; now only the webhook row causes it |

The page stays read-only: no button, field, link or tab stop (a unit test and a real-browser check both look for one).
"Experimental" was chosen, not "Partially supported" or "Supported and tested", because nothing a person can do in the app reaches this code.
Each of the three rows says so in its first sentence and carries a limitation.

## 1. Network behaviour

**Question.** Does SUL (or what it is built on) reach a network? The previous version of the page said the Tauri framework "includes an HTTP client library (reqwest)".

**Finding: that statement was wrong for the Windows build, and has been corrected.** `reqwest` is in `Cargo.lock` because Tauri 2.11.6 declares it under
`[target.'cfg(any(target_os = "android", all(target_vendor = "apple", not(target_os = "macos"))))'.dependencies]` only. Evidence:

- `cargo tree -i reqwest|hyper|mio|socket2 -p sul-tauri --target x86_64-pc-windows-msvc -e normal` prints nothing for each.
- The earlier claim came from `cargo tree --target all`, which merges every platform. That was the error.
- Dependency list for the Windows target, normal dependencies only: `sul-core` 31 crates, `sul-tauri` 218, `sul-tauri` with the optional `steam` feature 221.
  None is on a deny list of 42 HTTP, socket, DNS, TLS and WebSocket crates (`.github/scripts/check-network-crates.mjs`).
- `tokio` 1.54.1 is in the Windows graph through Tauri with features `bytes`, `fs`, `io-util`, `rt`, `rt-multi-thread`, `sync`: not `net`.
  In its source the TCP and UDP types sit inside `cfg_net! { ... }` (`src/net/mod.rs`).
- The `windows` and `windows-sys` crates contain WinHTTP, WinINet and WinSock bindings, but each is behind a `Win32_Networking_*` feature.
  Every `Win32_*` feature that is on was listed (about 50, all UI, graphics, registry, threading, COM, shell, security and file system); none is a networking or remote-management feature.
- A text search of all 228 crates in the Windows graph for socket API names found hits only in: `tokio` (gated, above), `windows`/`windows-sys` (gated, above),
  and address data types or documentation examples in `log`, `schemars`, `serde_json`, `serde_with` and `url`. `url::Url::socket_addrs` can resolve a host name if something calls it; SUL's code does not.

**What now keeps it true.**
- `.github/scripts/check-network-crates.mjs`, run in the `core` CI job (windows-latest) on every change: fails on a deny-listed crate, on tokio's `net` feature, or on any `Win32_Networking*`, `Win32_NetworkManagement*` or URLMon feature,
  for `sul-core`, `sul-tauri`, and `sul-tauri` with `--features steam`. In CI it writes its result as an annotation. Its decision logic is unit-tested (`ui/tests/network-crates.test.ts`).
- `ui/tests/no-network.test.ts`: no network API, crate or address in SUL's own Rust and window source; the window's only `fetch` is for the wallpaper list it ships.
- `ui/tests/dom-wire.test.tsx` (existing): the window's content policy has no remote origin and the capability file grants no file, shell or network permission.

**Not confirmed** (see the list at the end): actual traffic; WebView2; the inside of Tauri, tokio, wry and the Windows crates.

## 2. Credentials and what SUL writes

**Question.** The Credentials rule said "the rest of what SUL writes to disk was not re-audited".

**Finding.** Every place in SUL's production Rust code that creates, overwrites, copies, renames, creates a folder for, or deletes a file was listed (10 source files; `crates/sul-core/tests/write_sites.rs`).
What each one writes: the journal and the log (appends; the log passes through the redactor first), the library index (atomic temp file then rename), a template the user asked for, the updater's last-seen version,
the app's own data folder, and the new quarantine. Nothing writes a credential: no field or JSON key in production code is named like one (`token`, `secret`, `password`, `credential`, `api_key`, `bearer`, `authorization`, `webhook_url`),
apart from the webhook address type (see 5) and the redactor's own list of names to hide.

**What now keeps it true** (`crates/sul-core/tests/write_sites.rs`, 11 tests, read source text, run on Windows CI too):
- each file that changes files is listed with what it may do and why; a new file, a new kind of change, or a stale entry fails;
- only `quarantine.rs` may delete anything; only `steam.rs` may start another program;
- a bare `use std::fs::write` (which would slip past the scan) is refused;
- no secret-named field or key outside the webhook type and the redactor;
- `WebhookUrl` does not derive `Serialize`/`Deserialize` and does not touch files;
- no credential-store or HTTP-client crate is declared in any manifest (it fails when one is added, so the page text must be updated in the same change).

**Limit.** Source text, not behaviour. It cannot see what Steam, Discord or Windows keep on their own, and a change that avoids the listed names and calls would pass.

## 3. Integrity checks (`crates/sul-core/src/trust/integrity.rs`, 16 tests)

Built on the existing `BuildManifest::verify`, read-only. Output is three piles plus a verdict:
- **violations**: a recorded file changed or missing, in an Original/Optimized/Legacy build (expected to stay as recorded);
- **changes**: the same differences in an Experimental/Custom build (facts, not violations);
- **suspicious**: an unlisted program file; a name that differs only by letter case (reported as one signal, not as a missing file plus an extra one).
Verdicts: ViolationsFound / SignalsOnly / ChangesAllowed / Matches. No wording says "safe" or "clean". Pairing is linear (a test with 40,000 case-pairs asserts it finishes in under 5 s).
A link inside a build is an error, not a pass (Windows-only test, runs in CI).
**Not claimed:** cheat detection, malware detection, knowing what the real game files should be.

## 4. Package scanning (`trust/package.rs`, 30 tests) and quarantine (`trust/quarantine.rs`, 28 tests)

Package scanning reads names and sizes only, from a folder or a `.zip` central directory (hand-written reader, no new dependency). Nothing is decompressed or opened.
Confirmed: empty or control-character names, absolute paths, `..`, reserved Windows names, trailing dot or space, invalid characters, alternate data streams, symlink entries, duplicate paths, case collisions.
Suspicious: program, script, shortcut and Java files, double extensions, nested archives, encrypted entries, high compression ratio, very many or very large entries.
Refused rather than guessed: ZIP64, multi-disk, a second end-of-archive record (tools disagree about which counts), an oversize central directory. Links in a folder are reported and never followed (Windows-only test).

Quarantine moves one file: copy, hash the copy, write `record.json` atomically, hash the original again, then remove it, with a journal entry around the whole thing. It refuses to move anything out of, or restore anything into, an original game install
(`ProtectedRoots`); a restore never overwrites; ids are validated before any path is built; `recover()` settles only cases with no data at risk and lists the rest. There is no purge and no empty operation, and a guard test lists the four places that remove anything.

## 5. Webhooks (`trust/webhook.rs`, 7 tests)

Built: `WebhookUrl::parse`, a strict check that an address is `https://<Discord host>/api[/v<n>]/webhooks/<17-20 digits>/<token>` and nothing else (no http, no look-alike host, no port, login, query, fragment, percent-encoding, extra path);
`Debug`, `Display` and `masked()` never show the token; refusal messages never repeat the input; the log redactor catches every form. **Not built:** sending, and storing in Windows Credential Manager. Blockers are in the list below.

## 6. Verification

Commands and results, Linux container, branch `feature/security-trust-investigation`:
- `cargo fmt --all -- --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo clippy -p sul-core --features steam --all-targets -- -D warnings` clean.
- `cargo test --workspace` → 316 passed, 0 failed (246 `sul-core` unit tests of which 84 are `trust::`, 16 `core.rs`, 17 `discord.rs`, 6 `steam.rs`, 11 `write_sites.rs`, 20 `sul-tauri`).
- `npm run typecheck` clean; `npm test` (ui) → 275 passed, 0 failed; `npm run build` built.
- `node .github/scripts/check-network-crates.mjs` → exit 0 ("sul-core: 31 crates; sul-tauri: 218 crates; sul-tauri (steam): 221 crates. No network crate, no tokio net feature, no Windows networking feature.").
- Mutation checks (a mutation is a deliberate one-line break of the code; a test must fail for each): 27 breaks of the Rust trust code (integrity 6, package scanning 8, quarantine 7, webhook 6), 6 of the write-site tripwire and 12 of the page and its data
  were made earlier in this work and every one was caught. The first pass left three alive; each pointed at a real gap and was closed: the quarantine never exercised "the copy differs from what was hashed" (a new test seam and a same-length-swap test),
  the id format was only tested with path-traversal strings (a 22-case table), and the webhook's specific refusal reason was untested.
  **Re-run on 2026-10-31 before commit** (a fresh sample, not the same list): 10 breaks of the trust code, 5 of the write-site tripwire, 10 of the page. All caught except one: the compression-ratio threshold of package scanning
  could be changed a thousand-fold without a test failing. That was a real gap (no test at the boundary); a test of the exact ratio and minimum-size limits was added, and four breaks of those two limits are now all caught.
  One side effect noticed and undone: adding a crate to a manifest for the tripwire check made cargo rewrite `Cargo.lock`; it was restored with git and `cargo metadata --locked` confirms the lock matches the manifests.
- Stress: the pane's UI tests 12 of 12 runs passed; the Rust trust tests 25 of 25.
- Real browser (headless Chromium on the built `ui/dist`, no Tauri window), widths 320, 360, 420, 700, 900, 1100 px, every view and every Settings tab (72 measurements): zero horizontal overflow; no page error, no console error and no `unhandledrejection`;
  no request to any other origin; Security & Trust has four cards inside the content column, six labels, no control of any kind, no mention of `reqwest`, and the "not used by the app" note. The scripts are ad hoc and not in the repo.

## Unconfirmed, non-functional, or blocked

1. **Webhook sending.** Not built. What is posted is not designed, and SUL has no network client; adding one (a crate and its TLS stack) ends the statement "no network crate is compiled into the app" and the check above must be changed on purpose.
   Owner decision needed: the payload, the client, and whether a dependency is acceptable.
2. **Windows Credential Manager storage.** Decided (2026-10-29), not built. It is a Windows-only API and needs a dependency choice (`keyring` or the `windows` crate); it can only be tested on Windows. The tripwire fails when a credential crate is added, so building it forces the page to be updated.
3. **Actual network traffic.** Never captured. The Linux container cannot run the Windows app, and no Windows machine was available to this session. A CI job that runs the app and records connections was considered and not built.
4. **WebView2 (Microsoft Edge's engine inside the window).** Not part of Cargo, so no dependency check sees it. What it contacts on its own (update checks, telemetry, SmartScreen, DNS prefetch) was not audited and is controlled by Microsoft and Windows policy, not by SUL's code.
5. **Inside Tauri, wry, tokio and the Windows crates.** The graph and enabled features were read, and the sources were searched as text for socket and HTTP calls. They were not read line by line, and a text search cannot prove an absence.
6. **Rich Presence.** Never run against a live Discord client.
7. **Integrity checks, package scanning and quarantine are not reachable from the app.** No Tauri command, no UI. That was deliberate: the page must stay read-only and wiring them needs decisions (what a person is asked, where the quarantine lives, which game builds to check). They have only been run on made-up files in a temporary folder, never on real mods or a real game install.
8. **Windows-only tests.** The junction and link tests (`#[cfg(windows)]`) compile to nothing in the Linux container and run only in Windows CI; see `PROJECT_STATUS.md` for that run.
9. **Quarantine on a real disk.** Crashes were simulated at each step; a power cut, a full disk, a file locked by another program on Windows, or antivirus touching the quarantine folder was not tested.
10. **Package scanning limits.** Names and sizes only; no content is read, so it cannot find malware or a renamed executable. ZIP64 and multi-part archives are refused, not scanned; other archive formats (`.7z`, `.rar`, `.tar`) are not read: they are refused with an error.
11. **Out of scope by the project's own rules:** malware detection, universal cheat detection, anti-cheat interaction, publisher provenance and signature checks. None is claimed anywhere.
12. **Not run:** the real Tauri window, a screen reader, a Windows machine by hand, a Windows high-contrast mode.
