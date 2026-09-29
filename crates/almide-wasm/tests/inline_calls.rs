//! Small scalar-fn inlining on the structural leg (#2980, src/inline_calls.rs).
//! Each case is a way an inlined call could stop behaving like the call it
//! replaced; each runs on the wasm leg and on the interpreter (the
//! definition), and pins the expected text, so a bug in both legs cannot
//! agree silently. `ALMIDE_INLINE_OFF` is the pass's A/B switch.

mod harness;
use harness::run_wasm;

fn run(name: &str, src: &str) -> (i32, String, String) {
    let file = format!("{name}.almd");
    let ir = almide_spine::s5::lower_to_ir(&file, src).expect("lowers");
    let bytes = almide_wasm::emit_program(&ir).expect("emits");
    let r = run_wasm(&bytes).expect("runs");
    (r.exit, r.stdout, r.stderr)
}

fn check(name: &str, src: &str, expected: &str) {
    let (exit, stdout, stderr) = run(name, src);
    assert_eq!(exit, 0, "{name}: wasm exited {exit}: {stderr}");
    let interp = almide_spine::s5::run_file(&format!("{name}.almd"), src).expect("interp runs");
    assert_eq!(interp.exit, 0, "{name}: oracle run failed: {}", interp.stderr);
    assert_eq!(stdout, interp.stdout, "{name}: wasm and the interpreter disagree");
    assert_eq!(stdout, expected, "{name}: both legs agree on the wrong text");
}

/// Arguments evaluate once each, left to right, before the body — even when
/// the callee reads a param twice or never.
#[test]
fn arguments_keep_call_order_and_count() {
    check(
        "inl_order",
        r#"effect fn tick(n: Int) -> Int = {
  println("tick ${n}")
  n
}

fn twice_first(a: Int, b: Int) -> Int = a + a

effect fn main() -> Unit = {
  let r = twice_first(tick(1)!, tick(2)!)
  println("${r}")
}
"#,
        "tick 1\ntick 2\n2\n",
    );
}

/// A trapping argument to a param read only on the untaken branch still
/// traps (#2947, the fixture int_div_by_zero_inlined_arg on this pass): the
/// shared speculation rule binds it instead of substituting it.
#[test]
fn a_trapping_argument_to_an_unread_param_still_traps() {
    let src = r#"fn den(x: Int) -> Int = x - x

fn pick(c: Bool, x: Int) -> Int = if c then x else 0

fn main() -> Unit = {
  let z = den(7)
  println(int.to_string(pick(false, 10 / z)))
}
"#;
    let (exit, stdout, _) = run("inl_trap", src);
    let interp = almide_spine::s5::run_file("inl_trap.almd", src).expect("interp runs");
    assert_eq!(exit, interp.exit, "the trap must survive inlining (interp exit {}, stdout {:?})", interp.exit, interp.stdout);
    assert_ne!(exit, 0, "division by zero must abort, got stdout {stdout:?}");
}

/// Nested tiny callees, lets inside them, and a callee used in a `let` and in
/// an operand position.
#[test]
fn nested_callees_and_their_lets() {
    check(
        "inl_nested",
        r#"fn sq(x: Int) -> Int = x * x
fn sum_sq(a: Int, b: Int) -> Int = {
  let s = sq(a) + sq(b)
  s
}

effect fn main() -> Unit = {
  var acc = 0
  for i in 0..<5 {
    let t = sum_sq(i, i + 1)
    acc = acc + t + sq(i)
  }
  println("${acc}")
}
"#,
        "115\n",
    );
}

/// A `scoped fn` keeps its call (its region window is an obligation, #1997).
#[test]
fn a_scoped_fn_is_not_inlined_and_still_answers() {
    check(
        "inl_scoped",
        r#"scoped fn scale(x: Int) -> Int = x * 3

effect fn main() -> Unit = {
  println("${scale(4)} ${scoped { scale(4) }}")
}
"#,
        "12 12\n",
    );
}
