//! #3488: two tests whose labels fold to one identifier stay two tests.
//!
//! The native harness names each test fn after its label, and spelling a label
//! into a Rust identifier is lossy (`"a b"`, `"a_b"` and `"a-b"` all become
//! `a_b`), so distinct tests met on one fn: rustc E0428, and the file failed on
//! every route that runs it natively (a `// wasm:skip` file, a wasm wall, the
//! native re-run that checks a wasm failure). Each test fn now carries its
//! ordinal (`almide_ir::test_fn_name`), and the label is recovered for the
//! report from the same enumeration lowering names the fns from.
//!
//! Runs the `almide` binary (`ALMIDE_BIN`, else the one cargo built).

use std::process::Command;

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

/// `almide test <file>` on `source`, as (exit code, stdout + stderr).
fn almide_test(name: &str, source: &str, extra: &[&str]) -> (i32, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join(name);
    std::fs::write(&file, source).expect("write fixture");
    let out = Command::new(almide_bin())
        .arg("test")
        .arg(&file)
        .args(extra)
        .current_dir(dir.path())
        .output()
        .expect("spawn almide");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

/// The issue's repro, forced onto the native harness by `// wasm:skip`.
const COLLIDING: &str = "// wasm:skip\n\
test \"a b\" {\n  assert_eq(1, 1)\n}\n\n\
test \"a_b\" {\n  assert_eq(2, 2)\n}\n\n\
test \"a-b\" {\n  assert_eq(3, 3)\n}\n";

#[test]
fn labels_that_fold_alike_build_and_run_natively() {
    let (code, out) = almide_test("collide_test.almd", COLLIDING, &[]);
    assert_eq!(code, 0, "three distinct tests must build on the native harness:\n{out}");
    assert!(!out.contains("E0428"), "no fn may be defined twice:\n{out}");
    assert!(out.contains("3 tests"), "all three tests ran:\n{out}");
}

/// The native re-run that checks a wasm failure: the wasm leg fails `a-b`, and
/// the native harness has to build to confirm it — and name the failing test
/// by its own label, not one of its look-alikes'.
#[test]
fn a_failing_look_alike_is_reported_under_its_own_label() {
    let src = "test \"a b\" {\n  assert_eq(1, 1)\n}\n\n\
test \"a_b\" {\n  assert_eq(2, 2)\n}\n\n\
test \"a-b\" {\n  assert_eq(3, 4)\n}\n";
    let (code, out) = almide_test("collide_fail_test.almd", src, &[]);
    assert_ne!(code, 0, "the failing test fails the run:\n{out}");
    assert!(!out.contains("could not build"), "the native re-run must build:\n{out}");
    assert!(!out.contains("E0428"), "no fn may be defined twice:\n{out}");
    assert!(out.contains("test: a-b"), "the failure names `a-b`:\n{out}");
}

/// `where` cases are tests of their own; the native report names them by
/// their case label (the old line scan of the source never saw them).
#[test]
fn native_failures_in_where_cases_name_the_case() {
    let src = "// wasm:skip\n\
test \"sum\" where [\n  \"one\" [n = 1],\n  \"two\" [n = 2],\n] {\n  assert_eq(n, 1)\n}\n";
    let (code, out) = almide_test("cases_test.almd", src, &[]);
    assert!(!out.contains("error["), "the fixture compiles:\n{out}");
    assert_ne!(code, 0, "case `two` fails:\n{out}");
    assert!(out.contains("test: sum / two"), "the failing case is named:\n{out}");
}

/// `--run` still selects by a part of the label on the native harness.
#[test]
fn run_filter_still_selects_by_label() {
    let (code, out) = almide_test("collide_filter_test.almd", COLLIDING, &["--run", "a_b"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("3 tests"), "`a_b` is a substring of all three emitted names:\n{out}");
}
