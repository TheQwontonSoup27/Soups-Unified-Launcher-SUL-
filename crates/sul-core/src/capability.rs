//! Capability matrix model. Spec rule: never describe an untested feature as working.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    SupportedTested,
    Partial,
    Experimental,
    Unsupported,
    NotInvestigated,
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
    /// Pointer to evidence (log path, doc, issue). Required for SupportedTested.
    pub evidence: Option<String>,
    pub limitations: Vec<String>,
}

impl CapabilityEntry {
    /// Untested entries default to NotInvestigated; callers must opt in to stronger claims.
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
        if self.status == Status::SupportedTested {
            let has = |o: &Option<String>| o.as_deref().map_or(false, |s| !s.trim().is_empty());
            if !has(&self.test_date) || !has(&self.evidence) {
                return Err(Error::Invalid(format!(
                    "capability '{}' for '{}' claims SupportedTested without test_date and evidence",
                    self.feature, self.game
                )));
            }
        }
        Ok(())
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
