//! Baseline tracking & version self-updater (spec section 1).
//!
//! This module owns the *version log*: which launcher versions were installed
//! and which of them were explicitly marked stable. That log is what makes
//! "roll back to a known-good version" possible. Downloading and applying
//! updates is a separate concern and is not built yet.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionRecord {
    pub version: String,
    /// Set only by an explicit [`VersionLog::mark_stable`] call.
    pub stable: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionLog {
    pub current: Option<String>,
    /// In order of first installation, oldest first.
    pub history: Vec<VersionRecord>,
}

impl VersionLog {
    /// Load the log, or start an empty one if the file does not exist yet.
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| Error::json(path, e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(Error::io(path, e)),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| Error::json(path, e))?;
        fs::write(path, text).map_err(|e| Error::io(path, e))
    }

    /// Record that `version` is now the running version.
    pub fn record_installed(&mut self, version: &str) {
        if !self.history.iter().any(|r| r.version == version) {
            self.history.push(VersionRecord {
                version: version.to_owned(),
                stable: false,
            });
        }
        self.current = Some(version.to_owned());
    }

    /// Flag a known version as stable. Returns `false` if it was never installed.
    pub fn mark_stable(&mut self, version: &str) -> bool {
        match self.history.iter_mut().find(|r| r.version == version) {
            Some(record) => {
                record.stable = true;
                true
            }
            None => false,
        }
    }

    /// Most recently installed stable version other than the current one.
    pub fn rollback_target(&self) -> Option<&str> {
        self.history
            .iter()
            .rev()
            .find(|r| r.stable && Some(r.version.as_str()) != self.current.as_deref())
            .map(|r| r.version.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rollback_picks_latest_stable_that_is_not_current() {
        let mut log = VersionLog::default();
        for v in ["0.1.0", "0.2.0", "0.3.0"] {
            log.record_installed(v);
        }
        assert_eq!(log.rollback_target(), None, "nothing is marked stable yet");

        assert!(log.mark_stable("0.1.0"));
        assert!(log.mark_stable("0.2.0"));
        assert_eq!(log.current.as_deref(), Some("0.3.0"));
        assert_eq!(log.rollback_target(), Some("0.2.0"));
    }

    #[test]
    fn current_version_is_never_its_own_rollback_target() {
        let mut log = VersionLog::default();
        log.record_installed("0.1.0");
        log.mark_stable("0.1.0");
        assert_eq!(log.rollback_target(), None);
    }

    #[test]
    fn unknown_versions_cannot_be_marked_stable() {
        let mut log = VersionLog::default();
        assert!(!log.mark_stable("9.9.9"));
    }

    #[test]
    fn reinstalling_does_not_duplicate_history() {
        let mut log = VersionLog::default();
        log.record_installed("0.1.0");
        log.record_installed("0.2.0");
        log.record_installed("0.1.0");
        assert_eq!(log.history.len(), 2);
        assert_eq!(log.current.as_deref(), Some("0.1.0"));
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state/versions.json");

        assert_eq!(VersionLog::load(&path).unwrap(), VersionLog::default());

        let mut log = VersionLog::default();
        log.record_installed("0.1.0");
        log.mark_stable("0.1.0");
        log.save(&path).unwrap();
        assert_eq!(VersionLog::load(&path).unwrap(), log);
    }
}
