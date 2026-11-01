//! The library catalogue (spec 1.1): what the owner sees and organises around each game, and the
//! detail page that gathers everything SUL knows about one game.
//!
//! Three things live here:
//!
//! * **Categories and artwork** the owner chooses for each game, saved in one small file. Artwork is a
//!   *reference* to an image file the owner already has: SUL checks it is a normal image file of a
//!   sensible size and remembers where it is. It never copies it, bundles it or fetches it, because the
//!   spec asks that artwork be added only after its provenance and licence are checked.
//! * **Search** over game names, ids and categories. Every word of the query must match.
//! * **The detail view** ([`detail`]): the game's builds and which one is active in each slot, its
//!   adapter and the weakest capability label (so the headline never says more than a row does), and
//!   whether it is running. A game with no adapter says so instead of showing nothing.
//!
//! The file is only ever replaced atomically, and a file that cannot be read is an error, never
//! overwritten.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::adapters::AdapterRegistry;
use crate::builds::{BuildRegistry, Location};
use crate::capability::Status;
use crate::fsops::{self, ProtectedRoots};
use crate::manifest::State;
use crate::runtime::LaunchState;
use crate::storage::validate_game_id;
use crate::{Error, Result, error::io};

pub const CATALOGUE_SCHEMA: u32 = 1;
pub const MAX_CATEGORIES_PER_GAME: usize = 20;
pub const MAX_CATEGORY_LEN: usize = 40;
/// The largest artwork file accepted: 20 MiB.
pub const MAX_ARTWORK_BYTES: u64 = 20 * 1024 * 1024;
const ARTWORK_EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub game_id: String,
    pub categories: Vec<String>,
    /// Where the owner's image file is. A reference only.
    pub artwork: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Data {
    schema: u32,
    games: Vec<Entry>,
}

#[derive(Debug)]
pub struct Catalogue {
    file: PathBuf,
    data: Data,
}

fn clean_category(raw: &str) -> Result<String> {
    let c = raw.trim();
    if c.is_empty() || c.chars().count() > MAX_CATEGORY_LEN || c.chars().any(char::is_control) {
        return Err(Error::Invalid(format!(
            "a category must be 1 to {MAX_CATEGORY_LEN} characters with no control characters: {raw:?}"
        )));
    }
    Ok(c.to_owned())
}

