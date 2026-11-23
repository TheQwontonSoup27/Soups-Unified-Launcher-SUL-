//! The release-ladder check for `cargo xtask release-check` (SUL-BRIEF.md section 3, "The Release Ladder", and guards.md
//! "Release-plan guards").
//!
//! Where the data is (D-150): `SUL-LEDGER/releases.md` is meant to be the single source of truth for what ships in which
//! release, but the release plan has not been written and the file is a placeholder. The ladder and the Tier 1-core event
//! list are in `SUL-BRIEF.md` (sections 3 and 6.8), so this reads them there. The ladder is a tab-separated table: `Release,
//! Version, Name, Purpose, Prerequisite gates`, rows R0 to R4. This reads that table and checks:
//!
//! - the ladder lists R0, R1, R2, R3 and R4, once each, in that order;
//! - the version ranges rise from one release to the next (R4, "n/a", is research only and has none);
//! - every prerequisite gate the ladder names (`A.26`, `A.63-Instruct`, a range such as `A.119–A.124`) has a heading in
//!   gates.md, so no release waits on a gate that does not exist. A gate id inside parentheses is a note about the row, not a
//!   prerequisite of it, and is not read: the brief's R1 row says "(A.63-Thinking and A.63-Router are A.2 gated shipping
//!   components for R1)", where "A.2" is a bucket, not the gate that happens to have that id;
//! - the release asked for is on the ladder and the workspace version is in its range (v0.1 is R0, so `0.1.x` for R0);
//! - the 21 Tier 1-core events SUL-BRIEF.md lists (section 6.8) are exactly the events of `sul_ipc_types::Tier1Event`, in the
//!   same order, so the approvals file's identifiers cannot drift from the spec.
//!
//! What this is NOT: the rest of the release-plan guards. The brief and guards.md *describe* `[release.R<n>]` and
//! `[reference_config.R<n>]` blocks but none exists, so "every release has a reference config", "every component resolves to
//! a crate or toggle", the superset invariant and the classification of the Tier 1 events that R1 to R3 add
//! (`tier1_events_added`) have no data to be checked against. gates.md records each gate's criterion and not whether it has
//! passed, so only existence is checked. The installed `%APPDATA%/SUL/release.toml` is a runtime matter. guards.md says
//! releases.md must sit "at the workspace root" and the brief says its canonical location is `SUL-LEDGER/releases.md`; that
//! file is a placeholder, so nothing here reads it.

use crate::common::{ensure_repository_root, finish, read_text};
use crate::workspace;
use miette::Result;
use std::collections::BTreeSet;
use std::path::Path;

/// Where the ladder and the Tier 1-core list are read from (D-150: `SUL-LEDGER/releases.md` is a placeholder until the release
/// plan is written).
pub const BRIEF_PATH: &str = "SUL-BRIEF.md";
pub const GATES_PATH: &str = "SUL-LEDGER/gates.md";
/// The release classes, in order.
pub const LADDER: [&str; 5] = ["R0", "R1", "R2", "R3", "R4"];

/// One row of the ladder table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LadderRow {
    pub id: String,
    pub version: String,
    pub prerequisites: String,
}

fn is_release_id(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 2 && bytes[0] == b'R' && bytes[1].is_ascii_digit()
}

/// The rows of the ladder table: the lines after the `Release<TAB>Version<TAB>...` header, up to the first line that is not
/// a row. Blank lines between rows are skipped.
pub fn ladder_rows(brief: &str) -> Vec<LadderRow> {
    let mut lines = brief.lines();
    if !lines.any(|line| line.starts_with("Release\tVersion\t")) {
        return Vec::new();
    }
    let mut rows = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 5 || !is_release_id(fields[0]) {
            break;
        }
        rows.push(LadderRow {
            id: fields[0].to_owned(),
            version: fields[1].trim().to_owned(),
            prerequisites: fields[4].trim().to_owned(),
        });
    }
    rows
}

/// `(major, minor)`.
pub type Minor = (u64, u64);

