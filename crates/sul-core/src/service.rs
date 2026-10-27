//! The library as the UI uses it: scan, list, read a game's matrix, tick a feature.
//!
//! [`LibraryService`] owns the persisted [`LibraryIndex`] and the rules around it, so the Tauri
//! commands stay thin wrappers and every rule is testable without a window:
//!
//! * a scan runs **without** holding the lock (it reads the registry and every drive), then merges
//!   and saves in one step;
//! * every change is applied to a copy, saved atomically, and only then becomes the live state, so
//!   a failed save never leaves memory and disk disagreeing, and concurrent changes are not lost;
//! * ticks are validated by the matrix itself (a Steam-only feature is refused for a non-Steam
//!   game), never by the UI;
//! * a library file that cannot be parsed is moved aside, not overwritten, before starting empty.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::capability::Status;
use crate::library::{LibraryEntry, LibraryIndex, MergeReport};
use crate::matrix::{Compatibility, Feature};
use crate::registry::{GameScanner, GameSource, ScanOptions};
use crate::steam::lock;
use crate::{Error, Result};

/// One game as the library grid shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GameView {
    pub id: String,
    pub name: String,
    pub install_dir: String,
    pub source: GameSource,
    pub store_id: Option<String>,
    pub publisher: Option<String>,
    /// For Steam games, the app id the Play button hands to Steam. `None` for every other source.
    pub steam_app_id: Option<u32>,
    /// Whether the most recent scan found the game.
    pub present: bool,
    /// The weakest label among the features that can apply to the game.
    pub headline: Status,
}

/// The result of a scan: the whole library afterwards, and what the scan changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScanView {
    pub games: Vec<GameView>,
    pub added: usize,
    pub updated: usize,
    pub returned: usize,
    pub missing: usize,
}

/// One row of a game's tick-and-compatibility matrix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FeatureRowView {
    pub feature: Feature,
    pub label: &'static str,
    /// The user's tick.
    pub enabled: bool,
    pub compatibility: Compatibility,
    /// The capability label, derived from the verdict so it cannot overclaim.
    pub status: Status,
    /// Whether ticking is allowed. Unticking always is.
    pub can_enable: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatrixView {
    pub game_id: String,
    pub game_name: String,
    pub headline: Status,
    /// In [`Feature::ALL`] order.
    pub rows: Vec<FeatureRowView>,
}

fn game_view(entry: &LibraryEntry) -> GameView {
    let game = &entry.game;
    let steam_app_id = (game.source == GameSource::Steam)
        .then(|| game.store_id.as_deref()?.parse::<u32>().ok())
        .flatten();
    GameView {
        id: game.id.clone(),
        name: game.name.clone(),
        install_dir: game.install_dir.to_string_lossy().into_owned(),
        source: game.source,
        store_id: game.store_id.clone(),
        publisher: game.publisher.clone(),
        steam_app_id,
        present: entry.present,
        headline: entry.matrix.headline(),
    }
}

fn matrix_view(entry: &LibraryEntry) -> MatrixView {
    MatrixView {
        game_id: entry.game.id.clone(),
        game_name: entry.game.name.clone(),
        headline: entry.matrix.headline(),
        rows: entry
            .matrix
            .iter()
            .map(|(feature, state)| FeatureRowView {
                feature,
                label: feature.label(),
                enabled: state.enabled,
                compatibility: state.compatibility,
                status: state.status(),
                can_enable: state.compatibility.allows_enabling(),
                note: state.note.clone(),
            })
            .collect(),
    }
}

/// Games the last scan found first, then the rest; each group by name.
fn game_views(index: &LibraryIndex) -> Vec<GameView> {
    let mut games: Vec<GameView> = index.iter().map(game_view).collect();
    games.sort_by_cached_key(|g| (!g.present, g.name.to_lowercase(), g.id.clone()));
    games
}

fn scan_view(index: &LibraryIndex, report: &MergeReport) -> ScanView {
    ScanView {
        games: game_views(index),
        added: report.added.len(),
        updated: report.updated.len(),
        returned: report.returned.len(),
        missing: report.missing.len(),
    }
}

/// Move an unreadable library file out of the way, keeping its bytes.
fn quarantine(path: &Path) -> Result<PathBuf> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let mut aside = path.as_os_str().to_owned();
    aside.push(format!(".corrupt-{stamp}"));
    let aside = PathBuf::from(aside);
    std::fs::rename(path, &aside).map_err(|e| Error::io(path, e))?;
    Ok(aside)
}

/// The persisted library plus the operations the UI needs. Share it behind an `Arc`.
#[derive(Debug)]
pub struct LibraryService {
    path: PathBuf,
    index: Mutex<LibraryIndex>,
}

