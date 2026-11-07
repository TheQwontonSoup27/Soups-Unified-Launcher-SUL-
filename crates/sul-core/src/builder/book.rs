//! The saved recipes (spec 2.2): a list of [`Recipe`]s kept in one file, so a recipe can be written once and run again.
//!
//! * **A saved recipe is only a description.** Saving one reads no mod and touches no game file; it writes the list
//!   file, atomically, and nothing else. A failed save leaves the list as it was in memory and on disk.
//! * **A recipe is checked on its own before it is kept** ([`Recipe::validate`]): names, steps and patches must be
//!   well formed. Whether the mods it names exist, and whether its patches fit their files, is checked when the
//!   recipe is planned, not when it is saved, because a mod can be imported later.
//! * **Replacing needs the version you looked at.** A recipe is replaced only if the caller names the SHA-256 of the
//!   recipe it started from, so a stale window cannot silently overwrite a newer edit; a new recipe cannot take an
//!   id that is in use.
//! * **There is no remove**, on purpose, as for profiles: every run keeps its own copy of the recipe it came from, but
//!   a list that only grows is easier to explain than one that quietly drops what a person may still be looking at.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::recipe::Recipe;
use crate::error::io;
use crate::fsops::{self, ProtectedRoots};
use crate::{Error, Result};

pub const BOOK_SCHEMA: u32 = 1;
/// Most recipes the list holds. Chosen here to keep the file and the page bounded, not a measured figure.
pub const MAX_RECIPES: usize = 200;

#[derive(Debug, Serialize, Deserialize)]
struct BookData {
    schema: u32,
    recipes: Vec<Recipe>,
}

pub struct RecipeBook {
    file: PathBuf,
    data: BookData,
}