impl Catalogue {
    /// Open the catalogue file. A missing file is an empty catalogue; a file that cannot be read, or that
    /// a newer SUL wrote, is an error and is left untouched.
    pub fn open(file: &Path) -> Result<Self> {
        let data = match std::fs::read_to_string(file) {
            Ok(text) => {
                let d: Data = serde_json::from_str(&text).map_err(|e| Error::Json {
                    path: file.to_path_buf(),
                    source: e,
                })?;
                if d.schema == 0 || d.schema > CATALOGUE_SCHEMA {
                    return Err(Error::Invalid(format!(
                        "catalogue file schema {} is not one this SUL understands (it knows 1 to {CATALOGUE_SCHEMA})",
                        d.schema
                    )));
                }
                d
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Data {
                schema: CATALOGUE_SCHEMA,
                games: vec![],
            },
            Err(e) => return Err(Error::io(file, e)),
        };
        Ok(Self {
            file: file.to_path_buf(),
            data,
        })
    }

    pub fn entry(&self, game_id: &str) -> Option<&Entry> {
        self.data.games.iter().find(|e| e.game_id == game_id)
    }

    pub fn categories_of(&self, game_id: &str) -> &[String] {
        self.entry(game_id).map_or(&[], |e| e.categories.as_slice())
    }

    pub fn artwork_of(&self, game_id: &str) -> Option<&Path> {
        self.entry(game_id).and_then(|e| e.artwork.as_deref())
    }

    /// Change one game's entry on a copy, save it, and only then make it the live state.
    fn change(
        &mut self,
        protected: &ProtectedRoots,
        game_id: &str,
        f: impl FnOnce(&mut Entry) -> Result<()>,
    ) -> Result<()> {
        validate_game_id(game_id)?;
        let mut next = self.data.clone();
        let at = match next.games.iter().position(|e| e.game_id == game_id) {
            Some(i) => i,
            None => {
                next.games.push(Entry {
                    game_id: game_id.to_owned(),
                    categories: vec![],
                    artwork: None,
                });
                next.games.len() - 1
            }
        };
        f(&mut next.games[at])?;
        let e = &next.games[at];
        if e.categories.is_empty() && e.artwork.is_none() {
            next.games.remove(at);
        }
        fsops::atomic_write(
            protected,
            &self.file,
            serde_json::to_string_pretty(&next)?.as_bytes(),
        )?;
        self.data = next;
        Ok(())
    }

    /// Replace a game's categories. Names are trimmed; a repeat (ignoring case) is dropped, keeping the
    /// first spelling.
    pub fn set_categories(
        &mut self,
        protected: &ProtectedRoots,
        game_id: &str,
        categories: &[String],
    ) -> Result<()> {
        let mut clean: Vec<String> = Vec::new();
        for c in categories {
            let c = clean_category(c)?;
            if !clean.iter().any(|x| x.eq_ignore_ascii_case(&c)) {
                clean.push(c);
            }
        }
        if clean.len() > MAX_CATEGORIES_PER_GAME {
            return Err(Error::Invalid(format!(
                "at most {MAX_CATEGORIES_PER_GAME} categories per game"
            )));
        }
        self.change(protected, game_id, |e| {
            e.categories = clean;
            Ok(())
        })
    }

    /// Point a game at an image file, or clear it with `None`. The file is checked and referenced, never copied.
    pub fn set_artwork(
        &mut self,
        protected: &ProtectedRoots,
        game_id: &str,
        image: Option<&Path>,
    ) -> Result<()> {
        let stored = match image {
            None => None,
            Some(p) => {
                let canon = p.canonicalize().map_err(io(p))?;
                let meta = std::fs::metadata(&canon).map_err(io(&canon))?;
                let ext_ok = canon
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| ARTWORK_EXTENSIONS.iter().any(|a| a.eq_ignore_ascii_case(e)));
                if !meta.is_file() || !ext_ok {
                    return Err(Error::Invalid(format!(
                        "artwork must be a .png, .jpg, .jpeg or .webp file: {}",
                        p.display()
                    )));
                }
                if meta.len() == 0 || meta.len() > MAX_ARTWORK_BYTES {
                    return Err(Error::Invalid(format!(
                        "artwork must be between 1 byte and {MAX_ARTWORK_BYTES} bytes: {}",
                        p.display()
                    )));
                }
                Some(canon)
            }
        };
        self.change(protected, game_id, |e| {
            e.artwork = stored;
            Ok(())
        })
    }

    /// Every category in use with how many games carry it, sorted by name (ignoring case).
    pub fn all_categories(&self) -> Vec<(String, usize)> {
        let mut out: Vec<(String, usize)> = Vec::new();
        for e in &self.data.games {
            for c in &e.categories {
                match out.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(c)) {
                    Some((_, k)) => *k += 1,
                    None => out.push((c.clone(), 1)),
                }
            }
        }
        out.sort_by_key(|(n, _)| n.to_ascii_lowercase());
        out
    }

    /// The games (by id, in the order given) that carry `category`, ignoring case.
    pub fn in_category(&self, category: &str, games: &[(String, String)]) -> Vec<String> {
        games
            .iter()
            .filter(|(id, _)| {
                self.categories_of(id)
                    .iter()
                    .any(|c| c.eq_ignore_ascii_case(category.trim()))
            })
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Games whose name, id or category matches every word of `query` (case-insensitive substring). An
    /// empty query matches every game. `games` is `(id, display name)`; the result keeps its order.
    pub fn search(&self, query: &str, games: &[(String, String)]) -> Vec<String> {
        let words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();
        games
            .iter()
            .filter(|(id, name)| {
                let hay: Vec<String> = std::iter::once(name.to_lowercase())
                    .chain(std::iter::once(id.to_lowercase()))
                    .chain(self.categories_of(id).iter().map(|c| c.to_lowercase()))
                    .collect();
                words.iter().all(|w| hay.iter().any(|h| h.contains(w)))
            })
            .map(|(id, _)| id.clone())
            .collect()
    }
}

