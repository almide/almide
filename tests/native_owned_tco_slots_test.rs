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
    let body = out.split("fn join_acc").nth(1).expect("join_acc is rendered");
    let body = &body[..body.find("\nfn ").unwrap_or(body.len())];
    assert!(body.contains("while"), "join_acc is a loop:\n{body}");
    assert!(!body.contains("join_acc("), "join_acc no longer calls itself:\n{body}");
}

/// The #3550 wall is unchanged: a heap self tail call behind a `!` early
/// return (which the loop conversion still declines) walls with its reason.
#[test]
fn a_declined_heap_self_tail_call_still_walls_on_3550() {
    let src = r#"
effect fn f(x: Int) -> Result[Int, String] = if x < 0 then err("neg") else ok(x * 2)

effect fn build(n: Int, pos: Int, acc: List[Int]) -> Result[List[Int], String] = {
  if pos >= n then ok(acc)
  else {
    let x = f(pos)!
    build(n, pos + 1, acc + [x])
  }
}

effect fn main() -> Unit = {
  let xs = build(3, 0, [])!
  println("${list.len(xs)}")
}
"#;
    let e = render(src).expect_err("a declined heap self tail call must wall");
    assert!(e.contains("#3550"), "the wall names #3550: {e}");
}
