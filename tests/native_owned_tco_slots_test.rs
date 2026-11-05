//! #4006 class A (#3550): on the certified native render a self tail call may
//! carry ANY heap value, not only an append accumulator. The shared IR loop
//! conversion (`try_tco_rewrite`) gives it an owned `var` slot that each self
//! call rebinds, so the function becomes a `while` loop that the existing
//! ownership rules certify. There is no new proof rule, and the wasm leg's
//! lowering is unchanged (the mode is native-only).
//!
//! The #3550 wall stays as strict as it was: a heap self tail call the
//! conversion still declines (a `!` early return inside the recursion) walls.

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

/// The #3550 wall is unchanged: a heap self tail call the loop conversion
/// still declines (two heap carries that read each other, which no update
/// order can stage) walls with its reason.
#[test]
fn a_declined_heap_self_tail_call_still_walls_on_3550() {
    let src = r#"
fn go(n: Int, a: String, b: String) -> String = if n == 0 then a + b else go(n - 1, a + b, b + a)

effect fn main() -> Unit = println(go(3, "x", "y"))
"#;
    let e = render(src).expect_err("a declined heap self tail call must wall");
    assert!(e.contains("#3550"), "the wall names #3550: {e}");
}

fn run_native(src: &str) -> String {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("p.almd");
    std::fs::write(&file, src).expect("source");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).output().expect("almide run");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}
