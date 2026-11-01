//! When may SUL act on a file without asking, and what happens to a game whose files were edited? The owner's rules
//! (2026-10-31), written as one pure function.
//!
//! * **Tier 1, SUL's own files** (`Tier::Internal`): a file whose content is on the *known-harmful* list may be moved
//!   to quarantine automatically. Anything else that is wrong with it is *unknown*, so a person decides.
//! * **Tier 2, a game's or anti-cheat vendor's files** (`Tier::External`, anything inside a protected original install):
//!   SUL never moves, edits or signs them. When a file's content has been changed in a way that puts the game's file
//!   integrity or its standing with an anti-cheat at risk ([`RiskKind`]), SUL **does not launch the game** and points at
//!   the vendor's own verification (for Steam games, "Verify integrity of game files"). The two kinds of risk are kept apart
//!   so a card can say which one it is, but **they always get the same action** (owner, 2026-10-31).
//!   **Exception:** a game the owner has declared they mod themselves ([`GameModStatus::OwnerModded`]) gets a warning the
//!   owner can override instead. The override is a warning, not a bypass: SUL does nothing to hide the change from anyone.
//! * **Anywhere else** (`Tier::Unclassified`): not SUL's to move without asking, so never automatic.
//!
//! "Known harmful" is evidence, not a guess: the SHA-256 of the file's current content is on a list. The owner, the
//! hive-mind and Jarvis may add to it (owner, 2026-10-31); every entry records who added it, when and why. No list exists in
//! SUL yet, so [`KnownHarmful::empty`] is the only list there is today and **nothing is automatic**. A change event, a hash that
//! differs from the baseline, or a suspicious signal is *unknown*, not known-harmful: updates, installers and antivirus all change
//! files, and a changed file is not proof of anything (see [`super::Certainty`]).
//!
//! A list can be wrong, and entries can come from software, so one guard sits in front of the automatic action: a file whose
//! content is exactly what SUL recorded for it (its baseline) is not an *alteration*, so a list entry that matches it is a
//! conflict for a person to resolve, never a reason to move it.
//!
//! This module decides; it does not touch a file. The caller still goes through [`super::quarantine::Quarantine`], which
//! journals the move, keeps it reversible and refuses original installs on its own. Stopping the game is the caller's job too, and
//! only covers launching through SUL: nothing here can stop the owner starting the game some other way.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::Certainty;
use crate::error::io;
use crate::fsops::ProtectedRoots;
use crate::safepath::resolve_lenient;
use crate::{Error, Result};

/// Which verification model a file falls under (spec V5.1, section 12.7: the Two-Tier Verification Boundary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// SUL's own binaries, sidecars and configuration.
    Internal,
    /// A game, a game's anti-cheat or any other vendor's files: read-only to SUL.
    External,
    /// Neither: not decided by a root SUL knows about.
    Unclassified,
}

/// Decides the [`Tier`] of a path from two sets of folders. A path inside both is **External**: when in doubt, SUL does not touch it.
#[derive(Debug, Clone)]
pub struct Tiering {
    internal: Vec<PathBuf>,
    external: ProtectedRoots,
}

impl Tiering {
    /// `internal_roots` are folders that hold SUL's own files (they must exist); `external` is the set of protected original installs.
    pub fn new(internal_roots: &[&Path], external: ProtectedRoots) -> Result<Self> {
        let mut internal = Vec::with_capacity(internal_roots.len());
        for root in internal_roots {
            internal.push(root.canonicalize().map_err(io(root))?);
        }
        Ok(Self { internal, external })
    }

    /// The path need not exist. Symlinks and `..` are resolved first, so neither can be used to move a file into the wrong tier.
    pub fn classify(&self, path: &Path) -> Tier {
        if self.external.check_writable(path).is_err() {
            return Tier::External;
        }
        let resolved = resolve_lenient(path);
        if self.internal.iter().any(|root| resolved.starts_with(root)) {
            Tier::Internal
        } else {
            Tier::Unclassified
        }
    }
}

/// Who put an entry on the known-harmful list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ListSource {
    Owner,
    /// The shared threat log (`sul-hive-mind`, spec V5.1 12.7).
    HiveMind,
    /// The orchestrator agent (spec V5.1 12.1).
    Jarvis,
}

