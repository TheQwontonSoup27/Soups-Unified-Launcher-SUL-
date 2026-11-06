//! Game adapters (spec 1.1 and section 2, item 6): the one interface through which a game's specifics
//! reach the rest of SUL.
//!
//! The rest of the app never learns how a game is laid out. An adapter answers four questions about it:
//!
//! 1. **Is this folder your game, and which version?** ([`GameAdapter::recognise`]) A read-only look.
//! 2. **How do I start a build of it?** ([`GameAdapter::launch_plan`]) A plan, not an action.
//! 3. **Where may mods put files, and where may they never?** ([`GameAdapter::mod_layout`])
//! 4. **What of the above has been tested?** ([`GameAdapter::capabilities`]) Labelled like everything else.
//!
//! # What exists, and what does not
//!
//! The interface is built and tested against made-up game folders only. **No adapter for a real game is
//! written** (owner decision, 2026-11-01: "no real game yet"). The four first-wave targets named in the
//! spec are listed in [`INITIAL_TARGETS`], each labelled *Not investigated*, so the library can show them
//! without implying anything works. An adapter is not "supported" until it is registered here, its
//! capabilities are tested on that game, and the evidence is recorded.
//!
//! The Helldivers 2 7.1.1 isolation experiment is a separate track and is not a prerequisite of this
//! module (spec, "Helldivers 2 initial experiment").

use std::collections::BTreeSet;
use std::path::Path;

use serde::Serialize;

use crate::capability::{CapabilityEntry, Status};
use crate::safepath;
use crate::storage::validate_game_id;
use crate::{Error, Result};

/// What an adapter found when it looked at a folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Recognition {
    pub recognised: bool,
    /// The game's own version string, when the adapter could read one. Never guessed.
    pub version: Option<String>,
    /// What the adapter looked at to decide, one short line each. Shown to the owner.
    pub evidence: Vec<String>,
}

impl Recognition {
    pub fn not_this_game(why: &str) -> Self {
        Self {
            recognised: false,
            version: None,
            evidence: vec![why.to_owned()],
        }
    }
}

/// How to start one build of a game. Every path is relative to the build folder and is checked to stay
/// inside it before anything runs (see [`crate::process`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaunchPlan {
    /// The program to start, relative to the build folder, forward slashes.
    pub exe: String,
    pub args: Vec<String>,
    /// The folder the program starts in, relative to the build folder. Empty means the build folder.
    pub working_dir: String,
    /// Extra environment variables. Names are checked; values are never written to a log.
    pub env: Vec<(String, String)>,
    /// What the arguments and variables are for, in plain words (spec 1.4: "documented arguments").
    pub explanation: String,
}

/// Where a game lets mods put files. Default deny: a game with no allowed prefix takes no mods.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModLayout {
    /// Folders (relative to the build folder) a mod may write into. Everything else is refused.
    pub allowed_prefixes: Vec<String>,
    /// Folders a mod may never write into even if an allowed prefix contains them: the game's own
    /// program files, its launcher and, above all, its anti-cheat. Always wins over `allowed_prefixes`.
    pub forbidden_prefixes: Vec<String>,
    /// Whether the game's file system treats `A.txt` and `a.txt` as the same file (Windows does).
    pub case_insensitive: bool,
}

impl ModLayout {
    /// A layout that takes no mods.
    pub fn none() -> Self {
        Self {
            allowed_prefixes: vec![],
            forbidden_prefixes: vec![],
            case_insensitive: true,
        }
    }

    pub fn validate(&self) -> Result<()> {
        for p in self.allowed_prefixes.iter().chain(&self.forbidden_prefixes) {
            safepath::validate_relative(p)?;
        }
        Ok(())
    }

    /// Why a mod may not write `rel_path` (a forward-slash relative path), or `None` when it may.
    /// The forbidden list is checked first.
    pub fn refusal_for(&self, rel_path: &str) -> Option<String> {
        // A path that climbs out of its folder is refused outright: `data/../anticheat/x` starts with an
        // allowed folder and would otherwise be matched against the wrong place.
        if rel_path.replace('\\', "/").split('/').any(|p| p == "..") {
            return Some(format!("{rel_path} climbs out of its folder"));
        }
        let key = |s: &str| {
            let s = s.replace('\\', "/");
            // `./` and a doubled slash mean the same folder as without them.
            let s = s
                .split('/')
                .filter(|p| !p.is_empty() && *p != ".")
                .collect::<Vec<_>>()
                .join("/");
            if self.case_insensitive {
                s.to_ascii_lowercase()
            } else {
                s
            }
        };
        let path = key(rel_path);
        let inside = |prefix: &str| {
            let p = key(prefix);
            path == p || path.starts_with(&format!("{p}/"))
        };
        if let Some(f) = self.forbidden_prefixes.iter().find(|f| inside(f)) {
            return Some(format!(
                "{rel_path} is inside {f}, which mods may never change"
            ));
        }
        if self.allowed_prefixes.iter().any(|a| inside(a)) {
            return None;
        }
        Some(format!(
            "{rel_path} is outside the folders this game's adapter lets mods change"
        ))
    }
}

