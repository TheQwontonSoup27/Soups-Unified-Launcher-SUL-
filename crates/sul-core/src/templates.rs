//! Template serialization engine (spec section 1).
//!
//! Templates are named `.json` files exported to / imported from a user-chosen
//! directory (default: `<app_root>/templates`). A template is a bag of named
//! sections (for example `layout`, `overlay.hud`, `controller`), which lets the
//! import tool offer per-section checkboxes (all ticked by default).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::names::is_reserved_windows_name;
use crate::{Error, Result};

/// Schema version written by this build.
pub const CURRENT_SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Template {
    pub schema: u32,
    pub name: String,
    #[serde(default)]
    pub sections: BTreeMap<String, serde_json::Value>,
}

/// Which sections to take on import. The default selects everything.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportFilter {
    /// `None` = all sections; `Some(set)` = only the listed ones.
    only: Option<BTreeSet<String>>,
}

impl ImportFilter {
    pub fn all() -> Self {
        Self::default()
    }

    pub fn only<I, S>(sections: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            only: Some(sections.into_iter().map(Into::into).collect()),
        }
    }

    pub fn allows(&self, section: &str) -> bool {
        self.only.as_ref().is_none_or(|set| set.contains(section))
    }
}

/// Default export/import directory inside the app folder tree.
pub fn default_template_dir(app_root: &Path) -> PathBuf {
    app_root.join("templates")
}

impl Template {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            schema: CURRENT_SCHEMA,
            name: name.into(),
            sections: BTreeMap::new(),
        }
    }

    /// Copy of this template containing only the sections the filter allows.
    pub fn filtered(&self, filter: &ImportFilter) -> Self {
        Self {
            schema: self.schema,
            name: self.name.clone(),
            sections: self
                .sections
                .iter()
                .filter(|(k, _)| filter.allows(k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        }
    }

    /// Write `<dir>/<name>.json`, creating `dir` if needed. Returns the file path.
    pub fn export(&self, dir: &Path) -> Result<PathBuf> {
        fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        let path = dir.join(format!("{}.json", file_stem(&self.name)));
        let text = serde_json::to_string_pretty(self).map_err(|e| Error::json(&path, e))?;
        fs::write(&path, text).map_err(|e| Error::io(&path, e))?;
        Ok(path)
    }

    /// Read a template file, refusing ones written by a newer schema.
    pub fn import(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
        let template: Self = serde_json::from_str(&text).map_err(|e| Error::json(path, e))?;
        if template.schema > CURRENT_SCHEMA {
            return Err(Error::TemplateTooNew {
                found: template.schema,
                supported: CURRENT_SCHEMA,
            });
        }
        Ok(template)
    }
}

/// User-chosen names become file names; replace anything Windows would reject.
fn file_stem(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches(|c| c == '.' || c == ' ').to_owned();

    if cleaned.is_empty() {
        "template".to_owned()
    } else if is_reserved_windows_name(&cleaned) {
        format!("_{cleaned}")
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Template {
        let mut t = Template::new("Elite Dangerous");
        t.sections
            .insert("controller".into(), json!({ "preset": "switch-pro" }));
        t.sections
            .insert("overlay.hud".into(), json!({ "fps": true }));
        t.sections
            .insert("layout".into(), json!({ "sidebar": "left" }));
        t
    }

    #[test]
    fn export_import_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = sample().export(&dir.path().join("nested/out")).unwrap();
        assert_eq!(path.file_name().unwrap(), "Elite Dangerous.json");
        assert_eq!(Template::import(&path).unwrap(), sample());
    }

    #[test]
    fn default_filter_keeps_everything() {
        assert_eq!(sample().filtered(&ImportFilter::all()), sample());
    }

    #[test]
    fn filter_selects_sections() {
        let kept = sample().filtered(&ImportFilter::only(["controller", "layout"]));
        let keys: Vec<_> = kept.sections.keys().map(String::as_str).collect();
        assert_eq!(keys, ["controller", "layout"]);
    }

    #[test]
    fn unsafe_names_are_sanitised() {
        assert_eq!(file_stem("my/evil:name?"), "my_evil_name_");
        assert_eq!(file_stem("  "), "template");
        assert_eq!(file_stem("..."), "template");
        assert_eq!(file_stem("CON"), "_CON");
    }

    #[test]
    fn newer_schema_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("future.json");
        fs::write(&path, r#"{"schema": 99, "name": "x", "sections": {}}"#).unwrap();
        assert!(matches!(
            Template::import(&path),
            Err(Error::TemplateTooNew { found: 99, .. })
        ));
    }

    #[test]
    fn missing_sections_default_to_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("min.json");
        fs::write(&path, r#"{"schema": 1, "name": "min"}"#).unwrap();
        assert!(Template::import(&path).unwrap().sections.is_empty());
    }

    #[test]
    fn default_dir_is_under_app_root() {
        assert_eq!(
            default_template_dir(Path::new("/app")),
            Path::new("/app/templates")
        );
    }
}

#[cfg(test)]
mod filter_tests {
    use super::*;

    #[test]
    fn the_default_filter_takes_every_section() {
        for filter in [ImportFilter::all(), ImportFilter::default()] {
            assert!(filter.allows("theme"));
            assert!(filter.allows(""));
            assert!(filter.allows("anything else"));
        }
    }

    #[test]
    fn a_named_filter_takes_only_the_named_sections() {
        let filter = ImportFilter::only(["theme", "background"]);
        assert!(filter.allows("theme"));
        assert!(filter.allows("background"));
        assert!(!filter.allows("discord"));
        assert!(!filter.allows("Theme"), "section names are exact");
    }

    #[test]
    fn an_empty_selection_takes_nothing_rather_than_everything() {
        let filter = ImportFilter::only(Vec::<String>::new());
        assert!(!filter.allows("theme"));
        assert_ne!(filter, ImportFilter::all());
    }
}