/// One entry of the known-harmful list: what, who, when and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HarmfulEntry {
    /// SHA-256 of the file content, lowercase hex.
    pub sha256: String,
    pub source: ListSource,
    /// Why it is on the list. 1 to 300 characters, no control characters. Shown to the owner when the entry causes an action.
    pub reason: String,
    /// Seconds since 1970, supplied by the caller (this module reads no clock).
    pub added_unix: u64,
}

/// File contents known to be harmful, by SHA-256. Empty by default.
#[derive(Debug, Default, Clone)]
pub struct KnownHarmful {
    entries: BTreeMap<String, HarmfulEntry>,
}

impl KnownHarmful {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Add an entry. `sha256` must be exactly 64 hexadecimal characters (any case); a bad entry is refused and the list is
    /// unchanged, so a truncated or mistyped hash can never match something it should not. An entry already on the list is kept
    /// as it was (the first source and reason stay) and `Ok(false)` is returned.
    pub fn add(
        &mut self,
        sha256: &str,
        source: ListSource,
        reason: &str,
        added_unix: u64,
    ) -> Result<bool> {
        if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::Invalid(
                "a known-harmful entry must be a 64-character hexadecimal SHA-256".into(),
            ));
        }
        let chars = reason.chars().count();
        if !(1..=300).contains(&chars) || reason.chars().any(char::is_control) {
            return Err(Error::Invalid(
                "the reason for a known-harmful entry must be 1 to 300 characters with no control characters".into(),
            ));
        }
        let key = sha256.to_ascii_lowercase();
        if self.entries.contains_key(&key) {
            return Ok(false);
        }
        self.entries.insert(
            key.clone(),
            HarmfulEntry {
                sha256: key,
                source,
                reason: reason.to_owned(),
                added_unix,
            },
        );
        Ok(true)
    }

    /// Take an entry off the list (the owner's way to undo any entry, whoever added it). True if it was there.
    pub fn remove(&mut self, sha256: &str) -> bool {
        self.entries.remove(&sha256.to_ascii_lowercase()).is_some()
    }

    pub fn lookup(&self, sha256: &str) -> Option<&HarmfulEntry> {
        self.entries.get(&sha256.to_ascii_lowercase())
    }

    pub fn contains(&self, sha256: &str) -> bool {
        self.lookup(sha256).is_some()
    }

    /// All entries, ordered by hash.
    pub fn entries(&self) -> impl Iterator<Item = &HarmfulEntry> {
        self.entries.values()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The two kinds of risk an edit to a game's files can carry. They are different things (do the game's own files still match
/// the vendor's, versus might an anti-cheat object), so they are kept apart; the owner's rule is that they get the same action.
/// Which one applies is decided by the caller (a game adapter): SUL does not claim to know what any anti-cheat flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskKind {
    GameIntegrity,
    AntiCheatFlag,
}

/// Whether the owner mods this game themselves. It comes from an explicit choice by the owner for that game. It is never
/// inferred from file changes, and no list, adapter or agent may set it. It changes only what happens to a Tier 2 file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GameModStatus {
    Stock,
    OwnerModded,
}

/// What a check saw about one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    pub tier: Tier,
    pub certainty: Certainty,
    /// SHA-256 (hex) of the file's *current* content, if it could be read. A file that is missing or unreadable cannot match a list.
    pub sha256: Option<String>,
    /// SHA-256 (hex) SUL recorded for this file (its manifest), if it has one.
    pub baseline_sha256: Option<String>,
    /// Which kind of risk a Tier 2 edit carries. Ignored for other tiers.
    pub risk: RiskKind,
    /// Whether the owner mods this game themselves. Ignored for other tiers.
    pub game: GameModStatus,
}

impl Observation {
    pub fn new(tier: Tier, certainty: Certainty) -> Self {
        Self {
            tier,
            certainty,
            sha256: None,
            baseline_sha256: None,
            risk: RiskKind::GameIntegrity,
            game: GameModStatus::Stock,
        }
    }

    pub fn with_sha256(mut self, sha256: &str) -> Self {
        self.sha256 = Some(sha256.to_owned());
        self
    }

    pub fn with_baseline(mut self, sha256: &str) -> Self {
        self.baseline_sha256 = Some(sha256.to_owned());
        self
    }

    pub fn with_risk(mut self, risk: RiskKind) -> Self {
        self.risk = risk;
        self
    }

