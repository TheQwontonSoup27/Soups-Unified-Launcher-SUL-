//! Engine portability matrix (spec section 1).
//!
//! The launcher runs either as a standard install or as a self-contained portable
//! copy. Portable mode is signalled by a [`PORTABLE_MARKER`] file in the app root.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// Root file token that switches the launcher into portable mode.
pub const PORTABLE_MARKER: &str = "portable.marker";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallMode {
    Standard,
    Portable,
}

/// Decide the install mode from the presence of the marker file in `app_root`.
pub fn detect_mode(app_root: &Path) -> InstallMode {
    if app_root.join(PORTABLE_MARKER).is_file() {
        InstallMode::Portable
    } else {
        InstallMode::Standard
    }
}

/// The four options offered by the post-install toggle on an existing install.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortabilityAction {
    /// 1. Clone the layout configuration along with the current application templates.
    CloneLayoutWithTemplates,
    /// 2. Instantiate a fresh, clean portable save state.
    FreshPortableState,
    /// 3. Delete the original host installation paths and migrate template data over.
    MigrateTemplatesAndDeleteOriginal,
    /// 4. Delete the original installation paths completely; templates are NOT moved.
    DeleteOriginalWithoutTemplates,
}

impl PortabilityAction {
    pub const ALL: [Self; 4] = [
        Self::CloneLayoutWithTemplates,
        Self::FreshPortableState,
        Self::MigrateTemplatesAndDeleteOriginal,
        Self::DeleteOriginalWithoutTemplates,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::CloneLayoutWithTemplates => "Clone layout configuration with current templates",
            Self::FreshPortableState => "Create a fresh, clean portable save state",
            Self::MigrateTemplatesAndDeleteOriginal => {
                "Delete the original install paths and migrate templates"
            }
            Self::DeleteOriginalWithoutTemplates => {
                "Delete the original install paths completely (templates are not moved)"
            }
        }
    }

    /// Whether the action removes the original installation. The UI should
    /// require explicit confirmation for these.
    pub fn is_destructive(self) -> bool {
        matches!(
            self,
            Self::MigrateTemplatesAndDeleteOriginal | Self::DeleteOriginalWithoutTemplates
        )
    }

    /// Whether template data is carried into the new portable location.
    pub fn carries_templates(self) -> bool {
        matches!(
            self,
            Self::CloneLayoutWithTemplates | Self::MigrateTemplatesAndDeleteOriginal
        )
    }
}

/// Apply a portability action, moving from the `source` install to the `target` root.
///
/// Not built yet. Two of the four actions delete user data, so this lands together
/// with a dry-run/plan step and confirmation flow rather than as a bare function.
pub fn apply(_action: PortabilityAction, _source: &Path, _target: &Path) -> Result<()> {
    Err(Error::NotImplemented("portability::apply"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_switches_mode() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(detect_mode(dir.path()), InstallMode::Standard);

        std::fs::write(dir.path().join(PORTABLE_MARKER), b"").unwrap();
        assert_eq!(detect_mode(dir.path()), InstallMode::Portable);
    }

    #[test]
    fn marker_must_be_a_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(PORTABLE_MARKER)).unwrap();
        assert_eq!(detect_mode(dir.path()), InstallMode::Standard);
    }

    #[test]
    fn action_matrix_matches_spec() {
        use PortabilityAction::*;
        assert_eq!(PortabilityAction::ALL.len(), 4);

        let destructive: Vec<_> = PortabilityAction::ALL
            .into_iter()
            .filter(|a| a.is_destructive())
            .collect();
        assert_eq!(
            destructive,
            [
                MigrateTemplatesAndDeleteOriginal,
                DeleteOriginalWithoutTemplates
            ]
        );

        assert!(CloneLayoutWithTemplates.carries_templates());
        assert!(MigrateTemplatesAndDeleteOriginal.carries_templates());
        assert!(!FreshPortableState.carries_templates());
        assert!(!DeleteOriginalWithoutTemplates.carries_templates());
    }

    #[test]
    fn apply_is_a_stub() {
        let err = apply(
            PortabilityAction::FreshPortableState,
            Path::new("a"),
            Path::new("b"),
        )
        .unwrap_err();
        assert!(matches!(err, Error::NotImplemented(_)));
    }
}
