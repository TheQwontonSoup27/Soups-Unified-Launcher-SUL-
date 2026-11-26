# 0013 — nosignups.net, web lookup, a daily digest, an AI-only archive and a notation for the AI: proposals

Status: **proposals; the owner answered the four questions of section 8 on 2026-11-25 (section 9, recorded as D-156).** No code was written and no spec text was changed. Proposals that were not asked about (P1, P3 a/d/e, P4) are still only proposals. Written 2026-11-25 on branch `rebuild/r0-bootstrap`.

In plain words: the owner asked (2026-11-24, again 2026-11-25) for four things. (1) Use nosignups.net and the projects it lists to improve SUL, without cancelling planned features. (2) See whether the directory's web tools could help the Combat AI look things up online with fewer tokens and faster. (3) Compile the internet's data every day into a packet for Jarvis, and build a "Wayback Machine" of our own in a form only an AI reads. (4) Create a language made for the AI to read. This record says what I found, what the spec already contains (so nothing is built twice), six proposals that fit the rules and each other, what cannot be done as asked and why, and the questions I need answered. Each proposal can be accepted or dropped on its own.

## 1. What was asked, and what the owner said about effort

Quoted from the 2026-11-25 message: use the link and the repositories in it as references and to "further improve features or functionality"; "propose changes, additions or performance changes as long as they don't cancel out features already planned"; "make sure they are compatible with one another and if not, make sure we can make anything we apply … compatible"; "time is no factor to us, so even if you'd value anything to be not worth it due to solely time it'd take to do, ignore it". So the filters used below are **compatibility, legality and physical limits**, never "it would take long".

## 2. What I found

### 2.1 The directory

nosignups.net lists 273 free, mostly in-browser tools in 10 categories. I read all 70 entries of the development category one by one (name, tags, description) and ran a keyword scan (AI, token, scrape, crawl, archive, markdown, search, PDF, OCR, MCP and similar) over the other 203, which gave 44 hits, each judged from the directory's own text. Nothing was cloned, built or run (`docs/references.md`, sections 1c, 1d, 9).

**The directory holds no crawler, search engine, web-to-text extractor, archiver or AI web-agent tool.** For "web tools for the Combat AI" it therefore offers ideas, not parts. Its useful entries are few: a token-cost analyser (HappyTokenizer), a code-to-graph context server for AI assistants (CodeGraphContext), a log sanitiser (ShareClean), a viewer for very large logs (Log Voyager) and browser-side SQL over files (datasette-lite). The rest are unrelated (icons, diagram editors, converters) or conflict with SUL's rules (reasons in `references.md` 1d). I would rather say this plainly than invent a connection.

The better source of improvements turned out to be web standards and the spec's own chosen parts, below.

### 2.2 What the spec already has

| Need | Already in the spec | What is missing |
|---|---|---|
| Search, fetch, clean text | Ch. 8 §2.5: RustXNG (localhost MCP) -> adblock-rust -> readdown (spec figure: 90 to 95% fewer tokens than raw HTML) -> Exa-style excerpts -> verification packet; p50 about 660 ms; `camofox` fallback (A.195); `github_search` (§2.6) | Nothing to replace |
| Search cache | A.58 (R2): `blake3`-keyed `redb` verification cache, over 80% hit rate, TTL plus source-hash invalidation (A.46 resolved by D-146); warm-up A.58b | It caches **verdicts**, not pages, and has no time dimension |
| Exact token counts | D-146: `kitoken` for local token counting | Not yet wired to where it matters (pack budgets, excerpt budgets); readdown gives an estimate |
| Long-term memory | A.186 (R2): SQLite FTS5 plus `simsimd`, under 50 ms; knowledge packs with BM25 and embeddings (Ch. 11 §2.15) | It is the user's and the agent's memory, not web snapshots |
| Signed, sectioned, distributed data | Ch. 11 §2.17 to §2.21: JSON packs, Ed25519 Minisign, OS-modal approval, git sync, hive fetch (`raw.githubusercontent.com/sul-hive-mind/`, 24 h TTL, 5 per session) | A pack is one JSON object: fine for kilobytes, not for gigabytes |
| Compact model-facing text | TSCG compiles tool schemas (spec figure: at least 51% smaller); dense key-value packs (persona about 270 tokens); ReCAP and Bud compaction; Ch. 1 §11.3 code-shape rules | No compact view of **tables of data rows** |
| Jarvis learning new things | Ch. 14 §2.6: Jarvis ingests release notes, advisories and benchmarks (benchmarks on a 30-day cycle) and raises Tier 1 reviews | No scheduled, signed, daily feed |

