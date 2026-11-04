# 0008 — UI and Tauri commands for Module 1 and items 2.1 to 2.4: the plan

Status: plan, written before the code. Owner go-ahead 2026-11-04: "Proceed with building the UI components and Tauri IPC commands for
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