impl LibraryService {
    /// Open the library stored at `path`. A missing file starts an empty library; a file that
    /// cannot be parsed is renamed to `<name>.corrupt-<unix time>` and an empty library starts, so
    /// the next save cannot destroy what might still be recoverable.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let index = match LibraryIndex::load(&path) {
            Ok(index) => index,
            Err(Error::Json { source, .. }) => {
                let aside = quarantine(&path)?;
                tracing::warn!(
                    path = %path.display(),
                    moved_to = %aside.display(),
                    error = %source,
                    "library file could not be read; kept a copy and started empty"
                );
                LibraryIndex::new()
            }
            Err(other) => return Err(other),
        };
        Ok(Self {
            path,
            index: Mutex::new(index),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Every known game, including ones the last scan did not find (marked `present: false`).
    pub fn games(&self) -> Vec<GameView> {
        game_views(&lock(&self.index))
    }

    /// Scan with `scanner` and merge the result into the library.
    ///
    /// If the scan fails, or the result cannot be saved, the library is left exactly as it was.
    pub fn scan(&self, scanner: &dyn GameScanner) -> Result<ScanView> {
        let scanned = scanner.scan()?; // slow: the lock is not held

        let mut live = lock(&self.index);
        let mut next = live.clone();
        let report = next.merge_scan(scanned);
        next.save(&self.path)?;
        *live = next;
        Ok(scan_view(&live, &report))
    }

    /// Scan this machine: the Windows registry, every Steam library and the fixed drives.
    /// Unsupported on other platforms.
    pub fn scan_system(&self) -> Result<ScanView> {
        let scanner = crate::registry::system_scanner(ScanOptions::default())?;
        self.scan(scanner.as_ref())
    }

    /// One game's matrix as the UI shows it.
    pub fn matrix(&self, game_id: &str) -> Result<MatrixView> {
        lock(&self.index)
            .get(game_id)
            .map(matrix_view)
            .ok_or_else(|| Error::UnknownGame(game_id.to_owned()))
    }

    /// Tick or untick a feature for a game and save. `feature` is a [`Feature::id`].
    ///
    /// Refused, with nothing changed, for an unknown game or feature, and for a tick the matrix
    /// does not allow (for example `steam_headless` on a game that is not from Steam).
    pub fn set_tick(&self, game_id: &str, feature: &str, enabled: bool) -> Result<MatrixView> {
        let feature = Feature::from_id(feature)
            .ok_or_else(|| Error::Invalid(format!("unknown feature {feature:?}")))?;

        let mut live = lock(&self.index);
        let mut next = live.clone();
        next.set_feature_enabled(game_id, feature, enabled)?;
        next.save(&self.path)?;
        *live = next;
        live.get(game_id)
            .map(matrix_view)
            .ok_or_else(|| Error::UnknownGame(game_id.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;

    use super::*;
    use crate::registry::InstalledGame;

    struct Fixed(Vec<InstalledGame>);

    impl GameScanner for Fixed {
        fn scan(&self) -> Result<Vec<InstalledGame>> {
            Ok(self.0.clone())
        }
    }

    struct Failing;

    impl GameScanner for Failing {
        fn scan(&self) -> Result<Vec<InstalledGame>> {
            Err(Error::Unsupported("scanner unavailable"))
        }
    }

    fn game(id: &str, name: &str, source: GameSource, store_id: Option<&str>) -> InstalledGame {
        InstalledGame {
            id: id.into(),
            name: name.into(),
            install_dir: PathBuf::from(format!(r"D:\Games\{id}")),
            source,
            store_id: store_id.map(str::to_owned),
            publisher: None,
            steam: None,
        }
    }

    fn two_games() -> Fixed {
        Fixed(vec![
            game(
                "steam-553850",
                "Helldivers 2",
                GameSource::Steam,
                Some("553850"),
            ),
            game("gog-1", "Alpha", GameSource::Gog, Some("1")),
        ])
    }

    fn fresh() -> (tempfile::TempDir, LibraryService) {
        let dir = tempfile::tempdir().unwrap();
        let service = LibraryService::open(dir.path().join("library.json")).unwrap();
        (dir, service)
    }

    fn row(matrix: &MatrixView, feature: Feature) -> &FeatureRowView {
        matrix.rows.iter().find(|r| r.feature == feature).unwrap()
    }

    // --- opening ---------------------------------------------------------

    #[test]
    fn a_missing_file_starts_an_empty_library() {
        let (_dir, service) = fresh();
        assert!(service.games().is_empty());
        assert!(!service.path().exists(), "opening must not write");
    }

    #[test]
    fn a_corrupt_file_is_moved_aside_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        fs::write(&path, "{ this is not json").unwrap();

        let service = LibraryService::open(&path).unwrap();
        assert!(service.games().is_empty());
        assert!(!path.exists(), "the bad file was moved away");

        let kept: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("library.json.corrupt-"))
            .collect();
        assert_eq!(kept.len(), 1, "{kept:?}");
        assert_eq!(
            fs::read_to_string(dir.path().join(&kept[0])).unwrap(),
            "{ this is not json"
        );
    }

    // --- scanning --------------------------------------------------------

    #[test]
    fn a_scan_lists_games_and_persists_them() {
        let (dir, service) = fresh();
        let scan = service.scan(&two_games()).unwrap();
        assert_eq!(
            (scan.added, scan.updated, scan.returned, scan.missing),
            (2, 0, 0, 0)
        );
        assert_eq!(scan.games.len(), 2);

        let reopened = LibraryService::open(dir.path().join("library.json")).unwrap();
        assert_eq!(reopened.games(), service.games());
    }

    #[test]
    fn games_are_listed_present_first_then_by_name() {
        let (_dir, service) = fresh();
        service
            .scan(&Fixed(vec![
                game("b", "beta", GameSource::Gog, None),
                game("a", "Alpha", GameSource::Gog, None),
                game("z", "Zulu", GameSource::Gog, None),
            ]))
            .unwrap();
        // Zulu disappears from the next scan.
        let scan = service
            .scan(&Fixed(vec![
                game("b", "beta", GameSource::Gog, None),
                game("a", "Alpha", GameSource::Gog, None),
            ]))
            .unwrap();
        assert_eq!(scan.missing, 1);
        let order: Vec<_> = scan
            .games
            .iter()
            .map(|g| (g.name.as_str(), g.present))
            .collect();
        assert_eq!(order, [("Alpha", true), ("beta", true), ("Zulu", false)]);
    }

    #[test]
    fn only_steam_games_get_a_steam_app_id() {
        let (_dir, service) = fresh();
        let games = service.scan(&two_games()).unwrap().games;
        let by_id = |id: &str| games.iter().find(|g| g.id == id).unwrap();
        assert_eq!(by_id("steam-553850").steam_app_id, Some(553850));
        assert_eq!(
            by_id("gog-1").steam_app_id,
            None,
            "a GOG id is not a Steam app id"
        );

        // A Steam entry whose id is not a number gets no Play target rather than a wrong one.
        let (_dir, service) = fresh();
        let odd = Fixed(vec![game("s", "Odd", GameSource::Steam, Some("12x"))]);
        assert_eq!(service.scan(&odd).unwrap().games[0].steam_app_id, None);
    }

    #[test]
    fn a_failed_scan_leaves_the_library_alone() {
        let (_dir, service) = fresh();
        service.scan(&two_games()).unwrap();
        let before = service.games();
        assert!(matches!(service.scan(&Failing), Err(Error::Unsupported(_))));
        assert_eq!(service.games(), before);
    }

    #[cfg(not(windows))]
    #[test]
    fn scanning_the_system_is_unsupported_off_windows() {
        let (_dir, service) = fresh();
        assert!(matches!(service.scan_system(), Err(Error::Unsupported(_))));
    }

    // --- the matrix ------------------------------------------------------

    #[test]
    fn the_matrix_view_lists_all_eight_features_in_order() {
        let (_dir, service) = fresh();
        service.scan(&two_games()).unwrap();
        let matrix = service.matrix("steam-553850").unwrap();
        let ids: Vec<_> = matrix.rows.iter().map(|r| r.feature).collect();
        assert_eq!(ids, Feature::ALL);
        assert_eq!(matrix.game_name, "Helldivers 2");
        assert!(matrix.rows.iter().all(|r| !r.enabled && r.can_enable));
        assert_eq!(matrix.headline, Status::NotInvestigated);
    }

    #[test]
    fn an_unknown_game_has_no_matrix() {
        let (_dir, service) = fresh();
        assert!(matches!(service.matrix("nope"), Err(Error::UnknownGame(_))));
    }

    // --- ticking ---------------------------------------------------------

    #[test]
    fn a_tick_is_applied_saved_and_survives_a_rescan_and_a_restart() {
        let (dir, service) = fresh();
        service.scan(&two_games()).unwrap();

        let matrix = service
            .set_tick("steam-553850", "save_isolation", true)
            .unwrap();
        assert!(row(&matrix, Feature::SaveIsolation).enabled);

        service.scan(&two_games()).unwrap();
        assert!(
            row(
                &service.matrix("steam-553850").unwrap(),
                Feature::SaveIsolation
            )
            .enabled
        );

        let reopened = LibraryService::open(dir.path().join("library.json")).unwrap();
        assert!(
            row(
                &reopened.matrix("steam-553850").unwrap(),
                Feature::SaveIsolation
            )
            .enabled
        );

        let matrix = service
            .set_tick("steam-553850", "save_isolation", false)
            .unwrap();
        assert!(!row(&matrix, Feature::SaveIsolation).enabled);
    }

    #[test]
    fn steam_headless_is_allowed_for_steam_games_and_refused_for_others() {
        let (dir, service) = fresh();
        service.scan(&two_games()).unwrap();

        let ok = service
            .set_tick("steam-553850", "steam_headless", true)
            .unwrap();
        assert!(row(&ok, Feature::SteamHeadless).enabled);

        let gog = service.matrix("gog-1").unwrap();
        let headless = row(&gog, Feature::SteamHeadless);
        assert_eq!(headless.compatibility, Compatibility::NotApplicable);
        assert!(!headless.can_enable);
        assert_eq!(headless.note.as_deref(), Some("Steam titles only"));

        let error = service
            .set_tick("gog-1", "steam_headless", true)
            .unwrap_err();
        assert!(
            matches!(error, Error::FeatureUnavailable { .. }),
            "{error:?}"
        );
        assert!(!row(&service.matrix("gog-1").unwrap(), Feature::SteamHeadless).enabled);

        // Unticking is always allowed, and the refused tick was never written to disk.
        service.set_tick("gog-1", "steam_headless", false).unwrap();
        let reopened = LibraryService::open(dir.path().join("library.json")).unwrap();
        assert!(!row(&reopened.matrix("gog-1").unwrap(), Feature::SteamHeadless).enabled);
    }

    #[test]
    fn unknown_games_and_features_are_refused() {
        let (_dir, service) = fresh();
        service.scan(&two_games()).unwrap();
        assert!(matches!(
            service.set_tick("nope", "mods", true),
            Err(Error::UnknownGame(_))
        ));
        for bad in ["", "Mods", "mod", "Save isolation", "mods; drop"] {
            assert!(
                matches!(service.set_tick("gog-1", bad, true), Err(Error::Invalid(_))),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn a_failed_save_changes_nothing_in_memory() {
        let (dir, service) = fresh();
        service.scan(&two_games()).unwrap();
        // Make the rename target a directory so the atomic save cannot complete.
        let path = dir.path().join("library.json");
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        fs::write(path.join("keep"), "x").unwrap();

        assert!(service.set_tick("gog-1", "mods", true).is_err());
        assert!(
            !row(&service.matrix("gog-1").unwrap(), Feature::Mods).enabled,
            "memory must not run ahead of disk"
        );
        assert!(service.scan(&two_games()).is_err());
    }

    #[test]
    fn concurrent_ticks_are_not_lost() {
        let (_dir, service) = fresh();
        service.scan(&two_games()).unwrap();
        let service = Arc::new(service);

        let threads: Vec<_> = Feature::ALL
            .into_iter()
            .filter(|f| *f != Feature::SteamHeadless)
            .map(|feature| {
                let service = Arc::clone(&service);
                std::thread::spawn(move || {
                    service.set_tick("gog-1", feature.id(), true).unwrap();
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }

        let matrix = service.matrix("gog-1").unwrap();
        for r in &matrix.rows {
            assert_eq!(
                r.enabled,
                r.feature != Feature::SteamHeadless,
                "{:?}",
                r.feature
            );
        }
    }

    #[test]
    fn views_serialise_with_the_names_the_ui_reads() {
        let (_dir, service) = fresh();
        service.scan(&two_games()).unwrap();
        let games = serde_json::to_value(service.games()).unwrap();
        let steam = games
            .as_array()
            .unwrap()
            .iter()
            .find(|g| g["id"] == "steam-553850")
            .unwrap();
        assert_eq!(steam["source"], "steam");
        assert_eq!(steam["steam_app_id"], 553850);
        assert_eq!(steam["present"], true);
        assert_eq!(steam["headline"], "not_investigated");

        let matrix = serde_json::to_value(service.matrix("gog-1").unwrap()).unwrap();
        assert_eq!(matrix["game_id"], "gog-1");
        let headless = &matrix["rows"][2];
        assert_eq!(headless["feature"], "steam_headless");
        assert_eq!(headless["compatibility"], "not_applicable");
        assert_eq!(headless["status"], "unsupported");
        assert_eq!(headless["can_enable"], false);
    }
}