## 3. Proposals

Each has: what it is, where it attaches, what it does not change, risk, how we would know it works, and a suggested release. Release placement is my suggestion.

### P1. Exact token counts where they matter

- **What.** Use `kitoken` (already decided, D-146) with each profile's own tokenizer to count tokens, and use that count instead of byte-length estimates in: the token cost shown in the pack-load modal (Ch. 11 §2.8, §2.20); `ExcerptSelect`, so it fills a token budget exactly; the packet budget table (490 baseline, 640 gaming, 890 coding); each archive chunk's header (P4); and a CI check that fails when a pack's counted tokens exceed its budget. The idea comes from HappyTokenizer (GPL-3.0: no code is copied).
- **Attaches to.** Ch. 11 §2.8; Ch. 8 §2.5 `ExcerptSelect`; D-146.
- **Does not change.** Any pipeline stage or default.
- **Risk.** Small. A count is right only for the tokenizer of the model that is loaded, so a mismatch must be an error, not a guess. `kitoken`'s handling of each profile's tokenizer has **not been tested**.
- **Works when.** Counts equal the reference tokenizer's counts on a fixed text set (golden file).
- **Release.** The counter with the first crate that loads a model (R0 layer 6, `sul-ai-native`); the CI budget check at R1.

### P2. SDN, a dense notation the model reads (the "language for the AI")

- **What.** A small text notation that is a **view** of data, never its stored form. Stored and wire forms stay as they are (signed JSON packs, `rkyv` on the wire), so signatures, the "no executable code in packs" guard and the golden bytes do not change. Three shapes: `key: value` lines; indentation for nesting; and **tables**, where one header line names the fields once and each row carries only values (for example `claims[2]{id,text,verdict}:` followed by two rows). Tables are where the saving comes from, because JSON repeats every field name on every row. The encoder and decoder are written in Rust by us, lossless (JSON to SDN to JSON returns the same value; property tests and golden files). Used for: verification-packet claim and evidence tables, `game_db` and knowledge sections, tool **results**. Not used for: tool schemas (TSCG keeps those), the model's tool-call **output** (stays whatever the Verifier validates), anything the user edits.
- **Why measured only.** TOON, the idea's source, is reported (InfoQ, a self-reported claim I did not verify) to use 25% fewer tokens than compact JSON and 46% fewer on one accuracy test, and the saving depends on the data's shape. A format a small model has not seen can also cost accuracy: the spec's own TSCG evidence shows a 1.5B model falling from 91.5% to 48.0% when the format mismatched. So SDN is switched on **per profile, only if** SUL's own eval harness (Ch. 10 §10.6) shows accuracy equal to or better than JSON with fewer tokens (counted with P1). Otherwise JSON stays.
- **Where it pays off most.** If a profile is later fine-tuned (Ch. 10 §10.6 already plans LoRA training), SDN-formatted training examples can be included, and then the model reads SDN as a native format. That is the honest route to "a language made for the AI".
- **Attaches to.** Ch. 11 (packs, packets), Ch. 8 §2.7 (verification packet), Ch. 10 §10.13 (TSCG sits beside it).
- **Does not change.** Pack format, signing, TSCG, tool-call output.
- **Release.** R1 at the earliest (needs the profiles and the eval harness). A gate id would be assigned by the owner.

### P3. The lookup ladder and fetch-side token savers

