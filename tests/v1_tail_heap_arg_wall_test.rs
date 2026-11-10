//! #3550: the v1 native render has a loop conversion only for the shapes
//! `try_tco_rewrite` admits. A self call it leaves as a call, in tail
//! position, with a heap argument, is not a jump: the argument is released
//! after the call (`CallFn`, then `Drop`). Such a function recursed frame by
//! frame and overflowed at a depth wasm runs in constant stack. It now walls
//! onto the standard codegen, which loop-converts it.
//!
//! #4006 class A: on the native render `try_tco_rewrite` now takes any heap
//! carry as an owned `var` slot, so these two shapes are loops on v1 too. They
//! must render, and run a million deep in constant stack. The wall itself is
//! pinned on a shape the conversion still declines
//! (tests/native_owned_tco_slots_test.rs).
use std::process::Command;

const LIST_ARG: &str = "fn go(x: List[Int], i: Int) -> List[Int] = if i <= 0 then x else go([i, 1], i - 1)\n\nfn main() -> Unit = println(int.to_string(list.len(go([1, 2, 3], 1000000))))\n";

const VARIANT_ARG: &str = "type T = | A(Int) | B\n\nfn go(x: T, i: Int) -> T = if i <= 0 then x else go(match x { A(n) => A(n + 1), B => B }, i - 1)\n\nfn main() -> Unit = match go(A(0), 1000000) {\n  A(n) => println(int.to_string(n)),\n  B => println(\"b\"),\n}\n";

/// A heap accumulator the v1 loop conversion DOES take (#3031's corpus
/// program): it must keep rendering on v1, not wall.
const STRING_ACC: &str = "fn go(n: Int, acc: String) -> String =\n  if n == 0 then acc\n  else if n % 2 == 0 then go(n - 1, acc + \"y\")\n  else go(n - 1, acc + \"x\")\n\nfn main() -> Unit = println(int.to_string(string.len(go(1000, \"\"))))\n";

fn run_native(name: &str, src: &str) -> String {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join(format!("{name}.almd"));
    std::fs::write(&file, src).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).output().expect("almide run");
    assert!(out.status.success(), "{name}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn a_heap_arg_tail_call_renders_as_a_v1_loop_and_runs_in_constant_stack() {
    for (name, src, expected) in [("list_arg", LIST_ARG, "2\n"), ("variant_arg", VARIANT_ARG, "1000000\n")] {
        if let Err(e) = almide_mir::pipeline::try_render_rust_source(src) {
            panic!("{name}: the owned-slot loop conversion should take this carry: {e:?}");
        }
        assert_eq!(run_native(name, src), expected, "{name}");
    }
}

#[test]
fn a_heap_accumulator_the_v1_loop_takes_still_renders_on_v1() {
    almide_mir::pipeline::try_render_rust_source(STRING_ACC).expect("the v1 TCO loop takes this shape");
}

/// A function with BOTH a tail self call handing a heap value on and a self
/// call whose result is used further (`examples/roman-numeral.almd`): the
/// tail calls become the loop, the other call stays a call.
const MIXED: &str = "fn go(x: List[Int], i: Int) -> Int =\n  if i <= 0 then list.len(x)\n  else if i % 250000 == 7 then 1 + go(x, i - 1)\n  else go([i, 1], i - 1)\n\nfn walk(n: Int, xs: List[String], acc: String) -> String =\n  if list.is_empty(xs) then acc\n  else {\n    let h = list.get(xs, 0) ?? \"\"\n    if n > 0 and string.len(h) == 1 then h + walk(n - 1, xs, acc)\n    else walk(n, list.drop(xs, 1), acc + h)\n  }\n\nfn main() -> Unit = {\n  println(int.to_string(go([1, 2, 3], 1000000)))\n  println(walk(3, [\"a\", \"bb\", \"c\", \"dd\"], \"<\"))\n}\n";

const MIXED_TESTS: &str = "\ntest \"deep\" {\n  assert_eq(go([1, 2, 3], 1000000), 6)\n  assert_eq(walk(3, [\"a\", \"bb\", \"c\", \"dd\"], \"<\"), \"aaa<abbcdd\")\n}\n";

/// The same mix over scalars only: no heap value crosses the tail call, the
/// render never refused it, and it keeps its plain recursion.
const MIXED_SCALAR: &str = "fn f(n: Int, a: Int) -> Int = if n <= 0 then a else if n % 2 == 0 then 1 + f(n - 1, a) else f(n - 1, a + 1)\n\nfn main() -> Unit = println(int.to_string(f(1000, 0)))\n";

/// A base that reads a binding of the loop body, with a RECORD result: the
/// in-loop result slot starts at a record built fieldwise from the declaration
/// (nested, generic and anonymous records; one base or two), a million deep.
const RECORD_RESULT: &str = r#"type Inner = { tag: String, xs: List[Int] }
type R = { a: Int, s: String, inner: Inner, f: Float, ok: Bool }
type Box[T] = { v: T, n: Int }
type Shape = Dot | Circle(Int) | Named { label: String }

fn go(n: Int, xs: List[Int]) -> R = {
  let k = list.len(xs)
  if n == 0 then R { a: k, s: "done ${k}", inner: Inner { tag: "t", xs: xs }, f: 1.5, ok: true }
  else go(n - 1, [n, k])
}

fn anon(n: Int, acc: String) -> { len: Int, text: String } = {
  let l = string.len(acc)
  if n == 0 then { len: l, text: acc } else anon(n - 1, acc + "x")
}