/// The interface a game adapter implements. Adapters only read; the callers decide what to do.
pub trait GameAdapter {
    /// The game id used in the library and build registry (checked like any game id).
    fn id(&self) -> &str;
    fn display_name(&self) -> &str;
    /// The adapter's own version, recorded with every capability entry.
    fn version(&self) -> &str;
    fn recognise(&self, folder: &Path) -> Result<Recognition>;
    /// How to start the build in `build_dir`. `base_version` is the version recorded for the build.
    fn launch_plan(&self, build_dir: &Path, base_version: &str) -> Result<LaunchPlan>;
    fn mod_layout(&self) -> ModLayout;
    /// What has been tested. Entries that claim "Supported and tested" need a date and evidence.
    fn capabilities(&self) -> Vec<CapabilityEntry>;
}

/// The adapters this build of SUL knows. Registration is the only way in; an adapter that overclaims is
/// refused when it is registered, not when it is used.
#[derive(Default)]
pub struct AdapterRegistry {
    adapters: Vec<Box<dyn GameAdapter + Send>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, adapter: Box<dyn GameAdapter + Send>) -> Result<()> {
        validate_game_id(adapter.id())?;
        if adapter.display_name().trim().is_empty() || adapter.version().trim().is_empty() {
            return Err(Error::Invalid(format!(
                "adapter {:?} must have a display name and a version",
                adapter.id()
            )));
        }
        if self.get(adapter.id()).is_some() {
            return Err(Error::Invalid(format!(
                "an adapter for {:?} is already registered",
                adapter.id()
            )));
        }
        adapter.mod_layout().validate()?;
        for c in adapter.capabilities() {
            c.validate()?;
            if c.game != adapter.id() {
                return Err(Error::Invalid(format!(
                    "adapter {:?} declared a capability for another game ({:?})",
                    adapter.id(),
                    c.game
                )));
            }
        }
        self.adapters.push(adapter);
        Ok(())
    }

    pub fn get(&self, game_id: &str) -> Option<&dyn GameAdapter> {
        self.adapters
            .iter()
            .find(|a| a.id() == game_id)
            .map(|a| -> &dyn GameAdapter { a.as_ref() })
    }

    pub fn ids(&self) -> Vec<&str> {
        self.adapters.iter().map(|a| a.id()).collect()
    }

    pub fn len(&self) -> usize {
        self.adapters.len()
    }

    pub fn is_empty(&self) -> bool {
        self.adapters.is_empty()
    }

    /// Ask every adapter about a folder. An adapter that fails is reported as not recognising it, with
    /// the reason, rather than stopping the others.
    pub fn recognise_all(&self, folder: &Path) -> Vec<(String, Recognition)> {
        self.adapters
            .iter()
            .map(|a| {
                let r = a
                    .recognise(folder)
                    .unwrap_or_else(|e| Recognition::not_this_game(&format!("adapter error: {e}")));
                (a.id().to_owned(), r)
            })
            .collect()
    }

    /// Every adapter's capability entries.
    pub fn capabilities(&self) -> Vec<CapabilityEntry> {
        self.adapters
            .iter()
            .flat_map(|a| a.capabilities())
            .collect()
    }

    /// The weakest label among one game's capability entries; *Not investigated* when there is no adapter
    /// or it declares nothing. The weakest wins so the headline never says more than a row does.
    pub fn status_of(&self, game_id: &str) -> Status {
        match self.get(game_id) {
            None => Status::NotInvestigated,
            Some(a) => Status::weakest(a.capabilities().iter().map(|c| c.status)),
        }
    }
}

/// A game the spec names as a target for an adapter. A target is not an adapter: it records that the
/// work has not been done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Target {
    pub id: &'static str,
    pub display_name: &'static str,
    pub status: Status,
    pub note: &'static str,
}

