//! The catalogue half of the service (spec 1.1): the categories and artwork the owner gives each game, and the detail
//! page for one game.
//!
//! As in [`super`], the rules live elsewhere ([`crate::catalogue`]); this file wires them to the builds, adapters and
//! launch state the service holds and returns plain views.
//!
//! * **The library names the games.** Every call is given the games the library lists, as `(id, display name)`; the
//!   page never supplies a name, so a name on the detail page is always one the library found.
//! * **Categories and artwork are the owner's own labels.** Artwork is a *reference* to an image file the owner
//!   already has: SUL checks it is a normal image file of a sensible size and remembers where it is. It never
//!   copies it, fetches it or reads its pixels.
//! * **An edit is saved before it is shown.** A change that cannot be saved (an invalid name, a file that is not an
//!   image, a location SUL must not write into) leaves both the saved file and the live state as they were.
//! * **Counts and lists only cover the games the library lists.** A category kept for a game the library no longer
//!   lists stays in the saved file but is not offered as a filter, so a filter never shows a count it cannot fill.
//! * **Reading changes nothing.** The detail view is gathered from the builds, the adapter registry and the launch
//!   state; it never asks a program whether it ended and never starts anything.
//!
//! Not here: showing the image (that needs a Tauri permission the owner has not granted), downloading artwork, any
//! tag a program assigns by itself, or the search over names, ids and categories ([`crate::catalogue::Catalogue::search`]),
//! which exists in the library but is not connected to a page.

use std::path::PathBuf;

use serde::Serialize;

use super::{Inner, ManagerService};
use crate::catalogue::{self, GameDetail};
use crate::{Error, Result};

/// A game as the library lists it: `(id, display name)`. The names come from the library, never from the page.
pub type GameRef = (String, String);

/// One category in use, with the games (by id, in the library's order) that carry it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CategoryView {
    /// The first spelling the owner gave; a repeat that differs only in ASCII case is the same category.
    pub name: String,
    pub count: usize,
    pub game_ids: Vec<String>,
}

/// The categories one game carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntryView {
    pub game_id: String,
    pub categories: Vec<String>,
}

/// Everything the library page needs to filter and label games by category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogueView {
    /// Sorted by name (ignoring case). Only categories that at least one listed game carries.
    pub categories: Vec<CategoryView>,
    /// Only listed games that carry at least one category.
    pub entries: Vec<EntryView>,
}

/// What an edit returned: the detail of the game after it, and the catalogue view after it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Edited {
    pub detail: GameDetail,
    pub view: CatalogueView,
}

fn catalogue_view_of(g: &Inner, games: &[GameRef]) -> CatalogueView {
    let categories = g
        .catalogue
        .all_categories()
        .into_iter()
        .filter_map(|(name, _)| {
            let game_ids = g.catalogue.in_category(&name, games);
            (!game_ids.is_empty()).then_some(CategoryView {
                count: game_ids.len(),
                name,
                game_ids,
            })
        })
        .collect();
    let entries = games
        .iter()
        .filter_map(|(id, _)| {
            let categories = g.catalogue.categories_of(id);
            (!categories.is_empty()).then(|| EntryView {
                game_id: id.clone(),
                categories: categories.to_vec(),
            })
        })
        .collect();
    CatalogueView {
        categories,
        entries,
    }
}

fn detail_of(g: &Inner, game: &GameRef) -> GameDetail {
    catalogue::detail(
        &game.0,
        &game.1,
        &g.catalogue,
        &g.builds,
        &g.adapters,
        g.runtime.state_of(&game.0),
    )
}

fn edited_of(g: &Inner, game: &GameRef, games: &[GameRef]) -> Edited {
    Edited {
        detail: detail_of(g, game),
        view: catalogue_view_of(g, games),
    }
}

impl ManagerService {
    /// The categories in use and which listed games carry them. Reads only.
    pub fn catalogue_view(&self, games: &[GameRef]) -> Result<CatalogueView> {
        let g = self.lock()?;
        Ok(catalogue_view_of(&g, games))
    }

    /// Everything SUL knows about one game: its builds and which is active in each slot, its adapter and the weakest
    /// capability label, whether it is running, its categories and artwork reference, and plain notes about what is
    /// missing. Reads only.
    pub fn game_detail(&self, game: &GameRef) -> Result<GameDetail> {
        let g = self.lock()?;
        Ok(detail_of(&g, game))
    }

