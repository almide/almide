//! #2350: `almide check --json` must exit on the same verdict as plain
//! `almide check`.
//!
//! `--json` used to exit 0 for every program that PARSED, so a type error was
//! written to stdout as a `"level":"error"` row and then discarded in the
//! status. The mode furthest from a human — the one an editor, a CI step or a
//! model's harness consumes — was the one that threw its own verdict away, and
//! a consumer doing the normal thing (run, gate on status, parse only on
//! failure) concluded that every rejected program checked clean.
//!
//! What is asserted here is AGREEMENT, not a pair of constants: the two modes
//! are run over the same program and their exit codes compared. The plain path
//! being right is what hid this for so long, so a test that only pinned
//! `--json` to an expected number could be made green again by an equal and
//! opposite drift in the plain path. Comparing them cannot.

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
        .expect("run almide check");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

/// One program, both modes. Returns (plain accepted, json accepted, json text).
fn both_modes(program: &str) -> (bool, bool, String) {
    let directory = tempfile::tempdir().expect("tempdir");
    let source = directory.path().join("subject.almd");
    std::fs::write(&source, program).expect("write subject");
    let (plain, _) = run(&["check"], &source);
    let (json, text) = run(&["check", "--json"], &source);
    (plain, json, text)
}

const CLEAN: &str = "fn main() -> Unit = println(string.to_upper(\"a\"))\n";
// One argument too many: E004, reported identically by both modes.
const TYPE_ERROR: &str = "fn main() -> Unit = println(string.to_upper(\"a\", \"b\"))\n";
// Unbalanced parameter list: the file does not parse at all.
const PARSE_ERROR: &str = "fn main( -> Unit = println(\"a\")\n";
// Binds a name it never reads: a warning, and warnings are not rejections.
const WARNING_ONLY: &str = "fn main() -> Unit = {\n  let unread = 1\n  println(\"a\")\n}\n";

#[test]
fn the_two_modes_agree_on_every_verdict() {
    for (label, program, expected) in [
        ("a clean program", CLEAN, true),
        ("a type error", TYPE_ERROR, false),
        ("a parse error", PARSE_ERROR, false),
        ("a warning with no error", WARNING_ONLY, true),
    ] {
        let (plain, json, text) = both_modes(program);
        assert_eq!(
            plain, json,
            "the modes disagree on {label}: plain accepted={plain}, --json accepted={json}\n{text}"
        );
        assert_eq!(
            json, expected,
            "{label} should be accepted={expected} and --json said {json}\n{text}"
        );
    }
}

/// The invariant that keeps status and payload from drifting apart: `--json`
/// is red exactly when it emitted at least one error row. Either half alone
/// can be satisfied by a wrong implementation — emitting nothing and exiting
/// 1, or emitting errors and exiting 0, which is precisely the bug.
#[test]
fn a_red_json_check_is_exactly_one_that_emitted_an_error_row() {
    for (label, program) in [
        ("a clean program", CLEAN),
        ("a type error", TYPE_ERROR),
        ("a parse error", PARSE_ERROR),
        ("a warning with no error", WARNING_ONLY),
    ] {
        let (_, accepted, text) = both_modes(program);
        let rows: Vec<serde_json::Value> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("row is not JSON ({e}): {l}")))
            .collect();
        let has_error_row = rows.iter().any(|r| r["level"] == "error");
        assert_eq!(
            !accepted, has_error_row,
            "{label}: exit-is-red={} but an error row was {}present\n{text}",
            !accepted,
            if has_error_row { "" } else { "not " }
        );
    }
}

/// The regression in its original spelling, from the issue. Kept separate from
/// the matrix so a failure names the bug rather than a cell.
#[test]
fn a_type_error_under_json_does_not_report_success() {
    let (plain, json, text) = both_modes(TYPE_ERROR);
    assert!(!plain, "the plain path must reject this program:\n{text}");
    assert!(
        !json,
        "#2350: --json reported success for a program it had just rejected\n{text}"
    );
    assert!(
        text.contains("E004"),
        "the payload should still carry the diagnostic:\n{text}"
    );
}

/// `--json` writes its rows to stdout and nothing else: a consumer parses the
/// stream line by line, so a stray human verdict line would break it. The fix
/// changes the status, not the stream.
#[test]
fn a_clean_json_check_writes_nothing() {
    let (_, accepted, text) = both_modes(CLEAN);
    assert!(accepted, "the clean program must be accepted:\n{text}");
    assert_eq!(text.trim(), "", "a clean --json check emits no rows:\n{text}");
}
