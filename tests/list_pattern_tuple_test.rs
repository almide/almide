//! #2600: list patterns under a TUPLE pattern, run on both legs.
//!
//! Exhaustiveness used to read such a list pattern as a zero-field tuple, so
//! `([], [])` looked covered by `([x, ..xt], [y, ..yt])` (a false E014) and a
//! missing length went unseen. Once check accepts these exhaustive matches,
//! both legs have to run them. The Rust leg's `ListPatternLowering` desugars
//! a tuple arm into a length-test `if`. That desugaring used to drop the
//! arm's guard: `([x, ..], _) if x > 5` matched every non-empty first list
//! natively, while wasm honoured the guard (native printed 1, wasm printed 0).
//! It also ended the chain in `()`, which rustc rejects under a non-`Unit`
//! match. So these cells assert that the two legs agree.
//!
//! They live here rather than as a `spec/` test because the MIR brick walls
//! list patterns (C-332), and a corpus file with them would only move the
//! walled-real baseline.

use std::process::Command;

/// The issue's reachable-arm program, both arm orders.
const REST_FIRST_AND_LAST: &str = r#"
fn rest_first(xs: List[Int], ys: List[Int]) -> Int =
  match (xs, ys) {
    ([x, ..xt], [y, ..yt]) => x + y,
    ([], []) => 0,
    ([], _) => 1,
    (_, []) => 2,
  }

fn rest_last(xs: List[Int], ys: List[Int]) -> Int =
  match (xs, ys) {
    ([], []) => 0,
    ([], _) => 1,
    (_, []) => 2,
    ([x, ..xt], [y, ..yt]) => x + y,
  }

effect fn main() -> Unit = {
  println("${rest_first([1], [2])} ${rest_first([], [])} ${rest_first([], [7])} ${rest_first([7], [])}")
  println("${rest_last([1], [2])} ${rest_last([], [])} ${rest_last([], [7])} ${rest_last([7], [])}")
}
"#;

/// Exact lengths plus a rest pattern cover every list with no wildcard, and a
/// rest pattern with a refutable element is one case among the others.
const LENGTH_CLASSES: &str = r#"
fn by_length(xs: List[Int]) -> String = match xs {
  [] => "empty",
  [a] => "one ${int.to_string(a)}",
  [a, b] => "two ${int.to_string(a + b)}",
  [a, b, c, ..] => "many ${int.to_string(a + b + c)}",
}

fn head_option(xs: List[Option[Int]]) -> Int = match xs {
  [] => 0,
  [some(a), ..] => a,
  [none, ..] => -1,
}

effect fn main() -> Unit = {
  println("${by_length([])} ${by_length([4])} ${by_length([1, 2])} ${by_length([1, 2, 3, 4])}")
  println("${head_option([some(5), none])} ${head_option([none])} ${head_option([])}")
}
"#;

/// Refutable ELEMENT patterns, under a list and under a tuple. The Rust leg's
/// desugaring read each as a wildcard: `[none, ..]` matched every non-empty
/// list natively (native `-1`, wasm `7` on v0.63.1), and `some(x)` beside a
/// list in a tuple bound nothing. They are now tested by a residual match.
const REFUTABLE_ELEMENTS: &str = r#"
fn head(xs: List[Option[Int]]) -> Int = match xs {
  [none, ..] => -1,
  [some(a), ..] => a,
  _ => 0,
}

fn pair(a: Option[Int], xs: List[Int]) -> Int = match (a, xs) {
  (some(x), [y, ..]) => x + y,
  (none, [y, ..]) => y,
  (_, []) => 0,
}

fn first_is_zero(xs: List[Int], ys: List[Int]) -> Int = match (xs, ys) {
  ([0, ..], _) => 100,
  ([x, ..], [y]) => x + y,
  _ => -1,
}

fn labelled(xs: List[(Int, String)]) -> String = match xs {
  [(0, s), ..] => "zero ${s}",
  [(n, s), ..] => "${int.to_string(n)} ${s}",
  [] => "none",
}

fn nested(xs: List[List[Int]]) -> Int = match xs {
  [[a, ..], ..] => a,
  [[], ..] => -2,
  [] => -1,
}

effect fn main() -> Unit = {
  println("${head([some(5), none])} ${head([none])} ${head([])}")
  println("${pair(some(1), [2])} ${pair(none, [5])} ${pair(some(3), [])}")
  println("${first_is_zero([0, 9], [1])} ${first_is_zero([4], [5])} ${first_is_zero([4], [5, 6])}")
  println("${labelled([(0, "a")])} ${labelled([(2, "b")])} ${labelled([])}")
  println("${nested([[7, 8]])} ${nested([[]])} ${nested([])}")
}
"#;

/// A guard on a tuple arm that holds a list pattern: the guard runs after the
/// binds and a failing guard falls through to the later arms.
const GUARDED_TUPLE_ARM: &str = r#"
fn f(xs: List[Int], ys: List[Int]) -> Int =
  match (xs, ys) {
    ([x, ..xt], _) if x > 5 => 1,
    ([x, ..xt], [y]) if y > x => 2,
    _ => 0,
  }

effect fn main() -> Unit = println("${f([1], [2])} ${f([9], [])} ${f([1], [3])} ${f([], [3])} ${f([4], [1])}")
"#;

fn almide_bin() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// Build and run `program` on both legs; assert they agree and return stdout.
fn agree(program: &str, label: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("main.almd");
    std::fs::write(&source, program).expect("source");
    let mut first: Option<String> = None;
    for target in ["rust", "wasm"] {
        let artifact = dir.path().join(if target == "rust" { "native" } else { "m.wasm" });
        let built = Command::new(almide_bin())
            .args(["build", source.to_str().expect("path"), "--target", target, "-o", artifact.to_str().expect("path")])
            .env_remove("ALMIDE_COMPONENT_P3")
            .output()
            .expect("build");
        assert!(built.status.success(), "{label}/{target} build:\n{}", String::from_utf8_lossy(&built.stderr));
        let mut command = if target == "rust" {
            Command::new(&artifact)
        } else {
            let mut c = Command::new("wasmtime");
            c.arg("run").arg(&artifact);
            c
        };
        let out = command.output().expect("run");
        assert!(out.status.success(), "{label}/{target} exited {:?}: {}", out.status.code(), String::from_utf8_lossy(&out.stderr));
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        match &first {
            Some(native) => assert_eq!(&stdout, native, "{label}: the wasm leg answered differently from native"),
            None => first = Some(stdout),
        }
    }
    first.expect("one leg ran")
}

#[test]
fn an_exhaustive_tuple_of_list_patterns_runs_on_both_legs() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    assert_eq!(agree(REST_FIRST_AND_LAST, "rest-order"), "3 0 1 2\n3 0 1 2\n");
}

#[test]
fn length_classes_cover_a_list_without_a_wildcard() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    assert_eq!(agree(LENGTH_CLASSES, "length-classes"), "empty one 4 two 3 many 6\n5 -1 0\n");
}

#[test]
fn a_guarded_tuple_arm_with_a_list_pattern_honours_its_guard() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    assert_eq!(agree(GUARDED_TUPLE_ARM, "guarded"), "2 1 2 0 0\n");
}

#[test]
fn refutable_element_patterns_are_tested_on_both_legs() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    assert_eq!(
        agree(REFUTABLE_ELEMENTS, "refutable-elements"),
        "5 -1 0\n3 5 0\n100 9 -1\nzero a 2 b none\n7 -2 -1\n"
    );
}
