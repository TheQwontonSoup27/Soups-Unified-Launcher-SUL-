//! The `ipc-check` gate (SUL-ARCHITECTURE.md, Ch. 7 §2.3, "rkyv schema": "The cargo xtask ipc-check job verifies
//! golden-bytes stability on every PR").
//!
//! It runs the golden-bytes test of `sul-ipc-types` (`crates/sul-ipc-types/tests/golden.rs`), which serializes one sample of
//! every message variant, the enums whose discriminants travel inside messages, the Tier 1 identifiers and the ring layout
//! constants, and compares the bytes with `tests/golden/ipc-v1.txt`. A changed wire format therefore fails this gate until the
//! golden file is regenerated and reviewed on purpose.
//!
//! Two ways a run could pass while checking nothing are closed here. The test regenerates its reference file when
//! `SUL_UPDATE_GOLDEN` is set, so that variable is removed from the child process: the gate can never rewrite the file it
//! compares against. And a run that executed no test at all (the test file emptied or renamed) is a failure, not a pass.
//!
//! What this is NOT: a check that the golden file was changed *with* a migration step. The test sees the bytes, not the
//! reason they moved; the reviewer of the golden-file diff is the other half of the rule.

use crate::common::{ensure_repository_root, finish};
use miette::{Context, IntoDiagnostic, Result};
use std::path::Path;
use std::process::Command;

/// The environment variable that makes the golden test rewrite `tests/golden/ipc-v1.txt` instead of comparing with it.
pub const UPDATE_VARIABLE: &str = "SUL_UPDATE_GOLDEN";
/// The cargo arguments that run exactly the golden-bytes test, against the committed lockfile.
pub const GOLDEN_ARGS: [&str; 6] = [
    "test",
    "-p",
    "sul-ipc-types",
    "--test",
    "golden",
    "--locked",
];

/// How many lines of the test output a failure shows.
const TAIL_LINES: usize = 25;

/// Tests passed, summed over every `test result: ok. N passed; ...` line of a cargo test run.
pub fn passed_count(output: &str) -> usize {
    output
        .lines()
        .filter_map(|line| line.trim().strip_prefix("test result: ok. "))
        .filter_map(|rest| rest.split_whitespace().next())
        .filter_map(|n| n.parse::<usize>().ok())
        .sum()
}

/// What one run of the golden test means: the number of tests that passed, or why the gate fails.
pub fn verdict(success: bool, output: &str) -> std::result::Result<usize, String> {
    if !success {
        let lines: Vec<&str> = output.lines().collect();
        let start = lines.len().saturating_sub(TAIL_LINES);
        return Err(format!(
            "the golden-bytes test of sul-ipc-types did not pass (last {} lines follow; a changed wire format needs `{UPDATE_VARIABLE}=1 cargo test -p sul-ipc-types --test golden`, a review of the diff of tests/golden/ipc-v1.txt and a migration step):\n{}",
            lines.len() - start,
            lines[start..].join("\n")
        ));
    }
    match passed_count(output) {
        0 => Err("the golden-bytes test of sul-ipc-types ran no test, so nothing was compared with tests/golden/ipc-v1.txt".to_owned()),
        n => Ok(n),
    }
}

/// The `ipc-check` gate.
pub fn check() -> Result<()> {
    ensure_repository_root(Path::new("."))?;
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let out = Command::new(cargo)
        .args(GOLDEN_ARGS)
        .env_remove(UPDATE_VARIABLE)
        .output()
        .into_diagnostic()
        .context("failed to start cargo to run the golden-bytes test")?;
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let result = verdict(out.status.success(), &text);
    let problems: Vec<String> = result.as_ref().err().cloned().into_iter().collect();
    finish("ipc-check", &problems, || {
        format!(
            "{} golden-bytes tests passed: message variants, enum discriminants, Tier 1 identifiers and ring constants match tests/golden/ipc-v1.txt",
            result.as_ref().copied().unwrap_or(0)
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASSED: &str = "running 3 tests\ntest a ... ok\ntest b ... ok\ntest c ... ok\n\ntest result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n";

    #[test]
    fn a_passing_run_counts_its_tests() {
        assert_eq!(passed_count(PASSED), 3);
        assert_eq!(verdict(true, PASSED), Ok(3));
    }

    #[test]
    fn counts_are_summed_over_every_result_line() {
        let two = format!("{PASSED}{PASSED}");
        assert_eq!(passed_count(&two), 6);
    }

    #[test]
    fn a_run_that_executed_no_test_is_a_failure_not_a_pass() {
        let empty = "running 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n";
        let problem = verdict(true, empty).unwrap_err();
        assert!(problem.contains("ran no test"), "{problem}");
        assert!(verdict(true, "").is_err());
    }

    #[test]
    fn a_failing_run_is_a_failure_and_shows_the_end_of_the_output() {
        let mut output = String::new();
        for n in 0..60 {
            output.push_str(&format!("line {n}\n"));
        }
        output.push_str("test result: FAILED. 2 passed; 1 failed\n");
        let problem = verdict(false, &output).unwrap_err();
        assert!(problem.contains("did not pass"), "{problem}");
        assert!(problem.contains("FAILED. 2 passed; 1 failed"), "{problem}");
        assert!(problem.contains("line 59"), "{problem}");
        assert!(
            !problem.contains("line 10\n"),
            "only the tail is shown: {problem}"
        );
    }

    #[test]
    fn a_failed_run_is_never_rescued_by_an_ok_line_in_its_output() {
        // One of two result lines is ok, the process still exited non-zero: the exit status decides.
        assert!(verdict(false, PASSED).is_err());
    }

    #[test]
    fn the_command_runs_only_the_golden_test_against_the_lockfile() {
        assert_eq!(
            GOLDEN_ARGS,
            [
                "test",
                "-p",
                "sul-ipc-types",
                "--test",
                "golden",
                "--locked"
            ]
        );
        assert_eq!(UPDATE_VARIABLE, "SUL_UPDATE_GOLDEN");
    }
}
