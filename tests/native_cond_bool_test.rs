//! A comparison read only as a branch condition renders as a Rust `bool` on
//! the v1 native render (crates/almide-mir/src/render_native_cond.rs).
//!
//! The render spelled every comparison `let mut v: i64 = (a <= b) as i64;`
//! and its branch `if v != 0 {`. At opt-level 1, LLVM optimised the
//! one-level self-inlined `depth` of
//! spec/wasm_cross/recursion_depth_within_limits.almd less well in that shape
//! than in the standard codegen's `if n <= 0 {`: 1.31x the standard
//! codegen's in-main work on scripts/check-native-v1-perf-small.sh. The pins
//! below hold the emitted shape. A comparison with any other reader keeps the
//! i64 spelling, and the programs answer the same on both legs.

use std::process::Command;

/// The perf fixture's recursion, self-inlined one level by the native legs.
const DEPTH: &str = r#"fn depth(n: Int) -> Int = if n <= 0 then 0 else 1 + depth(n - 1)

fn main() -> Unit = {
  println(int.to_string(depth(1000)))
}
"#;

/// A loop condition, a comparison that is also printed, one that is returned,
/// one passed as an argument, and a float comparison in a branch.
const MIXED: &str = r#"fn big(n: Int) -> Bool = n > 3

fn pick(c: Bool, a: Int, b: Int) -> Int = if c then a else b

fn sum_to(n: Int) -> Int = {
  var i = 0
  var acc = 0
  while i < n {
    acc = acc + i
    i = i + 1
  }
  acc
}

fn half(x: Float) -> Int = if x < 0.5 then 0 else 1

fn main() -> Unit = {
  let b = 5 >= 2
  println(bool.to_string(b))
  println(int.to_string(sum_to(10)))
  println(bool.to_string(big(7)))
  println(int.to_string(pick(3 == 4, 10, 20)))
  println(int.to_string(half(0.25) + half(0.75)))
  if b then println("yes") else println("no")
}
"#;

fn render(src: &str) -> String {
    almide_mir::pipeline::try_render_rust_source(src)
        .unwrap_or_else(|e| panic!("the v1 native render walled: {e:?}"))
}

/// The body of rendered fn `name`.
fn body<'a>(rust: &'a str, name: &str) -> &'a str {
    let start = rust.find(&format!("fn {name}(")).unwrap_or_else(|| panic!("no fn {name}:\n{rust}"));
    let rest = &rust[start..];
    &rest[..rest.find("\n}\n").map_or(rest.len(), |e| e + 3)]
}

/// `(stdout, stderr)` of `almide run` on `target`, with the route note on.
fn run(program: &str, target: &str) -> (String, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("main.almd");
    std::fs::write(&source, program).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .env("ALMIDE_VERIFIED_DEBUG", "1")
        .args(["run", source.to_str().expect("path"), "--target", target])
        .output()
        .expect("spawn almide run");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(out.status.success(), "{target} run:\n{stderr}");
    (String::from_utf8_lossy(&out.stdout).to_string(), stderr)
}

fn same_on_both_legs(program: &str, expected: &str) {
    let (native, note) = run(program, "rust");
    assert!(
        note.contains("native: v1 trust-spine render") && !note.contains("walled"),
        "the native route left the v1 trust spine:\n{note}"
    );
    assert_eq!(native, expected, "native output");
    assert_eq!(run(program, "wasm").0, expected, "wasm output");
}

#[test]
fn a_branch_only_comparison_is_a_bool() {
    let rust = render(DEPTH);
    let depth = body(&rust, "almd_depth");
    assert!(depth.contains(": bool = "), "the branch's comparison is not a bool:\n{depth}");
    assert!(!depth.contains(") as i64;"), "a branch-only comparison kept the i64 spelling:\n{depth}");
    assert!(!depth.contains(" != 0 {"), "a bool branch is still tested against 0:\n{depth}");
}

#[test]
fn a_loop_exit_comparison_is_a_bool() {
    let rust = render(MIXED);
    let sum = body(&rust, "almd_sum_to");
    assert!(sum.contains(": bool = ") && sum.contains("if !v"), "the loop exit is not a bool test:\n{sum}");
    assert!(!sum.contains(" == 0 { break; }"), "the loop exit is still tested against 0:\n{sum}");
}

#[test]
fn a_comparison_with_another_reader_stays_an_i64() {
    let rust = render(MIXED);
    // Returned, so the caller reads it as the MIR's i64 0/1.
    let big = body(&rust, "almd_big");
    assert!(big.contains(") as i64;") && !big.contains(": bool = "), "a returned comparison became a bool:\n{big}");
    // Printed, and passed as an argument.
    let main = body(&rust, "main");
    assert!(main.contains(") as i64;"), "a printed or passed comparison became a bool:\n{main}");
}

#[test]
fn the_comparisons_answer_the_same_on_both_legs() {
    same_on_both_legs(DEPTH, "1000\n");
    same_on_both_legs(MIXED, "true\n45\ntrue\n20\n1\nyes\n");
}
