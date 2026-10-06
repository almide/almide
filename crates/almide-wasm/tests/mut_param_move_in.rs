//! #3337 — a `mut` list parameter is written in place, not copied per call.
//!
//! The C-132 write-back site judged its var unique and then SHARED it into
//! the call (+1, the callee's credit), so the callee met its buffer at rc 2
//! and its first `xs[i] = v` copied the whole list. A block above the 512 KiB
//! free-list ceiling is never reused, so `touch(buf, k)` on a 1M-Float list
//! grew linear memory by 8 MB per call — 200 calls reached 2 GB, a few
//! hundred more ran out of memory — where native peaks at a few MB. The site
//! now MOVES the var into the call (writeback_move.rs): no share, the var
//! emptied until the write-back rebinds it, so the callee meets rc 1.
//!
//! Each row runs its loop at N=2 and N=200 calls on a 70 000-slot list
//! (560 KB, above the free-list ceiling, so ONE copy shows in the watermark)
//! and pins the watermark growth between the two sizes below one list: no
//! call copies. The expected stdout is native's, measured on the same
//! programs (`almide run` vs `almide run --target wasm`).

mod harness;
use harness::run_wasm;

fn run(src: &str) -> (u64, String) {
    let ir = almide_spine::s5::lower_to_ir("mut_param_move_in.almd", src).expect("front");
    let bytes = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    let out = run_wasm(&bytes).expect("run");
    assert_eq!(out.exit, 0, "{}", out.stderr);
    (out.heap_end.expect("the module exports __heap"), out.stdout)
}

const SLOTS: u32 = 70_000;
/// One list's payload: a single per-call copy anywhere crosses it.
const ONE_LIST: u64 = SLOTS as u64 * 8;

fn program(fns: &str, decl: &str, call: &str, show: &str, n: u32) -> String {
    format!(
        "{fns}\neffect fn main() -> Unit = {{\n  {decl}\n  var t = 0\n  for k in 0..<{n} {{\n    {call}\n  }}\n  println({show})\n}}\n"
    )
}

/// The row at N=2 and N=200: native's stdout, and a watermark that does
/// not grow by a list between them.
fn in_place(name: &str, fns: &str, decl: &str, call: &str, show: &str, (expect_2, expect_200): (&str, &str)) {
    let (h2, o2) = run(&program(fns, decl, call, show, 2));
    let (h200, o200) = run(&program(fns, decl, call, show, 200));
    assert_eq!(o2, expect_2, "{name}: stdout at N=2");
    assert_eq!(o200, expect_200, "{name}: stdout at N=200");
    assert!(
        h200 < h2 + ONE_LIST,
        "{name}: 198 more calls grew the heap by {} bytes — a list is {ONE_LIST}: a call copies its `mut` argument",
        h200 - h2
    );
}

const FLOATS: &str = "var buf: List[Float] = list.repeat(0.0, 70000)";
const INTS: &str = "var buf: List[Int] = list.repeat(0, 70000)";
const SHOW: &str = r#""${buf[0]} ${buf[1]} ${t}""#;

/// The issue's program, verbatim: 1M Floats × 200 calls stays near native
/// (it reached 2 GB).
#[test]
fn the_issue_program_stays_near_native() {
    let src = r#"fn touch(mut xs: List[Float], i: Int) -> Unit = { xs[i] = xs[i] + 1.0 }

effect fn main() -> Unit = {
  var buf: List[Float] = list.repeat(0.0, 1000000)
  for k in 0..<200 { touch(buf, k) }
  println(float.to_string(buf[199]))
}
"#;
    let (heap, out) = run(src);
    assert_eq!(out, "1.0\n");
    assert!(heap < 64 << 20, "peak heap {heap} bytes for one 8 MB list");
}

#[test]
fn an_index_write_through_a_mut_param_is_in_place() {
    let fns = "fn bump(mut xs: List[Float], i: Int) -> Unit = { xs[i % 2] = xs[i % 2] + 1.0 }";
    in_place("Float, Unit fn", fns, FLOATS, "bump(buf, k)", SHOW, ("1 1 0\n", "100 100 0\n"));
    let fns = r#"fn tag(mut xs: List[String], i: Int) -> Unit = { xs[i % 2] = "y" }"#;
    in_place(
        "String elements",
        fns,
        // A String slot is 4 bytes: twice the slots for one list's bytes.
        r#"var buf: List[String] = list.repeat("x", 140000)"#,
        "tag(buf, k)",
        r#""${buf[0]} ${buf[1]} ${buf[2]} ${t}""#,
        ("y y x 0\n", "y y x 0\n"),
    );
}

/// A fn that also returns a value hands back `(value, buffer)`: the
/// destructured tuple is released right after the write-back, so the next
/// call meets the buffer unshared.
#[test]
fn a_value_returning_mut_fn_is_in_place() {
    let fns = "fn bump(mut xs: List[Int], i: Int) -> Int = {\n  xs[i % 2] = xs[i % 2] + 1\n  xs[i % 2]\n}";
    in_place("tuple return", fns, INTS, "t = t + bump(buf, k)", SHOW, ("1 1 2\n", "100 100 10100\n"));
}

/// An effect callee under `!` (never erring) and with a value.
#[test]
fn an_effect_mut_fn_under_bang_is_in_place() {
    let fns = "effect fn bump(mut xs: List[Int], i: Int) -> Unit = { xs[i % 2] = xs[i % 2] + 1 }";
    in_place("effect Unit", fns, INTS, "bump(buf, k)!", SHOW, ("1 1 0\n", "100 100 0\n"));
    let fns = "effect fn bump(mut xs: List[Int], i: Int) -> Int = {\n  xs[i % 2] = xs[i % 2] + 1\n  xs[i % 2]\n}";
    in_place("effect tuple", fns, INTS, "t = t + bump(buf, k)!", SHOW, ("1 1 2\n", "100 100 10100\n"));
}

