//! String-level helpers for Windows paths and stable game ids.
//!
//! These work on `&str` rather than `Path` on purpose: registry data is Windows
//! text no matter which OS the code runs on, and this keeps the logic testable
//! everywhere.

/// `C:\...` or `C:/...` -> `'C'`.
pub fn drive_letter(path: &str) -> Option<char> {
    let b = path.as_bytes();
    (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':')
        .then(|| b[0].to_ascii_uppercase() as char)
}

/// Drive-absolute (`C:\x`, `C:/x`) or UNC (`\\server\share`).
pub fn is_absolute_windows(path: &str) -> bool {
    let b = path.as_bytes();
    let drive_abs = b.len() >= 3 && drive_letter(path).is_some() && matches!(b[2], b'\\' | b'/');
    drive_abs || path.starts_with("\\\\")
}

/// Case-insensitive, separator-insensitive comparison key for a path.
pub fn normalize_key(path: &str) -> String {
    let unified = path.trim().trim_matches('"').replace('/', "\\");
    unified.trim_end_matches('\\').to_lowercase()
}

/// Parent directory of a Windows path string.
pub fn windows_parent(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches(['\\', '/']);
    let (head, _) = trimmed.rsplit_once(['\\', '/'])?;
    (!head.is_empty() && !head.ends_with(':')).then(|| head.to_owned())
}

/// Clean an install-directory value: trim quotes and trailing separators, use `\`,
/// and accept it only if it is absolute and not a bare drive root.
pub fn clean_dir(raw: &str) -> Option<String> {
    let s = raw.trim().trim_matches('"').trim().replace('/', "\\");
    let s = s.trim_end_matches('\\').to_owned();
    is_absolute_windows(&s).then_some(s)
}

/// Extract the executable/file path from values like `"C:\x\a.exe",0`,
/// `C:\x\a.ico,0` or `C:\x\unins000.exe /SILENT`.
pub fn parse_command_path(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let candidate = if let Some(rest) = raw.strip_prefix('"') {
        rest.split('"').next().unwrap_or("").to_owned()
    } else if let Some(pos) = raw.to_ascii_lowercase().find(".exe") {
        raw[..pos + 4].to_owned()
    } else {
        match raw.rsplit_once(',') {
            Some((head, tail)) if tail.trim().parse::<i32>().is_ok() => head.to_owned(),
            _ => raw.to_owned(),
        }
    };
    let candidate = candidate.trim().replace('/', "\\");
    is_absolute_windows(&candidate).then_some(candidate)
}