- **What.** (a) A fixed order of tries for every `web_search`: session packet and Entity Ledger, then knowledge packs and the archive (P4), then the A.58 cache, then the live pipeline, then `camofox`. Each tier answers or hands down, and the model always sees the one packet format (the spec's existing rule). (b) **Revalidate, do not refetch**: when the archive holds a page, ask the site whether it changed (`If-None-Match` / `If-Modified-Since`); a "not modified" answer costs no tokens. (c) **Ask for the machine-readable form first**: `Accept: text/markdown` (Cloudflare's Markdown for Agents: Pro, Business and Enterprise plans only, so not every site), a page's `rel="alternate" type="text/markdown"` link, and `/llms.txt` (a September 2024 proposal; how many sites use it is not measured), falling back to readdown. (d) **Pre-warm** while idle or before launch, never during a game: when `sul-governor` sees a game, fetch that game's patch notes and news into the archive through the already-declared Steam Web API, so in-game questions are answered locally. (e) A short-TTL **negative cache** for queries that returned nothing (`E_SEARCH_NO_RESULTS`).
- **Attaches to.** Ch. 8 §2.5; A.58 and A.58b; Ch. 16 §16.15.14.
- **Does not change.** RustXNG, adblock-rust, readdown, the verification packet, validator rules #17, #19, #20, profile routing.
- **Open conflict (question Q2).** The spec says only RustXNG talks HTTP for search ("the search pipeline … do not have their own HTTP clients") and the AST guard forbids `sul-net` an HTTP client beyond RustXNG on localhost and the GitHub API. (b) and (c) change the **fetch** stage. If RustXNG is a separate third-party program (its licence check, A.161, is not done), SUL cannot change its fetch. Options: patch a fork (AGPL duties then apply to the fork), or give `sul-net` a minimal fetch client of its own and declare it, as D-149 did for Steam and Discord. (a), (d) and (e) need neither.
- **Safety.** Markdown from a site is still untrusted web text. It goes through the verification packet exactly as HTML does; "markdown" does not make it safe (Ch. 16 §16.5).
- **Works when.** On a fixed set of about 100 URLs, a variant is kept only if it beats the readdown path on tokens and p50 latency without losing answer accuracy.
- **Release.** (a) and (e) with the search pipeline at R1; (b), (c), (d) at R2 with A.58.

### P4. The AI-only archive ("our own Wayback"), working name `.sularc`

- **What.** A local, append-only, signed store of web pages in the form the AI wants them. It is a **new file kind beside `.sulpack`**, with a small `.sulpack` index pack so loading authority, signing and the OS modal stay as they are. Record layout in section 5.4. Raw HTML is not kept by default (the pipeline already reduces it to Markdown), so this is an "AI view" archive, not a faithful browser replay; keeping raw HTML on request is an option.
- **Layout, borrowed from two real formats.** WACZ (read: a ZIP with an index you can binary-search and read by byte range, plus a hash manifest) and ZIM (read: content compressed in clusters, Zstandard by default since 2021). SUL's version: immutable segment files; records compressed in blocks with `zstd` and a per-segment trained dictionary (shared boilerplate); a sorted index (URL key plus capture time to segment, offset, length) memory-mapped so a lookup is a binary search and one read; a manifest with a `blake3` hash for every segment and an Ed25519 signature (the same trust model as Ch. 11 §2.21); paragraph-level `blake3` de-duplication. "As of" lookups pick the latest capture at or before a time, which is the Wayback behaviour.
- **Search** reuses the A.186 engine (SQLite FTS5 plus `simsimd`), not a new one. **Interchange:** export and import WARC/WACZ so standard tools can replay it and community archives can be imported; ZIM import for offline Wikipedia, subject to the licence check in 5.2.
- **Tool.** `recall_archive(query | url, as_of?, budget_tokens)` returns at most the token budget, with `sularc://host/path@time` citations like the existing `sulpack://` ones. Archive text is **evidence only**: it passes through the verification packet and never becomes instructions (Ch. 16 §16.5; Ch. 11 §2.21).
- **Relation to A.58.** A.58 stores short-lived **verdicts**; the archive stores long-lived **sources**. A.58 already keys on the `blake3` of the source (D-146), which is the archive's content hash, so invalidation is one hash comparison.
- **Reuses only what the workspace already has:** `blake3`, `ed25519-dalek`, `zstd`, `memmap2`, `redb`, `simsimd`, `kitoken`, SQLite.
- **Cost.** Disk, and one new crate (`sul-archive`, not in the R0 list).
- **Works when.** Export then import returns identical records; flipping one byte of a segment fails verification; retrieval meets the A.186 target (under 50 ms) on a benchmark; bytes per page are measured against the readdown Markdown alone. No number is promised before it is measured.
- **Release.** Format spec and crate at R2 with A.58 (it needs the R1 search pipeline to have anything to store).

