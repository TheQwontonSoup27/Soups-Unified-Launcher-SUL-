//! The `forbid(unsafe_code)` guard for `cargo xtask ac-regression` (D-153).
//!
//! The pure-Rust substrate crates are the ones Miri and Kani verify (D-141, D-147), and the alignment and no-undefined-
//! behaviour claims about `sul-ring` and `sul-wire` rest on the tests and on `#![forbid(unsafe_code)]`. Nothing noticed if
//! that attribute was deleted, so this fails when a crate on [`FORBID_UNSAFE`] no longer has it.
//!
//! What counts: a real inner attribute `#![forbid(unsafe_code)]` (other lints may sit in the same `forbid(...)` list) in
//! the run of comments and inner attributes at the very top of the crate root. These do **not** count, because none of
//! them makes the compiler refuse `unsafe` in the whole crate: the text in a comment (`sul-wire`'s own docs quote it), an
//! attribute inside a nested `mod`, an outer `#[forbid(unsafe_code)]`, `#![deny(unsafe_code)]` (a later `#[allow]` can lift
//! it) and `#![cfg_attr(..., forbid(unsafe_code))]` (conditional).
//!
//! What this is NOT: a check of any crate that is not on the list (a crate joins the list in the commit that creates it,
//! unless it must contain `unsafe` code), of `unsafe` in dependencies, or of the build itself: the compiler enforces the
//! attribute, this only notices that it is still there. It reads text, not syntax: a macro that writes the attribute would
//! not be seen.

use crate::common::{ensure_repository_root, finish, read_text};
use miette::Result;
use std::path::Path;

/// The crates that must keep `#![forbid(unsafe_code)]`: `(package, crate root)`. The six substrate crates that have it today.
pub const FORBID_UNSAFE: &[(&str, &str)] = &[
    ("sul-ipc-types", "crates/sul-ipc-types/src/lib.rs"),
    ("sul-ring", "crates/sul-ring/src/lib.rs"),
    ("sul-wire", "crates/sul-wire/src/lib.rs"),
    ("sul-trust", "crates/sul-trust/src/lib.rs"),
    ("sul-quarantine", "crates/sul-quarantine/src/lib.rs"),
    ("sul-metrics", "crates/sul-metrics/src/lib.rs"),
];