/// Expand `%NAME%` references using `lookup`; unknown names are left untouched.
pub fn expand_env_vars(input: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 => match lookup(&after[..end]) {
                Some(value) => {
                    out.push_str(&value);
                    rest = &after[end + 1..];
                }
                None => {
                    out.push('%');
                    rest = after;
                }
            },
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Lowercase ASCII slug safe for use inside a game id (`[a-z0-9_-]`).
pub fn slug(raw: &str, max_len: usize) -> String {
    let mut out = String::new();
    for c in raw.chars() {
        let c = c.to_ascii_lowercase();
        let mapped = if c.is_ascii_alphanumeric() || c == '_' {
            c
        } else {
            '-'
        };
        if mapped == '-' && out.ends_with('-') {
            continue;
        }
        out.push(mapped);
    }
    let trimmed = out.trim_matches('-');
    let cut: String = trimmed.chars().take(max_len).collect();
    cut.trim_matches('-').to_owned()
}

/// 8 hex chars of FNV-1a over `input`; stable across runs and platforms.
pub fn short_hash(input: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in input.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{:08x}", (h as u32) ^ ((h >> 32) as u32))
}

/// Id for entries with a launcher-provided identifier (Steam app id, Epic app name...).
pub fn store_id(prefix: &str, external: &str) -> String {
    let s = slug(external, 64);
    if s.is_empty() {
        format!("{prefix}-{}", short_hash(external))
    } else if s != external {
        // Slugging was lossy, so keep distinct inputs distinct.
        format!("{prefix}-{s}-{}", short_hash(external))
    } else {
        format!("{prefix}-{s}")
    }
}

/// Id for entries identified only by a display name plus a registry key name.
pub fn name_id(prefix: &str, label: &str, unique: &str) -> String {
    let s = slug(label, 40);
    if s.is_empty() {
        format!("{prefix}-{}", short_hash(unique))
    } else {
        format!("{prefix}-{s}-{}", short_hash(unique))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::validate_game_id;

    #[test]
    fn drive_letters() {
        assert_eq!(drive_letter(r"d:\Games"), Some('D'));
        assert_eq!(drive_letter("C:/x"), Some('C'));
        assert_eq!(drive_letter(r"\\srv\share"), None);
        assert_eq!(drive_letter("x"), None);
        assert_eq!(drive_letter("1:\\"), None);
    }

    #[test]
    fn absolute_paths() {
        assert!(is_absolute_windows(r"C:\Games"));
        assert!(is_absolute_windows("D:/Games"));
        assert!(is_absolute_windows(r"\\nas\games"));
        assert!(!is_absolute_windows(r"Games\Foo"));
        assert!(!is_absolute_windows("C:"));
        assert!(!is_absolute_windows("MsiExec.exe /X{ABC}"));
    }

    #[test]
    fn normalization_ignores_case_separators_and_trailing_slash() {
        assert_eq!(
            normalize_key("c:/Program Files (x86)/Steam/"),
            normalize_key(r#""C:\program files (x86)\steam""#)
        );
    }

    #[test]
    fn parents() {
        assert_eq!(
            windows_parent(r"C:\Games\Foo\foo.exe").as_deref(),
            Some(r"C:\Games\Foo")
        );
        assert_eq!(
            windows_parent(r"C:\foo.exe"),
            None,
            "drive root is not a useful directory"
        );
        assert_eq!(windows_parent("foo.exe"), None);
    }

    #[test]
    fn clean_dir_accepts_only_real_absolute_dirs() {
        assert_eq!(
            clean_dir(r#" "D:/Games/Foo/" "#).as_deref(),
            Some(r"D:\Games\Foo")
        );
        assert_eq!(clean_dir(r"C:\"), None);
        assert_eq!(clean_dir("Foo"), None);
        assert_eq!(clean_dir(""), None);
    }

    #[test]
    fn command_paths() {
        let cases = [
            (r#""C:\Games\Foo\foo.exe",0"#, Some(r"C:\Games\Foo\foo.exe")),
            (r"C:\Games\Foo\foo.ico,0", Some(r"C:\Games\Foo\foo.ico")),
            (
                r"C:\Games\Foo\unins000.exe /SILENT",
                Some(r"C:\Games\Foo\unins000.exe"),
            ),
            (
                r#""D:\a b\uninstall.exe" /x"#,
                Some(r"D:\a b\uninstall.exe"),
            ),
            ("MsiExec.exe /X{ABC}", None),
            ("", None),
        ];
        for (raw, want) in cases {
            assert_eq!(parse_command_path(raw).as_deref(), want, "{raw}");
        }
    }

    #[test]
    fn env_expansion() {
        let lookup = |n: &str| (n == "ProgramFiles").then(|| r"C:\Program Files".to_owned());
        assert_eq!(
            expand_env_vars(r"%ProgramFiles%\Foo", lookup),
            r"C:\Program Files\Foo"
        );
        assert_eq!(expand_env_vars(r"%Missing%\Foo", lookup), r"%Missing%\Foo");
        assert_eq!(expand_env_vars("100% sure", lookup), "100% sure");
        assert_eq!(expand_env_vars("%%", lookup), "%%");
    }

    #[test]
    fn generated_ids_always_validate() {
        let very_long = "a".repeat(500);
        let nasty = [
            "",
            "{8F1B3C2A-0000-1111-2222-333344445555}",
            "../../etc/passwd",
            "CON",
            "Ünïcödé Game™: Edition?",
            very_long.as_str(),
            "   ",
            "...",
        ];
        for raw in nasty {
            for id in [
                store_id("steam", raw),
                store_id("epic", raw),
                name_id("reg", raw, raw),
                name_id("reg", "Some Game", raw),
            ] {
                assert!(validate_game_id(&id).is_ok(), "{id:?} from {raw:?}");
            }
        }
    }

    #[test]
    fn ids_are_stable_and_distinct() {
        assert_eq!(store_id("steam", "359320"), "steam-359320");
        assert_eq!(name_id("reg", "Foo", "k1"), name_id("reg", "Foo", "k1"));
        assert_ne!(name_id("reg", "Foo", "k1"), name_id("reg", "Foo", "k2"));
        assert_ne!(
            store_id("epic", "Abc"),
            store_id("epic", "abc"),
            "lossy slug keeps a hash"
        );
    }
}
