//! What the program under test PRINTED, attributed to the test that printed it
//! (#2538).
//!
//! A failing test's `println` / `eprintln` output used to be dropped whenever
//! the structured report recognised the failure: the report rendered the
//! assertion and nothing else, so the values a user printed to find the bug
//! never reached them. This module recovers that output from the same captured
//! streams the report is built from, on both legs:
//!
//! - **stdout** is attributed PER TEST. Both harnesses bracket each test on
//!   stdout — libtest (run with `--test-threads=1`) prints `test <path> ... `
//!   before a test and `ok` / `FAILED` after it; the wasm runner prints
//!   `test: <name> ... ` and `ok` — so everything between the two brackets is
//!   that test's.
//! - **stderr** is attributed per FILE. Neither harness writes a boundary to
//!   stderr, so the stream cannot be split by test; it is shown whole, with the
//!   harness's own failure text (the panic banner, the T18 abort block) removed
//!   because the report already renders it. The label says so.
//!
//! Passing tests stay silent unless `--show-output` asks for everything.

use super::test_report::{display_name, is_block_end, panic_header};

/// One test's stdout, in run order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestStdout {
    /// The `test "…"` name as written in the source.
    pub name: String,
    pub stdout: String,
    /// The harness printed the test's verdict. A test that ended the process
    /// (the T18 assert abort, a wasm trap) has none — it is the failing one.
    pub finished: bool,
}

/// Everything one test file printed, split as finely as its streams allow.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TestOutput {
    pub tests: Vec<TestStdout>,
    /// The program's stderr, harness failure text removed. Per FILE.
    pub stderr: String,
}

impl TestOutput {
    /// A native libtest run (`--nocapture --test-threads=1`).
    pub fn native(names: &[(String, String)], stdout: &str, stderr: &str) -> Self {
        let tests = segment(stdout, |line| {
            let rest = line.strip_prefix("test ")?;
            let (path, tail) = rest.split_once(" ... ")?;
            (!path.contains(' ') && path.contains("__test_almd_"))
                .then(|| (display_name(names, path), tail.to_string()))
        });
        TestOutput { tests, stderr: native_program_stderr(stderr) }
    }

    /// A wasm test-runner run (`test: <name> ... ` / `ok` on stdout).
    pub fn wasm(stdout: &str, stderr: &str) -> Self {
        let tests = segment(stdout, |line| {
            let name = line.strip_prefix("test: ")?.strip_suffix(" ... \n")?;
            Some((name.to_string(), String::new()))
        });
        TestOutput { tests, stderr: wasm_program_stderr(stderr) }
    }

    /// The test a failure belongs to: by name, or — for a failure the report
    /// could not name (the T18 abort) — the test that never finished.
    fn test_of(&self, name: Option<&str>) -> Option<&TestStdout> {
        match name {
            Some(n) => self.tests.iter().find(|t| t.name == n),
            None => self.tests.iter().rev().find(|t| !t.finished),
        }
    }

    /// The `stdout:` block for the test a failure belongs to ("" when it
    /// printed nothing).
    pub fn failure_stdout(&self, name: Option<&str>) -> String {
        self.test_of(name).map(|t| stdout_block(&t.stdout)).unwrap_or_default()
    }

    /// The tail of a file's report: every OTHER test's stdout when `show_all`
    /// (`--show-output`), then the file's stderr. `reported` names the tests
    /// whose stdout already sits under their failure.
    pub fn render_rest(&self, reported: &[Option<String>], show_all: bool) -> String {
        let mut out = String::new();
        if show_all {
            let claimed: Vec<&TestStdout> = reported.iter().filter_map(|n| self.test_of(n.as_deref())).collect();
            for t in &self.tests {
                if t.stdout.is_empty() || claimed.iter().any(|c| std::ptr::eq(*c, t)) {
                    continue;
                }
                out.push_str(&format!("  test: {}\n", t.name));
                out.push_str(&stdout_block(&t.stdout));
            }
        }
        if !self.stderr.is_empty() {
            out.push_str("  stderr (whole file — stderr carries no per-test boundary):\n");
            out.push_str(&indent(&self.stderr));
        }
        out
    }

    /// `--show-output` for a file that passed: `output: <file>` and whatever
    /// its tests printed, or nothing when they printed nothing.
    pub fn render_passing(&self, file: &str) -> String {
        let rest = self.render_rest(&[], true);
        if rest.is_empty() { String::new() } else { format!("output: {file}\n{rest}") }
    }
}

/// Split a harness's stdout into per-test segments. `marker` recognises the
/// line that opens a test and returns its name plus whatever of the test's
/// own output shares that line (libtest leaves its `test x ... ` open).
fn segment(stdout: &str, marker: impl Fn(&str) -> Option<(String, String)>) -> Vec<TestStdout> {
    let mut tests = Vec::new();
    let mut open: Option<(String, String)> = None;
    for line in stdout.split_inclusive('\n') {
        if let Some(next) = marker(line) {
            tests.extend(open.take().map(close));
            open = Some(next);
            continue;
        }
        // libtest's closing sections: nothing after them is a test's output.
        if line == "failures:\n" || line.starts_with("test result: ") {
            tests.extend(open.take().map(close));
            continue;
        }
        if let Some((_, text)) = open.as_mut() {
            text.push_str(line);
        }
    }
    tests.extend(open.take().map(close));
    tests
}

/// A segment's text minus the harness's verdict, which both runners print
/// right after the test's own output (glued to it when that output did not
/// end in a newline).
fn close((name, text): (String, String)) -> TestStdout {
    let body = text.trim_end_matches('\n');
    for verdict in ["ok", "FAILED", "ignored"] {
        if let Some(rest) = body.strip_suffix(verdict) {
            return TestStdout { name, stdout: rest.trim_end_matches('\n').to_string(), finished: true };
        }
    }
    TestStdout { name, stdout: body.to_string(), finished: false }
}

