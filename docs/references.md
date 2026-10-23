# Reference projects: licences and what SUL may do with them
Checked 2026-10-23 by reading each repository's LICENSE file and README (not legal advice).

| Project | Licence | What SUL does |
|---|---|---|
| leem919/timedivers-manager-v2 | **No licence file** (all rights reserved by default) | Do not copy code. Its README documents the workflow openly (steamapps folders, SteamDB manifests, DepotDownloader or the Steam console, import version). SUL implements its own version of the idea. |
| ebkr/r2modmanPlus | MIT | May reuse code with the copyright notice kept. Profiles export as a `.r2x` zip (YAML) or as a share **code**; the code is an upload to Thunderstore's hosted API, so a SUL share code needs a service to host it. Plan: file export first, hosted code later (spec: do not assume hosting). |
| desertcod98/DeSteam | GPL-3.0 | **Not used.** It removes Steam's copy-protection wrapper from game executables, which conflicts with the spec's no-DRM-bypass rule (and GPL-3 would apply to SUL). |
| GabrielTaiga/Taiga-Launcher-for-Steam | Apache-2.0 | Adds non-Steam games to Steam. Not about blocking updates. May be consulted for ideas (game scanning, artwork); nothing copied. |
| discord/discord-rpc | MIT (deprecated by Discord) | Protocol reference only. SUL has its own implementation: `crates/sul-core/src/discord.rs`. |
| Arsenal (rsnl.gg) | Not checked; closed source as far as known | No code copied. Features may be re-implemented independently. |
