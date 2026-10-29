//! Per-game tick-and-compatibility matrix (spec section 1).
//!
//! Each game carries one row per launcher [`Feature`]. A row has two independent
//! parts, matching the "tick-and-compatibility checkbox" in the spec:
//!
//! * the **tick** (`enabled`): the user's choice to use the feature for this game;
//! * the **compatibility**: what the launcher knows about whether it can work.
//!
//! The scanner can only establish *applicability* (for example Steam-only features),
//! so everything else starts as [`Compatibility::Unverified`]. Later subsystems
//! (overlay, controller remap, ...) upgrade a row through
//! [`CompatibilityMatrix::set_compatibility`] once they have probed the game, or through
//! [`CompatibilityMatrix::set_status`] when they can back a claim with a test date and evidence.
//!
//! The project's five capability labels ([`Status`]) are layered on top: a row's label is
//! derived by [`FeatureState::status`], so it can never contradict the verdict, and
//! "Supported and tested" is only ever shown when the stored claim carries its evidence.
//!
//! The feature list is derived from the spec's subsystems and is meant to be edited
//! as those subsystems are built.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::capability::{CapabilityEntry, Claim, Status};
use crate::registry::{GameSource, InstalledGame};
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Feature {
    /// Redirect save profiles into the per-game sandbox (spec section 1).
    SaveIsolation,
    /// Intercept game configuration files with override rules (spec section 1).
    ConfigOverride,
    /// Launch through the headless Steam layer (spec section 1).
    SteamHeadless,
    /// Controller remapping and presets (spec section 2).
    ControllerRemap,
    /// In-game console overlay (spec section 4).
    Overlay,
    /// Per-game Discord Rich Presence (spec section 3).
    DiscordRpc,
    /// Mod tools (spec section 3).
    Mods,
    /// Telemetry and the embedded task manager (spec section 3).
    Diagnostics,
}

impl Feature {
    pub const ALL: [Feature; 8] = [
        Feature::SaveIsolation,
        Feature::ConfigOverride,
        Feature::SteamHeadless,
        Feature::ControllerRemap,
        Feature::Overlay,
        Feature::DiscordRpc,
        Feature::Mods,
        Feature::Diagnostics,
    ];

