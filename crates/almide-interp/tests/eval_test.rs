//! Library-level evaluator battery.
//!
//! Each test lowers `.almd` source to a linked `IrProgram` through the SAME
//! public frontend / optimize functions the driver uses — at the pre-codegen
//! cut point (`lower_program → optimize_program → monomorphize → ir_link`) —
//! then interprets `main` and asserts on the observable `(exit, stdout,
//! stderr)`.
//!
//! Stdlib bodies are NOT loaded (the lightweight `canonicalize(..,
//! iter::empty())` recipe), so stdlib calls resolve to `Module` targets and are
//! served by the interp's bridge / native HOFs — exactly the dispatch the
//! production cut point exercises.

use almide_frontend::canonicalize;
use almide_frontend::check::Checker;
use almide_frontend::ir_link;
use almide_frontend::lower::lower_program;
use almide_interp::{Interpreter, RunStatus};
use almide_lang::lexer::Lexer;
use almide_lang::parser::Parser;
use almide_optimize::{mono, optimize};

/// Lower source to a linked `IrProgram` at the interpreter's cut point.
fn lower(src: &str) -> almide_ir::IrProgram {
    let tokens = Lexer::tokenize(src);
    let mut parser = Parser::new(tokens);
    let mut prog = parser.parse().expect("parse failed");
    assert!(parser.errors.is_empty(), "parse errors: {:?}", parser.errors);

    let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
    let mut checker = Checker::from_env(canon.env);
    let diags = checker.infer_program(&mut prog);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.level == almide_frontend::diagnostic::Level::Error)
        .collect();
    assert!(errors.is_empty(), "type errors: {:#?}", errors);

    let mut ir = lower_program(&prog, &checker.env, &checker.type_map);
    optimize::optimize_program(&mut ir);
    mono::monomorphize(&mut ir);
    ir_link::ir_link(&mut ir);
    ir
}

/// Run `main` and return `(exit, stdout, stderr)`.
fn run(src: &str) -> (i32, String, String) {
    let ir = lower(src);
    let out = Interpreter::new(&ir).run_main();
    (out.exit_code(), out.stdout, out.stderr)
}

/// Convenience: assert a clean run with the given stdout.
fn expect_out(src: &str, expected_stdout: &str) {
    let (exit, stdout, stderr) = run(src);
    assert_eq!(
        exit, 0,
        "expected clean exit; stderr=<{}> stdout=<{}>",
        stderr, stdout
    );
    assert_eq!(stdout, expected_stdout, "stdout mismatch (stderr=<{}>)", stderr);
}

/// Wrap a list of statements in a `main` that prints `expr`.
fn main_print(body: &str) -> String {
    format!("fn main() -> Unit = {{\n{}\n}}", body)
}

// ── Literals ────────────────────────────────────────────────────