/// The versions a release covers: `v0.1`, `v0.2–0.3`, `v0.6+`. `to` is `None` when the range is open at the top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinorRange {
    pub from: Minor,
    pub to: Option<Minor>,
}

fn parse_minor(text: &str) -> Option<Minor> {
    let (major, minor) = text.trim().trim_start_matches('v').split_once('.')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}

/// `None` for `n/a` and for anything that is not a version range.
pub fn version_range(text: &str) -> Option<MinorRange> {
    let text = text.trim();
    if let Some(open) = text.strip_suffix('+') {
        return Some(MinorRange {
            from: parse_minor(open)?,
            to: None,
        });
    }
    if let Some((low, high)) = text.split_once(['–', '-']) {
        return Some(MinorRange {
            from: parse_minor(low)?,
            to: Some(parse_minor(high)?),
        });
    }
    let only = parse_minor(text)?;
    Some(MinorRange {
        from: only,
        to: Some(only),
    })
}

impl MinorRange {
    pub fn contains(&self, version: Minor) -> bool {
        version >= self.from && self.to.is_none_or(|to| version <= to)
    }
}

/// The major and minor of `0.1.0`.
pub fn minor_of(version: &str) -> Option<Minor> {
    let mut parts = version.trim().split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    parts
        .next()?
        .split(['-', '+'])
        .next()?
        .parse::<u64>()
        .ok()?;
    Some((major, minor))
}

fn digits_at(text: &str, start: usize) -> (&str, usize) {
    let end = text[start..]
        .find(|c: char| !c.is_ascii_digit())
        .map_or(text.len(), |offset| start + offset);
    (&text[start..end], end)
}

/// `text` with everything inside parentheses blanked out. A gate id in parentheses is a note about a ladder row, not one of its
/// prerequisites: the brief's R1 row says "(A.63-Thinking and A.63-Router are A.2 gated shipping components for R1)", and
/// "A.2" there names a bucket, not the gate that happens to have that id (`A.2 · Iterative CADENCE round 2`). Reading it as a
/// prerequisite would pass by accident. An unmatched `)` is left as it is.
fn without_notes(text: &str) -> String {
    let mut depth = 0usize;
    text.chars()
        .map(|c| match c {
            '(' => {
                depth += 1;
                ' '
            }
            ')' if depth > 0 => {
                depth -= 1;
                ' '
            }
            _ if depth > 0 => ' ',
            other => other,
        })
        .collect()
}

/// Gate ids in a prerequisite cell: `A.26`, `A.63-Instruct`, and a range `A.119–A.124` as every number in it. Text in
/// parentheses is a note and is not read (see `without_notes`).
pub fn gate_ids(text: &str) -> Vec<String> {
    let masked = without_notes(text);
    let text = masked.as_str();
    let bytes = text.as_bytes();
    let mut ids = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let starts_id = bytes[i] == b'A'
            && bytes.get(i + 1) == Some(&b'.')
            && bytes.get(i + 2).is_some_and(u8::is_ascii_digit)
            && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric());
        if !starts_id {
            i += 1;
            continue;
        }
        let (digits, mut end) = digits_at(text, i + 2);
        let Ok(first) = digits.parse::<u64>() else {
            i += 1;
            continue;
        };
        let mut id = format!("A.{first}");
        if bytes.get(end) == Some(&b'-') && bytes.get(end + 1).is_some_and(u8::is_ascii_alphabetic)
        {
            let letters = text[end + 1..]
                .find(|c: char| !c.is_ascii_alphabetic())
                .map_or(text.len(), |offset| end + 1 + offset);
            id.push_str(&text[end..letters]);
            end = letters;
        }
        ids.push(id);
        if let Some(rest) = text[end..].strip_prefix("–A.") {
            let (last_digits, consumed) = digits_at(rest, 0);
            if let Ok(last) = last_digits.parse::<u64>() {
                ids.extend((first + 1..=last).map(|n| format!("A.{n}")));
                // Continue after the range's own end, or `A.122` would be read a second time as an id of its own.
                end = text.len() - rest.len() + consumed;
            }
        }
        i = end.max(i + 1);
    }
    ids
}