/// One build as the detail page lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuildSummary {
    pub build_id: String,
    pub slot: String,
    pub state: State,
    /// True for a copy SUL manages, false for an install that stays where it is.
    pub managed: bool,
    /// Whether it is the active build of its slot.
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdapterInfo {
    pub id: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GameDetail {
    pub game_id: String,
    pub display_name: String,
    pub categories: Vec<String>,
    pub artwork: Option<PathBuf>,
    pub builds: Vec<BuildSummary>,
    pub adapter: Option<AdapterInfo>,
    /// The weakest capability label across the game's adapter entries; Not investigated with no adapter.
    pub capability: Status,
    pub capability_label: &'static str,
    pub launch: LaunchState,
    /// Plain statements about what is missing, so an empty section is explained.
    pub notes: Vec<String>,
}

/// Gather what SUL knows about one game. Reads only.
pub fn detail(
    game_id: &str,
    display_name: &str,
    catalogue: &Catalogue,
    registry: &BuildRegistry,
    adapters: &AdapterRegistry,
    launch: LaunchState,
) -> GameDetail {
    let mut builds: Vec<BuildSummary> = registry
        .builds()
        .iter()
        .filter(|b| b.game_id == game_id)
        .map(|b| BuildSummary {
            build_id: b.id.clone(),
            slot: b.slot.clone(),
            state: b.state.clone(),
            managed: matches!(b.location, Location::Managed { .. }),
            active: registry
                .active(game_id, &b.slot)
                .is_some_and(|a| a.build_id == b.id),
        })
        .collect();
    builds.sort_by(|a, b| (&a.slot, &a.build_id).cmp(&(&b.slot, &b.build_id)));

    let capability = adapters.status_of(game_id);
    let adapter = adapters.get(game_id).map(|a| AdapterInfo {
        id: a.id().to_owned(),
        version: a.version().to_owned(),
    });
    let mut notes = Vec::new();
    if adapter.is_none() {
        notes.push("No adapter is registered for this game, so launching and mod support are not available and nothing about it has been tested.".to_owned());
    }
    if builds.is_empty() {
        notes.push(
            "SUL has no build of this game yet. Import or register an install to manage it."
                .to_owned(),
        );
    } else if !builds.iter().any(|b| b.active) {
        notes.push("None of this game's builds is active in its slot.".to_owned());
    }
    GameDetail {
        game_id: game_id.to_owned(),
        display_name: display_name.to_owned(),
        categories: catalogue.categories_of(game_id).to_vec(),
        artwork: catalogue.artwork_of(game_id).map(Path::to_path_buf),
        builds,
        adapter,
        capability,
        capability_label: capability.label(),
        launch,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fixtures::MadeUpGame;
    use crate::manifest::Variant;
    use crate::testkit::{GAME, World, put};

    fn games() -> Vec<(String, String)> {
        vec![
            ("terraria".into(), "Terraria".into()),
            ("rimworld".into(), "RimWorld".into()),
            ("stardewvalley".into(), "Stardew Valley".into()),
        ]
    }

    fn open(w: &World) -> Catalogue {
        Catalogue::open(&w.base.join("state/catalogue.json")).unwrap()
    }

    #[test]
    fn categories_are_trimmed_deduplicated_and_saved() {
        let w = World::new();
        let mut c = open(&w);
        c.set_categories(
            &w.protected,
            "terraria",
            &[" Sandbox ".into(), "sandbox".into(), "Co-op".into()],
        )
        .unwrap();
        assert_eq!(c.categories_of("terraria"), ["Sandbox", "Co-op"]);
        let again = open(&w);
        assert_eq!(again.categories_of("terraria"), ["Sandbox", "Co-op"]);
        assert!(again.categories_of("rimworld").is_empty());
    }

    #[test]
    fn bad_categories_and_game_ids_are_refused_and_change_nothing() {
        let w = World::new();
        let mut c = open(&w);
        c.set_categories(&w.protected, "terraria", &["A".into()])
            .unwrap();
        for bad in ["", "   ", "a\nb", &"x".repeat(MAX_CATEGORY_LEN + 1)] {
            assert!(
                c.set_categories(&w.protected, "terraria", &[bad.into()])
                    .is_err(),
                "{bad:?}"
            );
        }
        let too_many: Vec<String> = (0..=MAX_CATEGORIES_PER_GAME)
            .map(|i| format!("c{i}"))
            .collect();
        assert!(
            c.set_categories(&w.protected, "terraria", &too_many)
                .is_err()
        );
        let exactly: Vec<String> = (0..MAX_CATEGORIES_PER_GAME)
            .map(|i| format!("c{i}"))
            .collect();
        assert!(c.set_categories(&w.protected, "terraria", &exactly).is_ok());
        assert!(
            c.set_categories(&w.protected, "../x", &["A".into()])
                .is_err()
        );
        assert!(c.set_categories(&w.protected, "", &["A".into()]).is_err());
        // an exactly-long name is allowed
        assert!(
            c.set_categories(&w.protected, "terraria", &["x".repeat(MAX_CATEGORY_LEN)])
                .is_ok()
        );
    }

    #[test]
    fn clearing_everything_removes_the_entry() {
        let w = World::new();
        let mut c = open(&w);
        c.set_categories(&w.protected, "terraria", &["A".into()])
            .unwrap();
        c.set_categories(&w.protected, "terraria", &[]).unwrap();
        assert!(c.entry("terraria").is_none());
    }

    #[test]
    fn artwork_is_a_reference_to_a_checked_image_and_is_never_copied() {
        let w = World::new();
        let mut c = open(&w);
        let img = w.base.join("pics/cover.PNG");
        put(&img, "not really a picture but a file");
        c.set_artwork(&w.protected, "terraria", Some(&img)).unwrap();
        assert_eq!(
            c.artwork_of("terraria").unwrap(),
            img.canonicalize().unwrap()
        );
        // nothing was copied next to the catalogue
        let state: Vec<_> = std::fs::read_dir(w.base.join("state"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(!state.iter().any(|n| n.contains("cover")), "{state:?}");
        c.set_artwork(&w.protected, "terraria", None).unwrap();
        assert!(c.artwork_of("terraria").is_none());
    }

    #[test]
    fn artwork_that_is_not_a_plain_image_file_of_sensible_size_is_refused() {
        let w = World::new();
        let mut c = open(&w);
        put(&w.base.join("pics/a.txt"), "x");
        put(&w.base.join("pics/empty.png"), "");
        std::fs::create_dir_all(w.base.join("pics/dir.png")).unwrap();
        for bad in [
            "pics/a.txt",
            "pics/empty.png",
            "pics/dir.png",
            "pics/missing.png",
        ] {
            assert!(
                c.set_artwork(&w.protected, "terraria", Some(&w.base.join(bad)))
                    .is_err(),
                "{bad}"
            );
        }
        assert!(c.entry("terraria").is_none());
    }

    #[test]
    fn search_needs_every_word_and_looks_at_names_ids_and_categories() {
        let w = World::new();
        let mut c = open(&w);
        c.set_categories(
            &w.protected,
            "terraria",
            &["Sandbox".into(), "Co-op".into()],
        )
        .unwrap();
        c.set_categories(&w.protected, "rimworld", &["Colony sim".into()])
            .unwrap();
        let g = games();
        assert_eq!(c.search("", &g).len(), 3);
        assert_eq!(c.search("terr", &g), ["terraria"]);
        assert_eq!(c.search("VALLEY", &g), ["stardewvalley"]);
        assert_eq!(c.search("sandbox co-op", &g), ["terraria"]);
        assert!(c.search("sandbox colony", &g).is_empty());
        assert_eq!(c.search("colony rim", &g), ["rimworld"]);
        assert!(c.search("zzz", &g).is_empty());
        // ids match as well as names
        assert_eq!(c.search("stardewvalley", &g), ["stardewvalley"]);
    }

    #[test]
    fn categories_can_be_listed_with_counts_and_used_as_a_filter() {
        let w = World::new();
        let mut c = open(&w);
        c.set_categories(
            &w.protected,
            "terraria",
            &["Sandbox".into(), "Co-op".into()],
        )
        .unwrap();
        c.set_categories(&w.protected, "rimworld", &["sandbox".into()])
            .unwrap();
        assert_eq!(
            c.all_categories(),
            [("Co-op".to_owned(), 1), ("Sandbox".to_owned(), 2)]
        );
        assert_eq!(c.in_category("SANDBOX", &games()), ["terraria", "rimworld"]);
        assert!(c.in_category("none", &games()).is_empty());
    }

    #[test]
    fn a_file_that_cannot_be_read_or_is_from_a_newer_sul_is_an_error_and_is_left_alone() {
        let w = World::new();
        let f = w.base.join("state/catalogue.json");
        put(&f, "{ not json");
        assert!(Catalogue::open(&f).is_err());
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "{ not json");
        put(&f, r#"{"schema": 2, "games": []}"#);
        assert!(Catalogue::open(&f).is_err());
        put(&f, r#"{"schema": 0, "games": []}"#);
        assert!(Catalogue::open(&f).is_err());
        put(&f, r#"{"schema": 1, "games": []}"#);
        assert!(Catalogue::open(&f).is_ok());
    }

    #[test]
    fn a_failed_save_leaves_memory_as_it_was() {
        let w = World::new();
        let mut c = open(&w);
        c.set_categories(&w.protected, "terraria", &["A".into()])
            .unwrap();
        // make the folder the file lives in unusable: protect it
        let mut locked = ProtectedRoots::new();
        locked.add(&w.base.join("state")).unwrap();
        assert!(
            c.set_categories(&locked, "terraria", &["B".into()])
                .is_err()
        );
        assert_eq!(c.categories_of("terraria"), ["A"]);
    }

    #[test]
    fn the_detail_page_lists_builds_the_active_one_the_adapter_and_the_weakest_label() {
        let w = World::new();
        let mut reg = w.registry();
        let id = w.import(&mut reg, Variant::Original);
        let mut adapters = AdapterRegistry::new();
        adapters.register(Box::new(MadeUpGame::new())).unwrap();
        let mut c = open(&w);
        c.set_categories(&w.protected, GAME, &["Test".into()])
            .unwrap();

        let d = detail(
            GAME,
            "Made-up Game",
            &c,
            &reg,
            &adapters,
            LaunchState::NotRunning,
        );
        assert_eq!(d.builds.len(), 1);
        assert_eq!(d.builds[0].build_id, id);
        assert!(d.builds[0].managed);
        assert_eq!(d.builds[0].state, State::Ready);
        assert_eq!(d.categories, ["Test"]);
        assert_eq!(d.adapter.as_ref().unwrap().id, GAME);
        assert_eq!(d.capability, Status::Experimental);
        assert_eq!(d.capability_label, "Experimental");
        assert!(d.notes.iter().all(|n| !n.contains("No adapter")));
    }

    #[test]
    fn a_game_with_no_adapter_or_no_build_says_so_and_is_not_investigated() {
        let w = World::new();
        let reg = w.registry();
        let c = open(&w);
        let d = detail(
            "terraria",
            "Terraria",
            &c,
            &reg,
            &AdapterRegistry::new(),
            LaunchState::NotRunning,
        );
        assert_eq!(d.capability, Status::NotInvestigated);
        assert!(d.adapter.is_none() && d.builds.is_empty());
        assert!(d.notes.iter().any(|n| n.contains("No adapter")));
        assert!(d.notes.iter().any(|n| n.contains("no build")));
    }

    #[test]
    fn the_detail_page_only_lists_builds_of_that_game() {
        let w = World::new();
        let mut reg = w.registry();
        w.import(&mut reg, Variant::Original);
        let c = open(&w);
        let d = detail(
            "terraria",
            "Terraria",
            &c,
            &reg,
            &AdapterRegistry::new(),
            LaunchState::NotRunning,
        );
        assert!(d.builds.is_empty());
    }
}