/// The inner attributes (`#![...]`) in the run of whitespace, comments and inner attributes at the top of a Rust file, each
/// as the text between `#![` and the closing `]` with its whitespace removed. Reading stops at the first thing that is none
/// of those (an item, an outer attribute), because an inner attribute cannot follow an item at the crate root: one found
/// later belongs to a nested module.
pub fn leading_inner_attributes(source: &str) -> Vec<String> {
    let chars: Vec<char> = source.chars().collect();
    let at = |index: usize| chars.get(index).copied();
    let mut attributes = Vec::new();
    let mut i = 0;
    loop {
        while at(i).is_some_and(char::is_whitespace) {
            i += 1;
        }
        match (at(i), at(i + 1), at(i + 2)) {
            // A line comment, including `//!` and `///`.
            (Some('/'), Some('/'), _) => {
                while at(i).is_some_and(|c| c != '\n') {
                    i += 1;
                }
            }
            // A block comment. Rust block comments nest.
            (Some('/'), Some('*'), _) => {
                let mut depth = 0usize;
                while i < chars.len() {
                    match (at(i), at(i + 1)) {
                        (Some('/'), Some('*')) => {
                            depth += 1;
                            i += 2;
                        }
                        (Some('*'), Some('/')) => {
                            depth = depth.saturating_sub(1);
                            i += 2;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => i += 1,
                    }
                }
            }
            // An inner attribute, which may span lines: read to the `]` that closes it.
            (Some('#'), Some('!'), Some('[')) => {
                let start = i + 3;
                let mut depth = 1usize;
                let mut j = start;
                while j < chars.len() && depth > 0 {
                    match chars[j] {
                        '[' => depth += 1,
                        ']' => depth -= 1,
                        _ => {}
                    }
                    j += 1;
                }
                let end = if depth == 0 { j - 1 } else { j };
                attributes.push(
                    chars[start..end]
                        .iter()
                        .filter(|c| !c.is_whitespace())
                        .collect(),
                );
                i = j;
            }
            _ => break,
        }
    }
    attributes
}

/// `forbid(unsafe_code)`, alone or with other lints in the same list. The text has no whitespace (see
/// [`leading_inner_attributes`]).
fn forbids_unsafe_code(attribute: &str) -> bool {
    attribute
        .strip_prefix("forbid(")
        .and_then(|rest| rest.strip_suffix(')'))
        .is_some_and(|lints| lints.split(',').any(|lint| lint == "unsafe_code"))
}

/// Whether a crate root carries `#![forbid(unsafe_code)]` as a real inner attribute at its top.
pub fn forbids_unsafe(source: &str) -> bool {
    leading_inner_attributes(source)
        .iter()
        .any(|attribute| forbids_unsafe_code(attribute))
}

/// Problems for a list of `(package, crate root)` pairs, with `read` giving the text of a crate root (`None` when the file
/// is missing or is not text). A listed crate whose root cannot be read is a problem and not a pass: the list is stale.
pub fn problems(crates: &[(&str, &str)], read: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let mut problems = Vec::new();
    for (package, path) in crates {
        match read(path) {
            None => problems.push(format!(
                "{package}: {path} cannot be read, so it is not known whether the crate forbids unsafe code (update the list in xtask/src/unsafe_code.rs if the crate moved or was removed)"
            )),
            Some(source) if !forbids_unsafe(&source) => problems.push(format!(
                "{package}: {path} has no `#![forbid(unsafe_code)]` inner attribute at the top (D-153: the substrate crates are pure Rust; a crate that must contain unsafe code leaves the list by a recorded decision)"
            )),
            Some(_) => {}
        }
    }
    problems
}

/// The `unsafe-code` gate.
pub fn check() -> Result<()> {
    let root = Path::new(".");
    ensure_repository_root(root)?;
    let problems = problems(FORBID_UNSAFE, |path| read_text(root, path));
    finish("unsafe-code", &problems, || {
        let names: Vec<&str> = FORBID_UNSAFE.iter().map(|(package, _)| *package).collect();
        format!(
            "{} substrate crates keep #![forbid(unsafe_code)]: {}. NOT checked: any crate that is not on the list, unsafe code in dependencies",
            FORBID_UNSAFE.len(),
            names.join(", ")
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_attribute_after_doc_comments_is_found() {
        let source =
            "//! Crate docs.\n//!\n//! More.\n\n#![forbid(unsafe_code)]\n\nuse std::fmt;\n";
        assert!(forbids_unsafe(source));
    }

    #[test]
    fn other_lints_in_the_same_list_and_odd_spacing_are_accepted() {
        assert!(forbids_unsafe("#![forbid(unsafe_code, missing_docs)]\n"));
        assert!(forbids_unsafe("#![forbid(missing_docs, unsafe_code)]\n"));
        assert!(forbids_unsafe("#![ forbid ( unsafe_code ) ]\n"));
        assert!(forbids_unsafe(
            "#![\n    forbid(\n        unsafe_code,\n    )\n]\n"
        ));
    }

    #[test]
    fn a_missing_attribute_is_found_missing() {
        assert!(!forbids_unsafe("//! docs\n\nuse std::fmt;\n"));
        assert!(!forbids_unsafe(""));
        assert!(!forbids_unsafe("\n\n   \n"));
    }

    #[test]
    fn the_text_in_a_comment_does_not_count() {
        // `sul-wire`'s own docs quote the attribute; deleting the real one must still be noticed.
        let line = "//! This crate holds no unsafe code: `#![forbid(unsafe_code)]` makes the compiler refuse it.\n";
        assert!(!forbids_unsafe(line));
        assert!(!forbids_unsafe("// #![forbid(unsafe_code)]\n"));
        assert!(!forbids_unsafe("/// #![forbid(unsafe_code)]\nfn f() {}\n"));
        assert!(!forbids_unsafe("/* #![forbid(unsafe_code)] */\n"));
        assert!(!forbids_unsafe(
            "/* a /* nested #![forbid(unsafe_code)] */ still a comment #![forbid(unsafe_code)] */\n"
        ));
    }

    #[test]
    fn a_block_comment_before_the_attribute_is_skipped() {
        assert!(forbids_unsafe("/* header */\n#![forbid(unsafe_code)]\n"));
        assert!(forbids_unsafe(
            "/* a /* nested */ comment */\n#![forbid(unsafe_code)]\n"
        ));
    }

    #[test]
    fn an_attribute_after_an_item_belongs_to_a_nested_module_and_does_not_count() {
        let source = "use std::fmt;\n\n#[cfg(test)]\nmod tests {\n    #![forbid(unsafe_code)]\n}\n";
        assert!(!forbids_unsafe(source));
        assert!(!forbids_unsafe("pub fn f() {}\n#![forbid(unsafe_code)]\n"));
    }

    #[test]
    fn weaker_or_conditional_forms_do_not_count() {
        assert!(!forbids_unsafe("#![deny(unsafe_code)]\n"));
        assert!(!forbids_unsafe("#![warn(unsafe_code)]\n"));
        assert!(!forbids_unsafe("#![forbid(unsafe_op_in_unsafe_fn)]\n"));
        assert!(!forbids_unsafe(
            "#![cfg_attr(not(test), forbid(unsafe_code))]\n"
        ));
        assert!(!forbids_unsafe("#![allow(unsafe_code)]\n"));
        assert!(!forbids_unsafe("#[forbid(unsafe_code)]\nmod m {}\n"));
        assert!(!forbids_unsafe("#![forbid(unsafe_code_extra)]\n"));
    }

    #[test]
    fn other_inner_attributes_before_it_do_not_hide_it() {
        let source = "#![cfg_attr(\n    docsrs,\n    feature(doc_cfg)\n)]\n#![warn(missing_docs)]\n#![forbid(unsafe_code)]\n\nmod m;\n";
        assert!(forbids_unsafe(source));
        assert_eq!(
            leading_inner_attributes(source),
            [
                "cfg_attr(docsrs,feature(doc_cfg))",
                "warn(missing_docs)",
                "forbid(unsafe_code)"
            ]
        );
    }

    #[test]
    fn a_broken_attribute_at_the_end_of_the_file_does_not_panic() {
        assert!(!forbids_unsafe("#!["));
        assert!(!forbids_unsafe("#![forbid(unsafe_code"));
        assert!(!forbids_unsafe("/* never closed"));
        assert!(!forbids_unsafe("//"));
    }

    fn crates() -> Vec<(&'static str, &'static str)> {
        vec![
            ("sul-a", "crates/sul-a/src/lib.rs"),
            ("sul-b", "crates/sul-b/src/lib.rs"),
        ]
    }

    #[test]
    fn a_crate_that_keeps_the_attribute_has_no_problem() {
        let read = |_: &str| Some("#![forbid(unsafe_code)]\n".to_owned());
        assert_eq!(problems(&crates(), read), Vec::<String>::new());
    }

    #[test]
    fn a_crate_that_lost_it_is_named_with_its_path() {
        let read = |path: &str| {
            Some(if path.contains("sul-b") {
                "//! no attribute\n".to_owned()
            } else {
                "#![forbid(unsafe_code)]\n".to_owned()
            })
        };
        let found = problems(&crates(), read);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].starts_with("sul-b: crates/sul-b/src/lib.rs"),
            "{found:?}"
        );
        assert!(found[0].contains("D-153"), "{found:?}");
    }

    #[test]
    fn a_listed_crate_that_cannot_be_read_is_a_problem_not_a_pass() {
        let found = problems(&crates(), |_| None);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found[0].contains("cannot be read"), "{found:?}");
    }

    #[test]
    fn the_real_list_names_the_six_substrate_crates() {
        let names: Vec<&str> = FORBID_UNSAFE.iter().map(|(package, _)| *package).collect();
        assert_eq!(
            names,
            [
                "sul-ipc-types",
                "sul-ring",
                "sul-wire",
                "sul-trust",
                "sul-quarantine",
                "sul-metrics"
            ]
        );
        for (package, path) in FORBID_UNSAFE {
            assert_eq!(*path, format!("crates/{package}/src/lib.rs"));
        }
    }
}