    /// Replace a game's categories. Names are trimmed, a repeat (ignoring ASCII case) is dropped, and a name that is
    /// empty, too long or has control characters refuses the whole change.
    pub fn catalogue_set_categories(
        &self,
        game: &GameRef,
        games: &[GameRef],
        categories: &[String],
    ) -> Result<Edited> {
        let mut g = self.lock()?;
        let g = &mut *g;
        g.catalogue
            .set_categories(&g.protected, &game.0, categories)?;
        Ok(edited_of(g, game, games))
    }

    /// Point a game at an image file the owner already has, or clear the reference with `None`. The file is checked
    /// (a normal `.png`, `.jpg`, `.jpeg` or `.webp` file, not empty, not huge) and referenced, never copied.
    pub fn catalogue_set_artwork(
        &self,
        game: &GameRef,
        games: &[GameRef],
        image: Option<&str>,
    ) -> Result<Edited> {
        let image = match image {
            None => None,
            Some(text) if text.trim().is_empty() => {
                return Err(Error::Invalid(
                    "type the path of an image file, or clear the artwork".into(),
                ));
            }
            Some(text) => Some(PathBuf::from(text.trim())),
        };
        let mut g = self.lock()?;
        let g = &mut *g;
        g.catalogue
            .set_artwork(&g.protected, &game.0, image.as_deref())?;
        Ok(edited_of(g, game, games))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::adapters::fixtures::MadeUpGame;
    use crate::adapters::{AdapterRegistry, GameAdapter};
    use crate::capability::Status;
    use crate::manager::BuildSource;
    use crate::manifest::{State, Variant};
    use crate::process::ExitInfo;
    use crate::process::fake::{FakeLauncher, Script};
    use crate::roots::{DiskSpace, RootKind};
    use crate::runtime::LaunchState;

    const GAME: &str = "madeupgame";

    struct Disk(Cell<u64>);
    impl DiskSpace for Disk {
        fn free_bytes(&self, _p: &Path) -> Result<u64> {
            Ok(self.0.get())
        }
    }

    struct Fx {
        _dir: tempfile::TempDir,
        base: PathBuf,
        state: PathBuf,
        svc: ManagerService,
    }

    fn game(id: &str, name: &str) -> GameRef {
        (id.to_owned(), name.to_owned())
    }

    fn library() -> Vec<GameRef> {
        vec![
            game("alpha", "Alpha"),
            game("beta", "Beta"),
            game("gamma", "Gamma"),
        ]
    }

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    fn open_in(state: &Path, with_adapter: bool) -> Result<ManagerService> {
        let mut adapters = AdapterRegistry::new();
        if with_adapter {
            adapters.register(Box::new(MadeUpGame::new())).unwrap();
        }
        ManagerService::with_parts_and_launcher(
            state,
            Box::new(Disk(Cell::new(u64::MAX / 2))),
            adapters,
            Box::new(FakeLauncher::new()),
        )
    }

    fn fx() -> Fx {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        let state = base.join("state");
        let svc = open_in(&state, false).unwrap();
        Fx {
            _dir: dir,
            base,
            state,
            svc,
        }
    }

    /// A service with an adapter for the made-up game and one managed build of it; the launcher starts nothing real.
    fn fx_with_build(script: Option<Script>) -> (Fx, String) {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        let state = base.join("state");
        let install = base.join("original_install");
        std::fs::create_dir_all(install.join("data")).unwrap();
        std::fs::write(install.join("game.exe"), "exe").unwrap();
        std::fs::write(install.join("data/a.bin"), "AAAA").unwrap();
        std::fs::create_dir_all(base.join("ssd")).unwrap();
        let launcher = FakeLauncher::new();
        if let Some(s) = script {
            launcher.scripts.lock().unwrap().push_back(s);
        }
        let mut adapters = AdapterRegistry::new();
        adapters.register(Box::new(MadeUpGame::new())).unwrap();
        let svc = ManagerService::with_parts_and_launcher(
            &state,
            Box::new(Disk(Cell::new(u64::MAX / 2))),
            adapters,
            Box::new(launcher),
        )
        .unwrap();
        svc.add_root("ssd", "Fast drive", &base.join("ssd"), RootKind::Builds)
            .unwrap();
        svc.protect_installs(std::slice::from_ref(&install))
            .unwrap();
        let built = svc
            .import_build(
                &BuildSource {
                    game_id: GAME.into(),
                    display_name: "Made-up Game".into(),
                    install_dir: install,
                    variant: Variant::Experimental,
                    base_version: "1.0.0".into(),
                    source_description: "my own install".into(),
                },
                "ssd",
            )
            .unwrap();
        (
            Fx {
                _dir: dir,
                base,
                state,
                svc,
            },
            built.build_id,
        )
    }

    fn picture(f: &Fx, name: &str) -> PathBuf {
        let p = f.base.join(name);
        std::fs::write(
            &p,
            b"not really a picture, only a file with a picture's name",
        )
        .unwrap();
        p.canonicalize().unwrap()
    }

    fn set(f: &Fx, id: &str, cats: &[&str]) -> Edited {
        let lib = library();
        let g = lib.iter().find(|(i, _)| i == id).unwrap().clone();
        f.svc
            .catalogue_set_categories(&g, &lib, &names(cats))
            .unwrap()
    }

    #[test]
    fn with_nothing_saved_the_view_is_empty_and_no_file_exists() {
        let f = fx();
        let v = f.svc.catalogue_view(&library()).unwrap();
        assert!(v.categories.is_empty() && v.entries.is_empty());
        assert!(!f.state.join("catalogue.json").exists());
    }

    #[test]
    fn categories_are_listed_by_name_with_the_games_that_carry_them_in_the_librarys_order() {
        let f = fx();
        // The library lists gamma before alpha: every list below must follow that order, not the order of saving.
        let lib = vec![
            game("gamma", "Gamma"),
            game("alpha", "Alpha"),
            game("beta", "Beta"),
        ];
        f.svc
            .catalogue_set_categories(&lib[1], &lib, &names(&["RPG", "Co-op"]))
            .unwrap();
        let after = f
            .svc
            .catalogue_set_categories(&lib[0], &lib, &names(&["rpg"]))
            .unwrap();
        assert_eq!(
            after.view.categories,
            vec![
                CategoryView {
                    name: "Co-op".into(),
                    count: 1,
                    game_ids: vec!["alpha".into()]
                },
                CategoryView {
                    name: "RPG".into(),
                    count: 2,
                    game_ids: vec!["gamma".into(), "alpha".into()]
                },
            ]
        );
        assert_eq!(
            after.view.entries,
            vec![
                EntryView {
                    game_id: "gamma".into(),
                    categories: names(&["rpg"])
                },
                EntryView {
                    game_id: "alpha".into(),
                    categories: names(&["RPG", "Co-op"])
                },
            ]
        );
        assert_eq!(f.svc.catalogue_view(&lib).unwrap(), after.view);
    }

    #[test]
    fn a_category_of_a_game_the_library_does_not_list_is_kept_but_not_offered() {
        let f = fx();
        let lib = library();
        let ghost = game("ghost", "Ghost");
        f.svc
            .catalogue_set_categories(&ghost, &lib, &names(&["Retired"]))
            .unwrap();
        set(&f, "alpha", &["Shared"]);
        // The ghost is not in the library's list, so it is neither counted nor listed.
        let v = f.svc.catalogue_view(&lib).unwrap();
        assert_eq!(
            v.categories
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Shared"]
        );
        assert!(v.entries.iter().all(|e| e.game_id != "ghost"));
        // It is still in the saved file: when the library lists it again, the category is back.
        let mut with_ghost = lib.clone();
        with_ghost.push(ghost);
        let v = f.svc.catalogue_view(&with_ghost).unwrap();
        let retired = v.categories.iter().find(|c| c.name == "Retired").unwrap();
        assert_eq!(
            (retired.count, retired.game_ids.clone()),
            (1, vec!["ghost".to_owned()])
        );
    }

    #[test]
    fn a_name_that_differs_only_in_case_is_dropped_and_the_first_spelling_is_kept() {
        let f = fx();
        let e = set(
            &f,
            "alpha",
            &["  Strategy ", "STRATEGY", "Co-op", "strategy"],
        );
        assert_eq!(e.detail.categories, names(&["Strategy", "Co-op"]));
        assert_eq!(e.view.categories.len(), 2);
    }

    #[test]
    fn a_bad_name_refuses_the_whole_change_and_nothing_is_saved() {
        let f = fx();
        set(&f, "alpha", &["Keep"]);
        let lib = library();
        let a = lib[0].clone();
        let long = "x".repeat(crate::catalogue::MAX_CATEGORY_LEN + 1);
        let too_many: Vec<String> = (0..crate::catalogue::MAX_CATEGORIES_PER_GAME + 1)
            .map(|i| format!("c{i}"))
            .collect();
        for bad in [
            names(&["Fine", "   "]),
            names(&["Fine", &long]),
            names(&["Fine", "tab\there"]),
            too_many,
        ] {
            assert!(
                f.svc.catalogue_set_categories(&a, &lib, &bad).is_err(),
                "{bad:?}"
            );
        }
        let live = f.svc.catalogue_view(&lib).unwrap();
        assert_eq!(live.entries.len(), 1);
        assert_eq!(live.entries[0].categories, names(&["Keep"]));
        // And the file on disk says the same: a service opened fresh sees only what was kept.
        let again = open_in(&f.state, false).unwrap();
        assert_eq!(again.catalogue_view(&lib).unwrap(), live);
    }

    #[test]
    fn what_is_saved_is_still_there_when_the_service_is_opened_again() {
        let f = fx();
        let art = picture(&f, "cover.png");
        let lib = library();
        f.svc
            .catalogue_set_categories(&lib[1], &lib, &names(&["Racing"]))
            .unwrap();
        f.svc
            .catalogue_set_artwork(&lib[1], &lib, Some(art.to_str().unwrap()))
            .unwrap();
        assert!(f.state.join("catalogue.json").is_file());
        let again = open_in(&f.state, false).unwrap();
        let d = again.game_detail(&lib[1]).unwrap();
        assert_eq!(d.categories, names(&["Racing"]));
        assert_eq!(d.artwork, Some(art));
        assert_eq!(again.catalogue_view(&lib).unwrap().categories.len(), 1);
    }

    #[test]
    fn an_edit_that_cannot_be_saved_leaves_the_live_state_and_the_file_as_they_were() {
        let f = fx();
        set(&f, "alpha", &["Before"]);
        let file = f.state.join("catalogue.json");
        let saved = std::fs::read(&file).unwrap();
        // Once the folder that holds the file is a protected location, SUL must not write there.
        f.svc
            .protect_installs(std::slice::from_ref(&f.state))
            .unwrap();
        let lib = library();
        assert!(
            f.svc
                .catalogue_set_categories(&lib[0], &lib, &names(&["After"]))
                .is_err()
        );
        assert_eq!(std::fs::read(&file).unwrap(), saved);
        let v = f.svc.catalogue_view(&lib).unwrap();
        assert_eq!(v.entries[0].categories, names(&["Before"]));
    }

    #[test]
    fn a_catalogue_file_that_cannot_be_read_stops_the_service_and_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state");
        std::fs::create_dir_all(&state).unwrap();
        let file = state.join("catalogue.json");
        std::fs::write(&file, "{ this is not a catalogue").unwrap();
        assert!(open_in(&state, false).is_err());
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "{ this is not a catalogue"
        );
    }

    #[test]
    fn the_detail_of_a_game_with_a_build_lists_it_with_its_adapter_and_says_nothing_runs() {
        let (f, build) = fx_with_build(None);
        let d = f.svc.game_detail(&game(GAME, "Made-up Game")).unwrap();
        assert_eq!(
            (d.game_id.as_str(), d.display_name.as_str()),
            (GAME, "Made-up Game")
        );
        assert_eq!(d.builds.len(), 1);
        let b = &d.builds[0];
        assert_eq!(b.build_id, build);
        assert!(b.managed);
        assert_eq!(b.state, State::Ready);
        // Importing a build does not make it the active one of its slot, and the page is told.
        assert!(!b.active);
        assert!(
            d.notes
                .iter()
                .any(|n| n.contains("None of this game's builds is active")),
            "{:?}",
            d.notes
        );
        f.svc.activate_build(&build).unwrap();
        let d = f.svc.game_detail(&game(GAME, "Made-up Game")).unwrap();
        assert!(d.builds[0].active);
        assert_eq!(
            d.adapter.as_ref().map(|a| a.id.as_str()),
            Some(MadeUpGame::new().id())
        );
        assert_eq!(d.launch, LaunchState::NotRunning);
        assert_eq!(d.capability_label, d.capability.label());
        assert!(d.notes.is_empty(), "{:?}", d.notes);
    }

    #[test]
    fn a_game_with_no_adapter_and_no_build_says_so_instead_of_showing_nothing() {
        let f = fx();
        let d = f.svc.game_detail(&game("alpha", "Alpha")).unwrap();
        assert!(d.adapter.is_none() && d.builds.is_empty());
        assert_eq!(d.capability, Status::NotInvestigated);
        assert!(
            d.notes.iter().any(|n| n.contains("No adapter")),
            "{:?}",
            d.notes
        );
        assert!(
            d.notes.iter().any(|n| n.contains("no build")),
            "{:?}",
            d.notes
        );
    }

    #[test]
    fn a_running_game_shows_as_running_in_its_detail() {
        let (f, build) = fx_with_build(Some(Script {
            polls_before_exit: 1_000,
            exit: ExitInfo {
                code: Some(0),
                success: true,
            },
            output: None,
        }));
        f.svc.launch_start(&build, false).unwrap();
        let d = f.svc.game_detail(&game(GAME, "Made-up Game")).unwrap();
        match d.launch {
            LaunchState::Running { build_id, .. } => assert_eq!(build_id, build),
            other => panic!("expected a running game, got {other:?}"),
        }
        // Reading the detail did not ask the program whether it ended: it still runs.
        let again = f.svc.game_detail(&game(GAME, "Made-up Game")).unwrap();
        assert!(matches!(again.launch, LaunchState::Running { .. }));
    }

    #[test]
    fn artwork_is_a_reference_to_the_owners_image_and_nothing_is_copied() {
        let f = fx();
        let art = picture(&f, "cover.PNG");
        let before = std::fs::read(&art).unwrap();
        let lib = library();
        let e = f
            .svc
            .catalogue_set_artwork(&lib[0], &lib, Some(art.to_str().unwrap()))
            .unwrap();
        assert_eq!(e.detail.artwork.as_deref(), Some(art.as_path()));
        assert_eq!(std::fs::read(&art).unwrap(), before);
        let in_state: Vec<String> = std::fs::read_dir(&f.state)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            in_state.iter().all(|n| !n.to_lowercase().ends_with(".png")),
            "{in_state:?}"
        );
        // Artwork alone does not make a category.
        assert!(e.view.categories.is_empty() && e.view.entries.is_empty());
    }

    #[test]
    fn artwork_that_is_not_a_normal_image_file_is_refused_and_the_reference_stays_unset() {
        let f = fx();
        let lib = library();
        let text = f.base.join("notes.txt");
        std::fs::write(&text, "words").unwrap();
        let dir_named_like_image = f.base.join("folder.png");
        std::fs::create_dir_all(&dir_named_like_image).unwrap();
        let empty = f.base.join("empty.png");
        std::fs::write(&empty, "").unwrap();
        for bad in [
            f.base.join("missing.png"),
            text,
            dir_named_like_image,
            empty,
        ] {
            assert!(
                f.svc
                    .catalogue_set_artwork(&lib[0], &lib, Some(bad.to_str().unwrap()))
                    .is_err(),
                "{}",
                bad.display()
            );
        }
        assert_eq!(f.svc.game_detail(&lib[0]).unwrap().artwork, None);
        assert!(!f.state.join("catalogue.json").exists());
    }

    #[test]
    fn a_blank_path_is_refused_and_only_an_explicit_clear_removes_the_reference() {
        let f = fx();
        let art = picture(&f, "cover.png");
        let lib = library();
        f.svc
            .catalogue_set_artwork(&lib[0], &lib, Some(art.to_str().unwrap()))
            .unwrap();
        // The refusal says what to do (the page shows this sentence as it is), not that some file was not found.
        match f.svc.catalogue_set_artwork(&lib[0], &lib, Some("   ")) {
            Err(Error::Invalid(why)) => {
                assert_eq!(why, "type the path of an image file, or clear the artwork")
            }
            other => panic!("a blank path must be refused as invalid: {other:?}"),
        }
        assert_eq!(f.svc.game_detail(&lib[0]).unwrap().artwork, Some(art));
        let cleared = f.svc.catalogue_set_artwork(&lib[0], &lib, None).unwrap();
        assert_eq!(cleared.detail.artwork, None);
        assert_eq!(f.svc.game_detail(&lib[0]).unwrap().artwork, None);
    }

    #[test]
    fn the_typed_path_is_trimmed() {
        let f = fx();
        let art = picture(&f, "cover.jpg");
        let lib = library();
        let typed = format!("  {} \n", art.display());
        let e = f
            .svc
            .catalogue_set_artwork(&lib[0], &lib, Some(&typed))
            .unwrap();
        assert_eq!(e.detail.artwork, Some(art));
    }

    #[test]
    fn an_edit_returns_the_detail_and_the_view_as_they_are_after_it() {
        let (f, _) = fx_with_build(None);
        let lib = vec![game(GAME, "Made-up Game"), game("alpha", "Alpha")];
        let e = f
            .svc
            .catalogue_set_categories(&lib[0], &lib, &names(&["Test bed"]))
            .unwrap();
        assert_eq!(e.detail.categories, names(&["Test bed"]));
        assert_eq!(e.detail.builds.len(), 1);
        assert_eq!(e.view.categories[0].game_ids, vec![GAME.to_owned()]);
        assert_eq!(e.detail, f.svc.game_detail(&lib[0]).unwrap());
        assert_eq!(e.view, f.svc.catalogue_view(&lib).unwrap());
        // Emptying the list removes the game's entry.
        let none = f.svc.catalogue_set_categories(&lib[0], &lib, &[]).unwrap();
        assert!(none.view.categories.is_empty() && none.view.entries.is_empty());
    }
}
