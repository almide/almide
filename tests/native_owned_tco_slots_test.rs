//! #4006 class A (#3550): on the certified native render a self tail call may
//! carry ANY heap value, not only an append accumulator. The shared IR loop
//! conversion (`try_tco_rewrite`) gives it an owned `var` slot that each self
//! call rebinds, so the function becomes a `while` loop that the existing
//! ownership rules certify. There is no new proof rule, and the wasm leg's
//! lowering is unchanged (the mode is native-only).
//!
//! Unconverted heap self tail calls still wall. Converted calls snapshot
//! every changed argument before any carried slot is replaced.

fn render(src: &str) -> Result<String, String> {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).map_err(|e| e.to_string()))
        .unwrap()
        .join()
        .unwrap()
}

/// A variant list carried by a self tail call (`yy`, a pattern-bound tail)
/// with a String accumulator beside it: loop-converted and rendered.
#[test]
fn a_variant_carry_self_tail_call_renders_as_a_loop() {
    let src = r#"
type SList = | SCons(String, SList) | SNil

fn join_acc(ys: SList, sep: String, acc: String) -> String = match ys {
  SCons(y, yy) => join_acc(yy, sep, acc + sep + y),
  SNil => acc,
}

effect fn main() -> Unit = {
  println(join_acc(SCons("b", SCons("c", SNil)), ",", "a"))
}
"#;
    let out = render(src).unwrap_or_else(|e| panic!("the variant carry walls: {e}"));
    // The rendered fn's header line names it (whatever its mangled spelling).
    let at = out
        .lines()
        .position(|l| l.trim_start().starts_with("fn ") && l.contains("join_acc"))
        .unwrap_or_else(|| panic!("join_acc is rendered:\n{out}"));
    let lines: Vec<&str> = out.lines().collect();
    let header = lines[at];
    let name = header.trim_start()["fn ".len()..].split('(').next().unwrap().trim().to_string();
    let end = (at + 1..lines.len()).find(|&k| lines[k].starts_with("fn ")).unwrap_or(lines.len());
    let body = lines[at + 1..end].join("\n");
    assert!(body.contains("while") || body.contains("loop"), "join_acc is a loop:\n{header}\n{body}");
    assert!(!body.contains(&format!("{name}(")), "join_acc no longer calls itself:\n{header}\n{body}");
}

/// A heap self tail call behind a `!` early return: the `err` the desugar
/// leaves in the tail is delivered through the loop's result accumulator with
/// the function's own Result type (the cap-as-tag block, tag @16), so the
/// rendered Rust compiles and both exits are right, a million deep.
#[test]
fn a_bang_early_return_self_tail_call_renders_and_runs_both_exits() {
    let src = r#"
effect fn f(x: Int) -> Result[Int, String] = if x == 777777 then err("bad ${x}") else ok(x % 7)

effect fn build(n: Int, pos: Int, acc: List[Int]) -> Result[List[Int], String] = {
  if pos >= n then ok(acc)
  else {
    let x = f(pos)!
    build(n, pos + 1, acc + [x])
  }
}

effect fn main() -> Unit = {
  println(match build(1000000, 0, []) { ok(xs) => "ok ${list.len(xs)}", err(e) => "err ${e}" })
  println(match build(1000, 0, []) { ok(xs) => "ok ${list.len(xs)}", err(e) => "err ${e}" })
}
"#;
    render(src).unwrap_or_else(|e| panic!("the `!` early-return carry walls: {e}"));
    assert_eq!(run_native(src), "err bad 777777\nok 1000\n");
}

/// The scalar-result tuple-destructure gate (2026-07-03: `let (cp, l) =
/// __trim_cp(addr)` then `__trim_start(addr + l, end)`, whose loop spun on
/// the wasm leg) is lifted on the native render only. The same shape — the
/// destructured step feeding the self call — and a destructured heap tuple
/// carry (`fib_help`) loop and terminate with the right answers, a million
/// deep.
#[test]
fn a_destructuring_scalar_tail_call_loops_and_terminates_on_native() {
    let src = r#"
fn step(t: String, i: Int) -> (Int, Int) = if string.slice(t, i, i + 1) == "x" then (1, 2) else (0, 1)

fn scan(t: String, i: Int, n: Int) -> Int = if i >= string.len(t) then n else {
  let (c, l) = step(t, i)
  scan(t, i + l, n + c)
}

fn count(k: Int, i: Int, n: Int) -> Int = if i >= k then n else {
  let (c, l) = if i % 3 == 0 then (1, 2) else (0, 1)
  count(k, i + l, n + c)
}

fn fib_help(n: Int, acc: (Int, Int)) -> Int = {
  let (cur, next) = acc
  if n == 0 then cur else fib_help(n - 1, (next, cur + next))
}

effect fn main() -> Unit = {
  println("${scan("axbxxc", 0, 0)} ${count(1000000, 0, 0)} ${fib_help(50, (0, 1))}")
}
"#;
    render(src).unwrap_or_else(|e| panic!("the destructuring tail calls wall: {e}"));
    assert_eq!(run_native(src), "2 333334 12586269025\n");
}

/// Two heap carries read each other. Both arguments must observe the old
/// values before either slot is replaced.
#[test]
fn cyclic_heap_carries_snapshot_both_old_values() {
    let src = r#"
fn go(n: Int, a: String, b: String) -> String = if n == 0 then a + b else go(n - 1, a + b, b + a)

effect fn main() -> Unit = println(go(3, "x", "y"))
"#;
    render(src).expect("owned snapshots admit a cyclic carry");
    assert_eq!(run_native(src), "xyyxyxxyyxxyxyyx\n");
}

#[test]
fn short_circuit_heap_tail_calls_run_a_million_deep() {
    let src = r#"
fn all(n: Int, s: String) -> Bool = n == 0 or (string.len(s) == 1 and all(n - 1, s))
fn any(n: Int, s: String) -> Bool = n > 0 and (string.len(s) == 2 or any(n - 1, s))
fn main() -> Unit = { println("${all(1000000, "x")} ${any(1000000, "x")}") }
"#;
    render(src).expect("short-circuit tails loop with heap arguments");
    assert_eq!(run_native(src), "true false\n");
}

fn run_native(src: &str) -> String {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("p.almd");
    std::fs::write(&file, src).expect("source");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).output().expect("almide run");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}
