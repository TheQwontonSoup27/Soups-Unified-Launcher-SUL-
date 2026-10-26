//! Capability labels. Project rule: never describe an untested feature as working.
//!
//! Every capability carries one of five [`Status`] labels. "Supported and tested" is only
//! allowed with a test date and a pointer to evidence ([`check_evidence`]).
//!
//! The per-game tick-and-compatibility matrix ([`crate::matrix`]) stores a coarse
//! [`Compatibility`](crate::matrix::Compatibility) verdict per feature; the five labels are
//! layered on top of it through [`Claim`]. A row's label is always *derived* from the verdict
//! plus its claim, so the two cannot disagree: see
//! [`FeatureState::status`](crate::matrix::FeatureState::status).

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    SupportedTested,
    Partial,
    Experimental,
    Unsupported,
    NotInvestigated,
}

impl Status {
    /// Strongest first: the order a legend should list them in.
    pub const ALL: [Status; 5] = [
        Status::SupportedTested,
        Status::Partial,
        Status::Experimental,
        Status::Unsupported,
        Status::NotInvestigated,
    ];

    /// The wording used everywhere in the UI and documentation.
    pub fn label(self) -> &'static str {
        match self {
            Status::SupportedTested => "Supported and tested",
            Status::Partial => "Partially supported",
            Status::Experimental => "Experimental",
            Status::Unsupported => "Unsupported",
            Status::NotInvestigated => "Not investigated",
        }
    }

    /// One-line meaning, for tooltips and legends.
    pub fn help(self) -> &'static str {
        match self {
            Status::SupportedTested => "Test evidence exists for the stated environment/build.",
            Status::Partial => "A defined subset works; limitations are listed.",
            Status::Experimental => "Implemented or under test; reliability not established.",
            Status::Unsupported => "Known not to work or intentionally not provided.",
            Status::NotInvestigated => "No adequate feasibility assessment yet.",
        }
    }

    /// `true` when the label says the feature is implemented to some degree (so it may be
    /// switched on, and presence may be published for the game).
    pub fn is_available(self) -> bool {
        matches!(
            self,
            Status::SupportedTested | Status::Partial | Status::Experimental
        )
    }

    /// Lower is weaker. `Unsupported` is weaker than `NotInvestigated`: a known "no" is a
    /// stronger negative than an unanswered question.
    fn strength(self) -> u8 {
        match self {
            Status::Unsupported => 0,
            Status::NotInvestigated => 1,
            Status::Experimental => 2,
            Status::Partial => 3,
            Status::SupportedTested => 4,
        }
    }

    /// The headline rule: a set of capabilities is only as good as its weakest member, so a
    /// summary can never overclaim. An empty set is `NotInvestigated`.
    pub fn weakest(statuses: impl IntoIterator<Item = Status>) -> Status {
        statuses
            .into_iter()
            .min_by_key(|s| s.strength())
            .unwrap_or(Status::NotInvestigated)
    }
}

fn has_text(value: &Option<String>) -> bool {
    value.as_deref().is_some_and(|s| !s.trim().is_empty())
}

/// The truthfulness rule: "Supported and tested" needs a test date and evidence.
pub fn check_evidence(
    status: Status,
    test_date: &Option<String>,
    evidence: &Option<String>,
    what: impl FnOnce() -> String,
) -> Result<()> {
    if status == Status::SupportedTested && !(has_text(test_date) && has_text(evidence)) {
        return Err(Error::Invalid(format!(
            "{} claims SupportedTested without test_date and evidence",
            what()
        )));
    }
    Ok(())
}

/// A status together with what backs it. This is what the matrix stores per feature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    pub status: Status,
    /// ISO date (YYYY-MM-DD) of the test that backs `status`.
    #[serde(default)]
    pub test_date: Option<String>,
    /// Pointer to evidence (log path, doc, issue). Required for `SupportedTested`.
    #[serde(default)]
    pub evidence: Option<String>,
    #[serde(default)]
    pub limitations: Vec<String>,
}

