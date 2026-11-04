//! #4456: the native legs inline a small pure scalar recursion into itself
//! once (`almide_ir::self_inline`), so LLVM — whose inliner never follows a
//! recursive edge — sees a node's grandchild calls in one function and merges
//! the identical ones. fib(35) went from 293M to 11M instructions.
//!
//! What is pinned here:
//! * the shape, on both native renderers: the v1 rungs (what `almide build`
//!   ships for these programs) and the standard codegen (`--target rust`);
//! * the negative control: `ALMIDE_SELF_INLINE_OFF=1` gives back the
//!   accumulator loop with one call per node;
//! * the declines: a tail self-call, Int division, an effect fn;
//! * the semantics: the same programs built with and without the rewrite
//!   print the same bytes, including Int wrap-around and Float rounding, and
//!   the known values.
//!
//! The speed itself is the `fib` VICTORY row of scripts/check-perf-ratio.sh,
//! ablated with the same knob.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("almd_self_inline_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The body of `fn {name}(` in `rust`.
fn body_of(rust: &str, name: &str) -> String {
    let head = format!("fn {name}(");
    let at = rust.find(&head).unwrap_or_else(|| panic!("no `{head}` in:\n{rust}"));
    let body = &rust[at..];
    body[..body.find("\n}\n").expect("fn body ends")].to_string()
}

fn calls(body: &str, name: &str) -> usize {
    body.matches(&format!("{name}(")).count() - 1 // minus the signature
}

/// The v1 native render (`almide build`'s route for these programs).
fn v1(src: &str) -> String {
    almide_mir::pipeline::try_render_rust_source(src).expect("renders on the certified path")
}

/// The standard codegen's Rust (`--target rust`), with `env` set.
fn v0(name: &str, src: &str, env: &[(&str, &str)]) -> String {
    let file = scratch(name).join("prog.almd");
    std::fs::write(&file, src).unwrap();
    let mut cmd = Command::new(almide());
    cmd.args([file.to_str().unwrap(), "--target", "rust"]);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("almide emit");
    assert!(out.status.success(), "emit failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

const FIB: &str = "fn fib(n: Int) -> Int = if n < 2 then n else fib(n - 1) + fib(n - 2)\n\nfn main() -> Unit = println(int.to_string(fib(20)))\n";

#[test]
fn the_v1_render_inlines_fib_into_itself_once() {
    let body = body_of(&v1(FIB), "almd_fib");
    // Two copies of the body, each keeping its two calls; no accumulator loop.
    assert_eq!(calls(&body, "almd_fib"), 4, "{body}");
    assert!(!body.contains("loop {"), "the accumulator loop is not taken:\n{body}");
}

#[test]
fn the_standard_codegen_inlines_fib_and_the_knob_takes_it_back() {
    let on = body_of(&v0("v0-on", FIB, &[]), "fib");
    assert_eq!(calls(&on, "fib"), 4, "{on}");
    // Negative control: with the rewrite off, the accumulator loop (#2577)
    // is back — one call per node.
    let off = body_of(&v0("v0-off", FIB, &[("ALMIDE_SELF_INLINE_OFF", "1")]), "fib");
    assert_eq!(calls(&off, "fib"), 1, "{off}");
}

#[test]
fn a_tail_self_call_is_left_to_the_loop() {
    let src = "fn sum(n: Int, acc: Int) -> Int = if n <= 0 then acc else sum(n - 1, acc + n)\n\nfn main() -> Unit = println(int.to_string(sum(100, 0)))\n";
    let body = body_of(&v0("tail", src, &[]), "sum");
    assert!(calls(&body, "sum") == 0, "the TCO loop, not a copy:\n{body}");
}

#[test]
fn int_division_declines() {
    // A trap (division by zero) and a runtime helper: outside the language.
    let src = "fn h(n: Int) -> Int = if n < 2 then n else h(n / 2) + h(n / 3)\n\nfn main() -> Unit = println(int.to_string(h(1000)))\n";
    let body = body_of(&v1(src), "almd_h");
    assert_eq!(calls(&body, "almd_h"), 2, "{body}");
}

#[test]
fn an_effect_fn_declines() {
    let src = "effect fn effrec(n: Int) -> Int = if n < 2 then n else effrec(n - 1)! + effrec(n - 2)!\n\neffect fn main() -> Unit = println(int.to_string(effrec(15)!))\n";
    let body = body_of(&v0("effect", src, &[]), "effrec");
    assert_eq!(calls(&body, "effrec"), 2, "{body}");
}

/// Every admitted shape: wrapping `+`/`*`, negation, Float `/` (IEEE), a Bool
/// param, a `let`, two params, a self-call nested in a self-call's argument,
/// four self-calls. Values that wrap many times and Float results that would
/// change under any reassociation.
const SHAPES: &str = r#"fn fib(n: Int) -> Int = if n < 2 then n else fib(n - 1) + fib(n - 2)

fn wrapf(n: Int) -> Int = if n < 1 then 9223372036854775807 else wrapf(n - 5) * 3 + wrapf(n - 1)

fn neg(n: Int) -> Int = if n < 2 then -n else -neg(n - 1) - neg(n - 2)

fn fl(x: Float, d: Int) -> Float = if d <= 0 then x else fl(x * 0.5, d - 1) / 3.0 + fl(x + 1.0, d - 2) - 0.1

fn par(n: Int, b: Bool) -> Int = {
  let m = n - 1
  if n <= 0 then (if b then 1 else 0) else par(m, not b) + par(m - 1, b) * 2
}

fn nest(n: Int) -> Int = if n <= 0 then 0 else nest(n - 1 - nest(n - 1) * 0) + 1

fn four(n: Int) -> Int = if n < 4 then n else four(n - 1) + four(n - 2) - four(n - 3) * four(n - 4)

fn main() -> Unit = {
  println(int.to_string(fib(30)))
  println(int.to_string(fib(1)))
  println(int.to_string(fib(-7)))
  println(int.to_string(wrapf(60)))
  println(int.to_string(neg(25)))
  println(float.to_string(fl(1.7, 18)))
  println(int.to_string(par(16, true)))
  println(int.to_string(nest(12)))
  println(int.to_string(four(22)))
}
"#;

fn run_native(name: &str, src: &str, env: &[(&str, &str)]) -> String {
    let dir = scratch(name);
    let file = dir.join("prog.almd");
    std::fs::write(&file, src).unwrap();
    let bin = dir.join("prog");
    let mut cmd = Command::new(almide());
    cmd.args(["build", file.to_str().unwrap(), "-o", bin.to_str().unwrap()]);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("almide build");
    assert!(out.status.success(), "build failed: {}", String::from_utf8_lossy(&out.stderr));
    let run = Command::new(&bin).output().expect("run");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    String::from_utf8_lossy(&run.stdout).into_owned()
}

#[test]
fn the_rewrite_changes_no_printed_byte() {
    // Every function above is admitted (or the test proves nothing).
    let rust = v1(SHAPES);
    for f in ["fib", "wrapf", "neg", "fl", "par", "nest", "four"] {
        let body = body_of(&rust, &format!("almd_{f}"));
        assert!(!body.contains("loop {"), "`{f}` was not self-inlined:\n{body}");
    }
    let on = run_native("diff-on", SHAPES, &[]);
    let off = run_native("diff-off", SHAPES, &[("ALMIDE_SELF_INLINE_OFF", "1")]);
    assert_eq!(on, off, "self-inlined vs not");
    let lines: Vec<&str> = on.lines().collect();
    assert_eq!(&lines[..3], &["832040", "1", "-7"], "{on}");
}