/// The gate ids that have a heading (`A.26 · Title`) in gates.md.
pub fn gate_headings(gates: &str) -> BTreeSet<String> {
    gates
        .lines()
        .filter_map(|line| {
            let (id, _) = line.split_once(" · ")?;
            id.starts_with("A.").then(|| id.trim().to_owned())
        })
        .collect()
}

/// What the check found, and how much it looked at.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Findings {
    pub problems: Vec<String>,
    pub rows: usize,
    pub gates_checked: usize,
}

/// Runs the ladder checks on the text of the two ledger files and the workspace version.
pub fn findings(
    brief: Option<&str>,
    gates: Option<&str>,
    workspace_version: Option<&str>,
    release: &str,
) -> Findings {
    let mut found = Findings::default();
    let Some(brief) = brief else {
        found.problems.push(format!("{BRIEF_PATH} is missing"));
        return found;
    };
    let Some(gates) = gates else {
        found.problems.push(format!("{GATES_PATH} is missing"));
        return found;
    };
    let rows = ladder_rows(brief);
    found.rows = rows.len();
    if rows.is_empty() {
        found.problems.push(format!(
            "{BRIEF_PATH}: no release ladder table (a `Release<TAB>Version<TAB>...` header followed by rows R0 to R4)"
        ));
        return found;
    }
    let listed: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    if listed != LADDER {
        found.problems.push(format!(
            "{BRIEF_PATH}: the ladder lists {} but must list {} once each, in order",
            listed.join(", "),
            LADDER.join(", ")
        ));
    }

    let mut previous: Option<(&str, Minor)> = None;
    for row in &rows {
        match version_range(&row.version) {
            Some(range) => {
                if let Some((earlier, from)) = previous.filter(|(_, from)| range.from <= *from) {
                    found.problems.push(format!(
                        "{BRIEF_PATH}: {} starts at {} which is not above {earlier} at {}",
                        row.id,
                        row.version,
                        format_args!("v{}.{}", from.0, from.1)
                    ));
                }
                previous = Some((row.id.as_str(), range.from));
            }
            None if row.version.eq_ignore_ascii_case("n/a") => {}
            None => found.problems.push(format!(
                "{BRIEF_PATH}: {} has the version \"{}\", which is not vMAJOR.MINOR, a range or n/a",
                row.id, row.version
            )),
        }
    }

    let headings = gate_headings(gates);
    let mut distinct = BTreeSet::new();
    for row in &rows {
        for id in gate_ids(&row.prerequisites) {
            distinct.insert(id.clone());
            if !headings.contains(&id) {
                found.problems.push(format!(
                    "{BRIEF_PATH}: {} waits on {id}, which has no heading in {GATES_PATH}",
                    row.id
                ));
            }
        }
    }
    found.gates_checked = distinct.len();

    match rows.iter().find(|r| r.id == release) {
        None => found
            .problems
            .push(format!("{release} is not on the release ladder")),
        Some(row) => match (version_range(&row.version), workspace_version.map(minor_of)) {
            (None, _) => found.problems.push(format!(
                "{release} has no version ({}): it is not a buildable release",
                row.version
            )),
            (Some(_), None) => found
                .problems
                .push("the workspace Cargo.toml has no [workspace.package] version".to_owned()),
            (Some(_), Some(None)) => found.problems.push(format!(
                "the workspace version \"{}\" is not major.minor.patch",
                workspace_version.unwrap_or("")
            )),
            (Some(range), Some(Some(version))) => {
                if !range.contains(version) {
                    found.problems.push(format!(
                        "the workspace version {} is outside {release}'s range {}",
                        workspace_version.unwrap_or(""),
                        row.version
                    ));
                }
            }
        },
    }
    found
}