#[test]
fn lit_int_and_string() {
    expect_out(&main_print(r#"  println("${42}")"#), "42\n");
    expect_out(&main_print(r#"  println("hi")"#), "hi\n");
}

#[test]
fn lit_bool_and_unit() {
    expect_out(&main_print(r#"  println("${true}")"#), "true\n");
    expect_out(&main_print(r#"  println("${false}")"#), "false\n");
}

#[test]
fn lit_float_display_is_plain() {
    // EMPIRICAL: `${3.0}` renders `3` (plain Display), not `3.0`.
    expect_out(&main_print(r#"  println("${3.0}")"#), "3\n");
    expect_out(&main_print(r#"  println("${1.5}")"#), "1.5\n");
}

// ── Arithmetic ──────────────────────────────────────────────────

#[test]
fn int_arithmetic() {
    expect_out(&main_print(r#"  println("${1 + 2 * 3}")"#), "7\n");
    expect_out(&main_print(r#"  println("${10 - 4}")"#), "6\n");
    expect_out(&main_print(r#"  println("${20 / 5}")"#), "4\n");
    expect_out(&main_print(r#"  println("${17 % 5}")"#), "2\n");
}

#[test]
fn int_div_by_zero_aborts() {
    let (exit, _stdout, stderr) = run(&main_print(
        "  let z = 0\n  println(\"${10 / z}\")",
    ));
    assert_eq!(exit, 1, "div-by-zero must abort with exit 1");
    assert!(
        stderr.contains("division by zero"),
        "stderr should carry the native message, got <{}>",
        stderr
    );
}

#[test]
fn int_mod_by_zero_aborts() {
    let (exit, _stdout, stderr) = run(&main_print(
        "  let z = 0\n  println(\"${10 % z}\")",
    ));
    assert_eq!(exit, 1);
    assert!(stderr.contains("division by zero"), "got <{}>", stderr);
}

#[test]
fn float_arithmetic_inheritance() {
    // The known float divergence lives in `{}` Display; the interp inherits
    // native's shortest-roundtrip.
    expect_out(&main_print(r#"  println("${0.1 + 0.2}")"#), "0.30000000000000004\n");
}

#[test]
fn unary_negation_and_not() {
    expect_out(&main_print(r#"  println("${-5}")"#), "-5\n");
    expect_out(&main_print(r#"  let b = true
  println("${not b}")"#), "false\n");
}

// ── Strings ─────────────────────────────────────────────────────

#[test]
fn string_concat_and_bridge() {
    expect_out(&main_print(r#"  println("a" + "b")"#), "ab\n");
    expect_out(&main_print(r#"  println(string.trim("  hi  "))"#), "hi\n");
    expect_out(&main_print(r#"  println(string.to_upper("abc"))"#), "ABC\n");
    expect_out(&main_print(r#"  println("${string.len("hello")}")"#), "5\n");
}

#[test]
fn string_interp_compound_repr() {
    // The load-bearing display contract for this branch: a List/Map/Tuple/etc.
    // in a string-interp part renders via `almide_repr`.
    expect_out(&main_print(r#"  let xs = [1, 2, 3]
  println("${xs}")"#), "[1, 2, 3]\n");
    expect_out(&main_print(r#"  let t = (1, "two", true)
  println("${t}")"#), "(1, \"two\", true)\n");
    expect_out(&main_print(r#"  let o = some(5)
  println("${o}")"#), "some(5)\n");
    expect_out(&main_print(r#"  let n: Option[Int] = none
  println("${n}")"#), "none\n");
    expect_out(&main_print(r#"  let nested = [[1], [2, 3]]
  println("${nested}")"#), "[[1], [2, 3]]\n");
}

#[test]
fn string_interp_string_in_container_is_quoted() {
    // A bare String stays raw; a String inside a container is quoted+escaped.
    expect_out(&main_print(r#"  let s = "hi"
  println("${s}")"#), "hi\n");
    expect_out(&main_print(r#"  let xs = ["a", "b"]
  println("${xs}")"#), "[\"a\", \"b\"]\n");
}

// ── Lists / Maps / Sets ─────────────────────────────────────────

#[test]
fn list_ops() {
    expect_out(&main_print(r#"  let xs = [1, 2, 3]
  println("${list.len(xs)}")"#), "3\n");
    expect_out(&main_print(r#"  let xs = [3, 1, 2]
  println("${list.sort(xs)}")"#), "[1, 2, 3]\n");
    expect_out(&main_print(r#"  let xs = [1, 2, 3]
  println("${list.reverse(xs)}")"#), "[3, 2, 1]\n");
    expect_out(&main_print(r#"  let xs = [1, 2, 3]
  println("${list.sum(xs)}")"#), "6\n");
}

#[test]
fn list_index_oob_aborts() {
    let (exit, _o, stderr) = run(&main_print(
        "  let xs = [1, 2]\n  println(\"${xs[5]}\")",
    ));
    assert_eq!(exit, 1, "OOB index must abort");
    assert!(stderr.contains("index out of bounds"), "got <{}>", stderr);
}

#[test]
fn map_literal_and_access() {
    expect_out(&main_print(r#"  let m = ["a": 1, "b": 2]
  println("${m}")"#), "[\"a\": 1, \"b\": 2]\n");
    expect_out(&main_print(r#"  let m: Map[String, Int] = [:]
  println("${m}")"#), "[:]\n");
}

#[test]
fn map_insertion_order_preserved() {
    expect_out(&main_print(r#"  let m = ["z": 1, "a": 2, "m": 3]
  println("${m}")"#), "[\"z\": 1, \"a\": 2, \"m\": 3]\n");
}

// ── Records / Variants ──────────────────────────────────────────

#[test]
fn record_construct_and_display() {
    let src = r#"
type Point = { x: Int, y: Int }
fn main() -> Unit = {
  let p = Point { x: 1, y: 2 }
  println("${p}")
}"#;
    expect_out(src, "Point { x: 1, y: 2 }\n");
}

#[test]
fn record_field_access() {
    let src = r#"
type Point = { x: Int, y: Int }
fn main() -> Unit = {
  let p = Point { x: 10, y: 20 }
  println("${p.x}")
  println("${p.y}")
}"#;
    expect_out(src, "10\n20\n");
}

#[test]
fn variant_construct_and_display() {
    let src = r#"
type Shape =
  | Circle(Int)
  | Named { label: String }
  | Dot
fn main() -> Unit = {
  let c = Circle(3)
  let nm = Named { label: "hi" }
  let d = Dot
  println("${c}")
  println("${nm}")
  println("${d}")
}"#;
    expect_out(src, "Circle(3)\nNamed { label: \"hi\" }\nDot\n");
}

#[test]
fn variant_pattern_match() {
    let src = r#"
type Shape =
  | Circle(Int)
  | Dot
fn area(s: Shape) -> Int =
  match s {
    Circle(r) => r * r,
    Dot => 0,
  }
fn main() -> Unit = {
  println("${area(Circle(4))}")
  println("${area(Dot)}")
}"#;
    expect_out(src, "16\n0\n");
}

#[test]
fn record_pattern_destructure() {
    // Shorthand `{ x, y }` binds each field via lowered Bind sub-patterns.
    let src = r#"
type P = { x: Int, y: Int }
fn f(p: P) -> Int =
  match p {
    P { x, y } => x + y,
  }
fn main() -> Unit = {
  println("${f(P { x: 3, y: 4 })}")
}"#;
    expect_out(src, "7\n");
}

#[test]
fn list_pattern_match() {
    // List patterns survive to the interp (ListPatternLowering is post-cut),
    // so the interp matches them itself.
    let src = r#"
fn describe(xs: List[Int]) -> String =
  match xs {
    [] => "empty",
    [a] => "one ${a}",
    [a, b] => "two ${a} ${b}",
    _ => "many",
  }
fn main() -> Unit = {
  let e: List[Int] = []
  println(describe(e))
  println(describe([9]))
  println(describe([1, 2]))
  println(describe([1, 2, 3]))
}"#;
    expect_out(src, "empty\none 9\ntwo 1 2\nmany\n");
}

// ── Closures: capture + HOF ─────────────────────────────────────

#[test]
fn closure_capture() {
    let src = r#"
fn main() -> Unit = {
  let base = 10
  let add = (x: Int) => x + base
  println("${add(5)}")
}"#;
    expect_out(src, "15\n");
}

#[test]
fn hof_list_map() {
    let src = r#"
fn main() -> Unit = {
  let xs = [1, 2, 3]
  let ys = xs |> list.map((n) => n + 1)
  println("${ys}")
}"#;
    expect_out(src, "[2, 3, 4]\n");
}

#[test]
fn hof_list_filter_and_fold() {
    let src = r#"
fn main() -> Unit = {
  let xs = [1, 2, 3, 4, 5]
  let evens = xs |> list.filter((n) => n % 2 == 0)
  println("${evens}")
  let total = list.fold(xs, 0, (acc, n) => acc + n)
  println("${total}")
}"#;
    expect_out(src, "[2, 4]\n15\n");
}

#[test]
fn hof_find_and_any_all() {
    let src = r#"
fn main() -> Unit = {
  let xs = [1, 2, 3]
  println("${list.find(xs, (n) => n > 1)}")
  println("${list.any(xs, (n) => n > 2)}")
  println("${list.all(xs, (n) => n > 0)}")
}"#;
    expect_out(src, "some(2)\ntrue\ntrue\n");
}

#[test]
fn hof_capturing_closure_in_map() {
    let src = r#"
fn main() -> Unit = {
  let factor = 3
  let xs = [1, 2, 3]
  let scaled = xs |> list.map((n) => n * factor)
  println("${scaled}")
}"#;
    expect_out(src, "[3, 6, 9]\n");
}

// ── Match ───────────────────────────────────────────────────────

#[test]
fn match_literal_and_wildcard() {
    let src = r#"
fn classify(n: Int) -> String =
  match n {
    0 => "zero",
    1 => "one",
    _ => "many",
  }
fn main() -> Unit = {
  println(classify(0))
  println(classify(1))
  println(classify(99))
}"#;
    expect_out(src, "zero\none\nmany\n");
}

#[test]
fn match_option_and_guard() {
    let src = r#"
fn describe(o: Option[Int]) -> String =
  match o {
    some(n) if n > 10 => "big",
    some(n) => "small",
    none => "nothing",
  }
fn main() -> Unit = {
  println(describe(some(50)))
  println(describe(some(5)))
  println(describe(none))
}"#;
    expect_out(src, "big\nsmall\nnothing\n");
}

#[test]
fn match_tuple_destructure() {
    let src = r#"
fn main() -> Unit = {
  let p = (1, 2)
  let r = match p {
    (0, _) => "x-zero",
    (a, b) => "${a}-${b}",
  }
  println(r)
}"#;
    expect_out(src, "1-2\n");
}

// ── Recursion ───────────────────────────────────────────────────

#[test]
fn recursion_factorial() {
    let src = r#"
fn fact(n: Int) -> Int =
  if n <= 1 then 1 else n * fact(n - 1)
fn main() -> Unit = {
  println("${fact(5)}")
}"#;
    expect_out(src, "120\n");
}

#[test]
fn recursion_fib() {
    let src = r#"
fn fib(n: Int) -> Int =
  if n < 2 then n else fib(n - 1) + fib(n - 2)
fn main() -> Unit = {
  println("${fib(10)}")
}"#;
    expect_out(src, "55\n");
}

// ── for / while ─────────────────────────────────────────────────

#[test]
fn for_in_range_accumulate() {
    let src = r#"
fn main() -> Unit = {
  var sum = 0
  for i in [1, 2, 3, 4] {
    sum = sum + i
  }
  println("${sum}")
}"#;
    expect_out(src, "10\n");
}

#[test]
fn for_in_range_exclusive_and_inclusive() {
    // `1..5` is exclusive (sum 1+2+3+4 = 10); `1..=3` materializes to
    // [1, 2, 3] when displayed.
    let src = r#"
fn main() -> Unit = {
  var sum = 0
  for i in 1..5 {
    sum = sum + i
  }
  println("${sum}")
  let r = 1..=3
  println("${r}")
}"#;
    expect_out(src, "10\n[1, 2, 3]\n");
}

#[test]
fn while_loop() {
    let src = r#"
fn main() -> Unit = {
  var i = 0
  var acc = 0
  while i < 5 {
    acc = acc + i
    i = i + 1
  }
  println("${acc}")
}"#;
    expect_out(src, "10\n");
}

#[test]
fn for_in_break_continue() {
    let src = r#"
fn main() -> Unit = {
  var sum = 0
  for i in [1, 2, 3, 4, 5, 6] {
    if i == 5 then { break } else { () }
    if i % 2 == 0 then { continue } else { () }
    sum = sum + i
  }
  println("${sum}")
}"#;
    // odds below 5: 1 + 3 = 4
    expect_out(src, "4\n");
}

// ── Result / Option propagation ─────────────────────────────────

#[test]
fn option_unwrap_or() {
    let src = r#"
fn main() -> Unit = {
  let a: Option[Int] = some(7)
  let b: Option[Int] = none
  println("${a ?? 0}")
  println("${b ?? 99}")
}"#;
    expect_out(src, "7\n99\n");
}

#[test]
fn unwrap_ok_continues_in_effect_main() {
    // `!` (Unwrap) on an Ok value continues; the program prints normally.
    let src = r#"
effect fn main() -> Unit = {
  let x = int.parse("41")!
  println("${x + 1}")
}"#;
    expect_out(src, "42\n");
}

#[test]
fn unwrap_err_aborts_with_inner_error() {
    // `!` (Unwrap) on an Err short-circuits; reaching `main` unhandled, the
    // program terminates with `Error: <inner>` and exit 1 — the
    // unhandled-main-error termination contract.
    let src = r#"
effect fn main() -> Unit = {
  let x = int.parse("nope")!
  println("${x}")
}"#;
    let (exit, stdout, stderr) = run(src);
    assert_eq!(exit, 1, "stdout=<{}> stderr=<{}>", stdout, stderr);
    assert!(stderr.starts_with("Error:"), "got <{}>", stderr);
    assert!(stdout.is_empty(), "no output should precede the abort, got <{}>", stdout);
}

// ── Fuel ────────────────────────────────────────────────────────

#[test]
fn fuel_exhaustion_is_clean() {
    // An unbounded loop must terminate as FuelExhausted, not hang/panic.
    let src = r#"
fn main() -> Unit = {
  var i = 0
  while true {
    i = i + 1
  }
}"#;
    let ir = lower(src);
    let out = Interpreter::new(&ir).with_fuel(10_000).run_main();
    assert_eq!(out.status, RunStatus::FuelExhausted);
}

// ── Unhandled abort termination contract ────────────────────────

#[test]
fn panic_terminates_with_error_line() {
    let src = r#"
fn main() -> Unit = {
  panic("kaboom")
}"#;
    let (exit, _o, stderr) = run(src);
    assert_eq!(exit, 1);
    assert!(stderr.contains("Error: kaboom"), "got <{}>", stderr);
}

#[test]
fn assert_eq_pass_and_fail() {
    expect_out(&main_print(r#"  assert_eq(1 + 1, 2)
  println("ok")"#), "ok\n");

    let (exit, _o, stderr) = run(&main_print("  assert_eq(1, 2)"));
    assert_eq!(exit, 1);
    assert!(stderr.contains("Error:"), "got <{}>", stderr);
}

// ── Spread record ───────────────────────────────────────────────

#[test]
fn spread_record_override() {
    let src = r#"
type Cfg = { a: Int, b: Int, c: Int }
fn main() -> Unit = {
  let base = Cfg { a: 1, b: 2, c: 3 }
  let updated = { ...base, b: 20 }
  println("${updated}")
}"#;
    expect_out(src, "Cfg { a: 1, b: 20, c: 3 }\n");
}

// ── Top-level let visibility (known gap probe) ──────────────────

#[test]
fn top_level_let_referenced_from_fn() {
    let src = r#"
let BASE: Int = 100
fn bump(x: Int) -> Int = x + BASE
fn main() -> Unit = {
  println("${bump(5)}")
}"#;
    // Documents current behavior. If top-lets aren't threaded into nested-call
    // scopes this will surface as an unbound-variable abort.
    let (exit, stdout, stderr) = run(src);
    eprintln!("exit={} stdout=<{}> stderr=<{}>", exit, stdout, stderr);
    assert_eq!(exit, 0, "top-let not visible from fn: stderr=<{}>", stderr);
    assert_eq!(stdout, "105\n");
}

// ── list.sort_by is KEY-EXTRACTION, not a comparator ─────────────
//
// The stdlib contract is `sort_by[A, B](xs, f: (A) -> B)`: `f` extracts a sort
// KEY from each element and the list is STABLY sorted by the keys' natural
// ordering (native `xs.sort_by_key(|x| f(x.clone()))`, `B: Ord`). These assert
// the interp matches that — and the byte-identical native/wasm output probed in
// /tmp/sort{i,f}.almd.

#[test]
fn sort_by_int_key_is_stable() {
    // Int key with ties: stability must preserve input order among equal keys
    // (native sort_by_key + wasm strict-`>` bubble sort both keep (3,"a") before
    // (3,"c") and (1,"b") before (1,"d")).
    let src = r#"
fn main() -> Unit = {
  let xs = [(3, "a"), (1, "b"), (3, "c"), (1, "d"), (2, "e")]
  let sorted = list.sort_by(xs, (p) => p.0)
  println("${sorted}")
}"#;
    expect_out(src, r#"[(1, "b"), (1, "d"), (2, "e"), (3, "a"), (3, "c")]
"#);
}

#[test]
fn sort_by_negative_int_key() {
    // Negative keys order by signed `i64::cmp` (not unsigned), matching native.
    let src = r#"
fn main() -> Unit = {
  let ns = [3, -1, 0, -5, 2]
  println("${list.sort_by(ns, (n) => n)}")
}"#;
    expect_out(src, "[-5, -1, 0, 2, 3]\n");
}

#[test]
fn sort_by_string_key() {
    // String key: lexicographic `str::cmp`, stable on duplicates. Matches the
    // native/wasm probe (["apple","apple","banana","cherry"]).
    let src = r#"
fn main() -> Unit = {
  let ws = ["banana", "apple", "cherry", "apple"]
  let sorted = list.sort_by(ws, (w) => w)
  println("${sorted}")
}"#;
    expect_out(src, r#"["apple", "apple", "banana", "cherry"]
"#);
}

#[test]
fn sort_by_derived_string_len_key() {
    // The canonical spec example (spec/lang/function_test.almd): sort by a
    // DERIVED Int key (string length), stable on equal lengths.
    let src = r#"
fn main() -> Unit = {
  let xs = ["bb", "a", "ccc", "dd"]
  let sorted = list.sort_by(xs, (s) => string.len(s))
  println("${sorted}")
}"#;
    // lengths 1, 2, 2, 3 → "a", then "bb"/"dd" (input order on the len-2 tie),
    // then "ccc".
    expect_out(src, r#"["a", "bb", "dd", "ccc"]
"#);
}

#[test]
fn sort_by_float_derived_key_is_stable() {
    // A *Float key* is a compile error in both backends (`f64: !Ord`), so it can
    // never reach the interp on a runnable program. But a key DERIVED to an Ord
    // type FROM float elements is fine and common: here we sort float pairs by an
    // Int key. This exercises the key-extraction path over Float-bearing elements
    // and confirms ordering + stability without ever needing a Float key.
    let src = r#"
fn main() -> Unit = {
  let pts = [(2, 3.5), (1, 9.9), (2, 1.1), (1, 0.0)]
  let sorted = list.sort_by(pts, (p) => p.0)
  println("${sorted}")
}"#;
    // Int key 1,1,2,2; ties keep input order → (1,9.9),(1,0.0),(2,3.5),(2,1.1).
    expect_out(src, "[(1, 9.9), (1, 0), (2, 3.5), (2, 1.1)]\n");
}

#[test]
fn sort_by_empty_list() {
    let src = r#"
fn main() -> Unit = {
  let xs: List[Int] = []
  println("${list.sort_by(xs, (n) => n)}")
}"#;
    expect_out(src, "[]\n");
}

// ── fan block materializes a TUPLE (both backends), not a list ───
//
// `fan { a; b; c }` (the block form) lowers to `IrExprKind::Fan` and BOTH
// backends materialize a tuple of the (auto-`?`-unwrapped) results: native
// `(j0, j1, j2)`, wasm a packed tuple. A single-expr fan is the bare value (no
// 1-tuple). Probed byte-identical native/wasm in /tmp/fan{1,2}.almd.

#[test]
fn fan_block_yields_tuple_destructurable() {
    let src = r#"
effect fn double(x: Int) -> Int = x * 2
effect fn main() -> Unit = {
  let r = fan {
    double(1)
    double(2)
    double(3)
  }
  let (a, b, c) = r
  println("${a} ${b} ${c}")
  println("${r}")
}"#;
    // Destructure proves it is a 3-tuple; the repr line proves the display form
    // `(2, 4, 6)` (a list would render `[2, 4, 6]`).
    expect_out(src, "2 4 6\n(2, 4, 6)\n");
}

#[test]
fn fan_block_single_expr_is_bare_value() {
    // Exactly one expr: the result is the bare value, NOT a 1-tuple — matching
    // both backends' single-expr fan path.
    let src = r#"
effect fn double(x: Int) -> Int = x * 2
effect fn main() -> Unit = {
  let r = fan {
    double(5)
  }
  println("${r}")
}"#;
    expect_out(src, "10\n");
}

// ── Recursion depth bound is HOST-STACK-INDEPENDENT ──────────────

#[test]
fn deep_recursion_hits_depth_guard_cleanly() {
    // `sum_to(5000)` nests 5000 interp call frames — past MAX_DEPTH (4000). The
    // evaluator runs on a dedicated big-stack thread, so this terminates as a
    // CLEAN `FuelExhausted` (the depth guard) rather than overflowing the native
    // stack of the default cargo-test worker thread (~2 MiB) and aborting the
    // whole process. This test is itself driven from that default-stack worker,
    // so it is the real regression scenario.
    let src = r#"
fn sum_to(n: Int) -> Int =
  if n <= 0 then 0 else n + sum_to(n - 1)
fn main() -> Unit = {
  println("${sum_to(5000)}")
}"#;
    let ir = lower(src);
    let out = Interpreter::new(&ir).run_main();
    assert_eq!(
        out.status,
        RunStatus::FuelExhausted,
        "deep recursion must trip the depth guard cleanly, not overflow; got {:?}",
        out.status
    );
}

#[test]
fn moderate_recursion_completes_under_depth_guard() {
    // A recursion depth WELL under MAX_DEPTH must complete normally on the
    // big-stack worker — proving the guard does not fire early and the dedicated
    // thread carries the depth a 2 MiB host stack could not (sum_to(3000) blows
    // a 2 MiB native stack but is fine on the interp's worker).
    let src = r#"
fn sum_to(n: Int) -> Int =
  if n <= 0 then 0 else n + sum_to(n - 1)
fn main() -> Unit = {
  println("${sum_to(3000)}")
}"#;
    // 3000 * 3001 / 2 = 4_501_500
    expect_out(src, "4501500\n");
}

// ── #556: Map/Set order-independent ==, NaN-compare IEEE false ──

#[test]
fn map_eq_order_independent() {
    expect_out(
        &main_print(r#"  println("${["a": 1, "b": 2] == ["b": 2, "a": 1]}")"#),
        "true\n",
    );
}

// NOTE: Set `==` order-independence is fixed in value.rs alongside Map, but
// `set.from_list` is not yet bridged in the interp (F5 latent — the fix takes
// effect when the bridge is widened), so no runtime test here.

#[test]
fn nan_compare_is_false_not_abort() {
    expect_out(
        &main_print("  let nan = 0.0 / 0.0\n  println(\"${nan < 1.0} ${nan >= 1.0}\")"),
        "false false\n",
    );
}
