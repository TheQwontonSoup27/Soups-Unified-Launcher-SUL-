# Steam integration and keeping a legacy build from being updated

## Launching Steam games
SUL does **not** need your Steam password. Steam stays logged in; SUL reads the library files (`libraryfolders.vdf`, `appmanifest_*.acf`, read-only) and launches with `steam://rungameid/<appid>`. Code: `crates/sul-core/src/steam.rs`.

## Why a managed legacy build is not updated by Steam (design, not yet tested on a real install)
Steam updates and verifies only games it has an `appmanifest_<id>.acf` for, inside its library folders. So SUL keeps a managed copy **outside every `steamapps` folder** and with no manifest, and never edits Steam's own files. `is_inside_steam_library()` is the guard: a managed build placed inside a library is refused. Editing Steam's `AutoUpdateBehavior` is deliberately not done, because it modifies the original installation and Steam can reset it.

## Not known yet (must be tested on the real game)
- Whether Helldivers 2 7.1.1 starts from a folder outside `steamapps` (it may need Steam running and the game owned).
- Whether the game can run without reaching its servers. It is an online-service game; "offline" is unproven and SUL will not claim it. SUL will not bypass Steam authentication, DRM or anti-cheat.
