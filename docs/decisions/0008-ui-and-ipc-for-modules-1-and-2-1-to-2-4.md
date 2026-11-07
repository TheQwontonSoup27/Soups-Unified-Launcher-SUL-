# 0008 — UI and Tauri commands for Module 1 and items 2.1 to 2.4: the plan

Status: plan, written before the code; slice A built 2026-11-04, slice B built 2026-11-06 and slice C1 built 2026-11-07 (see the end of this file). Owner go-ahead 2026-11-04: "Proceed with building the UI components and Tauri IPC commands for
Modules 1.1 through 2.4", in the order that "gives us a complete, testable interface slice before tackling Module 2.5+".
Work is on branch `feature/ui-modules-1-and-2-1-to-2-4`. **Nothing here merges into `main` without the owner's go-ahead.**

## What exists to build on

The backend for these modules is library code in `sul-core` (decision 0007) that no command calls. The app already has the pattern to
follow: rules live in `sul-core` and are tested without a window; `src-tauri/src/commands.rs` has thin commands over a shared
`AppState`; the UI has a wrapper per backend (`ui/src/lib/library.ts`: an `Api` interface, a `Controller`, a real `tauri…Api()` that is
`null` in a plain browser) and views that take an `api` prop so tests can pass a fake.

## The design

1. **One service in `sul-core` (`manager.rs`).** It owns the pieces every operation needs (library roots, protected roots, journal,
   disk-space reader, build registry, and later the mod store, profiles, tool registry, catalogue and runtime manager) behind one
   lock, and exposes methods that return plain serialisable views. Rules stay in the modules built in 0007; the service only wires them
   and checks arguments. It is tested with temporary folders and a fake disk, like the modules.
2. **Thin Tauri commands** over that service, run on the blocking thread pool (a copy of a whole game can take minutes), registered in
   the one `handlers()` list so the IPC tests can reach every command. Errors reach the UI as plain text, as now.
3. **No new Tauri plugin and no new permission.** Folders are typed in as paths for now rather than picked in a dialog, because a
   folder picker needs the dialog plugin and a new capability grant (least privilege, `CLAUDE.md`). That is an owner decision; until
   then a typed path is checked by the core exactly as any other path.
4. **Protected roots come from the library.** An "original install" is whatever the library lists as a game's install folder. The
   service is told those folders at start-up and after every scan, and an import or a register-in-place always takes the game from the
   library (never a typed source path), so the original install is protected before anything reads or writes near it.
5. **Destructive or long actions are previewed and confirmed.** Import (copies a whole game), deploy, purge and undo show what will
   happen first (the core already produces a dry run for deploy and purge) and ask for confirmation. A confirmation is not a safety
   check: the core refuses unsafe actions whatever the UI does.
6. **Honest labels.** Every panel shows the capability label (all *Experimental*, because they have only been tested on made-up
   folders), says that no real game adapter exists, and shows the core's own explanation when something is refused.
7. **Where it goes.** Module 1 lives on the Game Library page (a Folders panel and a Builds panel below the games). Module 2 replaces
   the placeholder behind "Mods & Mod Builder" in the sidebar, with tabs.

## Slices (each tested and pushed on its own; the report says exactly which exist)

| Slice | Core service | Commands | UI |
|---|---|---|---|
| A | roots and builds: add, forget and relink folders; import a library game as a managed build; register an install in place; verify, make active, roll back | yes | Folders and Builds panels on the Game Library page |
| B | mod store, profiles, deploy/purge preview, Purge, Purge & Deploy, undo | yes | Mods tab |
| C | recipes (plan and run), byte patches, external tools and their run records | yes | Builder and Tools tabs |
| D | launch and runtime, catalogue (categories, artwork reference, search) and the game detail view | yes | Play for a managed build, game detail |

Each slice gets: Rust tests for the service, IPC tests that send real messages to the commands in the mock runtime, UI tests with a
fake backend (including that nothing destructive is sent before confirmation), deliberate-bug checks on the new rules, a repeat of the
whole check suite, and Windows CI.

## Not part of this plan

Spec 2.5 and later, Module 3 onward, any real game adapter, acquisition of builds, and anything that needs a decision still open in
0006 and 0007. If a slice cannot be finished it is reported as not finished, not shown as working.

## Slice A, built 2026-11-04 (library folders and managed builds)

What was built, as planned: `sul-core::manager::ManagerService`; nine commands (`manager_view`, `manager_add_root`, `manager_relink_root`,
`manager_forget_root`, `manager_import_build`, `manager_register_in_place`, `manager_verify_build`, `manager_activate_build`,
`manager_rollback_slot`); and the Folders and Builds panels on the Game Library page.

Calls made while building it (all inside the plan; say so if any should change):

- **A game's folder always comes from the library.** The page sends a game id; the command looks the game up and refuses an unknown game, or a
  game the last scan did not find ("scan again before using its folder"). A typed source path is never accepted.
- **Protection after a scan.** The library's install folders are protected at start-up and again after every scan, so a game found later is
  protected before anything is built near it. To test this without Windows the scan command was split: `scan` finds the system scanner and
  `scan_with` takes any scanner.
- **One operation at a time on the page.** A second click while one runs is ignored (the backend would make it wait anyway), every button
  is off while one runs, and the page says a copy cannot be cancelled once started and shows no progress.
