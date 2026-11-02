//! Mod management (spec 2.1): the mod store, named profiles with a load order, and deploying a profile into a
//! managed build and taking it out again.
//!
//! What this module promises, and what it does not:
//!
//! * A mod is imported into SUL's own **store** (a library root of kind `Mods`) as a verified copy. The folder it
//!   came from is only read.
//! * A **profile** is a named, ordered list of (mod, version, enabled) for one game and one managed build, plus a
//!   few settings. Editing a profile changes only the profile file; it never touches a game.
//! * **Deploying** a profile copies the files of its enabled mods into the build, but only inside the folders the
//!   game's adapter allows mods to change, never inside a forbidden one (the game's own programs and its
//!   anti-cheat), and never into an install that stays where it is. Every file it adds or replaces is written
//!   down in an **ownership ledger** with the hash that was deployed. A file it replaces is first moved into a
//!   recovery folder, not deleted.
//! * **Purging** removes only what the ledger proves SUL put there: a file whose hash still equals the deployed
//!   hash. A file that changed since, or a file that is not in the ledger, is left alone and reported. What a
//!   purge removes is moved into a recovery folder, and the latest purge can be undone. There is a preview
//!   (dry run) of every deploy and purge that lists the exact paths.
//! * Nothing in this module deletes a file. The write-site list in `tests/write_sites.rs` fails if that changes.
//!
//! Not done here: telling a verified conflict from a mere overlap (spec 2.6), scanning a mod for malware
//! (spec 3.1; the package scanner exists but is not called from here), checking that a mod works in the game, and
//! anything about a real game: every test uses made-up files.

pub mod deploy;
pub mod profile;
pub mod store;

use std::cmp::Ordering;

use crate::names::is_reserved_windows_name;
use crate::{Error, Result, safepath};

pub const MAX_PATH_LEN: usize = 400;
pub const MAX_COMPONENT_LEN: usize = 255;
pub const MAX_TEXT_LEN: usize = 200;
pub const MAX_LIST_LEN: usize = 200;

/// A mod file path as it will be written into a build: forward slashes, no `..`, nothing Windows would rewrite
/// or refuse (a colon, a trailing dot or space, a device name such as `NUL`, a control character).
pub fn validate_mod_path(rel: &str) -> Result<String> {
    let p = safepath::validate_relative(rel)?;
    let mut parts = Vec::new();
    for c in p.components() {
        let s = c
            .as_os_str()
            .to_str()
            .ok_or_else(|| Error::UnsafePath(format!("{rel:?} is not valid text")))?;
        let bad_char = s
            .chars()
            .any(|ch| ch.is_control() || "<>:\"|?*".contains(ch));
        if s.len() > MAX_COMPONENT_LEN
            || bad_char
            || s.ends_with(' ')
            || s.ends_with('.')
            || is_reserved_windows_name(s)
        {
            return Err(Error::UnsafePath(format!(
                "{rel:?} has a name that Windows would refuse or change"
            )));
        }
        parts.push(s.to_owned());
    }
    let joined = parts.join("/");
    if joined.len() > MAX_PATH_LEN {
        return Err(Error::UnsafePath(format!("{rel:?} is too long")));
    }
    Ok(joined)
}

/// An id that becomes a folder or file name: the same rules as a game id, reported in plain words.
pub(crate) fn validate_name(kind: &str, s: &str) -> Result<()> {
    crate::storage::validate_game_id(s).map_err(|_| {
        Error::Invalid(format!(
            "{kind} {s:?} must be 1-128 letters, digits, '-', '_' or '.', not start or end with '.', and not be a device name"
        ))
    })
}

/// Text shown to people: not empty, not long, no control characters.
pub(crate) fn validate_text(kind: &str, s: &str, max: usize) -> Result<()> {
    if s.trim().is_empty() || s.chars().count() > max || s.chars().any(|c| c.is_control()) {
        return Err(Error::Invalid(format!(
            "{kind} must be 1-{max} characters with no control characters"
        )));
    }
    Ok(())
}

/// Free text that may be empty (notes, a source description).
pub(crate) fn validate_note(kind: &str, s: &str, max: usize) -> Result<()> {
    if s.chars().count() > max || s.chars().any(|c| c.is_control() && c != '\n') {
        return Err(Error::Invalid(format!(
            "{kind} must be at most {max} characters with no control characters"
        )));
    }
    Ok(())
}

/// Compare two dotted numeric versions (`1.2`, `v1.2.10`). `None` when either has a part that is not a number:
/// SUL does not guess an order for versions it cannot read.
pub fn compare_versions(a: &str, b: &str) -> Option<Ordering> {
    fn parts(v: &str) -> Option<Vec<u64>> {
        let v = v.strip_prefix(['v', 'V']).unwrap_or(v);
        if v.is_empty() {
            return None;
        }
        v.split('.').map(|p| p.parse::<u64>().ok()).collect()
    }
    let (mut x, mut y) = (parts(a)?, parts(b)?);
    let n = x.len().max(y.len());
    x.resize(n, 0);
    y.resize(n, 0);
    Some(x.cmp(&y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mod_paths_must_be_plain_names_windows_would_keep() {
        assert_eq!(validate_mod_path("data\\a.bin").unwrap(), "data/a.bin");
        assert_eq!(validate_mod_path("./data//a.bin").unwrap(), "data/a.bin");
        for bad in [
            "",
            "../x",
            "/abs",
            "C:/x",
            "data/a.bin:stream",
            "data/NUL",
            "data/nul.txt",
            "data/trailing.",
            "data/trailing ",
            "data/a?b",
            "data/a\u{1}b",
            &"x".repeat(MAX_PATH_LEN + 1),
        ] {
            assert!(validate_mod_path(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn versions_are_compared_only_when_they_can_be_read() {
        assert_eq!(compare_versions("1.2", "1.10"), Some(Ordering::Less));
        assert_eq!(compare_versions("v2", "2.0.0"), Some(Ordering::Equal));
        assert_eq!(compare_versions("1.0.1", "1.0"), Some(Ordering::Greater));
        assert_eq!(compare_versions("1.0-beta", "1.0"), None);
        assert_eq!(compare_versions("", "1"), None);
    }

    #[test]
    fn names_and_text_are_checked() {
        assert!(validate_name("mod id", "better-ui").is_ok());
        assert!(validate_name("mod id", "..").is_err());
        assert!(validate_name("mod id", "a/b").is_err());
        assert!(validate_name("mod id", "con").is_err());
        assert!(validate_text("name", "Better UI", 80).is_ok());
        assert!(validate_text("name", "  ", 80).is_err());
        assert!(validate_text("name", "a\nb", 80).is_err());
        assert!(validate_note("notes", "line one\nline two", 100).is_ok());
        assert!(validate_note("notes", "bell\u{7}", 100).is_err());
    }
}
