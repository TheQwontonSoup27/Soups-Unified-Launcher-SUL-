# 0003 — Steam: no credentials, attach only on request, launch by `steam://run`; library IPC commands
Date: 2026-10-27 · Status: accepted by owner (architecture rules given in the request); implementation choices below are reversible

## Owner's rules (this settles the Step 1 spec's "persistent credentials" item)
1. SUL does not store or manage Steam passwords, tokens or any other credential.
2. `steamworks-rs` is used only to attach to the local Steam client that is already running.
3. Public player data (playtime, achievements, owned games) is meant to come from standard public HTTP requests keyed by the user's
   SteamID64 (Steam OpenID / Steam Web API model). **Not built.** Open question for when it is: the Web API needs a key, and a key is a
   secret, so it must be supplied by the user and kept out of source control and logs (`CLAUDE.md`).
4. Launching: use the client if it is up; if it is not, `cmd /C start steam://run/<AppID>` on Windows, which makes the system start Steam.

## What the `steamworks` crate really offers (read from steamworks 0.13.1 source, 2026-10-27)
- `Client::init()` fails if Steam is not running, if no app id can be determined (`steam_appid.txt` or `Client::init_app`), or if the
  user does not own that app. `init_app` sets `SteamAppId` / `SteamGameId` for the whole process.
- While attached, Steam shows the user as playing that app.
- There is **no call that starts another game**. The only related items are `launch_command_line` / `launch_query_param` (read this
  process's own launch data) and `restart_app_if_necessary`, which relaunches *the calling app* through Steam and tells it to exit.

So rule 4's "launch through the client pipe" cannot be done with the crate. Both routes end in the same `steam://run/<id>` request,
which the operating system hands to the running Steam client or uses to start Steam first.

## Implementation (`crates/sul-core/src/steam.rs`)
- `SteamManager`: thread-safe, cheap to clone, one shared state. `launch_steam_game(app_id) -> Result<(), String>` as requested;
  `launch(app_id) -> Result<LaunchRoute>` also says which situation it found (`RunningClient` / `StartedSteam`). One launch at a time.
  App id 0 is refused. A poisoned lock is recovered rather than wedging the manager.
- "Is a client up?" is answered without side effects: a connected `steamworks` session, or the `pid` under
  `HKCU\SOFTWARE\Valve\Steam\ActiveProcess` (Windows). Attaching on every launch would have shown the user as playing a test game each time.
- `connect()` / `disconnect()`: the explicit, opt-in `steamworks` attachment (feature `steam`), as Valve's public test app 480, reporting
  SteamID64 (as text) and persona name. Nothing calls it yet; the UI does not offer it.
- The real URL opener passes on only the exact string `steam://run/<digits>` (checked by parsing and re-formatting), so it cannot be made
  to run an arbitrary command line. It uses `CREATE_NO_WINDOW` so no console flashes.
- The `steam` feature stays **off by default**: on Windows the executable then links `steam_api64.dll` and will not start without it.
  CI compiles the feature (clippy) but does not run its tests.

## Library IPC (`crates/sul-core/src/service.rs`, `src-tauri/src/commands.rs`)
- Commands: `scan_games_command`, `get_game_matrix`, `update_feature_tick`, `launch_game_command` as requested, plus `list_games`
  (the saved library without a scan, so the page opens instantly and keeps its ticks when a scan cannot run).
- All rules live in `sul-core`: the matrix refuses a tick it does not allow (Steam headless on a non-Steam game), unknown games and
  feature ids are refused, and a change is saved atomically before it becomes the live state. The UI only explains the rules.
- The library file lives in the data folder: next to the app in portable mode, the per-user app data folder otherwise.
  A library file that cannot be parsed is renamed to `library.json.corrupt-<unix time>` before an empty one starts.
- The commands are registered in the Tauri builder in `src-tauri/src/lib.rs` (`main.rs` only calls `run()`), as the existing commands are.

## Not done / still unknown
- Public player metrics over HTTP (rule 3).
- `-silent` is not used when Steam has to be started, so a closed Steam shows its window. Starting Steam with `-silent` first is a possible
  refinement, untested.
- Nothing here has been run against a real Steam client. Whether Steam starts the game depends on Steam (ownership, installation, updates).
- The `ActiveProcess\pid` convention is from memory of how Steam behaves and is unverified on a real machine; if it is wrong the only effect
  is the reported route, not the launch.