/// A `mut` param handed on to another `mut` call, as a statement and as
/// the tail; two `mut` params at once; a loop inside the callee.
#[test]
fn nested_and_multi_mut_calls_are_in_place() {
    let bump = "fn bump(mut xs: List[Int], i: Int) -> Unit = { xs[i % 2] = xs[i % 2] + 1 }";
    let fns = format!("{bump}\nfn outer(mut xs: List[Int], i: Int) -> Unit = {{\n  bump(xs, i)\n  bump(xs, i + 1)\n}}");
    in_place("nested", &fns, INTS, "outer(buf, k)", SHOW, ("2 2 0\n", "200 200 0\n"));
    let fns = format!("{bump}\nfn outer(mut xs: List[Int], i: Int) -> Unit = bump(xs, i)");
    in_place("nested tail", &fns, INTS, "outer(buf, k)", SHOW, ("1 1 0\n", "100 100 0\n"));
    let fns = "fn two(mut a: List[Int], mut b: List[Int], i: Int) -> Unit = {\n  a[i % 2] = a[i % 2] + 1\n  b[0] = b[0] + 2\n}";
    in_place(
        "two mut params",
        fns,
        "var buf: List[Int] = list.repeat(0, 70000)\n  var other: List[Int] = list.repeat(0, 70000)",
        "two(buf, other, k)",
        r#""${buf[0]} ${buf[1]} ${other[0]}""#,
        ("1 1 4\n", "100 100 400\n"),
    );
    let fns = "fn sweep(mut xs: List[Int], i: Int) -> Unit = {\n  for j in 0..<10 { xs[j] = xs[j] + i }\n}";
    in_place("loop in the callee", fns, INTS, "sweep(buf, k)", SHOW, ("1 1 0\n", "19900 19900 0\n"));
}

/// The move never lets a write show through an alias: one bound before the
/// call in the caller, and one the callee binds itself, both keep the old
/// value (C-033) — the callee's judge sees the alias's credit and copies.
#[test]
fn an_alias_still_keeps_its_value() {
    let src = r#"fn bump(mut xs: List[Int], i: Int) -> Unit = { xs[i] = xs[i] + 1 }
fn alias_in(mut xs: List[Int]) -> Unit = {
  let ys = xs
  xs[0] = 99
  println("${ys[0]} ${xs[0]}")
}
effect fn ebump(mut xs: List[Int], i: Int) -> Unit = { xs[i] = xs[i] + 1 }
effect fn main() -> Unit = {
  var a: List[Int] = [0, 0]
  let held = a
  bump(a, 0)
  ebump(a, 1)!
  alias_in(a)
  println("${a} ${held}")
}
"#;
    let (_, out) = run(src);
    assert_eq!(out, "1 99\n[99, 1] [0, 0]\n");
}

/// #3343: a closure-captured var (a C-319 cell) and a record field place
/// move in too — the cell is emptied through its address, the field slot
/// in the (unshared) record — so neither copies per call.
#[test]
fn a_captured_var_and_a_field_place_are_in_place() {
    let bump = "fn bump(mut xs: List[Int], i: Int) -> Unit = { xs[i % 2] = xs[i % 2] + 1 }";
    in_place(
        "closure-captured var",
        bump,
        "var buf: List[Int] = list.repeat(0, 70000)\n  let f = (k: Int) => bump(buf, k)",
        "f(k)",
        SHOW,
        ("1 1 0\n", "100 100 0\n"),
    );
    let fns = format!("type H = {{ xs: List[Int], n: Int }}\n{bump}");
    in_place(
        "record field place",
        &fns,
        "var h = H { xs: list.repeat(0, 70000), n: 3 }",
        "bump(h.xs, k)",
        r#""${h.xs[0]} ${h.xs[1]} ${h.n}""#,
        ("1 1 3\n", "100 100 3\n"),
    );
    let fns = format!("type H = {{ xs: List[Int], n: Int }}\n{bump}");
    in_place(
        "field place inside a closure",
        &fns,
        "var h = H { xs: list.repeat(0, 70000), n: 3 }\n  let f = (k: Int) => bump(h.xs, k)",
        "f(k)",
        r#""${h.xs[0]} ${h.xs[1]} ${h.n}""#,
        ("1 1 3\n", "100 100 3\n"),
    );
}

/// The moves never let a write show through an alias of the captured var,
/// of the field's block, or of the whole record (C-033).
#[test]
fn a_moved_cell_or_field_keeps_its_aliases() {
    let src = r#"type H = { xs: List[Int], n: Int }
fn bump(mut xs: List[Int], i: Int) -> Unit = { xs[i] = xs[i] + 1 }
fn grow(mut xs: List[Int], v: Int) -> Unit = list.push(xs, v)
effect fn main() -> Unit = {
  var buf: List[Int] = [0, 0, 0]
  let f = (k: Int) => bump(buf, k)
  let snap = buf
  f(0)
  f(1)
  println("${buf} ${snap}")
  var h = H { xs: [0, 0], n: 1 }
  let kept = h.xs
  let h2 = h
  bump(h.xs, 0)
  grow(h.xs, 5)
  println("${h.xs} ${kept} ${h2.xs} ${h.n}")
}
"#;
    let (_, out) = run(src);
    assert_eq!(out, "[1, 1, 0] [0, 0, 0]\n[1, 0, 5] [0, 0] [0, 0] 1\n");
}
