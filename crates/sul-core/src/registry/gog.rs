//! GOG Galaxy installs, recorded under `HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\<id>`.
//!
//! The value names (`gameID`, `gameName`, `path`) reflect how GOG Galaxy registers
//! games; they have not been checked against a live GOG install from this codebase.

use std::path::PathBuf;

use super::access::{Hive, KeyPath, RegistryAccess, View};
use super::paths::{clean_dir, store_id};
use super::{GameSource, InstalledGame, expand_env};
use crate::Result;

pub(crate) fn discover<R: RegistryAccess + ?Sized>(registry: &R) -> Result<Vec<InstalledGame>> {
    // GOG Galaxy is 32-bit: this is the WOW6432Node branch on 64-bit Windows.
    let games_key = KeyPath::new(
        Hive::LocalMachine,
        View::Registry32,
        r"SOFTWARE\GOG.com\Games",
    );

    let mut games = Vec::new();
    for key in registry.subkeys(&games_key)? {
        let Some(name) = key.string("gameName") else {
            continue;
        };
        let Some(dir) = key
            .string("path")
            .map(expand_env)
            .and_then(|path| clean_dir(&path))
        else {
            continue;
        };
        let game_id = key.string("gameID").unwrap_or(&key.name);

        games.push(InstalledGame {
            id: store_id("gog", game_id),
            name: name.to_owned(),
            install_dir: PathBuf::from(dir),
            source: GameSource::Gog,
            store_id: Some(game_id.to_owned()),
            publisher: None,
            steam: None,
        });
    }
    Ok(games)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::access::{MemoryRegistry, s};

    #[test]
    fn reads_gog_entries_and_skips_incomplete_ones() {
        let mut reg = MemoryRegistry::default();
        let base = r"SOFTWARE\GOG.com\Games";
        reg.add(
            Hive::LocalMachine,
            View::Registry32,
            &format!(r"{base}\1207658924"),
            &[
                ("gameID", s("1207658924")),
                ("gameName", s("The Witcher 3")),
                ("path", s(r"D:\GOG Games\The Witcher 3\")),
            ],
        );
        reg.add(
            Hive::LocalMachine,
            View::Registry32,
            &format!(r"{base}\broken"),
            &[("gameName", s("No Path"))],
        );
        reg.add(
            Hive::LocalMachine,
            View::Registry32,
            &format!(r"{base}\relative"),
            &[("gameName", s("Relative")), ("path", s("games\\foo"))],
        );

        let games = discover(&reg).unwrap();
        assert_eq!(games.len(), 1);
        let game = &games[0];
        assert_eq!(game.id, "gog-1207658924");
        assert_eq!(game.name, "The Witcher 3");
        assert_eq!(
            game.install_dir,
            PathBuf::from(r"D:\GOG Games\The Witcher 3")
        );
        assert_eq!(game.source, GameSource::Gog);
        assert_eq!(game.store_id.as_deref(), Some("1207658924"));
    }
}