/// The first-wave targets from spec 1.1, all *Not investigated*.
pub const INITIAL_TARGETS: [Target; 4] = [
    Target {
        id: "helldivers2",
        display_name: "Helldivers 2",
        status: Status::NotInvestigated,
        note: "Build 7.1.1 isolation is a separate track; the original Steam install is never altered. Anti-cheat and access controls are not bypassed.",
    },
    Target {
        id: "terraria",
        display_name: "Terraria",
        status: Status::NotInvestigated,
        note: "Vanilla and tModLoader versions, profiles and dependencies, host-and-play and dedicated-server workflows.",
    },
    Target {
        id: "rimworld",
        display_name: "RimWorld",
        status: Status::NotInvestigated,
        note: "Version-specific mod profiles, load order and dependencies, save safety. The base game has no native multiplayer.",
    },
    Target {
        id: "stardewvalley",
        display_name: "Stardew Valley",
        status: Status::NotInvestigated,
        note: "Game and SMAPI compatibility, mod profiles, co-op host and client behaviour.",
    },
];

/// Possible later adapters: not a committed first-wave target.
pub const LATER_TARGETS: [&str; 1] = ["Project Zomboid"];

/// Every target id is a valid game id and none repeats. Used by a test; kept here so the list cannot drift.
pub fn targets_are_well_formed() -> bool {
    let mut seen = BTreeSet::new();
    INITIAL_TARGETS
        .iter()
        .all(|t| validate_game_id(t.id).is_ok() && seen.insert(t.id))
}

#[cfg(test)]
pub(crate) mod fixtures {
    //! A made-up game for tests. It is not a real game and not a real adapter.
    use super::*;

    pub struct MadeUpGame {
        pub id: String,
        pub claim_tested_without_evidence: bool,
    }

    impl MadeUpGame {
        pub fn new() -> Self {
            Self {
                id: "madeupgame".into(),
                claim_tested_without_evidence: false,
            }
        }
    }