### P5. The daily digest (the packet for Jarvis)

- **What.** The request as worded ("all the data of the internet every day") cannot be done: see 5.1 and 5.2. The version that can is a **scoped daily delta** with three layers. (1) A **watch list**: the sources Ch. 14 §2.6 already says Jarvis reads (WebView2 and Tauri release notes and CVEs, the upstream releases and benchmarks listed there), plus anything the owner adds, plus patch notes for the owner's games through the Steam Web API. (2) **Open bulk**, optional and owner-chosen: topics from openly licensed sources, each record carrying its licence. Wikimedia publishes "adds/changes" dumps, but its own page calls them experimental, possibly down for days, with no completeness guarantee. (3) **On-use**: every page the AI itself looked up (P3, P4), kept locally.
- **Where the work runs: off the PC.** A scheduled GitHub Actions workflow (the CI route Ch. 11 §2.14 already uses) fetches the watch list, runs SUL's own pipeline code (readdown, claim extraction), writes a small signed `.sularc` delta and publishes it to `sul-hive-mind`. The PC pulls it with the existing hive fetch, which is already a declared network surface (`raw.githubusercontent.com/sul-hive-mind/`, 24 h TTL, 5 per session). So the delta itself needs no new surface. If the daily size outgrows what git comfortably holds, the next option is GitHub Releases, which **would** add a surface and needs the owner's say (not proposed now).
- **What goes in.** Derived facts, short excerpts, links and hashes, not copies of third-party pages, unless the source's licence allows copying (5.2).
- **What Jarvis reads.** Never the digest. He gets a section of about 150 tokens (today's top items, like `game_context`) and calls `recall_archive` for detail, so the 490 and 640-token budgets hold. Ranking is deterministic: source tier, recency, match with the user's profile, game and hardware (an idea from World Monitor, AGPL-3.0: no code copied).
- **Works when.** The signature verifies, the delta applies, rollback works, and the daily size stays inside a budget the owner sets.
- **Release.** R2 (needs P4 and the hive). Reading the watch list can start earlier as ordinary Tier 1 reviews.

### P6. Share-safe diagnostic bundles

- **What.** The spec says the diagnostics bundle has "redaction applied to paths" (Ch. 12 §2.9), and the old build already has `redact_text` for secrets in logs (`crates/sul-core/src/logging.rs`) and a webhook masker. Idea from ShareClean: before a bundle is written or uploaded to Sentry (opt-in), run a **rule table** over every text file: Windows user name in paths, SteamID64, e-mail, IP addresses, token-shaped strings, webhook URLs, machine name. Show the owner what was removed, and ship nothing until approved. Written in Rust as an extension of `redact_text`, no new dependency. The categories are SUL's own, not ShareClean's (its rules were not read).
- **Attaches to.** Ch. 12 §2.9; `sul-core` logging; the Sentry opt-in (Ch. 16 §16.11).
- **Risk.** Low. A missed pattern leaks, so the bundle keeps its "preview before sharing" step.
- **Works when.** Unit tests with made-up secrets of each kind; a deliberate-bug check per rule.
- **Release.** With `sul-crash` (R0 layer 3, which is next in the build order) or just after. This is the one proposal I would start now.

## 4. Notes: ideas kept, not proposed now

- **CodeGraphContext (and the similar CodeMap AI).** `codebase_map.sulpack` is a skeleton plus a blueprint (Ch. 11 §2.7). A call and import graph ("what breaks if I change X") would help the Coder AI. SUL would build it in Rust with Tree-sitter into SQLite, not a Python tool or a graph database. Propose when Coder AI work starts (R2).
- **Log Voyager.** Reads 10 GB or larger logs with filters and bookmarks in the browser. A model for a viewer of SUL's own JSONL logs on the diagnostics page; UI work, later.
- **datasette-lite, sqlite playgrounds.** A browser SQL view could browse the archive, but WebAssembly in the window needs a looser content-security policy, which `tauri-capability-audit` exists to catch. A native query page over FTS5 is the better fit.
- **RomPatcher.js.** A pointer for patch-format support in the mod toolchain at R2; patches would apply only to SUL's managed copies, never to an original install.
- **WebLLM, Sipp.** Rejected: a second model in the window would compete with `sul-ai-native` for video memory (Ch. 10 §10.10).
- **OSINT Framework, Osiris.** R3 territory, gated on A.152.
- **Windhawk, Rosint+, fmhy, Champagne Wiki.** Not used (reasons in `references.md` 1d).

## 5. The "whole internet, every day" request, and the archive in depth

### 5.1 Scale

Common Crawl's August 2026 crawl is 2.14 billion pages and 360 TiB uncompressed, 84.78 TiB as compressed WARC files, for one month, run by a nonprofit on a cluster. My arithmetic: about 181 KiB per page; downloading 84.78 TiB takes **8.6 days of non-stop transfer on a full gigabit line**, 17.3 days at 500 Mbps and 86.3 days at 100 Mbps, before storing, reading or processing any of it. That is a bandwidth, storage and site-limit problem, not a coding-hours problem, so "time is no factor" does not change it. And the AI's context is 2K to 8K tokens: a digest is useful only if it is queried (P4), never read whole.

### 5.2 Law and licences (not legal advice)

- **Common Crawl's terms** (read): crawled content "may be subject to separate terms of use or terms of service" from its owners; users must "respect the copyrights and other applicable rights of third parties"; the terms grant a "non-sublicensable" licence and say nothing about redistribution; users indemnify Common Crawl against claims arising from use "in connection with artificial intelligence, machine learning"; they recommend legal counsel.
- **Wikipedia text.** Its licence was **not confirmed**: Wikimedia's legal page could not be opened in this session (the permission prompt went unanswered). It must be read before any Wikimedia text enters a pack, because share-alike duties decide what the hive may redistribute.
- **Consequences for the design.** A personal, local archive of pages the owner's own AI looked up is a personal copy. A public hive that serves copies of pages is redistribution. So the digest ships derived facts, short excerpts, links and hashes, plus full text only from openly licensed sources, with the licence on each record. Anything fetched through `camofox` (A.195, "blocked public pages") is marked and excluded from anything shared. Every fetch honours `robots.txt` and rate limits.

### 5.3 What can be done daily

The three layers of P5, with Common Crawl left as an optional **import of chosen domains by index lookup**. That would need a new declared network surface and a legal read of its terms, so it is not proposed now.

### 5.4 Record layout sketch (P4)

| Field | Purpose |
|---|---|
| URL key and capture time | Lookup; "as of" time travel |
| Status, `ETag`, `Last-Modified` | Cheap revalidation (P3 b) |
| Content hash (`blake3`) | De-duplication; A.58 invalidation key |
| Clean Markdown (readdown output) | The AI view |
| Token count per tokenizer id | Exact budgeting (P1) |
| Atomic claims with source offsets; entity list | Feeds the Entity Ledger and AFEV offline |
| Domain trust tier | Persisted from the trust ledger |
| Quantised embedding | `simsimd` retrieval at 0 MB video memory |
| Licence flag, `robots` flag, fetch route (direct or `camofox`) | Decides what may ever be shared (5.2) |

## 6. Compatibility check

| Rule | P1 | P2 | P3 | P4 | P5 | P6 |
|---|---|---|---|---|---|---|
| One binary, one config schema, one installer, one update channel | yes | yes (a profile setting) | yes (keys under `[net.search]`) | yes (an `[archive]` section) | yes (rides the hive pack channel, not a second app updater) | yes |
| Zero Electron or Node in `sul.exe` | yes | yes | yes | yes | yes (the daily job runs in CI, not in `sul.exe`) | yes |
| No kernel code | yes | yes | yes | yes | yes | yes |
| Structural refusals and A.154 (safety below the model) | untouched | untouched; output side unchanged | untouched; web text stays untrusted evidence | untouched; archive text is evidence only | untouched | untouched |
| Declared network surfaces (Ch. 16 §16.15.14) | none | none | none for (a), (d), (e); **(b), (c) depend on Q2** | none (local) | none while it rides the hive; Releases would add one | none |
| Signed packs, OS-modal loading, no executable code in packs | n/a | stored packs stay JSON | n/a | index pack follows the rules; segments are signed data | signed by the maintainer key like hive packs | n/a |
| Performance (2 GB video-memory cap on games, below-normal priority) | negligible | negligible | pre-warm only when no game runs | memory-mapped, CPU search, 0 MB video memory | download once per day, off-game | negligible |
| Existing gates (A.55, A.58, A.58b, A.161, A.186, A.195) | feeds them | none | A.58 and A.195 unchanged | A.58 keyed on the same hash; A.186 engine reused | none | none |

Between proposals: P1 feeds P2 (measuring), P3 and P4 (counts). P4 backs P3 and P5. P5 produces P4 segments. P6 stands alone. **The only real conflicts found:** the HTTP-client rule (Q2), and the JSON-object pack envelope versus a multi-gigabyte archive (resolved by a separate file kind plus an index pack).

## 7. Not verified, not done

- No code and no benchmarks. Every saving figure above is someone else's claim or the spec's own figure, **not measured in SUL**.
- Pages were opened through the fetch tool, which returns a short answer written by a small summarising model, not the page itself (`references.md` section 9 lists each). The Wikimedia legal page was not opened.
- Whether RustXNG exists as a separate project, and its licence, is A.161 and was not checked.
- `kitoken` against each profile's tokenizer: not tested.
- The ZIM structure was taken from an encyclopedia page (compression and identity only), not from the format's own specification.
- Release placement is my suggestion.

## 8. Questions for the owner

1. **Q1, the digest's scope.** The three layers of P5 (recommended), the watch list only, or hold it for now?
2. **Q2, where the fetch-side savers live (P3 b and c).** A patch to a RustXNG fork, a small fetch client in `sul-net` declared as a new surface (like D-149), or skip them and do only (a), (d), (e) and the archive?
3. **Q3, the notation (P2).** Measured-only as written (recommended), build it first and measure after, or drop it?
4. **Q4, what I build next.** Continue the R0 order (`sul-watchdog`, then `sul-crash` with P6), start with P6 and P1, or write the `.sularc` format spec first?

Deferred until Q1 is answered: the exact archive container, the daily size budget, and the release placement of P3 to P5.

## 9. The owner's answers (2026-11-25, recorded as D-156)

The owner chose the recommended option on all four questions.

| Question | Answer | What follows |
|---|---|---|
| Q1, the digest's scope | **Three layers**: the watch list, owner-chosen openly licensed topics, and the pages the AI looked up | P5 is planned for R2 with P4 and the hive. Nothing is built. The "whole internet every day" version stays out (5.1, 5.2). |
| Q2, where the fetch-side savers live | **An own client in `sul-net`**, declared as a new network surface | Written from scratch in Rust when its turn comes. Its declaration (an amendment of Ch. 16 §16.15.14 and of the AST guard that limits `sul-net`'s HTTP use) is its own decision, written when the client is built. Minimum safeguards, so they are not forgotten: https only; refuse private, loopback and link-local addresses (a page must not be able to point the client at the owner's own network); a redirect limit; a size cap; a content-type allowlist; timeouts; `robots.txt` and per-host rate limits; no cookies, no credentials. Until then the spec's rule that only RustXNG makes HTTP requests for search stands. |
| Q3, the notation | **Measured-only** | The lossless encoder and decoder come with the model profiles and the eval harness (R1). Stored packs stay JSON. |
| Q4, what to build next | **Continue the R0 order** | `sul-watchdog` next, then `sul-crash`, where P6 (share-safe diagnostic bundles) is built. Stop and report after each. |

**Still open, asked before any R2 work starts:** the archive container (a new `.sularc` kind with an index pack, as proposed, or something else), the daily size budget, the release placement of P3 to P5, and the Wikipedia text licence. P1 (exact token counts) comes up again at R0 layer 6, when the first crate that loads a model is built.

## Sources (all in `docs/references.md`, section 9 unless noted)

- https://nosignups.net (section 1c)
- https://developers.cloudflare.com/fundamentals/reference/markdown-for-agents
- https://llmstxt.org/
- https://specs.webrecorder.net/wacz/1.2.0/
- https://en.wikipedia.org/wiki/ZIM_(file_format)
- https://commoncrawl.org/blog/august-2026-crawl-archive-now-available
- https://commoncrawl.org/terms-of-use
- https://www.infoq.com/news/2025/11/toon-reduce-llm-cost-tokens
- https://archive.sunet.se/mirror/wikimedia.org/other/incr/index.html
- https://github.com/hsr88/log-voyager (section 1d)
