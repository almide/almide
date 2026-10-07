//! #3449: `almide test --json` must exit on the same verdict as plain
//! `almide test` — the `test` counterpart of #2350 (`check --json`).
//!
//! `--json` used to report `"status":"fail"` in its JSONL row and then exit 0,
//! so a CI step that asked for machine-readable output counted a failing file
//! as passed. As in `check_json_exit_code_test.rs`, what is asserted is
//! AGREEMENT between the two modes over the same file, plus the expected
//! verdict, so an opposite drift in the plain path cannot make it green.

use std::path::Path;
use std::process::Command;

fn bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn run(args: &[&str], file: &Path) -> (bool, String) {
    let out = Command::new(bin())
        .args(args)
        .arg(file)
        .output()
        .expect("run almide test");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

/// One test file, both modes. Returns (plain passed, json passed, json text).
fn both_modes(program: &str) -> (bool, bool, String) {
    let directory = tempfile::tempdir().expect("tempdir");
    let source = directory.path().join("subject_test.almd");
    std::fs::write(&source, program).expect("write subject");
    let (plain, _) = run(&["test"], &source);
    let (json, text) = run(&["test", "--json"], &source);
    (plain, json, text)
}

const PASSING: &str = "test \"passes\" {\n  assert_eq(1, 1)\n}\n";
// The repro from the issue.
const FAILING: &str = "test \"fails\" {\n  assert_eq(1, 2)\n}\n";

#[test]
fn the_two_modes_agree_on_every_verdict() {
    for (label, program, expected) in [("a passing file", PASSING, true), ("a failing file", FAILING, false)] {
        let (plain, json, text) = both_modes(program);
        assert_eq!(
            plain, json,
            "the modes disagree on {label}: plain passed={plain}, --json passed={json}\n{text}"
        );
        assert_eq!(json, expected, "{label} should pass={expected} and --json said {json}\n{text}");
    }
}

/// The status and the payload must say the same thing: red exactly when some
/// row reports `"status":"fail"`.
#[test]
fn a_red_json_run_is_exactly_one_with_a_failing_row() {
    for (label, program) in [("a passing file", PASSING), ("a failing file", FAILING)] {
        let (_, passed, text) = both_modes(program);
        let rows: Vec<serde_json::Value> = text
            .lines()
            .filter(|l| l.trim_start().starts_with('{'))
            .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("row is not JSON ({e}): {l}")))
            .collect();
        assert!(!rows.is_empty(), "{label}: --json emitted no row\n{text}");
        let has_fail_row = rows.iter().any(|r| r["status"] == "fail");
        assert_eq!(
            !passed, has_fail_row,
            "{label}: exit-is-red={} but a failing row was {}present\n{text}",
            !passed,
            if has_fail_row { "" } else { "not " }
        );
    }
}
