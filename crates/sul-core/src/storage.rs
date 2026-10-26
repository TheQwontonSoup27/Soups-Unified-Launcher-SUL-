//! Storage isolator (spec section 1).
//!
//! Every game gets its own sandbox directory under a single root so that
//! intercepted config files and save profiles never touch the host locations.
//! This module only decides *where* things live; the override/interception rules
//! build on top of it.

use std::path::{Path, PathBuf};

use crate::names::is_reserved_windows_name;
use crate::{Error, Result};

const MAX_ID_LEN: usize = 128;

/// Directory layout for sandboxed game data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxLayout {
    root: PathBuf,
}

impl SandboxLayout {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Conventional location: `<app_root>/sandbox`.
    pub fn under_app_root(app_root: &Path) -> Self {
        Self::new(app_root.join("sandbox"))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Per-game sandbox directory. Rejects ids that could escape the root.
    pub fn game_dir(&self, game_id: &str) -> Result<PathBuf> {
        validate_game_id(game_id)?;
        Ok(self.root.join(game_id))
    }

    pub fn saves_dir(&self, game_id: &str) -> Result<PathBuf> {
        Ok(self.game_dir(game_id)?.join("saves"))
    }

    pub fn config_dir(&self, game_id: &str) -> Result<PathBuf> {
        Ok(self.game_dir(game_id)?.join("config"))
    }
}

/// Ids become directory names, so they must be plain, portable path segments.
pub fn validate_game_id(id: &str) -> Result<()> {
    let ok = !id.is_empty()
        && id.len() <= MAX_ID_LEN
        && !id.starts_with('.')
        && !id.ends_with('.')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !is_reserved_windows_name(id);

    if ok {
        Ok(())
    } else {
        Err(Error::InvalidGameId(id.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_per_game_paths() {
        let layout = SandboxLayout::new("/data/sandbox");
        assert_eq!(
            layout.saves_dir("steam-359320").unwrap(),
            Path::new("/data/sandbox/steam-359320/saves")
        );
        assert_eq!(
            layout.config_dir("elite_dangerous").unwrap(),
            Path::new("/data/sandbox/elite_dangerous/config")
        );
    }

    #[test]
    fn rejects_unsafe_ids() {
        let layout = SandboxLayout::new("/data/sandbox");
        let long = "a".repeat(MAX_ID_LEN + 1);
        for bad in [
            "",
            "..",
            ".hidden",
            "a/b",
            "a\\b",
            "C:evil",
            "../up",
            "trailing.",
            "spa ce",
            "nul",
            "COM1",
            &long,
        ] {
            assert!(
                matches!(layout.game_dir(bad), Err(Error::InvalidGameId(_))),
                "{bad:?} should be rejected"
            );
        }
    }

    #[test]
    fn app_root_convention() {
        let layout = SandboxLayout::under_app_root(Path::new("/app"));
        assert_eq!(layout.root(), Path::new("/app/sandbox"));
    }
}