    impl GameAdapter for MadeUpGame {
        fn id(&self) -> &str {
            &self.id
        }
        fn display_name(&self) -> &str {
            "Made-up Game"
        }
        fn version(&self) -> &str {
            "0.0-test"
        }
        fn recognise(&self, folder: &Path) -> Result<Recognition> {
            let marker = folder.join("madeup.txt");
            if !marker.is_file() {
                return Ok(Recognition::not_this_game("no madeup.txt"));
            }
            let text = std::fs::read_to_string(&marker).map_err(crate::error::io(&marker))?;
            Ok(Recognition {
                recognised: true,
                version: text.lines().next().map(|l| l.trim().to_owned()),
                evidence: vec!["madeup.txt present".into()],
            })
        }
        fn launch_plan(&self, _build_dir: &Path, _base_version: &str) -> Result<LaunchPlan> {
            Ok(LaunchPlan {
                exe: "game.exe".into(),
                args: vec!["--list".into()],
                working_dir: String::new(),
                env: vec![],
                explanation: "--list asks the test program to print its tests and stop.".into(),
            })
        }
        fn mod_layout(&self) -> ModLayout {
            ModLayout {
                allowed_prefixes: vec!["mods".into(), "data".into()],
                forbidden_prefixes: vec!["data/anticheat".into()],
                case_insensitive: true,
            }
        }
        fn capabilities(&self) -> Vec<CapabilityEntry> {
            let mut e = CapabilityEntry::not_investigated("launch", &self.id);
            e.status = Status::Experimental;
            let mut tested = CapabilityEntry::not_investigated("mods", &self.id);
            if self.claim_tested_without_evidence {
                tested.status = Status::SupportedTested;
            } else {
                tested.status = Status::Partial;
            }
            vec![e, tested]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::MadeUpGame;
    use super::*;

    #[test]
    fn the_first_wave_targets_are_listed_and_all_say_not_investigated() {
        assert!(targets_are_well_formed());
        let names: Vec<_> = INITIAL_TARGETS.iter().map(|t| t.display_name).collect();
        assert_eq!(
            names,
            ["Helldivers 2", "Terraria", "RimWorld", "Stardew Valley"]
        );
        assert!(
            INITIAL_TARGETS
                .iter()
                .all(|t| t.status == Status::NotInvestigated)
        );
        assert_eq!(LATER_TARGETS, ["Project Zomboid"]);
    }

    #[test]
    fn no_real_adapter_is_registered_by_default() {
        let r = AdapterRegistry::new();
        assert!(r.is_empty());
        for t in &INITIAL_TARGETS {
            assert_eq!(r.status_of(t.id), Status::NotInvestigated);
        }
    }

    #[test]
    fn a_registered_adapter_is_found_and_its_weakest_label_is_the_game_label() {
        let mut r = AdapterRegistry::new();
        r.register(Box::new(MadeUpGame::new())).unwrap();
        assert_eq!(r.ids(), ["madeupgame"]);
        assert!(r.get("madeupgame").is_some());
        assert!(r.get("other").is_none());
        // launch is Experimental, mods is Partial: the weaker of the two is Experimental.
        assert_eq!(r.status_of("madeupgame"), Status::Experimental);
        assert_eq!(r.capabilities().len(), 2);
    }

    #[test]
    fn an_adapter_that_claims_tested_without_evidence_is_refused_at_registration() {
        let mut r = AdapterRegistry::new();
        let mut a = MadeUpGame::new();
        a.claim_tested_without_evidence = true;
        assert!(r.register(Box::new(a)).is_err());
        assert!(r.is_empty());
    }

    #[test]
    fn a_duplicate_or_badly_named_adapter_is_refused() {
        let mut r = AdapterRegistry::new();
        r.register(Box::new(MadeUpGame::new())).unwrap();
        assert!(r.register(Box::new(MadeUpGame::new())).is_err());
        let mut bad = MadeUpGame::new();
        bad.id = "../escape".into();
        assert!(r.register(Box::new(bad)).is_err());
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn recognition_reads_only_and_reports_the_version_it_found() {
        let dir = tempfile::tempdir().unwrap();
        let mut r = AdapterRegistry::new();
        r.register(Box::new(MadeUpGame::new())).unwrap();

        let none = r.recognise_all(dir.path());
        assert!(!none[0].1.recognised);

        std::fs::write(dir.path().join("madeup.txt"), "1.2.3\nmore").unwrap();
        let yes = r.recognise_all(dir.path());
        assert!(yes[0].1.recognised);
        assert_eq!(yes[0].1.version.as_deref(), Some("1.2.3"));
        // nothing was written
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn a_failing_adapter_is_reported_not_fatal() {
        let mut r = AdapterRegistry::new();
        r.register(Box::new(MadeUpGame::new())).unwrap();
        let dir = tempfile::tempdir().unwrap();
        // madeup.txt is a directory: reading it fails
        std::fs::create_dir(dir.path().join("madeup.txt")).unwrap();
        let out = r.recognise_all(dir.path());
        assert!(!out[0].1.recognised);
    }

    #[test]
    fn the_mod_layout_is_default_deny_and_the_forbidden_list_wins() {
        let l = MadeUpGame::new().mod_layout();
        assert!(l.refusal_for("mods/a/b.txt").is_none());
        assert!(l.refusal_for("data/x.bin").is_none());
        assert!(l.refusal_for("game.exe").is_some());
        assert!(l.refusal_for("bin/other.dll").is_some());
        // inside an allowed prefix but also forbidden: refused
        assert!(
            l.refusal_for("data/anticheat/x.dll")
                .unwrap()
                .contains("never")
        );
        assert!(l.refusal_for("data/anticheat").is_some());
        // a name that merely starts with the same letters is not inside the folder
        assert!(l.refusal_for("data/anticheatish.txt").is_none());
        assert!(l.refusal_for("modsx/a.txt").is_some());
    }

    #[test]
    fn the_mod_layout_ignores_case_and_slash_style_where_the_game_does() {
        let l = MadeUpGame::new().mod_layout();
        assert!(l.refusal_for("MODS\\a.txt").is_none());
        assert!(l.refusal_for("DATA/AntiCheat/x.dll").is_some());
        let mut strict = l.clone();
        strict.case_insensitive = false;
        assert!(strict.refusal_for("MODS/a.txt").is_some());
    }

    #[test]
    fn a_layout_with_no_allowed_folder_takes_no_mods() {
        let l = ModLayout::none();
        assert!(l.refusal_for("anything.txt").is_some());
    }

    #[test]
    fn an_unsafe_prefix_in_a_layout_is_refused() {
        let mut l = MadeUpGame::new().mod_layout();
        l.allowed_prefixes.push("../outside".into());
        assert!(l.validate().is_err());
    }
}