impl RecipeBook {
    /// Load the list. A missing file is no recipes; a corrupt or newer one is an error, never a reset.
    pub fn open(file: &Path) -> Result<Self> {
        let data = match std::fs::read_to_string(file) {
            Ok(text) => {
                let d: BookData = serde_json::from_str(&text).map_err(|e| Error::json(file, e))?;
                if d.schema > BOOK_SCHEMA {
                    return Err(Error::Invalid(format!(
                        "recipe list schema {} is newer than supported {BOOK_SCHEMA}",
                        d.schema
                    )));
                }
                d
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BookData {
                schema: BOOK_SCHEMA,
                recipes: vec![],
            },
            Err(e) => return Err(io(file)(e)),
        };
        Ok(Self {
            file: file.to_path_buf(),
            data,
        })
    }

    pub fn list(&self) -> &[Recipe] {
        &self.data.recipes
    }

    pub fn get(&self, id: &str) -> Option<&Recipe> {
        self.data.recipes.iter().find(|r| r.id == id)
    }

    fn write(&self, protected: &ProtectedRoots) -> Result<()> {
        let body = serde_json::to_string_pretty(&self.data)?;
        fsops::atomic_write(protected, &self.file, body.as_bytes())
    }

    /// Keep `recipe`. With `replaces` empty it is a new recipe and its id must be unused; with `replaces` set to the
    /// SHA-256 of the recipe the caller started from, it replaces the saved recipe with that id, which must still be
    /// exactly that one.
    pub fn save(
        &mut self,
        protected: &ProtectedRoots,
        recipe: &Recipe,
        replaces: Option<&str>,
    ) -> Result<()> {
        recipe.validate()?;
        let before = self.data.recipes.clone();
        let result = (|| {
            let existing = self.data.recipes.iter().position(|r| r.id == recipe.id);
            match (existing, replaces) {
                (None, None) => {
                    if self.data.recipes.len() >= MAX_RECIPES {
                        return Err(Error::Invalid(format!(
                            "the list holds at most {MAX_RECIPES} recipes"
                        )));
                    }
                    self.data.recipes.push(recipe.clone());
                }
                (Some(_), None) => {
                    return Err(Error::Invalid(format!(
                        "a recipe {:?} already exists; open it to change it, or pick another id",
                        recipe.id
                    )));
                }
                (None, Some(_)) => {
                    return Err(Error::Invalid(format!(
                        "there is no saved recipe {:?} to replace",
                        recipe.id
                    )));
                }
                (Some(i), Some(hash)) => {
                    if self.data.recipes[i].sha256()? != hash {
                        return Err(Error::Invalid(format!(
                            "recipe {:?} changed since it was opened; open it again and redo the change",
                            recipe.id
                        )));
                    }
                    self.data.recipes[i] = recipe.clone();
                }
            }
            self.write(protected)
        })();
        if result.is_err() {
            self.data.recipes = before;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::recipe::{RECIPE_SCHEMA, Step};

    fn recipe(id: &str, name: &str) -> Recipe {
        Recipe {
            schema: RECIPE_SCHEMA,
            id: id.into(),
            name: name.into(),
            game_id: "madeupgame".into(),
            description: String::new(),
            steps: vec![Step::LayerMod {
                mod_id: "louder".into(),
                version: "1.0".into(),
                into: String::new(),
            }],
        }
    }

    fn open(dir: &Path) -> RecipeBook {
        RecipeBook::open(&dir.join("recipes.json")).unwrap()
    }

    #[test]
    fn a_missing_file_is_an_empty_list_and_a_saved_recipe_survives_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let p = ProtectedRoots::new();
        let mut b = open(dir.path());
        assert!(b.list().is_empty());
        b.save(&p, &recipe("one", "One"), None).unwrap();
        let again = open(dir.path());
        assert_eq!(again.list(), &[recipe("one", "One")]);
        assert_eq!(again.get("one").unwrap().name, "One");
        assert!(again.get("two").is_none());
    }

    #[test]
    fn a_new_recipe_cannot_take_an_id_that_is_in_use_and_nothing_changes() {
        let dir = tempfile::tempdir().unwrap();
        let p = ProtectedRoots::new();
        let mut b = open(dir.path());
        b.save(&p, &recipe("one", "One"), None).unwrap();
        let e = b
            .save(&p, &recipe("one", "Other"), None)
            .unwrap_err()
            .to_string();
        assert!(e.contains("already exists"), "{e}");
        assert_eq!(open(dir.path()).get("one").unwrap().name, "One");
    }

    #[test]
    fn a_recipe_is_replaced_only_from_the_exact_version_the_caller_opened() {
        let dir = tempfile::tempdir().unwrap();
        let p = ProtectedRoots::new();
        let mut b = open(dir.path());
        let first = recipe("one", "One");
        b.save(&p, &first, None).unwrap();
        let hash = first.sha256().unwrap();

        b.save(&p, &recipe("one", "Renamed"), Some(&hash)).unwrap();
        assert_eq!(open(dir.path()).get("one").unwrap().name, "Renamed");

        // The same hash again is now stale: the saved recipe is no longer the one it describes.
        let e = b
            .save(&p, &recipe("one", "Third"), Some(&hash))
            .unwrap_err()
            .to_string();
        assert!(e.contains("changed since it was opened"), "{e}");
        assert_eq!(open(dir.path()).get("one").unwrap().name, "Renamed");

        let e = b
            .save(&p, &recipe("gone", "Gone"), Some(&hash))
            .unwrap_err()
            .to_string();
        assert!(e.contains("no saved recipe"), "{e}");
        assert_eq!(b.list().len(), 1);
    }

    #[test]
    fn an_invalid_recipe_is_refused_before_anything_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let p = ProtectedRoots::new();
        let mut b = open(dir.path());
        let mut bad = recipe("one", "One");
        bad.steps.clear();
        assert!(b.save(&p, &bad, None).is_err());
        assert!(
            !dir.path().join("recipes.json").exists(),
            "no file for a refused save"
        );
        let mut bad_id = recipe("Not An Id!", "One");
        bad_id.id = "Not An Id!".into();
        assert!(b.save(&p, &bad_id, None).is_err());
        assert!(b.list().is_empty());
    }

    #[test]
    fn a_failed_write_leaves_the_list_as_it_was() {
        let dir = tempfile::tempdir().unwrap();
        // The list file's name is taken by a folder, so the atomic write cannot replace it.
        std::fs::create_dir(dir.path().join("recipes.json")).unwrap();
        let p = ProtectedRoots::new();
        let mut b = RecipeBook {
            file: dir.path().join("recipes.json"),
            data: BookData {
                schema: BOOK_SCHEMA,
                recipes: vec![],
            },
        };
        assert!(b.save(&p, &recipe("one", "One"), None).is_err());
        assert!(b.list().is_empty(), "the in-memory list is rolled back too");
    }

    #[test]
    fn a_corrupt_or_newer_file_is_an_error_not_a_reset() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("recipes.json");
        std::fs::write(&file, "not json").unwrap();
        assert!(RecipeBook::open(&file).is_err());
        std::fs::write(&file, r#"{"schema":99,"recipes":[]}"#).unwrap();
        let e = RecipeBook::open(&file).err().unwrap().to_string();
        assert!(e.contains("newer than supported"), "{e}");
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            r#"{"schema":99,"recipes":[]}"#
        );
    }

    #[test]
    fn the_list_is_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let p = ProtectedRoots::new();
        let mut b = open(dir.path());
        for i in 0..MAX_RECIPES {
            b.data.recipes.push(recipe(&format!("r{i}"), "R"));
        }
        let e = b
            .save(&p, &recipe("more", "More"), None)
            .unwrap_err()
            .to_string();
        assert!(e.contains("at most"), "{e}");
    }
}
