//! The four shapes of an `almide test` run must be mutually distinguishable (#2084).
//!
//! Before this, the summary counted FILES and never tests, so a run that
//! executed nothing rendered byte-identical to a run where everything passed —
//! a `.almd` file that lost its `test` block during a refactor kept reporting as
//! a passing test file, forever.
//!
//! The zeroes are not one case. A `--run` pattern that excluded everything is
//! the caller's own narrowing and stays green; "there was nothing to run" is a
//! mistake and exits 5, distinct from 1 so a script can tell it from a real
//! failure without parsing prose (pytest's precedent; `.github/workflows/
//! almide-pkg-ci.yml` used to grep the message for exactly this).

use std::path::Path;
use std::process::{Command, Output};

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

const TWO_TESTS: &str = "fn main() -> Unit = println(\"x\")\n\
\n\
test \"alpha passes\" {\n\
  assert_eq(1, 1)\n\
}\n\
\n\
test \"beta also passes\" {\n\
  assert_eq(2, 2)\n\
}\n";

const NO_TESTS: &str = "fn main() -> Unit = println(\"no tests here\")\n";

/// Each case gets its own directory so the "no test file at all" shape is real
/// rather than simulated.
fn run(dir: &Path, args: &[&str]) -> (i32, String) {
    let out: Output = Command::new(almide_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn almide");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code().unwrap_or(-1), text)
}

fn case_dir(name: &str, files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::Builder::new().prefix(name).tempdir().expect("tempdir");
    std::fs::write(dir.path().join("almide.toml"), "[package]\nname = \"z\"\nversion = \"0.1.0\"\n")
        .expect("manifest");
    for (name, body) in files {
        std::fs::write(dir.path().join(name), body).expect("fixture");
    }
    dir
}

#[test]
fn a_real_pass_reports_the_tests_it_ran() {
    let dir = case_dir("pass", &[("a_test.almd", TWO_TESTS)]);
    let (code, out) = run(dir.path(), &["test", "a_test.almd"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("2 tests in 1 file"), "{out}");
    assert!(!out.contains("filtered out"), "nothing was filtered:\n{out}");
}

/// Green, because the operator asked for a narrowing and got it — but the count
/// says so out loud, which is the whole point.
#[test]
fn a_filter_matching_nothing_is_green_and_says_what_it_excluded() {
    let dir = case_dir("filtered", &[("a_test.almd", TWO_TESTS)]);
    let (code, out) = run(dir.path(), &["test", "a_test.almd", "--run", "zzz-no-such-test"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("0 tests in 1 file (2 filtered out)"), "{out}");
}

/// The shape that used to report "All 1 test file(s) passed" with exit 0.
#[test]
fn a_named_file_with_no_test_block_is_not_a_pass() {
    let dir = case_dir("notests", &[("a.almd", NO_TESTS)]);
    let (code, out) = run(dir.path(), &["test", "a.almd"]);
    assert_eq!(code, 5, "a file with no test block must not pass:\n{out}");
    assert!(out.contains("0 tests in 1 file"), "{out}");
    assert!(out.contains("--allow-no-tests"), "the message must name the opt-out:\n{out}");
}

#[test]
fn a_directory_with_no_test_files_takes_the_same_verdict() {
    let dir = case_dir("emptydir", &[("a.almd", NO_TESTS)]);
    let (code, out) = run(dir.path(), &["test", "."]);
    assert_eq!(code, 5, "{out}");
    assert!(out.contains("0 tests in 0 files"), "{out}");
}

/// Both "nothing to run" shapes opt out through one flag — the property that
/// let `almide-pkg-ci.yml` drop its twelve-line message grep.
#[test]
fn allow_no_tests_opts_both_empty_shapes_out() {
    let dir = case_dir("allowed", &[("a.almd", NO_TESTS)]);
    for args in [
        ["test", "a.almd", "--allow-no-tests"].as_slice(),
        ["test", ".", "--allow-no-tests"].as_slice(),
    ] {
        let (code, out) = run(dir.path(), args);
        assert_eq!(code, 0, "{args:?}:\n{out}");
    }
}

/// 5 rather than 1, so "nothing ran" is separable from "tests failed" without
/// reading the output.
#[test]
fn the_empty_verdict_is_distinct_from_a_test_failure() {
    let failing = "fn main() -> Unit = println(\"x\")\n\ntest \"beta fails\" {\n  assert_eq(1, 2)\n}\n";
    let dir = case_dir("distinct", &[("f_test.almd", failing), ("n.almd", NO_TESTS)]);
    let (fail_code, _) = run(dir.path(), &["test", "f_test.almd"]);
    let (empty_code, _) = run(dir.path(), &["test", "n.almd"]);
    assert_eq!(fail_code, 1);
    assert_eq!(empty_code, 5);
    assert_ne!(fail_code, empty_code);
}

/// `--json` consumers get the same distinction without a summary line to read.
#[test]
fn json_carries_the_per_file_counts() {
    let dir = case_dir("json", &[("a_test.almd", TWO_TESTS)]);
    let (code, out) = run(dir.path(), &["test", "a_test.almd", "--json", "--run", "alpha"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"tests\":1"), "{out}");
    assert!(out.contains("\"filtered_out\":1"), "{out}");
}
