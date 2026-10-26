//! A thin seam over the Windows registry.
//!
//! All scanner logic reads the registry through [`RegistryAccess`], so it can be
//! tested on any OS with an in-memory fake. [`WindowsRegistry`] (Windows only) is
//! the real implementation and deliberately contains no scanning logic.

use std::collections::BTreeMap;

use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Hive {
    LocalMachine,
    CurrentUser,
}

/// Which registry view to read. On 64-bit Windows, 32-bit software registers under
/// `WOW6432Node`; the 32-bit view reads that branch at the plain `SOFTWARE\...` path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum View {
    Registry64,
    Registry32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyPath {
    pub hive: Hive,
    pub view: View,
    /// Backslash-separated path below the hive, e.g. `SOFTWARE\Valve\Steam`.
    pub path: String,
}

impl KeyPath {
    pub fn new(hive: Hive, view: View, path: impl Into<String>) -> Self {
        Self {
            hive,
            view,
            path: path.into(),
        }
    }
}

/// The only value types the scanner cares about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegValue {
    Str(String),
    Dword(u32),
}

/// A registry key's name and values. Value names are case-insensitive, as in Windows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawKey {
    pub name: String,
    values: BTreeMap<String, RegValue>,
}

impl RawKey {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            values: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, value_name: &str, value: RegValue) {
        self.values.insert(value_name.to_ascii_lowercase(), value);
    }

    /// A string value, trimmed. Blank strings and non-string values read as `None`.
    pub fn string(&self, value_name: &str) -> Option<&str> {
        match self.values.get(&value_name.to_ascii_lowercase())? {
            RegValue::Str(s) => {
                let s = s.trim();
                (!s.is_empty()).then_some(s)
            }
            RegValue::Dword(_) => None,
        }
    }

    pub fn dword(&self, value_name: &str) -> Option<u32> {
        match self.values.get(&value_name.to_ascii_lowercase())? {
            RegValue::Dword(n) => Some(*n),
            RegValue::Str(_) => None,
        }
    }
}

pub trait RegistryAccess {
    /// Every immediate subkey of `path`, with its values. A missing key, or a subkey
    /// the process may not read, is skipped rather than reported.
    fn subkeys(&self, path: &KeyPath) -> Result<Vec<RawKey>>;

    /// The values of `path` itself, or `None` if the key does not exist.
    fn key(&self, path: &KeyPath) -> Result<Option<RawKey>>;
}

// ---------------------------------------------------------------------------
// Windows implementation
// ---------------------------------------------------------------------------

#[cfg(windows)]
pub use windows_impl::WindowsRegistry;

#[cfg(windows)]
mod windows_impl {
    use std::io::ErrorKind;

    use winreg::RegKey;
    use winreg::enums::{
        HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY, RegType,
    };
    use winreg::types::FromRegValue;

    use super::{Hive, KeyPath, RawKey, RegValue, RegistryAccess, View};
    use crate::{Error, Result};

    /// Reads the live system registry with read-only access.
    #[derive(Debug, Default, Clone, Copy)]
    pub struct WindowsRegistry;

    fn flags(view: View) -> u32 {
        KEY_READ
            | match view {
                View::Registry64 => KEY_WOW64_64KEY,
                View::Registry32 => KEY_WOW64_32KEY,
            }
    }

    /// Open `path`; a missing or unreadable key is `Ok(None)`.
    fn open(path: &KeyPath) -> Result<Option<RegKey>> {
        let root = RegKey::predef(match path.hive {
            Hive::LocalMachine => HKEY_LOCAL_MACHINE,
            Hive::CurrentUser => HKEY_CURRENT_USER,
        });
        match root.open_subkey_with_flags(&path.path, flags(path.view)) {
            Ok(key) => Ok(Some(key)),
            Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::PermissionDenied) => {
                Ok(None)
            }
            Err(source) => Err(Error::Registry {
                key: path.path.clone(),
                source,
            }),
        }
    }

    fn convert(value: &winreg::RegValue<'_>) -> Option<RegValue> {
        match value.vtype {
            RegType::REG_SZ | RegType::REG_EXPAND_SZ => {
                String::from_reg_value(value).ok().map(RegValue::Str)
            }
            RegType::REG_DWORD => u32::from_reg_value(value).ok().map(RegValue::Dword),
            _ => None,
        }
    }

    fn read_values(name: &str, key: &RegKey) -> RawKey {
        let mut raw = RawKey::new(name);
        for item in key.enum_values() {
            match item {
                Ok((value_name, value)) => {
                    if let Some(value) = convert(&value) {
                        raw.insert(&value_name, value);
                    }
                }
                Err(e) => tracing::debug!(key = name, error = %e, "skipping unreadable value"),
            }
        }
        raw
    }

    impl RegistryAccess for WindowsRegistry {
        fn subkeys(&self, path: &KeyPath) -> Result<Vec<RawKey>> {
            let Some(parent) = open(path)? else {
                return Ok(Vec::new());
            };

            let mut out = Vec::new();
            for name in parent.enum_keys() {
                let name = match name {
                    Ok(name) => name,
                    Err(e) => {
                        tracing::debug!(key = %path.path, error = %e, "skipping unreadable subkey");
                        continue;
                    }
                };
                match parent.open_subkey_with_flags(&name, flags(path.view)) {
                    Ok(child) => out.push(read_values(&name, &child)),
                    Err(e) => {
                        tracing::debug!(key = %name, error = %e, "skipping unopenable subkey");
                    }
                }
            }
            Ok(out)
        }

        fn key(&self, path: &KeyPath) -> Result<Option<RawKey>> {
            let name = path.path.rsplit('\\').next().unwrap_or("");
            Ok(open(path)?.map(|key| read_values(name, &key)))
        }
    }
}