/// The sentence in SUL-BRIEF.md section 6.8 that introduces the list of Tier 1-core events.
const TIER1_CORE_SENTENCE: &str = "Tier 1-core events are";

/// The Tier 1-core event ids SUL-BRIEF.md names: the comma-separated list in the first non-blank line after the sentence "The
/// 21 Tier 1-core events are ...". Empty when the sentence or the list is missing, so a renamed section is a problem and not
/// a pass. A Markdown escape (`\_`) and the full stop that ends the list are removed.
pub fn tier1_core_ids(brief: &str) -> Vec<String> {
    let mut lines = brief.lines();
    if !lines.any(|line| line.contains(TIER1_CORE_SENTENCE)) {
        return Vec::new();
    }
    let Some(list) = lines.find(|line| !line.trim().is_empty()) else {
        return Vec::new();
    };
    list.split(',')
        .map(|id| id.trim().trim_end_matches('.').replace('\\', ""))
        .filter(|id| !id.is_empty())
        .collect()
}

/// Problems between the Tier 1-core list in SUL-BRIEF.md (`spec`) and the events in `sul-ipc-types` (`code`): an id on only
/// one side, an id listed twice, or the same ids in a different order (the order is part of the contract: `Tier1Event::ALL`
/// follows the brief).
pub fn tier1_problems(spec: &[String], code: &[&str]) -> Vec<String> {
    if spec.is_empty() {
        return vec![format!(
            "{BRIEF_PATH}: no Tier 1-core event list found (looked for a line containing \"{TIER1_CORE_SENTENCE}\" and a comma-separated list after it)"
        )];
    }
    let mut problems = Vec::new();
    let mut seen = BTreeSet::new();
    for id in spec {
        if !seen.insert(id.as_str()) {
            problems.push(format!(
                "{BRIEF_PATH}: the Tier 1-core event \"{id}\" is listed twice"
            ));
        }
    }
    for id in spec {
        if !code.contains(&id.as_str()) {
            problems.push(format!(
                "{BRIEF_PATH} lists the Tier 1-core event \"{id}\" but sul-ipc-types::Tier1Event has no such event"
            ));
        }
    }
    for id in code {
        if !spec.iter().any(|s| s == id) {
            problems.push(format!(
                "sul-ipc-types::Tier1Event has the event \"{id}\" but the Tier 1-core list in {BRIEF_PATH} does not"
            ));
        }
    }
    if problems.is_empty() && spec.iter().map(String::as_str).ne(code.iter().copied()) {
        problems.push(format!(
            "sul-ipc-types::Tier1Event::ALL lists the Tier 1-core events in a different order from {BRIEF_PATH}"
        ));
    }
    problems
}

