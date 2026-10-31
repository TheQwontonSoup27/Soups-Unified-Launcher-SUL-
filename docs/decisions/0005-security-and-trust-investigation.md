# 0005 — Security & Trust: what was built, how it is labelled, and what was left alone

Status: made by Claude while carrying out the owner's task of 2026-10-30 ("investigate and implement until functional"). Every choice below is open to change.
Evidence and commands: `docs/investigations/security-and-trust-2026-10-31.md`.

## Context
Five things on the Security & Trust page were "Not investigated": package scanning, quarantine, integrity checks, the webhook channel, and the network and credential statements.
CLAUDE.md requires that a confirmed integrity violation and a suspicious signal are kept apart, that deletion is never ambiguous, that no label says more than the evidence, and that nothing is claimed that was not run.

## Decisions
1. **Built as library code in `sul-core::trust`, not connected to the app.** Integrity checks, package scanning, quarantine and a webhook address check exist and are tested. No Tauri command or UI calls them.
   Reason: the page must stay read-only, and connecting them needs product decisions the owner has not made (what a person is asked, where the quarantine lives, which builds are checked).
2. **Label: Experimental, with "In the code, not in the app" as the first words of each row and a stated limitation.** "Not investigated" would now be false; "Partially supported" or "Supported and tested" would suggest a person can use them.
   The page headline is still Not investigated, now only because of the webhook row.
3. **Two piles, never merged.** *Confirmed* means the evidence settles a fact (a hash differs, a path climbs out of its folder). *Suspicious* means a person has to look and it may be a false positive (an unlisted `.dll`, which many legitimate mods ship).
   A suspicious signal is never reported as a confirmed violation. For builds that are allowed to change (Experimental, Custom) a difference is a *change*, not a violation. No result, and no text, says "safe" or "clean"; "no findings" is not a verdict.
4. **Quarantine has no delete.** It copies, verifies, records, re-verifies the original, then removes the original (a move). It can restore, never overwrites, refuses anything inside an original game install in either direction, and has no purge or empty operation.
   A guard test lists the four places in the module that remove anything, and `write_sites.rs` allows deletion in no other file.
5. **Package scanning reads names and sizes only.** No decompression, no content inspection, no new dependency. ZIP64, multi-disk and ambiguous archives are refused rather than guessed.
6. **The webhook is only half-built, on purpose.** Built: a strict address check that never prints the secret and that the log redactor also catches. Not built: sending, and storage in Windows Credential Manager.
   Sending needs a network client, which would end the statement that no network crate is compiled into the app; the payload is undesigned. Both need the owner.
7. **The network statement was corrected.** The Windows build contains no `reqwest`; the earlier text was wrong (see 0004, corrected). The claim is now backed by a dependency-graph check in CI, not only by a source search, and the unaudited remainder (traffic, WebView2, framework internals) is stated on the page.
8. **Tripwires read source text and say so.** `write_sites.rs` (where files change, secret-like names, no credential or HTTP crate declared), `no-network.test.ts`, and `check-network-crates.mjs` fail when something new appears so that a person decides. They are not proofs.

## Needs the owner
- Should the three protections be connected to the app, and as what (a Mods-page scan on import, a Diagnostics action, a background check)? Until then they stay unused.
- Webhook payload, HTTP client (and TLS stack) choice, and acceptance that the network-crate check changes when it is added.
- Credential storage crate for Windows Credential Manager.
- Whether `docs/investigations/` is the right home for investigation notes.