// ---------------------------------------------------------------------------
// In-memory fake used by tests
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) use fake::{MemoryRegistry, s};

#[cfg(test)]
mod fake {
    use super::*;

    type Slot = (Hive, View, String);

    #[derive(Default)]
    pub(crate) struct MemoryRegistry {
        keys: BTreeMap<Slot, RawKey>,
    }

    pub(crate) fn s(text: &str) -> RegValue {
        RegValue::Str(text.to_owned())
    }

    fn slot(hive: Hive, view: View, path: &str) -> Slot {
        (hive, view, path.to_ascii_lowercase())
    }

    impl MemoryRegistry {
        /// Add the key at `full_path` (created implicitly; parents need no entry).
        pub(crate) fn add(
            &mut self,
            hive: Hive,
            view: View,
            full_path: &str,
            values: &[(&str, RegValue)],
        ) {
            let name = full_path.rsplit('\\').next().unwrap_or(full_path);
            let mut key = RawKey::new(name);
            for (value_name, value) in values {
                key.insert(value_name, value.clone());
            }
            self.keys.insert(slot(hive, view, full_path), key);
        }
    }

    impl RegistryAccess for MemoryRegistry {
        fn subkeys(&self, path: &KeyPath) -> Result<Vec<RawKey>> {
            let parent = path.path.to_ascii_lowercase();
            Ok(self
                .keys
                .iter()
                .filter(|((hive, view, p), _)| {
                    *hive == path.hive
                        && *view == path.view
                        && p.rsplit_once('\\').map(|(dir, _)| dir) == Some(parent.as_str())
                })
                .map(|(_, key)| key.clone())
                .collect())
        }

        fn key(&self, path: &KeyPath) -> Result<Option<RawKey>> {
            Ok(self
                .keys
                .get(&slot(path.hive, path.view, &path.path))
                .cloned())
        }
    }

    #[test]
    fn value_names_are_case_insensitive_and_blank_is_none() {
        let mut key = RawKey::new("k");
        key.insert("DisplayName", s("  Half-Life 2  "));
        key.insert("Blank", s("   "));
        key.insert("SystemComponent", RegValue::Dword(1));

        assert_eq!(key.string("displayname"), Some("Half-Life 2"));
        assert_eq!(key.string("blank"), None);
        assert_eq!(key.string("SystemComponent"), None, "dword is not a string");
        assert_eq!(key.dword("SYSTEMCOMPONENT"), Some(1));
        assert_eq!(key.dword("displayname"), None);
    }

    #[test]
    fn memory_registry_lists_only_direct_children_in_the_right_view() {
        let mut reg = MemoryRegistry::default();
        let (lm, v64, v32) = (Hive::LocalMachine, View::Registry64, View::Registry32);
        reg.add(
            lm,
            v64,
            r"SOFTWARE\Test\Uninstall\A",
            &[("DisplayName", s("A"))],
        );
        reg.add(lm, v64, r"SOFTWARE\Test\Uninstall\B", &[]);
        reg.add(lm, v64, r"SOFTWARE\Test\Uninstall\B\Deep", &[]);
        reg.add(lm, v32, r"SOFTWARE\Test\Uninstall\C", &[]);

        let names = |view| {
            let mut n: Vec<_> = reg
                .subkeys(&KeyPath::new(lm, view, r"software\test\uninstall"))
                .unwrap()
                .into_iter()
                .map(|k| k.name)
                .collect();
            n.sort();
            n
        };
        assert_eq!(names(v64), ["A", "B"]);
        assert_eq!(names(v32), ["C"]);

        let a = reg
            .key(&KeyPath::new(lm, v64, r"SOFTWARE\Test\Uninstall\A"))
            .unwrap()
            .unwrap();
        assert_eq!(a.string("DisplayName"), Some("A"));
        assert!(
            reg.key(&KeyPath::new(lm, v64, r"SOFTWARE\Nope"))
                .unwrap()
                .is_none()
        );
    }
}
