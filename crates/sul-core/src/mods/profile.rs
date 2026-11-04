//! Mod profiles (spec 2.1): a named, ordered list of mods for one game and one managed build.
//!
//! Load order is the order of the list: a later mod wins when two enabled mods provide the same file, and a
//! mod's requirements must come earlier in the list. A profile pins an exact version of each mod it uses.
//!
//! Editing a profile only edits the profile file, saved atomically; a failed save leaves the profile as it was in
//! memory and on disk. Nothing here touches a game, and nothing here deletes a profile: there is no remove
//! operation, on purpose, because a profile may be the record of what is deployed.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::io;
use crate::fsops::{self, ProtectedRoots};
use crate::manifest::now_unix;
use crate::{Error, Result};

use super::store::ModStore;
use super::{
    MAX_LIST_LEN, MAX_TEXT_LEN, compare_versions, validate_name, validate_note, validate_text,
};

pub const PROFILES_SCHEMA: u32 = 1;
pub const MAX_CONFIG_ENTRIES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileMod {
    pub mod_id: String,
    pub version: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub game_id: String,
    /// The managed build this profile deploys into.
    pub build_id: String,
    /// In load order, first loaded first.
    pub mods: Vec<ProfileMod>,
    /// Settings the owner keeps with the profile (spec 2.1 "per-profile config"). Stored and shown only: nothing
    /// passes them to a game yet, and values are never written to a log.
    pub config: BTreeMap<String, String>,
    pub notes: String,
    pub created_unix: u64,
    pub updated_unix: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProfilesData {
    schema: u32,
    profiles: Vec<Profile>,
}

pub struct Profiles {
    file: PathBuf,
    data: ProfilesData,
}

impl Profiles {
    /// Load the profiles. A missing file is no profiles; a corrupt or newer one is an error, never a reset.
    pub fn open(file: &Path) -> Result<Self> {
        let data = match std::fs::read_to_string(file) {
            Ok(text) => {
                let d: ProfilesData =
                    serde_json::from_str(&text).map_err(|e| Error::json(file, e))?;
                if d.schema > PROFILES_SCHEMA {
                    return Err(Error::Invalid(format!(
                        "profiles schema {} is newer than supported {PROFILES_SCHEMA}",
                        d.schema
                    )));
                }
                d
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => ProfilesData {
                schema: PROFILES_SCHEMA,
                profiles: vec![],
            },
            Err(e) => return Err(io(file)(e)),
        };
        Ok(Self {
            file: file.to_path_buf(),
            data,
        })
    }

    pub fn list(&self) -> &[Profile] {
        &self.data.profiles
    }

    pub fn get(&self, id: &str) -> Option<&Profile> {
        self.data.profiles.iter().find(|p| p.id == id)
    }

    fn save(&self, protected: &ProtectedRoots) -> Result<()> {
        let body = serde_json::to_string_pretty(&self.data)?;
        fsops::atomic_write(protected, &self.file, body.as_bytes())
    }

    /// Change one profile and save. Nothing is kept unless the change and the save both succeed.
    fn edit<T>(
        &mut self,
        protected: &ProtectedRoots,
        id: &str,
        change: impl FnOnce(&mut Profile) -> Result<T>,
    ) -> Result<T> {
        let before = self.data.profiles.clone();
        let result = (|| {
            let p = self
                .data
                .profiles
                .iter_mut()
                .find(|p| p.id == id)
                .ok_or_else(|| Error::Invalid(format!("no profile with id {id:?}")))?;
            let v = change(p)?;
            p.updated_unix = now_unix();
            if p.mods.len() > MAX_LIST_LEN {
                return Err(Error::Invalid("too many mods in one profile".into()));
            }
            self.save(protected)?;
            Ok(v)
        })();
        if result.is_err() {
            self.data.profiles = before;
        }
        result
    }

    pub fn create(
        &mut self,
        protected: &ProtectedRoots,
        id: &str,
        name: &str,
        game_id: &str,
        build_id: &str,
    ) -> Result<()> {
        validate_name("profile id", id)?;
        validate_name("game id", game_id)?;
        validate_name("build id", build_id)?;
        validate_text("profile name", name, MAX_TEXT_LEN)?;
        if self.get(id).is_some() {
            return Err(Error::Invalid(format!("a profile {id:?} already exists")));
        }
        let now = now_unix();
        self.data.profiles.push(Profile {
            id: id.into(),
            name: name.trim().into(),
            game_id: game_id.into(),
            build_id: build_id.into(),
            mods: vec![],
            config: BTreeMap::new(),
            notes: String::new(),
            created_unix: now,
            updated_unix: now,
        });
        if let Err(e) = self.save(protected) {
            self.data.profiles.pop();
            return Err(e);
        }
        Ok(())
    }

    pub fn rename(&mut self, protected: &ProtectedRoots, id: &str, name: &str) -> Result<()> {
        validate_text("profile name", name, MAX_TEXT_LEN)?;
        self.edit(protected, id, |p| {
            p.name = name.trim().into();
            Ok(())
        })
    }

    pub fn set_notes(&mut self, protected: &ProtectedRoots, id: &str, notes: &str) -> Result<()> {
        validate_note("notes", notes, 4000)?;
        self.edit(protected, id, |p| {
            p.notes = notes.into();
            Ok(())
        })
    }

    /// Point the profile at another build of the same game. The caller must have purged the old one first;
    /// this only edits the profile.
    pub fn set_build(
        &mut self,
        protected: &ProtectedRoots,
        id: &str,
        build_id: &str,
    ) -> Result<()> {
        validate_name("build id", build_id)?;
        self.edit(protected, id, |p| {
            p.build_id = build_id.into();
            Ok(())
        })
    }

    /// Add a mod at `position` (the end when `None`; a position past the end is the end).
    pub fn add_mod(
        &mut self,
        protected: &ProtectedRoots,
        id: &str,
        mod_id: &str,
        version: &str,
        position: Option<usize>,
    ) -> Result<()> {
        validate_name("mod id", mod_id)?;
        validate_name("version", version)?;
        self.edit(protected, id, |p| {
            if p.mods.iter().any(|m| m.mod_id == mod_id) {
                return Err(Error::Invalid(format!(
                    "{mod_id} is already in this profile; change its version or move it"
                )));
            }
            let at = position.unwrap_or(p.mods.len()).min(p.mods.len());
            p.mods.insert(
                at,
                ProfileMod {
                    mod_id: mod_id.into(),
                    version: version.into(),
                    enabled: true,
                },
            );
            Ok(())
        })
    }

    pub fn remove_mod(&mut self, protected: &ProtectedRoots, id: &str, mod_id: &str) -> Result<()> {
        self.edit(protected, id, |p| {
            let i = position_of(p, mod_id)?;
            p.mods.remove(i);
            Ok(())
        })
    }

    pub fn set_enabled(
        &mut self,
        protected: &ProtectedRoots,
        id: &str,
        mod_id: &str,
        enabled: bool,
    ) -> Result<()> {
        self.edit(protected, id, |p| {
            let i = position_of(p, mod_id)?;
            p.mods[i].enabled = enabled;
            Ok(())
        })
    }

    pub fn set_version(
        &mut self,
        protected: &ProtectedRoots,
        id: &str,
        mod_id: &str,
        version: &str,
    ) -> Result<()> {
        validate_name("version", version)?;
        self.edit(protected, id, |p| {
            let i = position_of(p, mod_id)?;
            p.mods[i].version = version.into();
            Ok(())
        })
    }

    /// Move a mod to `to_index` in the load order (clamped to the end).
    pub fn move_mod(
        &mut self,
        protected: &ProtectedRoots,
        id: &str,
        mod_id: &str,
        to_index: usize,
    ) -> Result<()> {
        self.edit(protected, id, |p| {
            let i = position_of(p, mod_id)?;
            let m = p.mods.remove(i);
            let at = to_index.min(p.mods.len());
            p.mods.insert(at, m);
            Ok(())
        })
    }

    /// Set a setting, or remove it with `None`.
    pub fn set_config(
        &mut self,
        protected: &ProtectedRoots,
        id: &str,
        key: &str,
        value: Option<&str>,
    ) -> Result<()> {
        validate_text("setting name", key, 64)?;
        if let Some(v) = value {
            validate_note("setting value", v, 1000)?;
        }
        self.edit(protected, id, |p| {
            match value {
                Some(v) => {
                    if !p.config.contains_key(key) && p.config.len() >= MAX_CONFIG_ENTRIES {
                        return Err(Error::Invalid("too many settings in one profile".into()));
                    }
                    p.config.insert(key.into(), v.into());
                }
                None => {
                    p.config.remove(key);
                }
            }
            Ok(())
        })
    }
}

fn position_of(p: &Profile, mod_id: &str) -> Result<usize> {
    p.mods
        .iter()
        .position(|m| m.mod_id == mod_id)
        .ok_or_else(|| Error::Invalid(format!("{mod_id} is not in this profile")))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Blocks a deploy.
    Error,
    /// Shown before a deploy; the owner decides.
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProblemKind {
    UnknownMod,
    WrongGame,
    MissingDependency,
    DependencyDisabled,
    DependencyLoadsLater,
    VersionTooOld,
    VersionNotComparable,
    /// The mod lists game versions and the build's is not among them.
    NotListedAsCompatible,
    /// Nobody said which game versions the mod works with. Never read as compatible.
    CompatibilityNotStated,
    // Raised when a deploy is planned.
    BuildNotFound,
    BuildNotManaged,
    BuildIsOriginal,
    BuildNotSound,
    AlreadyDeployed,
    OutsideAllowedFolders,
    StoreFileProblem,
    NothingToDeploy,
    NotEnoughSpace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    pub severity: Severity,
    pub kind: ProblemKind,
    pub mod_id: Option<String>,
    pub path: Option<String>,
    pub message: String,
}

impl Problem {
    pub(crate) fn error(kind: ProblemKind, mod_id: Option<&str>, message: String) -> Self {
        Self {
            severity: Severity::Error,
            kind,
            mod_id: mod_id.map(str::to_owned),
            path: None,
            message,
        }
    }

    pub(crate) fn warning(kind: ProblemKind, mod_id: Option<&str>, message: String) -> Self {
        Self {
            severity: Severity::Warning,
            ..Self::error(kind, mod_id, message)
        }
    }
}

/// Everything wrong or doubtful about a profile, without touching any file. `build_base_version` is the version
/// recorded for the build the profile deploys into.
pub fn check_profile(
    profile: &Profile,
    store: &ModStore,
    build_base_version: &str,
) -> Vec<Problem> {
    let mut out = Vec::new();
    for (i, entry) in profile.mods.iter().enumerate() {
        let id = Some(entry.mod_id.as_str());
        let Some(rec) = store.get(&profile.game_id, &entry.mod_id, &entry.version) else {
            let other_game = store.mods().iter().any(|m| {
                m.manifest.mod_id == entry.mod_id
                    && m.manifest.version == entry.version
                    && m.manifest.game_id != profile.game_id
            });
            let (kind, text) = if other_game {
                (
                    ProblemKind::WrongGame,
                    format!(
                        "{}@{} is in the store for a different game",
                        entry.mod_id, entry.version
                    ),
                )
            } else {
                (
                    ProblemKind::UnknownMod,
                    format!("{}@{} is not in the mod store", entry.mod_id, entry.version),
                )
            };
            // A mod that is switched off and missing is worth a line, but it blocks nothing.
            out.push(if entry.enabled {
                Problem::error(kind, id, text)
            } else {
                Problem::warning(kind, id, text)
            });
            continue;
        };
        if !entry.enabled {
            continue;
        }
        let m = &rec.manifest;
        if m.compatible_versions.is_empty() {
            out.push(Problem::warning(
                ProblemKind::CompatibilityNotStated,
                id,
                format!("nobody has said which game versions {} works with", m.name),
            ));
        } else if !m
            .compatible_versions
            .iter()
            .any(|v| v == build_base_version)
        {
            out.push(Problem::warning(
                ProblemKind::NotListedAsCompatible,
                id,
                format!(
                    "{} lists game versions {:?}, not {build_base_version}",
                    m.name, m.compatible_versions
                ),
            ));
        }
        for req in &m.requires {
            let Some(dep_index) = profile.mods.iter().position(|d| d.mod_id == req.mod_id) else {
                out.push(Problem::error(
                    ProblemKind::MissingDependency,
                    id,
                    format!(
                        "{} needs {}, which is not in this profile",
                        m.name, req.mod_id
                    ),
                ));
                continue;
            };
            let dep = &profile.mods[dep_index];
            if !dep.enabled {
                out.push(Problem::error(
                    ProblemKind::DependencyDisabled,
                    id,
                    format!("{} needs {}, which is switched off", m.name, req.mod_id),
                ));
            }
            if dep_index > i {
                out.push(Problem::error(
                    ProblemKind::DependencyLoadsLater,
                    id,
                    format!(
                        "{} needs {} to load first, but it is later in the load order",
                        m.name, req.mod_id
                    ),
                ));
            }
            if let Some(min) = &req.min_version {
                match compare_versions(&dep.version, min) {
                    Some(Ordering::Less) => out.push(Problem::error(
                        ProblemKind::VersionTooOld,
                        id,
                        format!(
                            "{} needs {} {min} or newer; the profile has {}",
                            m.name, req.mod_id, dep.version
                        ),
                    )),
                    Some(_) => {}
                    None => out.push(Problem::warning(
                        ProblemKind::VersionNotComparable,
                        id,
                        format!(
                            "cannot tell whether {} {} is at least {min}",
                            req.mod_id, dep.version
                        ),
                    )),
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::store::tests::{make_mod, mod_world, req};
    use super::super::store::{ModImport, Requirement};
    use super::*;
    use crate::testkit::{GAME, World};

    fn setup() -> (World, ModStore, Profiles) {
        let w = mod_world();
        let store = ModStore::open(&w.base.join("state/mods.json")).unwrap();
        let mut profiles = Profiles::open(&w.base.join("state/profiles.json")).unwrap();
        profiles
            .create(
                &w.protected,
                "main",
                "Main",
                GAME,
                "madeupgame-experimental-1",
            )
            .unwrap();
        (w, store, profiles)
    }

    fn import(w: &World, store: &mut ModStore, r: ModImport) {
        store.import_folder(&w.ctx(), &r).unwrap();
    }

    #[test]
    fn a_profile_is_edited_and_saved_atomically() {
        let (w, _store, mut p) = setup();
        let pr = &w.protected;
        p.add_mod(pr, "main", "a", "1.0", None).unwrap();
        p.add_mod(pr, "main", "b", "1.0", None).unwrap();
        p.add_mod(pr, "main", "c", "2.0", Some(0)).unwrap();
        let order = |p: &Profiles| -> Vec<String> {
            p.get("main")
                .unwrap()
                .mods
                .iter()
                .map(|m| m.mod_id.clone())
                .collect()
        };
        assert_eq!(order(&p), ["c", "a", "b"]);
        p.move_mod(pr, "main", "c", 99).unwrap();
        assert_eq!(order(&p), ["a", "b", "c"]);
        p.set_enabled(pr, "main", "b", false).unwrap();
        p.set_version(pr, "main", "a", "1.1").unwrap();
        p.set_config(pr, "main", "difficulty", Some("hard"))
            .unwrap();
        p.rename(pr, "main", "Main profile").unwrap();
        p.set_notes(pr, "main", "my notes").unwrap();

        let again = Profiles::open(&w.base.join("state/profiles.json")).unwrap();
        let got = again.get("main").unwrap();
        assert_eq!(got.name, "Main profile");
        assert_eq!(got.mods[0].version, "1.1");
        assert!(!got.mods[1].enabled);
        assert_eq!(got.config["difficulty"], "hard");
        p.set_config(pr, "main", "difficulty", None).unwrap();
        assert!(p.get("main").unwrap().config.is_empty());
        p.remove_mod(pr, "main", "b").unwrap();
        assert_eq!(order(&p), ["a", "c"]);
    }

    #[test]
    fn bad_edits_are_refused_and_change_nothing() {
        let (w, _s, mut p) = setup();
        let pr = &w.protected;
        p.add_mod(pr, "main", "a", "1.0", None).unwrap();
        let before = std::fs::read_to_string(w.base.join("state/profiles.json")).unwrap();
        assert!(p.add_mod(pr, "main", "a", "1.0", None).is_err(), "twice");
        assert!(p.add_mod(pr, "main", "../x", "1.0", None).is_err());
        assert!(
            p.add_mod(pr, "nope", "z", "1.0", None).is_err(),
            "unknown profile"
        );
        assert!(p.remove_mod(pr, "main", "zzz").is_err());
        assert!(p.set_enabled(pr, "main", "zzz", true).is_err());
        assert!(p.set_config(pr, "main", "k", Some("bad\u{0}")).is_err());
        assert!(
            p.create(pr, "main", "Again", GAME, "b").is_err(),
            "duplicate id"
        );
        assert!(p.create(pr, "x/y", "Bad", GAME, "b").is_err());
        assert_eq!(
            std::fs::read_to_string(w.base.join("state/profiles.json")).unwrap(),
            before
        );
        assert_eq!(p.list().len(), 1);
    }

    #[test]
    fn a_failed_save_leaves_the_profile_as_it_was() {
        let (w, _s, mut p) = setup();
        let pr = &w.protected;
        p.add_mod(pr, "main", "a", "1.0", None).unwrap();
        // Make the save fail: the profiles file's folder is replaced by a plain file.
        std::fs::remove_dir_all(w.base.join("state")).unwrap();
        std::fs::write(w.base.join("state"), "in the way").unwrap();
        assert!(p.add_mod(pr, "main", "b", "1.0", None).is_err());
        assert_eq!(p.get("main").unwrap().mods.len(), 1);
        assert!(p.create(pr, "second", "Second", GAME, "b").is_err());
        assert_eq!(p.list().len(), 1);
    }

    #[test]
    fn a_corrupt_or_newer_profiles_file_is_an_error_not_an_empty_list() {
        let (w, _s, _p) = setup();
        let f = w.base.join("state/profiles.json");
        std::fs::write(&f, "garbage").unwrap();
        assert!(Profiles::open(&f).is_err());
        std::fs::write(&f, r#"{"schema": 9, "profiles": []}"#).unwrap();
        assert!(Profiles::open(&f).is_err());
    }

    #[test]
    fn the_checks_say_what_blocks_a_deploy_and_what_only_warns() {
        let (w, mut store, mut p) = setup();
        let pr = &w.protected;
        let mut lib = req(&w, "lib", "1.2", &[("data/lib.bin", "L")]);
        lib.compatible_versions = vec!["1.0.0".into()];
        import(&w, &mut store, lib);
        let mut ui = req(&w, "ui", "1.0", &[("data/ui.bin", "U")]);
        ui.requires = vec![Requirement {
            mod_id: "lib".into(),
            min_version: Some("1.5".into()),
        }];
        ui.compatible_versions = vec![];
        import(&w, &mut store, ui);
        fn kinds(p: &Profiles, store: &ModStore) -> Vec<(ProblemKind, Severity)> {
            check_profile(p.get("main").unwrap(), store, "1.0.0")
                .into_iter()
                .map(|x| (x.kind, x.severity))
                .collect()
        }

        // An empty profile has nothing wrong with it.
        assert!(kinds(&p, &store).is_empty());

        // The dependency is missing: an error, plus "not stated" for the mod that has no compatibility list.
        p.add_mod(pr, "main", "ui", "1.0", None).unwrap();
        let k = kinds(&p, &store);
        assert!(k.contains(&(ProblemKind::MissingDependency, Severity::Error)));
        assert!(k.contains(&(ProblemKind::CompatibilityNotStated, Severity::Warning)));

        // Present but loaded after the mod that needs it, and too old.
        p.add_mod(pr, "main", "lib", "1.2", None).unwrap();
        let k = kinds(&p, &store);
        assert!(k.contains(&(ProblemKind::DependencyLoadsLater, Severity::Error)));
        assert!(k.contains(&(ProblemKind::VersionTooOld, Severity::Error)));
        assert!(!k.contains(&(ProblemKind::MissingDependency, Severity::Error)));

        // Fixing the order does not fix the version; switching the dependency off adds another error.
        p.move_mod(pr, "main", "lib", 0).unwrap();
        let k = kinds(&p, &store);
        assert!(!k.contains(&(ProblemKind::DependencyLoadsLater, Severity::Error)));
        assert!(k.contains(&(ProblemKind::VersionTooOld, Severity::Error)));
        p.set_enabled(pr, "main", "lib", false).unwrap();
        assert!(kinds(&p, &store).contains(&(ProblemKind::DependencyDisabled, Severity::Error)));

        // A newer lib satisfies it. A game version that is not listed is a warning, not an error.
        let mut lib2 = req(&w, "lib", "1.6", &[("data/lib.bin", "L2")]);
        lib2.compatible_versions = vec!["2.0.0".into()];
        import(&w, &mut store, lib2);
        p.set_enabled(pr, "main", "lib", true).unwrap();
        p.set_version(pr, "main", "lib", "1.6").unwrap();
        let k = kinds(&p, &store);
        assert!(!k.iter().any(|(_, s)| *s == Severity::Error), "{k:?}");
        assert!(k.contains(&(ProblemKind::NotListedAsCompatible, Severity::Warning)));

        // A version that cannot be read is said so, not guessed.
        p.set_version(pr, "main", "lib", "1.6").unwrap();
        let mut ui2 = req(&w, "ui", "2.0", &[("data/ui.bin", "U2")]);
        ui2.requires = vec![Requirement {
            mod_id: "lib".into(),
            min_version: Some("1.5".into()),
        }];
        import(&w, &mut store, ui2);
        let mut lib3 = req(&w, "lib", "1.7-beta", &[("data/lib.bin", "L3")]);
        lib3.compatible_versions = vec!["1.0.0".into()];
        // "1.7-beta" is a legal folder name but not a number, so it cannot be compared.
        import(&w, &mut store, lib3);
        p.set_version(pr, "main", "lib", "1.7-beta").unwrap();
        p.set_version(pr, "main", "ui", "2.0").unwrap();
        assert!(
            kinds(&p, &store).contains(&(ProblemKind::VersionNotComparable, Severity::Warning))
        );
    }

    #[test]
    fn unknown_mods_block_only_when_they_are_switched_on() {
        let (w, store, mut p) = setup();
        let pr = &w.protected;
        p.add_mod(pr, "main", "ghost", "1.0", None).unwrap();
        let k = check_profile(p.get("main").unwrap(), &store, "1.0.0");
        assert_eq!(k.len(), 1);
        assert_eq!(
            (k[0].kind, k[0].severity),
            (ProblemKind::UnknownMod, Severity::Error)
        );
        p.set_enabled(pr, "main", "ghost", false).unwrap();
        let k = check_profile(p.get("main").unwrap(), &store, "1.0.0");
        assert_eq!(
            (k[0].kind, k[0].severity),
            (ProblemKind::UnknownMod, Severity::Warning)
        );
        let _ = make_mod; // shared test helpers are used by the other test modules
    }
}