    pub fn with_game(mut self, game: GameModStatus) -> Self {
        self.game = game;
        self
    }

    /// True when both hashes are known and equal: the file is exactly what SUL recorded, so nothing was altered.
    fn is_as_recorded(&self) -> bool {
        match (&self.sha256, &self.baseline_sha256) {
            (Some(now), Some(recorded)) => now.eq_ignore_ascii_case(recorded),
            _ => false,
        }
    }
}

/// What SUL may do about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Put it in the list of findings. Nothing is asked and nothing is changed.
    ReportOnly,
    /// Show it to the owner and wait. Nothing is moved or changed until they say so.
    AskOwner,
    /// SUL's own file, known-harmful content: it may be quarantined now (journaled and reversible, see [`super::quarantine`]).
    QuarantineAutomatically,
    /// A game whose files were edited in a way that risks it: SUL does not launch it. It never touches the files; it offers the
    /// vendor's own verification (for example Steam's "Verify integrity of game files").
    BlockLaunch,
    /// The same finding on a game the owner mods themselves: show what changed and that the game's file check or its anti-cheat
    /// may object, and let the owner continue. SUL does nothing to hide the change. The override should be logged by the caller.
    WarnOwnerOverridable,
}

impl Action {
    /// True only for the one action that changes a file before a person has said yes.
    pub fn acts_without_asking(self) -> bool {
        self == Action::QuarantineAutomatically
    }

    /// True for the action that stops a launch and cannot be overridden by the owner (a game they do not mod themselves).
    pub fn blocks_launch(self) -> bool {
        self == Action::BlockLaunch
    }
}

/// An [`Action`], the sentence that explains it, and what the decision rested on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Decision {
    pub action: Action,
    pub reason: &'static str,
    /// For Tier 2 decisions, the kind of risk that was reported, so a card can say which one it was.
    pub risk: Option<RiskKind>,
    /// When a known-harmful entry caused or shaped the decision, who added it.
    pub source: Option<ListSource>,
}