    /// The stable identifier used on the wire and in files (the same text serde writes).
    pub fn id(self) -> &'static str {
        match self {
            Feature::SaveIsolation => "save_isolation",
            Feature::ConfigOverride => "config_override",
            Feature::SteamHeadless => "steam_headless",
            Feature::ControllerRemap => "controller_remap",
            Feature::Overlay => "overlay",
            Feature::DiscordRpc => "discord_rpc",
            Feature::Mods => "mods",
            Feature::Diagnostics => "diagnostics",
        }
    }

    /// Parse an identifier from [`Feature::id`]. Anything else is `None`: callers that take
    /// the text from the UI must reject it rather than guess.
    pub fn from_id(id: &str) -> Option<Feature> {
        Feature::ALL.into_iter().find(|f| f.id() == id)
    }

    pub fn label(self) -> &'static str {
        match self {
            Feature::SaveIsolation => "Save isolation",
            Feature::ConfigOverride => "Config override",
            Feature::SteamHeadless => "Steam headless launch",
            Feature::ControllerRemap => "Controller remapping",
            Feature::Overlay => "Console overlay",
            Feature::DiscordRpc => "Discord Rich Presence",
            Feature::Mods => "Mods",
            Feature::Diagnostics => "Diagnostics",
        }
    }

    /// `Some(reason)` when the feature can never apply to this game.
    fn inapplicable_reason(self, game: &InstalledGame) -> Option<&'static str> {
        match self {
            Feature::SteamHeadless if game.source != GameSource::Steam => Some("Steam titles only"),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Compatibility {
    /// Nothing has checked yet. The tick is allowed.
    #[default]
    Unverified,
    Compatible,
    /// Known not to work with this game. The tick is refused.
    Incompatible,
    /// Can never apply to this game. The tick is refused.
    NotApplicable,
}

impl Compatibility {
    pub fn allows_enabling(self) -> bool {
        matches!(self, Self::Unverified | Self::Compatible)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FeatureState {
    /// The user's tick.
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub compatibility: Compatibility,
    /// Why the compatibility is what it is, for display next to the checkbox.
    #[serde(default)]
    pub note: Option<String>,
    /// The evidence-backed label behind a `Compatible` verdict, when someone recorded one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim: Option<Claim>,
}

impl FeatureState {
    /// The capability label for this row, derived so it cannot overclaim:
    ///
    /// * not yet checked -> `NotInvestigated`;
    /// * ruled out, or not applicable -> `Unsupported`;
    /// * compatible with a valid claim -> the claimed label;
    /// * compatible with no claim, or a claim of "Supported and tested" that lacks its test
    ///   date or evidence (for example a hand-edited file) -> `Experimental`.
    pub fn status(&self) -> Status {
        match self.compatibility {
            Compatibility::Unverified => Status::NotInvestigated,
            Compatibility::Incompatible | Compatibility::NotApplicable => Status::Unsupported,
            Compatibility::Compatible => match &self.claim {
                Some(claim)
                    if claim.status.is_available() && claim.validate(String::new).is_ok() =>
                {
                    claim.status
                }
                _ => Status::Experimental,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CompatibilityMatrix {
    features: BTreeMap<Feature, FeatureState>,
}

impl CompatibilityMatrix {
    /// A fresh matrix: nothing ticked, applicability decided from the game itself.
    pub fn for_game(game: &InstalledGame) -> Self {
        let mut matrix = Self::default();
        matrix.reconcile(game);
        matrix
    }

    pub fn get(&self, feature: Feature) -> Option<&FeatureState> {
        self.features.get(&feature)
    }

    /// Rows in [`Feature::ALL`] order, which is the order a UI should list them.
    pub fn iter(&self) -> impl Iterator<Item = (Feature, &FeatureState)> {
        Feature::ALL
            .into_iter()
            .filter_map(|f| self.features.get(&f).map(|state| (f, state)))
    }

    /// Set or clear the tick. Clearing always works; ticking is refused for
    /// features that are not applicable or known to be incompatible.
    pub fn set_enabled(&mut self, feature: Feature, enabled: bool) -> Result<()> {
        let state = self.features.entry(feature).or_default();
        if enabled && !state.compatibility.allows_enabling() {
            return Err(Error::FeatureUnavailable {
                feature: feature.label(),
                reason: match state.compatibility {
                    Compatibility::NotApplicable => "not applicable to this game",
                    _ => "known to be incompatible with this game",
                },
            });
        }
        state.enabled = enabled;
        Ok(())
    }

    /// Record what a probe learned. A verdict that rules the feature out also clears the tick.
    pub fn set_compatibility(
        &mut self,
        feature: Feature,
        compatibility: Compatibility,
        note: Option<String>,
    ) {
        let state = self.features.entry(feature).or_default();
        state.compatibility = compatibility;
        state.note = note;
        if compatibility != Compatibility::Compatible {
            // A claim only describes a working feature; keeping it would contradict the verdict.
            state.claim = None;
        }
        if !compatibility.allows_enabling() {
            state.enabled = false;
        }
    }

    /// Record an evidence-backed label for a feature. The label decides the verdict:
    /// `Unsupported` means incompatible (and clears the tick), `NotInvestigated` means
    /// unverified, anything else means compatible.
    ///
    /// Refused when the claim overclaims ("Supported and tested" needs a test date and
    /// evidence) or when it says the feature works on a row that can never apply to the game.
    pub fn set_status(
        &mut self,
        feature: Feature,
        claim: Claim,
        note: Option<String>,
    ) -> Result<()> {
        claim.validate(|| format!("feature '{}'", feature.label()))?;

        let not_applicable = self
            .features
            .get(&feature)
            .is_some_and(|s| s.compatibility == Compatibility::NotApplicable);
        if not_applicable {
            if claim.status.is_available() {
                return Err(Error::FeatureUnavailable {
                    feature: feature.label(),
                    reason: "not applicable to this game",
                });
            }
            return Ok(()); // already at least as negative as the claim
        }

        let state = self.features.entry(feature).or_default();
        state.compatibility = match claim.status {
            Status::Unsupported => Compatibility::Incompatible,
            Status::NotInvestigated => Compatibility::Unverified,
            _ => Compatibility::Compatible,
        };
        state.note = note;
        if !state.compatibility.allows_enabling() {
            state.enabled = false;
        }
        state.claim = (claim.status != Status::NotInvestigated).then_some(claim);
        Ok(())
    }

    /// The game's headline label: the weakest label among the rows that can apply to it
    /// ("weakest status wins", so a summary never overclaims). Rows that can never apply
    /// (Steam-only features on a GOG game) are not failures and are left out.
    pub fn headline(&self) -> Status {
        Status::weakest(
            self.iter()
                .filter(|(_, s)| s.compatibility != Compatibility::NotApplicable)
                .map(|(_, s)| s.status()),
        )
    }

    /// The matrix as capability-matrix rows (feature, game, OS, adapter version, status, test
    /// date, evidence, limitations), in [`Feature::ALL`] order.
    pub fn capability_entries(
        &self,
        game: &str,
        os: &str,
        adapter_version: &str,
    ) -> Vec<CapabilityEntry> {
        self.iter()
            .map(|(feature, state)| {
                let claim = state.claim.as_ref();
                let status = state.status();
                // Only quote evidence for a label that is actually being shown.
                let backs_status = claim.filter(|c| c.status == status);
                let mut limitations: Vec<String> = state.note.iter().cloned().collect();
                if let Some(c) = claim {
                    limitations.extend(c.limitations.iter().cloned());
                }
                CapabilityEntry {
                    feature: feature.label().to_owned(),
                    game: game.to_owned(),
                    build: "*".to_owned(),
                    os: os.to_owned(),
                    adapter_version: adapter_version.to_owned(),
                    status,
                    test_date: backs_status.and_then(|c| c.test_date.clone()),
                    evidence: backs_status.and_then(|c| c.evidence.clone()),
                    limitations,
                }
            })
            .collect()
    }

    /// Bring the matrix in line with the game after a rescan or an upgrade: add rows
    /// for new features and refresh applicability, keeping every tick and every
    /// probe verdict that is still valid.
    pub fn reconcile(&mut self, game: &InstalledGame) {
        for feature in Feature::ALL {
            let state = self.features.entry(feature).or_default();
            match feature.inapplicable_reason(game) {
                Some(reason) => {
                    state.compatibility = Compatibility::NotApplicable;
                    state.note = Some(reason.to_owned());
                    state.claim = None;
                    state.enabled = false;
                }
                None if state.compatibility == Compatibility::NotApplicable => {
                    // It used to be inapplicable (for example the game moved from a
                    // generic entry to Steam), so start over from "unverified".
                    state.compatibility = Compatibility::Unverified;
                    state.note = None;
                }
                None => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn game(source: GameSource) -> InstalledGame {
        InstalledGame {
            id: "g-1".into(),
            name: "Game".into(),
            install_dir: PathBuf::from(r"D:\Games\G"),
            source,
            store_id: None,
            publisher: None,
            steam: None,
        }
    }

    #[test]
    fn fresh_matrix_has_every_feature_unticked() {
        let m = CompatibilityMatrix::for_game(&game(GameSource::Steam));
        assert_eq!(m.iter().count(), Feature::ALL.len());
        for (feature, state) in m.iter() {
            assert!(!state.enabled, "{feature:?}");
            assert_eq!(
                state.compatibility,
                Compatibility::Unverified,
                "{feature:?}"
            );
        }
    }

    #[test]
    fn steam_headless_applies_to_steam_games_only() {
        let steam = CompatibilityMatrix::for_game(&game(GameSource::Steam));
        assert_eq!(
            steam.get(Feature::SteamHeadless).unwrap().compatibility,
            Compatibility::Unverified
        );

        let gog = CompatibilityMatrix::for_game(&game(GameSource::Gog));
        let row = gog.get(Feature::SteamHeadless).unwrap();
        assert_eq!(row.compatibility, Compatibility::NotApplicable);
        assert_eq!(row.note.as_deref(), Some("Steam titles only"));
    }

    #[test]
    fn ticking_is_refused_for_ruled_out_features_but_unticking_never_is() {
        let mut m = CompatibilityMatrix::for_game(&game(GameSource::Gog));
        let err = m.set_enabled(Feature::SteamHeadless, true).unwrap_err();
        assert!(matches!(
            err,
            Error::FeatureUnavailable {
                reason: "not applicable to this game",
                ..
            }
        ));
        m.set_enabled(Feature::SteamHeadless, false).unwrap();

        m.set_enabled(Feature::Overlay, true).unwrap();
        assert!(m.get(Feature::Overlay).unwrap().enabled);

        m.set_compatibility(
            Feature::Overlay,
            Compatibility::Incompatible,
            Some("anti-cheat".into()),
        );
        let row = m.get(Feature::Overlay).unwrap();
        assert!(!row.enabled, "an incompatible verdict clears the tick");
        assert!(matches!(
            m.set_enabled(Feature::Overlay, true),
            Err(Error::FeatureUnavailable { .. })
        ));
    }

    #[test]
    fn reconcile_keeps_ticks_and_verdicts() {
        let mut m = CompatibilityMatrix::for_game(&game(GameSource::Steam));
        m.set_enabled(Feature::SaveIsolation, true).unwrap();
        m.set_compatibility(Feature::Mods, Compatibility::Compatible, None);

        m.reconcile(&game(GameSource::Steam));
        assert!(m.get(Feature::SaveIsolation).unwrap().enabled);
        assert_eq!(
            m.get(Feature::Mods).unwrap().compatibility,
            Compatibility::Compatible
        );
    }

    #[test]
    fn reconcile_follows_the_game_changing_source() {
        let mut m = CompatibilityMatrix::for_game(&game(GameSource::Registry));
        assert_eq!(
            m.get(Feature::SteamHeadless).unwrap().compatibility,
            Compatibility::NotApplicable
        );

        m.reconcile(&game(GameSource::Steam));
        let row = m.get(Feature::SteamHeadless).unwrap();
        assert_eq!(row.compatibility, Compatibility::Unverified);
        assert_eq!(row.note, None);

        m.set_enabled(Feature::SteamHeadless, true).unwrap();
        m.reconcile(&game(GameSource::Registry));
        assert!(
            !m.get(Feature::SteamHeadless).unwrap().enabled,
            "tick cleared when it stops applying"
        );
    }

    #[test]
    fn reconcile_adds_rows_missing_from_an_older_file() {
        let mut m: CompatibilityMatrix =
            serde_json::from_str(r#"{"features":{"overlay":{"enabled":true}}}"#).unwrap();
        assert_eq!(m.iter().count(), 1);

        m.reconcile(&game(GameSource::Steam));
        assert_eq!(m.iter().count(), Feature::ALL.len());
        assert!(m.get(Feature::Overlay).unwrap().enabled);
    }

    #[test]
    fn json_round_trip() {
        let mut m = CompatibilityMatrix::for_game(&game(GameSource::Steam));
        m.set_enabled(Feature::DiscordRpc, true).unwrap();
        let text = serde_json::to_string(&m).unwrap();
        assert!(text.contains("\"discord_rpc\""));
        assert_eq!(
            serde_json::from_str::<CompatibilityMatrix>(&text).unwrap(),
            m
        );
    }

    // ---- the five capability labels layered on the matrix ----

    #[test]
    fn feature_ids_match_what_serde_writes_and_round_trip() {
        for feature in Feature::ALL {
            let written = serde_json::to_value(feature).unwrap();
            assert_eq!(written, feature.id(), "{feature:?}");
            assert_eq!(Feature::from_id(feature.id()), Some(feature));
        }
        // Not a lookup by label, by case, or by a near miss.
        for bad in [
            "",
            "Save isolation",
            "SAVE_ISOLATION",
            "save-isolation",
            "mod",
            " mods",
        ] {
            assert_eq!(Feature::from_id(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn labels_are_derived_from_the_verdict() {
        let m = CompatibilityMatrix::for_game(&game(GameSource::Gog));
        assert_eq!(
            m.get(Feature::Overlay).unwrap().status(),
            Status::NotInvestigated,
            "nothing has checked yet"
        );
        assert_eq!(
            m.get(Feature::SteamHeadless).unwrap().status(),
            Status::Unsupported,
            "not applicable reads as unsupported"
        );
    }

    #[test]
    fn a_compatible_verdict_without_a_claim_is_only_experimental() {
        let mut m = CompatibilityMatrix::for_game(&game(GameSource::Steam));
        m.set_compatibility(Feature::Mods, Compatibility::Compatible, None);
        assert_eq!(m.get(Feature::Mods).unwrap().status(), Status::Experimental);
    }

    #[test]
    fn set_status_records_a_claim_and_sets_the_verdict() {
        let mut m = CompatibilityMatrix::for_game(&game(GameSource::Steam));

        let mut partial = Claim::new(Status::Partial);
        partial.limitations.push("windowed mode only".into());
        m.set_status(Feature::Overlay, partial, None).unwrap();
        let row = m.get(Feature::Overlay).unwrap();
        assert_eq!(row.compatibility, Compatibility::Compatible);
        assert_eq!(row.status(), Status::Partial);

        m.set_enabled(Feature::Overlay, true).unwrap();
        m.set_status(
            Feature::Overlay,
            Claim::new(Status::Unsupported),
            Some("anti-cheat blocks hooks".into()),
        )
        .unwrap();
        let row = m.get(Feature::Overlay).unwrap();
        assert_eq!(row.compatibility, Compatibility::Incompatible);
        assert!(!row.enabled, "an unsupported label clears the tick");
        assert_eq!(row.status(), Status::Unsupported);

        m.set_status(Feature::Overlay, Claim::new(Status::NotInvestigated), None)
            .unwrap();
        let row = m.get(Feature::Overlay).unwrap();
        assert_eq!(row.compatibility, Compatibility::Unverified);
        assert!(row.claim.is_none());
    }

    #[test]
    fn supported_and_tested_needs_evidence_to_be_recorded() {
        let mut m = CompatibilityMatrix::for_game(&game(GameSource::Steam));
        let err = m
            .set_status(
                Feature::SaveIsolation,
                Claim::new(Status::SupportedTested),
                None,
            )
            .unwrap_err();
        assert!(matches!(err, Error::Invalid(_)));
        assert_eq!(
            m.get(Feature::SaveIsolation).unwrap().status(),
            Status::NotInvestigated,
            "a refused claim changes nothing"
        );

        m.set_status(
            Feature::SaveIsolation,
            Claim::tested("2026-10-25", "docs/capability-matrix/README.md"),
            None,
        )
        .unwrap();
        assert_eq!(
            m.get(Feature::SaveIsolation).unwrap().status(),
            Status::SupportedTested
        );
    }

    #[test]
    fn a_hand_edited_tested_claim_without_evidence_is_shown_as_experimental() {
        let mut m: CompatibilityMatrix = serde_json::from_str(
            r#"{"features":{"mods":{"compatibility":"compatible","claim":{"status":"supported_tested"}}}}"#,
        )
        .unwrap();
        assert_eq!(m.get(Feature::Mods).unwrap().status(), Status::Experimental);

        m.reconcile(&game(GameSource::Steam));
        let entries = m.capability_entries("Game", "Windows", "test");
        assert!(entries.iter().all(|e| e.validate().is_ok()));
    }

    #[test]
    fn claims_cannot_say_a_never_applicable_feature_works() {
        let mut m = CompatibilityMatrix::for_game(&game(GameSource::Gog));
        let err = m
            .set_status(
                Feature::SteamHeadless,
                Claim::new(Status::Experimental),
                None,
            )
            .unwrap_err();
        assert!(matches!(err, Error::FeatureUnavailable { .. }));
        assert_eq!(
            m.get(Feature::SteamHeadless).unwrap().compatibility,
            Compatibility::NotApplicable
        );
    }

    #[test]
    fn a_new_verdict_drops_a_claim_that_no_longer_fits() {
        let mut m = CompatibilityMatrix::for_game(&game(GameSource::Steam));
        m.set_status(Feature::Mods, Claim::new(Status::Partial), None)
            .unwrap();
        m.set_compatibility(Feature::Mods, Compatibility::Incompatible, None);
        assert!(m.get(Feature::Mods).unwrap().claim.is_none());
    }

    #[test]
    fn headline_is_the_weakest_applicable_label() {
        let mut m = CompatibilityMatrix::for_game(&game(GameSource::Gog));
        assert_eq!(
            m.headline(),
            Status::NotInvestigated,
            "the not-applicable Steam row is not a failure"
        );

        for feature in Feature::ALL {
            if feature != Feature::SteamHeadless {
                m.set_status(feature, Claim::tested("2026-10-25", "evidence"), None)
                    .unwrap();
            }
        }
        assert_eq!(m.headline(), Status::SupportedTested);

        m.set_status(Feature::Overlay, Claim::new(Status::Experimental), None)
            .unwrap();
        assert_eq!(m.headline(), Status::Experimental);

        m.set_status(Feature::Mods, Claim::new(Status::Unsupported), None)
            .unwrap();
        assert_eq!(m.headline(), Status::Unsupported);
    }

    #[test]
    fn exports_capability_rows_that_pass_the_evidence_rule() {
        let mut m = CompatibilityMatrix::for_game(&game(GameSource::Steam));
        let mut claim = Claim::tested("2026-10-25", "logs/run1.jsonl");
        claim.limitations.push("one machine".into());
        m.set_status(Feature::SaveIsolation, claim, None).unwrap();

        let rows = m.capability_entries("Game", "Windows", "sul-core 0.1.0");
        assert_eq!(rows.len(), Feature::ALL.len());
        assert!(rows.iter().all(|r| r.validate().is_ok()));

        let saves = rows.iter().find(|r| r.feature == "Save isolation").unwrap();
        assert_eq!(saves.status, Status::SupportedTested);
        assert_eq!(saves.test_date.as_deref(), Some("2026-10-25"));
        assert_eq!(saves.evidence.as_deref(), Some("logs/run1.jsonl"));
        assert_eq!(saves.limitations, ["one machine"]);
        assert_eq!(saves.os, "Windows");

        let overlay = rows
            .iter()
            .find(|r| r.feature == "Console overlay")
            .unwrap();
        assert_eq!(overlay.status, Status::NotInvestigated);
        assert!(overlay.test_date.is_none() && overlay.evidence.is_none());
    }

    #[test]
    fn files_written_before_claims_existed_still_load() {
        let m: CompatibilityMatrix = serde_json::from_str(
            r#"{"features":{"overlay":{"enabled":true,"compatibility":"compatible","note":null}}}"#,
        )
        .unwrap();
        assert!(m.get(Feature::Overlay).unwrap().claim.is_none());
        let text = serde_json::to_string(&m).unwrap();
        assert!(!text.contains("claim"), "absent claims are not written");
    }
}

#[cfg(test)]
mod enabling_tests {
    use super::*;

    #[test]
    fn only_unverified_and_compatible_features_may_be_ticked() {
        assert!(Compatibility::Unverified.allows_enabling());
        assert!(Compatibility::Compatible.allows_enabling());
        assert!(!Compatibility::Incompatible.allows_enabling());
        assert!(!Compatibility::NotApplicable.allows_enabling());
    }

    #[test]
    fn a_new_feature_is_unverified_and_therefore_allowed() {
        assert_eq!(Compatibility::default(), Compatibility::Unverified);
        assert!(Compatibility::default().allows_enabling());
    }
}
