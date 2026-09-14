//! The zero-test verdict of `almide test` is the same on both targets (#2204).
//!
//! `docs/specs/cli.md` promises one exit-code table for a run that executed
//! nothing: a `--run` pattern that excluded everything stays 0, "nothing to
//! run" is 5, `--allow-no-tests` opts out. The table names no target, so both
//! lanes must honour it — and the wasm lane did not for one shape: a file with
//! no `main` and no `test` block. The v1 renderer refuses such a program
//! ("nothing to run"), the lane classified that refusal as a renderer WALL,
//! and a wall is a decline, which `finish_test_run` rightly exempts from the
//! verdict. So the same file exited 5 on native and 0 on wasm, and the
//! `Cross-Target CI` per-file loop read the split as a leg mismatch on every
//! push for four days (the generated `*_semantics_manifest_test.almd` stubs
//! are exactly this shape).
//!
//! The CLI's own exit codes are outside the leg-parity contract ledger
//! (`docs/contracts/contracts.toml` governs compiled programs), so this file is
//! the evidence: every zero shape, both targets, same code and same count line.

use std::path::Path;
use std::process::Command;

fn almide_bin() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

const NO_MAIN_NO_TESTS: &str = "// a library-shaped file: nothing for any leg to run\npub fn f() -> Int = 1\n";
const MAIN_NO_TESTS: &str = "fn main() -> Unit = println(\"no tests here\")\n";
const TWO_TESTS: &str = "fn main() -> Unit = println(\"x\")\n\n\
test \"alpha passes\" {\n  assert_eq(1, 1)\n}\n\n\
test \"beta also passes\" {\n  assert_eq(2, 2)\n}\n";

fn case_dir(name: &str, file: &str, body: &str) -> tempfile::TempDir {
    let dir = tempfile::Builder::new().prefix(name).tempdir().expect("tempdir");
    std::fs::write(dir.path().join("almide.toml"), "[package]\nname = \"z\"\nversion = \"0.1.0\"\n")
        .expect("manifest");
    std::fs::write(dir.path().join(file), body).expect("fixture");
    dir
}

/// `(exit code, stdout+stderr)` of `almide test <args>` in `dir`.
fn run(dir: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new(almide_bin()).arg("test").args(args).current_dir(dir).output().expect("spawn almide");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

/// The `N tests in M files` line both lanes print — the report line the
/// zero-test table is read from.
fn count_line(report: &str) -> String {
    report
        .lines()
        .find(|l| l.contains(" in ") && (l.contains(" test ") || l.contains(" tests ")) && l.contains(" file"))
        .unwrap_or_else(|| panic!("no count line in:\n{report}"))
        .to_string()
}

/// Run the same invocation on native and on wasm; both must agree on the exit
/// code and on the count line.
fn assert_both_targets(dir: &Path, args: &[&str], expected_code: i32, expected_count: &str) {
    let (native_code, native) = run(dir, args);
    let wasm_args: Vec<&str> = args.iter().copied().chain(["--target", "wasm"]).collect();
    let (wasm_code, wasm) = run(dir, &wasm_args);
    assert_eq!(native_code, expected_code, "native {args:?}:\n{native}");
    assert_eq!(wasm_code, expected_code, "wasm {wasm_args:?}:\n{wasm}");
    assert_eq!(count_line(&native), expected_count, "native {args:?}:\n{native}");
    assert_eq!(count_line(&wasm), expected_count, "wasm {wasm_args:?}:\n{wasm}");
}

/// The shape that diverged: 5 on native, 0 on wasm under a WALL banner that
/// blamed the renderer for a file nothing could run.
#[test]
fn a_file_with_no_main_and_no_test_block_exits_5_on_both_targets() {
    if !wasmtime_available() { eprintln!("skip: no wasmtime"); return; }
    let dir = case_dir("empty-both", "lib.almd", NO_MAIN_NO_TESTS);
    assert_both_targets(dir.path(), &["lib.almd"], 5, "0 tests in 1 file");
    let (_, wasm) = run(dir.path(), &["lib.almd", "--target", "wasm"]);
    assert!(
        !wasm.contains("WALL ") && !wasm.contains("did not run on wasm"),
        "an empty file is not a renderer wall — nothing declined it, there was nothing to run:\n{wasm}"
    );
    assert!(wasm.contains("--allow-no-tests"), "the wasm lane names the opt-out too:\n{wasm}");
}

#[test]
fn allow_no_tests_opts_the_empty_file_out_on_both_targets() {
    if !wasmtime_available() { eprintln!("skip: no wasmtime"); return; }
    let dir = case_dir("empty-allowed", "lib.almd", NO_MAIN_NO_TESTS);
    assert_both_targets(dir.path(), &["lib.almd", "--allow-no-tests"], 0, "0 tests in 1 file");
}

/// The three shapes that already agreed stay pinned alongside the fixed one,
/// so the table is asserted whole rather than one cell at a time.
#[test]
fn a_main_only_file_exits_5_on_both_targets() {
    if !wasmtime_available() { eprintln!("skip: no wasmtime"); return; }
    let dir = case_dir("main-only", "n.almd", MAIN_NO_TESTS);
    assert_both_targets(dir.path(), &["n.almd"], 5, "0 tests in 1 file");
}

#[test]
fn a_filter_matching_nothing_is_green_on_both_targets() {
    if !wasmtime_available() { eprintln!("skip: no wasmtime"); return; }
    let dir = case_dir("filtered", "a_test.almd", TWO_TESTS);
    assert_both_targets(dir.path(), &["a_test.almd", "--run", "zzz-no-such-test"], 0, "0 tests in 1 file (2 filtered out)");
}

#[test]
fn a_real_run_reports_the_same_count_on_both_targets() {
    if !wasmtime_available() { eprintln!("skip: no wasmtime"); return; }
    let dir = case_dir("real", "a_test.almd", TWO_TESTS);
    assert_both_targets(dir.path(), &["a_test.almd"], 0, "2 tests in 1 file");
}