impl Claim {
    /// A claim that needs no evidence (anything below `SupportedTested`).
    pub fn new(status: Status) -> Self {
        Self {
            status,
            test_date: None,
            evidence: None,
            limitations: Vec::new(),
        }
    }

    pub fn tested(test_date: &str, evidence: &str) -> Self {
        Self {
            status: Status::SupportedTested,
            test_date: Some(test_date.to_owned()),
            evidence: Some(evidence.to_owned()),
            limitations: Vec::new(),
        }
    }

    pub fn validate(&self, what: impl FnOnce() -> String) -> Result<()> {
        check_evidence(self.status, &self.test_date, &self.evidence, what)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityEntry {
    pub feature: String,
    pub game: String,
    pub build: String,
    pub os: String,
    pub adapter_version: String,
    pub status: Status,
    /// ISO date (YYYY-MM-DD) of the test that backs `status`.
    pub test_date: Option<String>,
    /// Pointer to evidence (log path, doc, issue). Required for `SupportedTested`.
    pub evidence: Option<String>,
    pub limitations: Vec<String>,
}

impl CapabilityEntry {
    /// Untested entries default to `NotInvestigated`; callers must opt in to stronger claims.
    pub fn not_investigated(feature: &str, game: &str) -> Self {
        Self {
            feature: feature.into(),
            game: game.into(),
            build: "*".into(),
            os: "*".into(),
            adapter_version: "0".into(),
            status: Status::NotInvestigated,
            test_date: None,
            evidence: None,
            limitations: vec![],
        }
    }

    /// Enforces the truthfulness rule: "Supported and tested" needs a date and evidence.
    pub fn validate(&self) -> Result<()> {
        check_evidence(self.status, &self.test_date, &self.evidence, || {
            format!("capability '{}' for '{}'", self.feature, self.game)
        })
    }
}

/// Implemented by each game/emulator/third-party adapter.
pub trait Adapter {
    fn id(&self) -> &str;
    fn version(&self) -> &str;
    fn capabilities(&self) -> Vec<CapabilityEntry>;
}

/// Collects capabilities from adapters, rejecting any entry that overclaims.
pub fn collect(adapters: &[&dyn Adapter]) -> Result<Vec<CapabilityEntry>> {
    let mut out = Vec::new();
    for a in adapters {
        for c in a.capabilities() {
            c.validate()?;
            out.push(c);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_use_the_spec_vocabulary() {
        let labels: Vec<_> = Status::ALL.iter().map(|s| s.label()).collect();
        assert_eq!(
            labels,
            [
                "Supported and tested",
                "Partially supported",
                "Experimental",
                "Unsupported",
                "Not investigated"
            ]
        );
    }

    #[test]
    fn weakest_status_wins_and_empty_means_not_investigated() {
        use Status::*;
        assert_eq!(Status::weakest([SupportedTested, Partial]), Partial);
        assert_eq!(
            Status::weakest([SupportedTested, Experimental, Partial]),
            Experimental
        );
        assert_eq!(
            Status::weakest([Experimental, NotInvestigated]),
            NotInvestigated
        );
        assert_eq!(Status::weakest([NotInvestigated, Unsupported]), Unsupported);
        assert_eq!(Status::weakest([SupportedTested]), SupportedTested);
        assert_eq!(Status::weakest([]), NotInvestigated);
    }

    #[test]
    fn availability() {
        assert!(Status::SupportedTested.is_available());
        assert!(Status::Partial.is_available());
        assert!(Status::Experimental.is_available());
        assert!(!Status::Unsupported.is_available());
        assert!(!Status::NotInvestigated.is_available());
    }

    #[test]
    fn claims_follow_the_evidence_rule() {
        assert!(Claim::new(Status::Partial).validate(|| "x".into()).is_ok());
        assert!(
            Claim::new(Status::SupportedTested)
                .validate(|| "x".into())
                .is_err()
        );
        assert!(
            Claim::tested("2026-10-25", "docs/evidence.md")
                .validate(|| "x".into())
                .is_ok()
        );
        let blank = Claim::tested("  ", "docs/evidence.md");
        assert!(blank.validate(|| "x".into()).is_err());
    }
}