/// The T18 abort block and the wasm trap banner open the harness's failure
/// text; nothing after them is the program's.
fn failure_block_start(line: &str) -> bool {
    line.starts_with("Error: assertion failed")
        || line.starts_with("Error: snapshot mismatch")
        || line.starts_with("Error: failed to run main module")
}

/// Native stderr minus libtest's panic banners (each with its payload, which
/// the report renders) and the T18 abort block.
fn native_program_stderr(stderr: &str) -> String {
    let lines: Vec<&str> = stderr.lines().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i];
        if failure_block_start(l) {
            break;
        }
        if panic_header(l).is_some() {
            // The panic hook opens its banner with a blank line of its own.
            if out.ends_with("\n\n") || out == "\n" {
                out.pop();
            }
            i += 1;
            while i < lines.len() && !is_block_end(lines[i]) {
                i += 1;
            }
            continue;
        }
        if !l.starts_with("note: run with `RUST_BACKTRACE=") {
            out.push_str(l);
            out.push('\n');
        }
        i += 1;
    }
    out
}

/// Wasm stderr up to the failure block (see [`failure_block_start`]).
pub fn wasm_program_stderr(stderr: &str) -> String {
    let mut out = String::new();
    for l in stderr.lines() {
        if failure_block_start(l) {
            break;
        }
        out.push_str(l);
        out.push('\n');
    }
    out
}

/// The wasm leg's two-line failure detail: from the failure block when there
/// is one, so what the program itself printed is not mistaken for the error.
pub fn wasm_failure_lines(stderr: &str) -> Vec<&str> {
    let lines: Vec<&str> = stderr.lines().collect();
    let start = lines.iter().position(|l| failure_block_start(l)).unwrap_or(0);
    lines[start..].iter().take(2).copied().collect()
}

fn stdout_block(stdout: &str) -> String {
    if stdout.is_empty() { String::new() } else { format!("  stdout:\n{}", indent(stdout)) }
}

fn indent(text: &str) -> String {
    text.lines().map(|l| format!("    {l}\n")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NATIVE_STDOUT: &str = "\nrunning 3 tests\ntest tests::__test_almd_one ... P1\nok\ntest tests::__test_almd_two ... nonlok\ntest tests::__test_almd_three ... STDOUT: value is 42\nFAILED\n\nfailures:\n\nfailures:\n    tests::__test_almd_three\n\ntest result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\n";
    const NATIVE_STDERR: &str = "E1\nDEBUG: value is 42\n\nthread 'tests::__test_almd_three' (1) panicked at x.rs:1:9:\nassertion `left == right` failed: at line 4\n  left: 1\n right: 2\nnote: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n";

    fn names() -> Vec<(String, String)> {
        ["one", "two", "three"].iter().map(|n| (format!("__test_almd_{n}"), n.to_string())).collect()
    }

    #[test]
    fn native_stdout_is_split_per_test_and_the_verdict_is_dropped() {
        let out = TestOutput::native(&names(), NATIVE_STDOUT, NATIVE_STDERR);
        let got: Vec<(&str, &str, bool)> =
            out.tests.iter().map(|t| (t.name.as_str(), t.stdout.as_str(), t.finished)).collect();
        assert_eq!(got, vec![("one", "P1", true), ("two", "nonl", true), ("three", "STDOUT: value is 42", true)]);
    }

    #[test]
    fn native_stderr_drops_the_panic_banner_the_report_already_renders() {
        let out = TestOutput::native(&names(), NATIVE_STDOUT, NATIVE_STDERR);
        assert_eq!(out.stderr, "E1\nDEBUG: value is 42\n");
    }

    #[test]
    fn a_process_ending_abort_leaves_the_failing_test_unfinished() {
        let stdout = "\nrunning 2 tests\ntest tests::__test_almd_one ... ok\ntest tests::__test_almd_two ... before abort\n";
        let stderr = "DEBUG\nError: assertion failed\n  at: line 7\n  expected: 3\n  found: 4\n";
        let out = TestOutput::native(&names(), stdout, stderr);
        assert_eq!(out.failure_stdout(None), "  stdout:\n    before abort\n");
        assert_eq!(out.stderr, "DEBUG\n");
    }

    #[test]
    fn wasm_stdout_is_split_per_test() {
        let stdout = "test: one ... \nP1\nok\ntest: three ... \nSTDOUT: value is 42\n";
        let stderr = "E1\nDEBUG: value is 42\nError: assertion failed\n  at: line 4\n  expected: 2\n  found: 1\n";
        let out = TestOutput::wasm(stdout, stderr);
        assert_eq!(out.failure_stdout(Some("three")), "  stdout:\n    STDOUT: value is 42\n");
        assert_eq!(out.failure_stdout(None), "  stdout:\n    STDOUT: value is 42\n");
        assert_eq!(out.stderr, "E1\nDEBUG: value is 42\n");
        assert_eq!(wasm_failure_lines(stderr), vec!["Error: assertion failed", "  at: line 4"]);
    }

    #[test]
    fn passing_tests_stay_silent_unless_show_all() {
        let out = TestOutput::native(&names(), NATIVE_STDOUT, "");
        let reported = vec![Some("three".to_string())];
        assert_eq!(out.render_rest(&reported, false), "");
        assert_eq!(
            out.render_rest(&reported, true),
            "  test: one\n  stdout:\n    P1\n  test: two\n  stdout:\n    nonl\n"
        );
    }
}
