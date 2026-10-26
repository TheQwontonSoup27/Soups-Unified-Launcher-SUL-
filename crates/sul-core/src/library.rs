//! The persisted game library: scan results plus each game's compatibility matrix.
//!
//! A scan only reports what is installed *right now*. The library index is what
//! remembers the user's ticks across scans, so rescanning merges into it instead of
//! replacing it. A game that vanishes from a scan (uninstalled, drive unplugged) is
//! marked not-present rather than deleted, so its ticks come back with it.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::matrix::{CompatibilityMatrix, Feature};
use crate::registry::{GameScanner, InstalledGame};
use crate::{Error, Result};

pub const INDEX_SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryEntry {
    pub game: InstalledGame,
    pub matrix: CompatibilityMatrix,
    /// Whether the most recent scan found the game.
    pub present: bool,
}

/// What a merge changed, by game id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeReport {
    pub added: Vec<String>,
    /// Present before and now, with different details (name, folder, source...).
    pub updated: Vec<String>,
    /// Missing from the previous scan, found again now.
    pub returned: Vec<String>,
    /// Present before, not found now.
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryIndex {
    schema: u32,
    games: BTreeMap<String, LibraryEntry>,
}

impl Default for LibraryIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl LibraryIndex {
    pub fn new() -> Self {
        Self {
            schema: INDEX_SCHEMA,
            games: BTreeMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.games.len()
    }

    pub fn is_empty(&self) -> bool {
        self.games.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&LibraryEntry> {
        self.games.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &LibraryEntry> {
        self.games.values()
    }

    /// Merge one scan into the index.
    ///
    /// Games the user added by hand (`Manual`, `Emulated`) are never marked missing,
    /// because a scan could not have found them.
    pub fn merge_scan(&mut self, scanned: Vec<InstalledGame>) -> MergeReport {
        let mut report = MergeReport::default();
        let mut seen = BTreeSet::new();

        for game in scanned {
            seen.insert(game.id.clone());
            match self.games.get_mut(&game.id) {
                Some(entry) => {
                    let was_present = entry.present;
                    let changed = entry.game != game;
                    entry.game = game;
                    entry.present = true;
                    entry.matrix.reconcile(&entry.game);

                    if !was_present {
                        report.returned.push(entry.game.id.clone());
                    } else if changed {
                        report.updated.push(entry.game.id.clone());
                    }
                }
                None => {
                    let id = game.id.clone();
                    let matrix = CompatibilityMatrix::for_game(&game);
                    self.games.insert(
                        id.clone(),
                        LibraryEntry {
                            game,
                            matrix,
                            present: true,
                        },
                    );
                    report.added.push(id);
                }
            }
        }

        for (id, entry) in &mut self.games {
            if entry.present && !seen.contains(id) && !entry.game.source.is_user_managed() {
                entry.present = false;
                report.missing.push(id.clone());
            }
        }
        report
    }

    /// Run a scan and merge it. If the scan fails the index is left untouched, so a
    /// failed scan can never make every game look uninstalled.
    pub fn refresh<S: GameScanner + ?Sized>(&mut self, scanner: &S) -> Result<MergeReport> {
        Ok(self.merge_scan(scanner.scan()?))
    }

    /// Tick or untick a feature for one game.
    pub fn set_feature_enabled(&mut self, id: &str, feature: Feature, enabled: bool) -> Result<()> {
        self.games
            .get_mut(id)
            .ok_or_else(|| Error::UnknownGame(id.to_owned()))?
            .matrix
            .set_enabled(feature, enabled)
    }

    /// Load the index, or start an empty one if the file does not exist yet.
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| Error::json(path, e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::new()),
            Err(e) => Err(Error::io(path, e)),
        }
    }

    /// Write atomically (temp file, then rename) so a crash cannot leave half a file.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| Error::json(path, e))?;

        let mut temp = path.as_os_str().to_owned();
        temp.push(".tmp");
        let temp = std::path::PathBuf::from(temp);
        fs::write(&temp, text).map_err(|e| Error::io(&temp, e))?;
        fs::rename(&temp, path).map_err(|e| Error::io(path, e))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::matrix::Compatibility;
    use crate::registry::GameSource;

    fn game(id: &str, source: GameSource) -> InstalledGame {
        InstalledGame {
            id: id.into(),
            name: id.to_uppercase(),
            install_dir: PathBuf::from(format!(r"D:\Games\{id}")),
            source,
            store_id: None,
            publisher: None,
            steam: None,
        }
    }

    fn ids(v: &[String]) -> Vec<&str> {
        v.iter().map(String::as_str).collect()
    }

    #[test]
    fn first_scan_adds_games_with_fresh_matrices() {
        let mut index = LibraryIndex::new();
        let report = index.merge_scan(vec![
            game("a", GameSource::Steam),
            game("b", GameSource::Gog),
        ]);

        assert_eq!(ids(&report.added), ["a", "b"]);
        assert_eq!(index.len(), 2);
        let a = index.get("a").unwrap();
        assert!(a.present);
        assert_eq!(a.matrix.iter().count(), Feature::ALL.len());
    }

    #[test]
    fn ticks_survive_rescans_and_absences() {
        let mut index = LibraryIndex::new();
        index.merge_scan(vec![game("a", GameSource::Steam)]);
        index
            .set_feature_enabled("a", Feature::SaveIsolation, true)
            .unwrap();

        // Drive unplugged: the game is not found.
        let report = index.merge_scan(vec![]);
        assert_eq!(ids(&report.missing), ["a"]);
        assert!(!index.get("a").unwrap().present);

        // Drive back: same game, same ticks.
        let report = index.merge_scan(vec![game("a", GameSource::Steam)]);
        assert_eq!(ids(&report.returned), ["a"]);
        assert!(index.get("a").unwrap().present);
        assert!(
            index
                .get("a")
                .unwrap()
                .matrix
                .get(Feature::SaveIsolation)
                .unwrap()
                .enabled
        );

        // A repeat scan with nothing new reports nothing.
        assert_eq!(
            index.merge_scan(vec![game("a", GameSource::Steam)]),
            MergeReport::default()
        );
    }

    #[test]
    fn changed_details_are_updated_in_place() {
        let mut index = LibraryIndex::new();
        index.merge_scan(vec![game("a", GameSource::Registry)]);
        index.set_feature_enabled("a", Feature::Mods, true).unwrap();

        let mut moved = game("a", GameSource::Registry);
        moved.install_dir = PathBuf::from(r"E:\Games\a");
        let report = index.merge_scan(vec![moved.clone()]);

        assert_eq!(ids(&report.updated), ["a"]);
        assert_eq!(index.get("a").unwrap().game, moved);
        assert!(
            index
                .get("a")
                .unwrap()
                .matrix
                .get(Feature::Mods)
                .unwrap()
                .enabled
        );
    }

    #[test]
    fn matrix_follows_a_game_that_becomes_steam() {
        let mut index = LibraryIndex::new();
        index.merge_scan(vec![game("a", GameSource::Registry)]);
        assert_eq!(
            index
                .get("a")
                .unwrap()
                .matrix
                .get(Feature::SteamHeadless)
                .unwrap()
                .compatibility,
            Compatibility::NotApplicable
        );

        index.merge_scan(vec![game("a", GameSource::Steam)]);
        assert_eq!(
            index
                .get("a")
                .unwrap()
                .matrix
                .get(Feature::SteamHeadless)
                .unwrap()
                .compatibility,
            Compatibility::Unverified
        );
    }

    #[test]
    fn hand_added_games_are_never_marked_missing() {
        let mut index = LibraryIndex::new();
        index.merge_scan(vec![
            game("m", GameSource::Manual),
            game("e", GameSource::Emulated),
        ]);
        let report = index.merge_scan(vec![]);
        assert!(report.missing.is_empty());
        assert!(index.get("m").unwrap().present);
    }

    struct Fails;
    impl GameScanner for Fails {
        fn scan(&self) -> Result<Vec<InstalledGame>> {
            Err(Error::Unsupported("test"))
        }
    }

    struct Finds(Vec<InstalledGame>);
    impl GameScanner for Finds {
        fn scan(&self) -> Result<Vec<InstalledGame>> {
            Ok(self.0.clone())
        }
    }

    #[test]
    fn a_failed_scan_leaves_the_index_untouched() {
        let mut index = LibraryIndex::new();
        index
            .refresh(&Finds(vec![game("a", GameSource::Steam)]))
            .unwrap();
        let before = index.clone();

        assert!(index.refresh(&Fails).is_err());
        assert_eq!(index, before);
    }

    #[test]
    fn works_through_a_trait_object() {
        let boxed: Box<dyn GameScanner> = Box::new(Finds(vec![game("a", GameSource::Gog)]));
        let mut index = LibraryIndex::new();
        let report = index.refresh(boxed.as_ref()).unwrap();
        assert_eq!(ids(&report.added), ["a"]);
    }

    #[test]
    fn unknown_game_and_refused_tick_are_errors() {
        let mut index = LibraryIndex::new();
        index.merge_scan(vec![game("a", GameSource::Gog)]);
        assert!(matches!(
            index.set_feature_enabled("nope", Feature::Mods, true),
            Err(Error::UnknownGame(_))
        ));
        assert!(matches!(
            index.set_feature_enabled("a", Feature::SteamHeadless, true),
            Err(Error::FeatureUnavailable { .. })
        ));
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state/library.json");
        assert_eq!(LibraryIndex::load(&path).unwrap(), LibraryIndex::new());

        let mut index = LibraryIndex::new();
        index.merge_scan(vec![game("a", GameSource::Steam)]);
        index
            .set_feature_enabled("a", Feature::Overlay, true)
            .unwrap();
        index.save(&path).unwrap();

        assert_eq!(LibraryIndex::load(&path).unwrap(), index);
        assert!(
            !path.with_extension("json.tmp").exists(),
            "temp file is renamed away"
        );
    }
}