fn boxed(n: Int, xs: List[String]) -> Box[List[String]] = {
  let k = list.len(xs)
  if n == 0 then Box { v: xs, n: k } else boxed(n - 1, xs + ["${n}"])
}

fn two(n: Int, xs: List[Int]) -> R = {
  let k = list.len(xs)
  if n < 0 then R { a: 0 - k, s: "neg", inner: Inner { tag: "n", xs: [] }, f: 0.0, ok: false }
  else if n == 0 then R { a: k, s: "zero", inner: Inner { tag: "z", xs: xs }, f: 2.5, ok: true }
  else two(n - 2, [k] + xs)
}

fn main() -> Unit = {
  let r = go(1000000, [])
  println("${r.a} ${r.s} ${r.inner.tag} ${r.inner.xs} ${r.f} ${r.ok}")
  let a = anon(5, "")
  println("${a.len} ${a.text}")
  let b = boxed(4, [])
  println("${b.n} ${b.v}")
  let t = two(7, [])
  println("${t.a} ${t.s} ${t.inner.xs}")
  let u = two(8, [])
  println("${u.a} ${u.s} ${u.inner.xs}")
}
"#;

const RECORD_RESULT_STDOUT: &str = "2 done 2 t [1, 2] 1.5 true\n5 xxxxx\n4 [\"4\", \"3\", \"2\", \"1\"]\n-4 neg []\n4 zero [3, 2, 1, 0]\n";

/// A heap tail call the loop conversion still declines: the base reads a
/// binding of the loop body and the result (a variant) has no empty value
/// for the in-loop result slot.
const VARIANT_RESULT: &str = r#"type Shape = Dot | Circle(Int) | Tagged { label: String }

fn pick(n: Int, xs: List[Int]) -> Shape = {
  let k = list.len(xs)
  if n == 0 then Circle(k) else pick(n - 1, [n, k])
}

fn main() -> Unit = {
  match pick(5, []) {
    Circle(r) => println("circle ${r}"),
    Dot => println("dot"),
    Tagged { label } => println(label),
  }
}
"#;

/// The rendered body of the fn whose header names `name`, and its mangled name.
fn rendered_fn(out: &str, name: &str) -> (String, String) {
    let lines: Vec<&str> = out.lines().collect();
    let at = lines
        .iter()
        .position(|l| l.starts_with("fn ") && l.contains(&format!("{name}(")))
        .unwrap_or_else(|| panic!("{name} is rendered:\n{out}"));
    let mangled = lines[at]["fn ".len()..].split('(').next().unwrap_or_default().trim().to_string();
    let end = (at + 1..lines.len()).find(|&k| lines[k].starts_with("fn ")).unwrap_or(lines.len());
    (mangled, lines[at + 1..end].join("\n"))
}

#[test]
fn a_tail_call_beside_a_non_tail_self_call_loops_and_the_other_call_stays() {
    let out = almide_mir::pipeline::try_render_rust_source(MIXED)
        .unwrap_or_else(|e| panic!("the mixed shape walls: {e:?}"));
    for name in ["go", "walk"] {
        let (mangled, body) = rendered_fn(&out, name);
        assert!(body.contains("loop {"), "{name} is a loop:\n{body}");
        assert_eq!(body.matches(&format!("{mangled}(")).count(), 1, "{name} keeps only its non-tail call:\n{body}");
    }
    assert_eq!(run_native("mixed", MIXED), "6\naaa<abbcdd\n");
}

#[test]
fn the_mixed_shape_runs_under_the_native_test_render() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("mixed_test.almd");
    std::fs::write(&file, format!("{MIXED}{MIXED_TESTS}")).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["test", "--target", "native"])
        .arg(&file)
        .output()
        .expect("almide test");
    assert!(out.status.success(), "{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
}

#[test]
fn a_scalar_only_mix_keeps_its_plain_recursion() {
    let out = almide_mir::pipeline::try_render_rust_source(MIXED_SCALAR).expect("never refused");
    let (mangled, body) = rendered_fn(&out, "f");
    assert!(!body.contains("loop {"), "f is not loop-converted:\n{body}");
    assert_eq!(body.matches(&format!("{mangled}(")).count(), 2, "both self calls stay:\n{body}");
}

#[test]
fn a_record_result_whose_base_reads_a_loop_binding_loops_from_a_fieldwise_empty_slot() {
    let out = almide_mir::pipeline::try_render_rust_source(RECORD_RESULT)
        .unwrap_or_else(|e| panic!("the record result walls: {e:?}"));
    for name in ["go", "anon", "boxed", "two"] {
        let (mangled, body) = rendered_fn(&out, name);
        assert!(body.contains("loop {"), "{name} is a loop:\n{body}");
        assert_eq!(body.matches(&format!("{mangled}(")).count(), 0, "{name} keeps no self call:\n{body}");
    }
    assert_eq!(run_native("record_result", RECORD_RESULT), RECORD_RESULT_STDOUT);
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("record_result.almd");
    std::fs::write(&file, RECORD_RESULT).expect("source");
    let wasm = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(["--target", "wasm"]).output().expect("wasm run");
    assert_eq!(String::from_utf8_lossy(&wasm.stdout), RECORD_RESULT_STDOUT, "wasm");
}

#[test]
fn a_heap_tail_call_the_loop_declines_is_still_refused() {
    let err = almide_mir::pipeline::try_render_rust_source(VARIANT_RESULT).expect_err("no loop conversion for this shape");
    assert!(format!("{err:?}").contains("passes a heap value to its own tail call"), "{err:?}");
}