- **The question is not the check.** Copy, record in place and stop listing ask first. The core refuses unsafe requests whatever the page sends.
- **A new line in the write-site list.** `crates/sul-core/tests/write_sites.rs` flagged the new IPC test file because it makes made-up folders in
  a temporary folder; it is listed with that reason. No production file gained a way to change files.

Still open from this slice: folders are typed, not picked (needs the dialog plugin and a permission, the owner's decision); no progress or
cancel for a large copy (needs core changes); no repair or migrate button.

## Slice B, built 2026-11-06 (mods, profiles, deploy, purge, undo)

What was built, as planned: the mod half of `ManagerService` (`manager/modding.rs`); eleven commands (`mods_view`, `mods_import`, `mods_verify`,
`mods_create_profile`, `mods_edit_profile`, `mods_plan_deploy`, `mods_deploy`, `mods_plan_purge`, `mods_purge`, `mods_undo_purge`,
`mods_purge_and_deploy`); and the Mods & Mod Builder page.

Calls made while building it (all inside the plan; say so if any should change):

- **The service owns the adapters.** `AdapterRegistry` now holds `GameAdapter + Send` adapters so it can live behind the service's one lock. A game
  with no adapter has no mod layout, so a preview, a deploy and Purge & Deploy are refused for it; a purge and an undo are ledger-based and need no
  adapter. No adapter for a real game exists, so in the app every deploy is refused today, and the page says so.
- **A previewed deploy is never trusted.** The page shows a preview and asks, but the core plans the deploy again when it runs and refuses anything
  unsafe or changed, whatever the page sends. A blocked plan cannot be confirmed on the page either.
- **Profiles may move builds only while not deployed.** A profile deployed in its build cannot be moved to another build until it is purged there,
  and it can only move to a build of its own game. (Related to the owner's 2026-11-04 answer that a build with mods deployed stays locked against moving until a purge; that answer is about builds, this rule is about profiles, so say if it should work differently.)
- **The "add to profile" picker offers the most recently imported version of each mod**, not the highest version number; the version is shown, and it
  can be changed on the profile afterwards.
- **The notes a person writes about a mod** (source, licence, game versions) are shown as their words and are not checked.
- **Mod layouts are validated at use, not only at registration.** The registry already refuses an invalid layout when an adapter is registered; the
  service checks again before every preview and deploy, so an adapter whose layout changes later is still stopped. A test with an adapter that turns
  invalid after registration covers it.

Still open from this slice: no real adapter (so deploying has never been tried on a real game); no conflict analysis; no way to choose which files
of a mod to deploy; no progress or cancel for a large copy; folders are typed, not picked.


## Slice C1, built 2026-11-07 (recipes and patches)

Slice C was split into C1 (recipes and patches, the builder panel) and C2 (external tools), so each could be tested and pushed on its own. This is C1. What was built, as planned: the builder half of `ManagerService` (`manager/building.rs`) and the saved-recipe list (`builder/book.rs`);
seven commands (`builder_view`, `builder_save_recipe`, `builder_plan_recipe`, `builder_run_recipe`, `builder_read_run`, `builder_verify_run`,
`builder_mod_files`); and the builder panel, which sits under the mods panel on the Mods & Mod Builder page (stacked panels, not tabs, as in slice B).

Calls made while building it (all inside the plan; say so if any should change):

- **A saved recipe has no remove.** Saving writes only the recipe list, atomically. A new recipe needs an unused id; a change must name the SHA-256 of
  the recipe it started from, so a stale window cannot overwrite a newer edit. The id of a saved recipe cannot change, and neither can its game or its
  name on the page (the id is made from the name once). There is no remove, as for profiles: every run keeps its own copy of the recipe it came from.
- **A preview writes nothing and a run is confirmed first.** The page shows what a run would build and asks; the core plans again when it runs. A blocked
  plan cannot be confirmed. A run goes only into the workspace folder of a mod library, never into a game or a build, and deletes nothing.
- **Previews and runs need an adapter.** Without one the core has no mod layout to judge a recipe's output, so both are refused, and a recipe for such a
  game can still be written and saved. No adapter for a real game exists, so in the app every preview and every run is refused today, and the page says so.
- **A runtime-code step is a note.** A recipe with one can be saved and is flagged on the page, but cannot be previewed or run.
- **`builder_mod_files` exists only to write patch steps.** A patch step names its file by path and SHA-256; the lookup lists some of a stored mod's files
  (the first 200 matches, with the number that matched) so the person need not work a hash out by hand.
- **Hunks are typed as text,** one per line: offset (decimal or 0x hexadecimal), old bytes, new bytes. The page checks the shape; the core checks the
  rest (order, overlap, same length, that the file has those bytes).
- **The question dialog scrolls inside itself.** Found when the page was loaded in a browser: a long preview could not be scrolled and its buttons were
  out of reach. The fix is in the shared dialog style, so it also applies to the questions of slices A and B.

Still open from this slice: no real adapter; no remove or rename for a recipe; no way to delete a run's workspace folder; no progress or cancel for a large
run; folders are typed, not picked. Slice C2 (external tools) and slice D (launch, catalogue, game detail) are not built.
