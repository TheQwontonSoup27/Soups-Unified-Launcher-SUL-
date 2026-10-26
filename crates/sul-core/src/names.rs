//! Helpers for turning user-supplied text into safe Windows file names.

/// Device names Windows refuses as file names, with or without an extension.
pub(crate) fn is_reserved_windows_name(stem: &str) -> bool {
    let upper = stem.trim_end_matches([' ', '.']).to_ascii_uppercase();
    let base = upper.split('.').next().unwrap_or("");
    matches!(base, "CON" | "PRN" | "AUX" | "NUL")
        || ((base.starts_with("COM") || base.starts_with("LPT"))
            && base.len() == 4
            && base.as_bytes()[3].is_ascii_digit()
            && base.as_bytes()[3] != b'0')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_reserved_names() {
        for n in ["con", "NUL", "com1", "LPT9", "aux.txt", "CON.json"] {
            assert!(is_reserved_windows_name(n), "{n} should be reserved");
        }
        for n in ["console", "com0", "com10", "elite", "lpt"] {
            assert!(!is_reserved_windows_name(n), "{n} should be allowed");
        }
    }
}