/// The rule. See the module documentation. Only one combination returns [`Action::QuarantineAutomatically`]: `Tier::Internal`, a
/// content hash that is on the known-harmful list, and a file that is not exactly what SUL recorded for it.
pub fn decide(obs: &Observation, known: &KnownHarmful) -> Decision {
    let hit = obs.sha256.as_deref().and_then(|h| known.lookup(h));
    let source = hit.map(|e| e.source);
    let plain = |action, reason| Decision {
        action,
        reason,
        risk: None,
        source,
    };
    match obs.tier {
        Tier::External => {
            let game_decision = |action, reason| Decision {
                action,
                reason,
                risk: Some(obs.risk),
                source,
            };
            match (obs.certainty, obs.game) {
                (Certainty::Confirmed, GameModStatus::Stock) => game_decision(
                    Action::BlockLaunch,
                    "This game's files were changed in a way that puts its file integrity or its standing with an anti-cheat at risk. SUL will not launch it and never edits its files; use the vendor's own file verification.",
                ),
                (Certainty::Confirmed, GameModStatus::OwnerModded) => game_decision(
                    Action::WarnOwnerOverridable,
                    "You mod this game yourself, so SUL lets you continue. Its files differ from the vendor's, which can fail the game's file check or be flagged by its anti-cheat; a ban is the vendor's decision, and SUL does nothing to hide the change.",
                ),
                (Certainty::Suspicious, _) => game_decision(
                    Action::ReportOnly,
                    "A signal about a game's files. SUL only reports it and never touches the files.",
                ),
            }
        }
        Tier::Internal if hit.is_some() && !obs.is_as_recorded() => plain(
            Action::QuarantineAutomatically,
            "SUL's own file, altered, and its content is on the known-harmful list.",
        ),
        Tier::Internal | Tier::Unclassified => match (obs.certainty, hit.is_some()) {
            (_, true) if obs.tier == Tier::Internal => plain(
                Action::AskOwner,
                "The known-harmful list matches content that is exactly what SUL recorded for this file. One of the two is wrong, so a person decides and nothing is moved.",
            ),
            (_, true) => plain(
                Action::AskOwner,
                "Known-harmful content, but the file is not one of SUL's own, so it is not moved without asking.",
            ),
            (Certainty::Confirmed, false) => plain(
                Action::AskOwner,
                "The file is not as expected, and nothing says it is harmful. A person decides.",
            ),
            (Certainty::Suspicious, false) => plain(
                Action::ReportOnly,
                "A signal that needs a person to look. It may be a false alarm.",
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::hash_file;
    use crate::trust::quarantine::Quarantine;

    const BAD: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const OTHER: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    const TIERS: [Tier; 3] = [Tier::Internal, Tier::External, Tier::Unclassified];
    const CERTAINTIES: [Certainty; 2] = [Certainty::Confirmed, Certainty::Suspicious];
    const HASHES: [Option<&str>; 3] = [None, Some(BAD), Some(OTHER)];
    const RISKS: [RiskKind; 2] = [RiskKind::GameIntegrity, RiskKind::AntiCheatFlag];
    const GAMES: [GameModStatus; 2] = [GameModStatus::Stock, GameModStatus::OwnerModded];

    fn obs(
        tier: Tier,
        certainty: Certainty,
        sha: Option<&str>,
        baseline: Option<&str>,
    ) -> Observation {
        let mut o = Observation::new(tier, certainty);
        o.sha256 = sha.map(str::to_owned);
        o.baseline_sha256 = baseline.map(str::to_owned);
        o
    }

    fn list_from(source: ListSource) -> KnownHarmful {
        let mut k = KnownHarmful::empty();
        assert!(
            k.add(BAD, source, "dropper seen in the wild", 1_700_000_000)
                .unwrap()
        );
        k
    }

    fn list() -> KnownHarmful {
        list_from(ListSource::Owner)
    }

    /// Every combination of inputs, for the properties that must hold everywhere.
    fn for_every_observation(mut f: impl FnMut(&Observation, &KnownHarmful)) {
        for known in [KnownHarmful::empty(), list()] {
            for tier in TIERS {
                for certainty in CERTAINTIES {
                    for sha in HASHES {
                        for baseline in HASHES {
                            for risk in RISKS {
                                for game in GAMES {
                                    let o = obs(tier, certainty, sha, baseline)
                                        .with_risk(risk)
                                        .with_game(game);
                                    f(&o, &known);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_only_automatic_case_is_an_altered_sul_file_with_known_harmful_content() {
        let mut automatic = 0;
        for_every_observation(|o, known| {
            let d = decide(o, known);
            let altered = !(o.sha256.is_some() && o.sha256 == o.baseline_sha256);
            let expected = o.tier == Tier::Internal
                && o.sha256.as_deref() == Some(BAD)
                && !known.is_empty()
                && altered;
            assert_eq!(
                d.action.acts_without_asking(),
                expected,
                "{o:?} list={}",
                known.len()
            );
            automatic += usize::from(expected);
        });
        assert!(
            automatic > 0,
            "the property test must reach the automatic case at least once"
        );
    }

    #[test]
    fn with_no_list_nothing_is_ever_automatic() {
        let empty = KnownHarmful::empty();
        for_every_observation(|o, _| {
            assert!(!decide(o, &empty).action.acts_without_asking(), "{o:?}")
        });
    }

    #[test]
    fn a_list_entry_that_matches_what_sul_recorded_is_never_moved() {
        // The list says the content is harmful, but it is exactly the content SUL recorded for its own file.
        let known = list_from(ListSource::HiveMind);
        let o = obs(Tier::Internal, Certainty::Confirmed, Some(BAD), Some(BAD));
        let d = decide(&o, &known);
        assert_eq!(d.action, Action::AskOwner);
        assert!(
            d.reason.contains("exactly what SUL recorded"),
            "{}",
            d.reason
        );
        assert_eq!(
            d.source,
            Some(ListSource::HiveMind),
            "the card can say whose entry conflicts"
        );
        // Case does not matter when comparing the two hashes.
        let upper = obs(
            Tier::Internal,
            Certainty::Confirmed,
            Some(BAD),
            Some(&BAD.to_ascii_uppercase()),
        );
        assert_eq!(decide(&upper, &known).action, Action::AskOwner);
        // A different baseline (or none) means the file was altered, so the list entry applies.
        for baseline in [None, Some(OTHER)] {
            let altered = obs(Tier::Internal, Certainty::Confirmed, Some(BAD), baseline);
            assert_eq!(
                decide(&altered, &known).action,
                Action::QuarantineAutomatically,
                "{baseline:?}"
            );
        }
    }

    #[test]
    fn unknown_alterations_to_sul_own_files_are_manual() {
        let known = list();
        let confirmed = decide(
            &obs(Tier::Internal, Certainty::Confirmed, Some(OTHER), Some(BAD)),
            &known,
        );
        assert_eq!(
            confirmed.action,
            Action::AskOwner,
            "a changed file nobody has called harmful: a person decides"
        );
        let unreadable = decide(
            &obs(Tier::Internal, Certainty::Confirmed, None, None),
            &known,
        );
        assert_eq!(
            unreadable.action,
            Action::AskOwner,
            "a file that cannot be read cannot match a list"
        );
        let signal = decide(
            &obs(Tier::Internal, Certainty::Suspicious, Some(OTHER), None),
            &known,
        );
        assert_eq!(
            signal.action,
            Action::ReportOnly,
            "a signal is reported, not acted on"
        );
    }

    #[test]
    fn a_file_outside_both_areas_is_asked_about_never_moved() {
        let known = list();
        let hit = decide(
            &obs(Tier::Unclassified, Certainty::Suspicious, Some(BAD), None),
            &known,
        );
        assert_eq!(
            hit.action,
            Action::AskOwner,
            "known-harmful, but not SUL's file"
        );
        let miss = decide(
            &obs(Tier::Unclassified, Certainty::Confirmed, Some(OTHER), None),
            &known,
        );
        assert_eq!(miss.action, Action::AskOwner);
    }

    #[test]
    fn a_game_edited_in_a_risky_way_is_not_launched_and_both_kinds_of_risk_get_the_same_action() {
        for known in [KnownHarmful::empty(), list()] {
            for sha in HASHES {
                for baseline in HASHES {
                    let base = obs(Tier::External, Certainty::Confirmed, sha, baseline);
                    let integrity =
                        decide(&base.clone().with_risk(RiskKind::GameIntegrity), &known);
                    let flag = decide(&base.clone().with_risk(RiskKind::AntiCheatFlag), &known);
                    assert_eq!(integrity.action, Action::BlockLaunch);
                    assert_eq!(
                        integrity.action, flag.action,
                        "the owner's rule: the two kinds of risk do the same"
                    );
                    assert_eq!(integrity.reason, flag.reason);
                    // ...but the card can still tell them apart.
                    assert_eq!(integrity.risk, Some(RiskKind::GameIntegrity));
                    assert_eq!(flag.risk, Some(RiskKind::AntiCheatFlag));
                    assert!(
                        integrity.action.blocks_launch() && !integrity.action.acts_without_asking()
                    );
                }
            }
        }
    }

    #[test]
    fn a_game_the_owner_mods_gets_a_warning_they_can_override_instead_of_a_block() {
        let known = list();
        for risk in RISKS {
            let o = obs(Tier::External, Certainty::Confirmed, Some(OTHER), Some(BAD))
                .with_risk(risk)
                .with_game(GameModStatus::OwnerModded);
            let d = decide(&o, &known);
            assert_eq!(d.action, Action::WarnOwnerOverridable, "{risk:?}");
            assert!(!d.action.blocks_launch() && !d.action.acts_without_asking());
            assert_eq!(d.risk, Some(risk));
            let r = d.reason.to_ascii_lowercase();
            assert!(
                r.contains("ban") && r.contains("flagged") && r.contains("hide"),
                "the warning must be honest about the risk: {r}"
            );
        }
    }

    #[test]
    fn declaring_a_game_as_owner_modded_changes_nothing_except_the_launch_block_on_that_games_files()
     {
        // The override cannot weaken anything else: every other tier decides identically with or without it,
        // and for a game's files only the Confirmed case changes.
        for_every_observation(|o, known| {
            let stock = decide(
                &Observation {
                    game: GameModStatus::Stock,
                    ..o.clone()
                },
                known,
            );
            let modded = decide(
                &Observation {
                    game: GameModStatus::OwnerModded,
                    ..o.clone()
                },
                known,
            );
            if o.tier == Tier::External && o.certainty == Certainty::Confirmed {
                assert_eq!(
                    (stock.action, modded.action),
                    (Action::BlockLaunch, Action::WarnOwnerOverridable)
                );
            } else {
                assert_eq!(stock, modded, "{o:?}");
            }
        });
    }

    #[test]
    fn a_game_file_is_never_moved_asked_about_or_quarantined_whatever_the_list_says() {
        for_every_observation(|o, known| {
            if o.tier == Tier::External {
                let a = decide(o, known).action;
                assert!(
                    matches!(
                        a,
                        Action::BlockLaunch | Action::WarnOwnerOverridable | Action::ReportOnly
                    ),
                    "{o:?} -> {a:?}"
                );
            }
        });
        // A suspicious signal is a signal, never a block (false positives: updates, overlays, antivirus).
        let s = decide(
            &obs(Tier::External, Certainty::Suspicious, Some(BAD), None),
            &list(),
        );
        assert_eq!(s.action, Action::ReportOnly);
    }

    #[test]
    fn the_list_records_who_added_what_and_why_and_the_decision_names_the_source() {
        for source in [ListSource::Owner, ListSource::HiveMind, ListSource::Jarvis] {
            let known = list_from(source);
            let e = known.lookup(BAD).unwrap();
            assert_eq!((e.source, e.added_unix), (source, 1_700_000_000));
            assert_eq!(e.reason, "dropper seen in the wild");
            let d = decide(
                &obs(Tier::Internal, Certainty::Confirmed, Some(BAD), None),
                &known,
            );
            assert_eq!(
                (d.action, d.source),
                (Action::QuarantineAutomatically, Some(source))
            );
            let none = decide(
                &obs(Tier::Internal, Certainty::Confirmed, Some(OTHER), None),
                &known,
            );
            assert_eq!(none.source, None);
        }
    }

    #[test]
    fn entries_are_validated_deduplicated_and_removable_by_the_owner() {
        let mut k = KnownHarmful::empty();
        for bad in [
            "",
            "abc",
            &BAD[..63],
            &format!("{BAD}0"),
            &format!("{}g", &BAD[..63]),
            &format!(" {}", &BAD[..63]),
        ] {
            let err = k
                .add(bad, ListSource::Jarvis, "why", 1)
                .unwrap_err()
                .to_string();
            assert!(err.contains("64-character"), "{bad:?}: {err}");
            assert!(
                bad.is_empty() || !err.contains(bad),
                "the message must not echo the entry"
            );
        }
        for reason in ["", &"x".repeat(301), "line\nbreak", "tab\there", "nul\0"] {
            assert!(
                k.add(BAD, ListSource::Jarvis, reason, 1).is_err(),
                "{reason:?}"
            );
        }
        assert!(k.is_empty(), "a refused entry leaves the list unchanged");
        assert!(
            k.add(
                &BAD.to_ascii_uppercase(),
                ListSource::Jarvis,
                &"x".repeat(300),
                5
            )
            .unwrap()
        );
        assert!(k.contains(BAD) && k.contains(&BAD.to_ascii_uppercase()) && !k.contains(OTHER));
        assert!(
            !k.add(BAD, ListSource::Owner, "a later duplicate", 9)
                .unwrap(),
            "a duplicate is not added"
        );
        let e = k.lookup(BAD).unwrap();
        assert_eq!(
            (e.source, e.added_unix),
            (ListSource::Jarvis, 5),
            "the first source stays"
        );
        assert_eq!(e.sha256, BAD, "stored in lowercase");
        assert!(k.add(OTHER, ListSource::HiveMind, "second", 6).unwrap());
        assert_eq!(
            k.entries().map(|e| e.sha256.as_str()).collect::<Vec<_>>(),
            [BAD, OTHER],
            "ordered by hash"
        );
        assert!(k.remove(&OTHER.to_ascii_uppercase()) && !k.remove(OTHER));
        assert_eq!(k.len(), 1);
    }

    #[test]
    fn tiers_follow_the_folders_and_the_external_one_wins() {
        let tmp = tempfile::tempdir().unwrap();
        let own = tmp.path().join("sul");
        let game = tmp.path().join("game");
        let nested = game.join("sul-inside-the-game");
        for d in [&own, &game, &nested] {
            std::fs::create_dir_all(d).unwrap();
        }
        let mut external = ProtectedRoots::new();
        external.add(&game).unwrap();
        let tiering = Tiering::new(&[own.as_path(), nested.as_path()], external).unwrap();

        assert_eq!(
            tiering.classify(&own.join("sul.exe")),
            Tier::Internal,
            "a file that does not exist yet is still classified"
        );
        assert_eq!(
            tiering.classify(&game.join("helldivers2.exe")),
            Tier::External
        );
        assert_eq!(
            tiering.classify(&nested.join("sul-helper.exe")),
            Tier::External,
            "a folder that is both SUL's and the game's counts as the game's"
        );
        assert_eq!(
            tiering.classify(&tmp.path().join("elsewhere.dll")),
            Tier::Unclassified
        );
        assert_eq!(
            tiering.classify(&own.join("..").join("game").join("a.dll")),
            Tier::External,
            "`..` cannot be used to look like SUL's own file"
        );
        assert_eq!(
            tiering.classify(&own.join("..").join("elsewhere.dll")),
            Tier::Unclassified
        );
        // A sibling that merely starts with the same letters is not inside the folder.
        assert_eq!(
            tiering.classify(&tmp.path().join("sul-other").join("a.dll")),
            Tier::Unclassified
        );
        assert!(
            Tiering::new(
                &[tmp.path().join("missing").as_path()],
                ProtectedRoots::new()
            )
            .is_err()
        );
    }

    #[test]
    fn the_rule_and_the_quarantine_agree_end_to_end() {
        let tmp = tempfile::tempdir().unwrap();
        let own = tmp.path().join("sul");
        let game = tmp.path().join("game");
        std::fs::create_dir_all(&own).unwrap();
        std::fs::create_dir_all(&game).unwrap();
        let mut external = ProtectedRoots::new();
        external.add(&game).unwrap();
        let tiering = Tiering::new(&[own.as_path()], external.clone()).unwrap();
        let q = Quarantine::open(&tmp.path().join("quarantine"), external).unwrap();

        let ours = own.join("sidecar.exe");
        let theirs = game.join("anticheat.dll");
        std::fs::write(&ours, b"tampered sidecar").unwrap();
        std::fs::write(&theirs, b"tampered anticheat").unwrap();
        let ours_hash = hash_file(&ours).unwrap().0;
        let theirs_hash = hash_file(&theirs).unwrap().0;
        let mut known = KnownHarmful::empty();
        known
            .add(&ours_hash, ListSource::HiveMind, "tampered sidecar", 1)
            .unwrap();
        known
            .add(&theirs_hash, ListSource::Jarvis, "tampered anticheat", 2)
            .unwrap();

        // SUL's own file with known-harmful content: automatic, and the quarantine can undo it.
        let o =
            Observation::new(tiering.classify(&ours), Certainty::Confirmed).with_sha256(&ours_hash);
        let d = decide(&o, &known);
        assert_eq!(
            (d.action, d.source),
            (Action::QuarantineAutomatically, Some(ListSource::HiveMind))
        );
        let rec = q
            .quarantine_file(&ours, "known-harmful content (automatic)")
            .unwrap();
        assert!(!ours.exists());
        q.restore(&rec.id).unwrap();
        assert_eq!(std::fs::read(&ours).unwrap(), b"tampered sidecar");

        // The same list entry on a game's file: the game is not launched, nothing is moved, and the quarantine refuses on its own too.
        let o = Observation::new(tiering.classify(&theirs), Certainty::Confirmed)
            .with_sha256(&theirs_hash)
            .with_risk(RiskKind::AntiCheatFlag);
        assert_eq!(decide(&o, &known).action, Action::BlockLaunch);
        assert!(q.quarantine_file(&theirs, "must be refused").is_err());
        assert_eq!(std::fs::read(&theirs).unwrap(), b"tampered anticheat");
        // Owner-modded: a warning they can override, and still nothing is moved.
        let modded = o.with_game(GameModStatus::OwnerModded);
        assert_eq!(decide(&modded, &known).action, Action::WarnOwnerOverridable);
        assert_eq!(std::fs::read(&theirs).unwrap(), b"tampered anticheat");
    }

    #[test]
    fn every_decision_has_a_reason_that_never_says_safe() {
        for_every_observation(|o, known| {
            let r = decide(o, known).reason.to_ascii_lowercase();
            assert!(r.len() > 20);
            for word in ["safe", "clean", "trusted", "verified"] {
                assert!(!r.contains(word), "{r}");
            }
        });
    }
}