/// The `release-ladder` gate for one release.
pub fn check(release: &str) -> Result<()> {
    let root = Path::new(".");
    ensure_repository_root(root)?;
    let brief = read_text(root, BRIEF_PATH);
    let gates = read_text(root, GATES_PATH);
    let version = read_text(root, "Cargo.toml").and_then(|raw| workspace::workspace_version(&raw));
    let mut found = findings(
        brief.as_deref(),
        gates.as_deref(),
        version.as_deref(),
        release,
    );
    let code_ids: Vec<&str> = sul_ipc_types::Tier1Event::ALL
        .iter()
        .map(|event| event.id())
        .collect();
    if let Some(text) = brief.as_deref() {
        found
            .problems
            .extend(tier1_problems(&tier1_core_ids(text), &code_ids));
    }
    println!(
        "[xtask] release-ladder: NOT checked: [release.R<n>] and [reference_config.R<n>] blocks (the release plan, SUL-LEDGER/releases.md, is a placeholder: D-150), component-to-crate resolution, the superset invariant, the Tier 1 events added by R1 to R3 (no per-release lists exist), the R0 build order's crates, whether a gate has passed (gates.md records criteria, not results)"
    );
    finish("release-ladder", &found.problems, || {
        format!(
            "{} ladder rows, {} prerequisite gates found in gates.md, workspace version {} is in {release}, {} Tier 1-core events match sul-ipc-types",
            found.rows,
            found.gates_checked,
            version.as_deref().unwrap_or("?"),
            code_ids.len()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LADDER_TEXT: &str = "intro\n\nRelease\tVersion\tName\tPurpose\tPrerequisite gates\n\nR0\tv0.1\t\"the launcher\"\tSubstrate\tA.26, A.63-Instruct, A.63-Common, A.92 (all release-blocking for R0)\n\nR1\tv0.2–0.3\t\"mainstream\"\tMusic\tA.63-Thinking, A.119–A.121, plus per-emulator A.74\n\nR2\tv0.4–0.5\t\"deep\"\tAI\tA.18\n\nR3\tv0.6+\t\"sensing\"\tCSI\tA.152 (prerequisite), A.136–A.137\n\nR4\tn/a\t\"research only\"\t400B\tNone; no shipping obligation\n\nEvery R3 feature is a strict superset of R2.\n";
    const GATES_TEXT: &str = "A.26 · Mainline llama.cpp\nA.63-Instruct · x\nA.63-Common · x\nA.92 · Vault\nA.63-Thinking · x\nA.119 · a\nA.120 · b\nA.121 · c\nA.74 · e\nA.18 · f\nA.152 · g\nA.136 · h\nA.137 · i\nA.26 passes when the build is fine\n";

    const TIER1_TEXT: &str = "6.8 Tier 1 events\n\nTier 1-core (R0): a setting is Tier 1-core if it disables a safety invariant. The 3 Tier 1-core events are the tier1_events array in R0 of releases.md:\n\nvault:disable_auth_factor, auth:disable, invariant:disable\\_destructive\\_action, refusal:voice_clone_attempt.\n\nEverything else is Tier 1 by release class.\n";

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|id| (*id).to_owned()).collect()
    }

    #[test]
    fn the_tier1_core_list_is_read_after_its_sentence_without_escapes_or_the_full_stop() {
        assert_eq!(
            tier1_core_ids(TIER1_TEXT),
            [
                "vault:disable_auth_factor",
                "auth:disable",
                "invariant:disable_destructive_action",
                "refusal:voice_clone_attempt"
            ]
        );
    }

    #[test]
    fn a_missing_sentence_or_list_gives_no_ids_and_so_a_problem() {
        assert!(tier1_core_ids("nothing here\n").is_empty());
        assert!(tier1_core_ids("The 21 Tier 1-core events are:\n\n\n").is_empty());
        let problems = tier1_problems(&[], &["auth:disable"]);
        assert_eq!(problems.len(), 1);
        assert!(
            problems[0].contains("no Tier 1-core event list found"),
            "{problems:?}"
        );
    }

    #[test]
    fn matching_lists_have_no_problems() {
        let spec = ids(&["a:x", "b:y", "c:z"]);
        assert!(tier1_problems(&spec, &["a:x", "b:y", "c:z"]).is_empty());
    }

    #[test]
    fn an_id_on_one_side_only_is_named_on_that_side() {
        let spec = ids(&["a:x", "b:y"]);
        let problems = tier1_problems(&spec, &["a:x", "c:z"]);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(
            problems
                .iter()
                .any(|p| p.contains("lists the Tier 1-core event \"b:y\" but")),
            "{problems:?}"
        );
        assert!(
            problems
                .iter()
                .any(|p| p.contains("has the event \"c:z\" but")),
            "{problems:?}"
        );
    }

    #[test]
    fn an_id_listed_twice_is_a_problem() {
        let spec = ids(&["a:x", "a:x"]);
        let problems = tier1_problems(&spec, &["a:x"]);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("listed twice"), "{problems:?}");
    }

    #[test]
    fn the_same_ids_in_another_order_are_a_problem_only_when_nothing_else_is() {
        let spec = ids(&["a:x", "b:y"]);
        let problems = tier1_problems(&spec, &["b:y", "a:x"]);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("different order"), "{problems:?}");
    }

    #[test]
    fn the_ladder_table_is_read_up_to_the_first_line_that_is_not_a_row() {
        let rows = ladder_rows(LADDER_TEXT);
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, LADDER);
        assert_eq!(rows[0].version, "v0.1");
        assert!(rows[0].prerequisites.starts_with("A.26"));
        assert!(ladder_rows("no table here").is_empty());
    }

    #[test]
    fn version_ranges_parse_and_contain() {
        let r0 = version_range("v0.1").unwrap();
        assert!(r0.contains((0, 1)) && !r0.contains((0, 2)));
        let r1 = version_range("v0.2–0.3").unwrap();
        assert!(r1.contains((0, 2)) && r1.contains((0, 3)) && !r1.contains((0, 4)));
        let r3 = version_range("v0.6+").unwrap();
        assert!(r3.contains((0, 6)) && r3.contains((1, 0)) && !r3.contains((0, 5)));
        assert_eq!(version_range("n/a"), None);
        assert_eq!(version_range("soon"), None);
        assert_eq!(minor_of("0.1.0"), Some((0, 1)));
        assert_eq!(minor_of("0.1.0-rc.1"), Some((0, 1)));
        assert_eq!(minor_of("0.1"), None);
        assert_eq!(minor_of("x.y.z"), None);
    }

    #[test]
    fn gate_ids_cover_plain_suffixed_and_ranged_gates() {
        assert_eq!(gate_ids("A.26, A.63-Instruct"), ["A.26", "A.63-Instruct"]);
        assert_eq!(
            gate_ids("A.119–A.122"),
            ["A.119", "A.120", "A.121", "A.122"]
        );
        assert_eq!(gate_ids("plus per-emulator A.74"), ["A.74"]);
        assert_eq!(
            gate_ids("A.152 (prerequisite), A.136–A.137"),
            ["A.152", "A.136", "A.137"]
        );
        assert!(gate_ids("None; no shipping obligation").is_empty());
        assert!(
            gate_ids("BA.5 and xA.7").is_empty(),
            "an id must start a word"
        );
    }

    #[test]
    fn a_gate_id_in_a_note_is_not_a_prerequisite() {
        // The brief's R1 row: the three ids in the note are not prerequisites, and "A.2" is a bucket, not a gate.
        let cell = "A.90, A.119–A.121, plus per-emulator A.74 (A.63-Thinking and A.63-Router are A.2 gated shipping components for R1)";
        assert_eq!(
            gate_ids(cell),
            ["A.90", "A.119", "A.120", "A.121", "A.74"],
            "text in parentheses must not be read"
        );
        // Ids before and after a note are both read; a nested note and an unmatched `)` do not confuse it.
        assert_eq!(gate_ids("A.1 (a (A.2) b) A.3"), ["A.1", "A.3"]);
        assert_eq!(gate_ids("A.1) A.3"), ["A.1", "A.3"]);
        // The R0 and R3 rows keep every id: their notes name none.
        assert_eq!(
            gate_ids("A.26, A.92 (all release-blocking for R0)"),
            ["A.26", "A.92"]
        );
        assert_eq!(
            gate_ids("A.152 (prerequisite), A.136–A.137"),
            ["A.152", "A.136", "A.137"]
        );
    }

    #[test]
    fn a_note_naming_gates_adds_no_prerequisites() {
        // Read as prerequisites, the note's ids would add two to the count, and `A.63-Conversation` has no heading here.
        let ladder = LADDER_TEXT.replace(
            "A.18\n",
            "A.18 (A.63-Conversation is an A.2 gated shipping component for R2)\n",
        );
        let gates = format!("{GATES_TEXT}A.2 · Some research gate\n");
        let found = findings(Some(&ladder), Some(&gates), Some("0.1.0"), "R0");
        assert_eq!(found.problems, Vec::<String>::new());
        assert_eq!(
            found.gates_checked, 13,
            "the ids in the note were counted as prerequisites"
        );
    }

    #[test]
    fn gate_headings_need_the_dot_separator_and_ignore_prose() {
        let headings = gate_headings(GATES_TEXT);
        assert!(headings.contains("A.26") && headings.contains("A.63-Instruct"));
        assert_eq!(headings.len(), 13);
    }

    #[test]
    fn a_consistent_ladder_has_no_problems() {
        let found = findings(Some(LADDER_TEXT), Some(GATES_TEXT), Some("0.1.0"), "R0");
        assert_eq!(found.problems, Vec::<String>::new());
        assert_eq!(found.rows, 5);
        assert_eq!(found.gates_checked, 13);
    }

    #[test]
    fn a_gate_with_no_heading_is_found_in_a_range_too() {
        let gates = GATES_TEXT.replace("A.120 · b\n", "");
        let found = findings(Some(LADDER_TEXT), Some(&gates), Some("0.1.0"), "R0");
        assert_eq!(found.problems.len(), 1);
        assert!(found.problems[0].contains("A.120"));
        assert!(found.problems[0].contains("R1"));
    }

    #[test]
    fn a_ladder_out_of_order_or_missing_a_row_is_refused() {
        let missing = LADDER_TEXT.replace("R2\tv0.4–0.5\t\"deep\"\tAI\tA.18\n\n", "");
        let found = findings(Some(&missing), Some(GATES_TEXT), Some("0.1.0"), "R0");
        assert!(found.problems.iter().any(|p| p.contains("must list")));
        let none = findings(Some("nothing"), Some(GATES_TEXT), Some("0.1.0"), "R0");
        assert_eq!(none.problems.len(), 1);
    }

    #[test]
    fn version_ranges_must_rise() {
        let backwards = LADDER_TEXT.replace("v0.4–0.5", "v0.2–0.3");
        let found = findings(Some(&backwards), Some(GATES_TEXT), Some("0.1.0"), "R0");
        assert!(
            found
                .problems
                .iter()
                .any(|p| p.contains("R2") && p.contains("not above"))
        );
        let garbled = LADDER_TEXT.replace("v0.6+", "later");
        let found = findings(Some(&garbled), Some(GATES_TEXT), Some("0.1.0"), "R0");
        assert!(
            found
                .problems
                .iter()
                .any(|p| p.contains("R3") && p.contains("later"))
        );
    }

    #[test]
    fn the_workspace_version_must_be_in_the_release_asked_for() {
        let r1 = findings(Some(LADDER_TEXT), Some(GATES_TEXT), Some("0.1.0"), "R1");
        assert_eq!(r1.problems.len(), 1);
        assert!(r1.problems[0].contains("outside R1"));
        let r1_ok = findings(Some(LADDER_TEXT), Some(GATES_TEXT), Some("0.3.2"), "R1");
        assert!(r1_ok.problems.is_empty());
        let r4 = findings(Some(LADDER_TEXT), Some(GATES_TEXT), Some("0.1.0"), "R4");
        assert!(
            r4.problems
                .iter()
                .any(|p| p.contains("not a buildable release"))
        );
        let r9 = findings(Some(LADDER_TEXT), Some(GATES_TEXT), Some("0.1.0"), "R9");
        assert!(
            r9.problems
                .iter()
                .any(|p| p.contains("not on the release ladder"))
        );
        let no_version = findings(Some(LADDER_TEXT), Some(GATES_TEXT), None, "R0");
        assert_eq!(no_version.problems.len(), 1);
        let bad_version = findings(Some(LADDER_TEXT), Some(GATES_TEXT), Some("zero"), "R0");
        assert_eq!(bad_version.problems.len(), 1);
    }

    #[test]
    fn a_missing_ledger_file_is_a_problem_not_a_pass() {
        assert_eq!(
            findings(None, Some(GATES_TEXT), Some("0.1.0"), "R0")
                .problems
                .len(),
            1
        );
        assert_eq!(
            findings(Some(LADDER_TEXT), None, Some("0.1.0"), "R0")
                .problems
                .len(),
            1
        );
    }
}
