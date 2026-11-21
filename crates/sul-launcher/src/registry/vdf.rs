//! Minimal parser for Valve's KeyValues text format (`.vdf` / `.acf`).
//!
//! Handles quoted strings with escapes, nested `{ }` blocks and `//` comments,
//! which is all Steam's `libraryfolders.vdf` and `appmanifest_*.acf` need. Like Valve's own
//! parser it also accepts *unquoted* tokens (`root { k v }`), so a hand-edited file still loads.
//! Structurally broken input (unbalanced braces, a key without a value, nesting beyond
//! [`MAX_DEPTH`], an unterminated string) yields `None` rather than a guess.

const MAX_DEPTH: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Value {
    Str(String),
    Obj(Vec<(String, Value)>),
}

impl Value {
    /// Case-insensitive child lookup (Steam is inconsistent about key casing).
    pub(super) fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(items) => items
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v),
            Value::Str(_) => None,
        }
    }

    pub(super) fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            Value::Obj(_) => None,
        }
    }

    pub(super) fn entries(&self) -> &[(String, Value)] {
        match self {
            Value::Obj(items) => items,
            Value::Str(_) => &[],
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Token {
    Str(String),
    Open,
    Close,
}

fn tokenize(text: &str) -> Option<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() || c == '\u{feff}' => {}
            '{' => tokens.push(Token::Open),
            '}' => tokens.push(Token::Close),
            '/' if chars.peek() == Some(&'/') => {
                for skipped in chars.by_ref() {
                    if skipped == '\n' {
                        break;
                    }
                }
            }
            '"' => {
                let mut s = String::new();
                loop {
                    match chars.next()? {
                        '"' => break,
                        '\\' => match chars.next()? {
                            'n' => s.push('\n'),
                            't' => s.push('\t'),
                            '\\' => s.push('\\'),
                            '"' => s.push('"'),
                            other => {
                                // Unknown escape: keep it verbatim (raw Windows paths).
                                s.push('\\');
                                s.push(other);
                            }
                        },
                        other => s.push(other),
                    }
                }
                tokens.push(Token::Str(s));
            }
            first => {
                // Unquoted token: runs to whitespace, a brace or a quote.
                let mut s = String::from(first);
                while let Some(&next) = chars.peek() {
                    if next.is_whitespace() || matches!(next, '{' | '}' | '"') {
                        break;
                    }
                    s.push(next);
                    chars.next();
                }
                tokens.push(Token::Str(s));
            }
        }
    }
    Some(tokens)
}

fn parse_pairs(
    tokens: &[Token],
    pos: &mut usize,
    depth: usize,
    nested: bool,
) -> Option<Vec<(String, Value)>> {
    if depth > MAX_DEPTH {
        return None;
    }
    let mut items = Vec::new();
    loop {
        match tokens.get(*pos) {
            None => return if nested { None } else { Some(items) },
            Some(Token::Close) => {
                if !nested {
                    return None;
                }
                *pos += 1;
                return Some(items);
            }
            Some(Token::Open) => return None,
            Some(Token::Str(key)) => {
                *pos += 1;
                match tokens.get(*pos)? {
                    Token::Str(value) => {
                        items.push((key.clone(), Value::Str(value.clone())));
                        *pos += 1;
                    }
                    Token::Open => {
                        *pos += 1;
                        let children = parse_pairs(tokens, pos, depth + 1, true)?;
                        items.push((key.clone(), Value::Obj(children)));
                    }
                    Token::Close => return None,
                }
            }
        }
    }
}

/// Parse a whole document into a root object holding its top-level pairs.
pub(super) fn parse(text: &str) -> Option<Value> {
    let tokens = tokenize(text)?;
    let mut pos = 0;
    parse_pairs(&tokens, &mut pos, 0, false).map(Value::Obj)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_blocks_and_lookup_ignores_case() {
        let doc = parse(
            r#"
            "AppState"
            {
                "appid"     "359320"
                "Name"      "Elite Dangerous"
                "UserConfig" { "language" "english" }
            }
            "#,
        )
        .unwrap();

        let state = doc.get("appstate").unwrap();
        assert_eq!(state.get("APPID").unwrap().as_str(), Some("359320"));
        assert_eq!(state.get("name").unwrap().as_str(), Some("Elite Dangerous"));
        assert_eq!(
            state
                .get("userconfig")
                .unwrap()
                .get("language")
                .unwrap()
                .as_str(),
            Some("english")
        );
        assert!(state.get("missing").is_none());
    }

    #[test]
    fn unescapes_strings() {
        let doc =
            parse(r#""path" "D:\\SteamLibrary" "quote" "say \"hi\"" "raw" "C:\Games""#).unwrap();
        assert_eq!(doc.get("path").unwrap().as_str(), Some(r"D:\SteamLibrary"));
        assert_eq!(doc.get("quote").unwrap().as_str(), Some(r#"say "hi""#));
        assert_eq!(doc.get("raw").unwrap().as_str(), Some(r"C:\Games"));
    }

    #[test]
    fn skips_comments_and_bom() {
        let doc = parse("\u{feff}// header\n\"a\" \"1\" // trailing\n\"b\" \"2\"").unwrap();
        assert_eq!(doc.entries().len(), 2);
    }

    #[test]
    fn tolerates_unquoted_tokens() {
        let doc = parse("// comment\nroot { k v }\n").unwrap();
        let root = doc.get("root").unwrap();
        assert_eq!(root.get("k").unwrap().as_str(), Some("v"));

        // Mixed quoting, and a lone slash that does not start a comment.
        let doc = parse(r#"bare "1" "quoted" plain path /not-a-comment"#).unwrap();
        assert_eq!(doc.get("bare").unwrap().as_str(), Some("1"));
        assert_eq!(doc.get("quoted").unwrap().as_str(), Some("plain"));
        assert_eq!(doc.get("path").unwrap().as_str(), Some("/not-a-comment"));
    }

    #[test]
    fn rejects_malformed_input() {
        for bad in [
            r#""a" {"#,     // unclosed block
            r#""a" "1" }"#, // stray close
            r#""a""#,       // key without value
            "bare",         // unquoted key without value
            r#""a" "unterminated"#,
            r#""a" "dangling\"#,
            "{ }",
        ] {
            assert!(parse(bad).is_none(), "{bad:?} should not parse");
        }
    }

    #[test]
    fn refuses_pathological_nesting() {
        let deep = format!(
            "{}{}",
            r#""k" { "#.repeat(MAX_DEPTH + 5),
            "}".repeat(MAX_DEPTH + 5)
        );
        assert!(parse(&deep).is_none());
    }
}
