//! #4956 (owner ruling 2026-11-27): a test program the verified native render
//! walls is built through the standard codegen, as `almide build` and `almide
//! run` build it and as v0.69.1's test lane did, with their note under `-v`.
//! From a71e577aec to this change the native test lane and `--json` refused
//! such a file (`native v1 test render: Unsupported(..)`) while the wasm lane
//! and `run` ran it: 30 dojo task-bank programs failed that way.
//!
//! The first test below walls on the v1 render today (a fallible tail tuple
//! with a spread record element, which the ownership self-check refuses as a
//! double release). When the v1 render covers it, the file stays a passing
//! test on every lane; only the note goes away.

use std::process::Command;

const PROGRAM: &str = r#"type Stock = { sku: String, lots: List[Int] }

fn ship(s: Stock) -> (Stock, Int)! = {
  guard list.len(s.lots) > 0 else err("empty")
  (Stock { ...s, lots: list.drop(s.lots, 1) }, list.len(s.lots))
}

fn collect(xs: List[String], acc: List[String]) -> List[String] =
  match xs {
    [] => acc,
    [h, ..rest] => collect(rest, acc + [h + "!"]),
  }

test "a fallible tail with a spread record element" {
  let (a, n) = ship(Stock { sku: "pen", lots: [1, 2, 3] })!
  assert_eq(a.lots, [2, 3])
  assert_eq(n, 3)
  assert_eq(ship(Stock { sku: "x", lots: [] }), err("empty"))
}

test "a heap value handed to its own tail call" {
  assert_eq(collect(["a", "b"], []), ["a!", "b!"])
}
"#;

fn almide(dir: &std::path::Path, args: &[&str]) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(args)
        .current_dir(dir)
        .env("ALMIDE_VERBOSE", "1")
        .output()
        .expect("almide");
    (out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn a_walled_test_program_builds_through_the_standard_codegen_on_the_native_lanes() {
    let dir = tempfile::tempdir().expect("scratch");
    std::fs::write(dir.path().join("lots_test.almd"), PROGRAM).unwrap();
    let (ok, out, err) = almide(dir.path(), &["test", "--target", "rust", "lots_test.almd"]);
    assert!(ok, "the native test lane refused the file:\n{out}{err}");
    assert!(!err.contains("native v1 test render"), "{err}");
    assert!(
        !err.contains("walled") || err.contains("verified native render walled — building via the standard codegen"),
        "a wall must take build and run's route and note:\n{err}"
    );
    let (ok, out, err) = almide(dir.path(), &["test", "--json", "lots_test.almd"]);
    assert!(ok, "--json refused the file:\n{out}{err}");
    assert!(out.contains(r#""status":"pass""#) && out.contains(r#""tests":2"#), "--json:\n{out}{err}");
    let (ok, out, err) = almide(dir.path(), &["test", "--target", "wasm", "lots_test.almd"]);
    assert!(ok, "the wasm lane:\n{out}{err}");
}
