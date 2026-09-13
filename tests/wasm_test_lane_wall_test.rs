//! #2121: `almide test --target wasm` must not report success for a file whose
//! tests never ran there.
//!
//! The test lane renders every file through the INCUMBENT brick, while
//! `build`/`run`/`check --target wasm` render through the two-leg router whose
//! default is the STRUCTURAL leg (#2179). So the lane walls on programs the
//! product compiles — and it reported those walls as benign SKIPs and exited 0.
//! In this repository's own spec corpus that was five files: `almide test spec/
//! --target wasm`, a CI gate, was green while five files' tests had not run on
//! wasm, under a reason line ("no verified wasm rendering") that named a v1
//! verdict as if it were the product's.
//!
//! The distinction this pins: a skip the AUTHOR declared and a skip a RENDERER
//! decided are not the same verdict. A declared `// wasm:skip` (with its reason,
//! greppable, reviewable) stays a skip. A wall is a failure of the target the
//! caller explicitly asked for.

use std::process::Command;

/// A `pub fn` matching a variant produced INSIDE it and returning a String —
/// the #2160 shape. The incumbent brick walls it ("heap-result `match` outside
/// the executable subset"); `almide build --target wasm` renders the same file
/// on the structural leg and reports `verified`.
const WALLS_THE_INCUMBENT: &str = r#"
type T = | A(String) | B

fn make(s: String) -> T = if s == "" then B else A(s)

pub fn f(s: String) -> String = match make(s) { A(x) => x, B => "b" }

test "f returns the payload" {
  assert_eq(f("x"), "x")
}
"#;

/// The control: an ordinary file whose tests DO run on the lane.
const RUNS_ON_THE_LANE: &str = r#"
fn double(n: Int) -> Int = n * 2

test "double doubles" {
  assert_eq(double(21), 42)
}
"#;

fn almide_bin() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// Run `almide test <file> --target wasm` and return `(exit code, report)`.
fn test_on_wasm(dir: &std::path::Path, source: &str) -> (Option<i32>, String) {
    let file = dir.join("t_test.almd");
    std::fs::write(&file, source).expect("source");
    let out = Command::new(almide_bin())
        .args(["test", file.to_str().expect("path"), "--target", "wasm"])
        .current_dir(dir)
        .output()
        .expect("run");
    let report =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    (out.status.code(), report)
}

#[test]
fn a_renderer_wall_fails_the_run_instead_of_passing_as_a_skip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (code, report) = test_on_wasm(dir.path(), WALLS_THE_INCUMBENT);
    assert_eq!(
        code,
        Some(1),
        "a file whose tests did not run on the requested target must not exit 0:\n{report}"
    );
    assert!(
        report.contains("tests did not run on wasm"),
        "the verdict must say the tests did not run, not merely that the file was skipped:\n{report}"
    );
    assert!(
        !report.contains("no verified wasm rendering"),
        "the reason must name the LEG that declined, not claim the product has no wasm \
         rendering — `almide build --target wasm` renders this same file on the structural \
         leg and reports `verified`:\n{report}"
    );
}

#[test]
fn the_same_file_with_a_declared_skip_is_a_skip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let declared = format!("// wasm:skip — pinned by tests/wasm_test_lane_wall_test.rs\n{WALLS_THE_INCUMBENT}");
    let (code, report) = test_on_wasm(dir.path(), &declared);
    assert_eq!(
        code,
        Some(0),
        "a skip the author DECLARED, with its reason in the file, stays benign:\n{report}"
    );
    assert!(
        report.contains("SKIP") && report.contains("wasm:skip"),
        "the declared skip must still be reported as a skip:\n{report}"
    );
}

/// The happy path must be untouched: classifying skips changed no verdict for a
/// file that actually runs.
#[test]
fn a_file_whose_tests_run_on_the_lane_still_passes() {
    if !wasmtime_available() {
        eprintln!("wasmtime unavailable — skipping");
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let (code, report) = test_on_wasm(dir.path(), RUNS_ON_THE_LANE);
    assert_eq!(code, Some(0), "an ordinary passing file must still pass:\n{report}");
    assert!(report.contains("1 tests passed"), "and must report its test:\n{report}");
}

/// The product lane's verdict on the SAME file, which is what makes the old
/// reason line a lie rather than merely terse.
#[test]
fn the_build_lane_renders_the_file_the_test_lane_walls() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("t_test.almd");
    std::fs::write(&file, WALLS_THE_INCUMBENT).expect("source");
    let out = Command::new(almide_bin())
        .args([
            "build",
            file.to_str().expect("path"),
            "--target",
            "wasm",
            "-o",
            dir.path().join("m.wasm").to_str().expect("path"),
        ])
        .env_remove("ALMIDE_WASM_INCUMBENT")
        .output()
        .expect("build");
    let report =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "the build lane must render this file:\n{report}");
    assert!(
        report.contains("structural leg") && report.contains("verified"),
        "and must render it on the structural leg — that is the leg the test lane never \
         asks (#2179):\n{report}"
    );
}
