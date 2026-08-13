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
    almide_driver::link_ir(&mut ir);
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
    // `1..<5` is exclusive (sum 1+2+3+4 = 10); `1...3` materializes to
    // [1, 2, 3] when displayed.
    let src = r#"
fn main() -> Unit = {
  var sum = 0
  for i in 1..<5 {
    sum = sum + i
  }
  println("${sum}")
  let r = 1...3
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

/// #1341: a NESTED variant match bound to a `let` under an explicit Result
/// carrier, then unwrapped with `!`. The bind is heap-typed, so `branch_lift`
/// hoists the whole match into a synthesized helper fn before the interpreter's
/// cut point — the arm binders of BOTH levels have to survive that hoist and
/// still be readable by the arm bodies. This is C-269's third vote, kept here
/// as a fast standalone check (the fixture leg needs two full backend builds).
#[test]
fn nested_variant_match_in_bind_position() {
    let src = r#"
fn pair_sum(xs: List[Int]) -> Result[Int, String] = {
  let r: Result[Int, String] = match list.get(xs, 0) {
    some(a) => match list.get(xs, 1) {
      some(b) => ok(a + b),
      none => err("need a second element"),
    },
    none => err("need a first element"),
  }
  let total = r!
  ok(total)
}

effect fn show(xs: List[Int]) -> Unit = {
  match pair_sum(xs) {
    ok(v) => println("sum ${v}"),
    err(e) => println("sum failed: ${e}"),
  }
}

effect fn main() -> Unit = {
  show([10, 32])!
  show([10])!
  show([])!
}"#;
    expect_out(
        src,
        "sum 42\nsum failed: need a second element\nsum failed: need a first element\n",
    );
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

// ── #561: huge ranges are fuel-bounded, never materialized ──

#[test]
fn huge_range_for_in_is_fuel_bounded_not_oom() {
    // A 2-billion range with a SMALL fuel budget must terminate as
    // FuelExhausted in O(fuel) steps and O(1) memory — the eager
    // materialization used to allocate the whole range (tens of GB) and
    // abort the process before the first fuel check.
    let src = "fn main() -> Unit = {\n  var s = 0\n  for i in 0..<2000000000 {\n    s = s + 1\n  }\n  println(\"${s}\")\n}";
    let ir = lower(src);
    let out = almide_interp::Interpreter::new(&ir).with_fuel(10_000).run_main();
    assert!(
        matches!(out.status, almide_interp::RunStatus::FuelExhausted),
        "huge range must FuelExhaust, got {:?}",
        out.status
    );
}

// ── #1022: mut-parameter copy-in/copy-out (C-132's interp leg) ──

#[test]
fn mut_param_list_push_reaches_the_caller() {
    // Statement position, value-returning callee in Bind position, and a
    // nested-expression position — every call position writes back.
    let src = r#"
fn push9(mut v: List[Int], x: Int) -> Int = {
  list.push(v, x)
  list.len(v) - 1
}

fn main() -> Unit = {
  var v = [1, 2]
  push9(v, 7)
  let i = push9(v, 8)
  let t = 100 + push9(v, 9)
  println("${v} ${i} ${t}")
}
"#;
    expect_out(src, "[1, 2, 7, 8, 9] 3 104\n");
}

#[test]
fn mut_param_copy_out_is_cow_for_aliases() {
    // An alias bound BEFORE the call keeps its own elements — the copy-out
    // assigns the callee's final buffer into the caller's slot only (the same
    // value-semantics promise as C-033 alias_cow).
    let src = r#"
fn grow(mut v: List[Int]) -> Unit = list.push(v, 9)

fn main() -> Unit = {
  var v = [1]
  let snap = v
  grow(v)
  println("${v} ${snap}")
}
"#;
    expect_out(src, "[1, 9] [1]\n");
}

#[test]
fn mut_param_record_field_argument_writes_back() {
    // The record-FIELD argument form (`push9(b.items, 7)`) — the backends
    // FieldAssign the buffer back; the interp's copy-out mirrors it.
    let src = r#"
type Box = { items: List[Int] }

fn add(mut v: List[Int], x: Int) -> Unit = list.push(v, x)

fn main() -> Unit = {
  var b = Box { items: [1] }
  add(b.items, 5)
  add(b.items, 6)
  println("${b.items}")
}
"#;
    expect_out(src, "[1, 5, 6]\n");
}

#[test]
fn mut_param_chains_through_a_nested_callee() {
    // A callee forwarding its OWN mut param to another mut-param fn: the inner
    // copy-out lands on the outer callee's frame binding, and the outer
    // copy-out carries it the rest of the way to the caller.
    let src = r#"
fn inner(mut v: List[Int]) -> Unit = list.push(v, 2)
fn outer(mut v: List[Int]) -> Unit = {
  list.push(v, 1)
  inner(v)
}

fn main() -> Unit = {
  var v: List[Int] = []
  outer(v)
  println("${v}")
}
"#;
    expect_out(src, "[1, 2]\n");
}

#[test]
fn mut_param_map_insert_reaches_the_caller() {
    // The C-061 shape: a `mut Map` param mutated by `map.insert`, insert-new
    // and overwrite both landing in the caller's slot. (The read-back
    // `map.get_or` route is covered by the real `mut_map_param` fixture in the
    // 3-way gate — its Int-key self-host body is out of scope at this
    // harness's no-mono cut.)
    let src = r#"
fn put(mut m: Map[Int, Int], k: Int, v: Int) -> Unit = map.insert(m, k, v)

fn main() -> Unit = {
  var counts: Map[Int, Int] = [:]
  put(counts, 1, 100)
  put(counts, 1, 111)
  put(counts, 2, 200)
  println("${map.len(counts)}")
  println("${counts}")
}
"#;
    expect_out(src, "2\n[1: 111, 2: 200]\n");
}



// ── the __fallible_* carriers (ADR-0006's fallibility-polymorphic HOFs) ──
//
// These are what `list.map(xs, (x) => f(x)!)` INSTANTIATES: the checker sees
// the callback propagate and swaps in the fallible form, whose contract is
// first-err short-circuit. Every case is written the way a user writes it (the
// plain HOF name with a `!` inside), so these pin the rewrite as well as the
// carrier.
//
// EVERY expectation below was MEASURED from the native backend first — the
// interp must MATCH the backends, not be "correct" in the abstract (see
// crates/almide-interp/CLAUDE.md). That is why the err payload reads
// `invalid digit found in string`: it is Rust's own `parse::<i64>` message,
// surfaced verbatim, not a wrapper the interp is free to invent.
//
// Each member gets BOTH polarities — a full pass and a first-err — because the
// short-circuit is the only thing separating these from their plain siblings
// (CLAUDE.md: extend a family by matrix, never point-wise).

/// The renderers the cases share: a `Result` has no bare `repr`, so each shape
/// is matched and printed explicitly.
const SHOW: &str = "\
fn si(r: Result[List[Int], String]) -> String = match r {
  ok(xs) => \"ok[\" + list.join(list.map(xs, (n) => int.to_string(n)), \",\") + \"]\",
  err(e) => \"err:\" + e,
}
fn ss(r: Result[List[String], String]) -> String = match r {
  ok(xs) => \"ok[\" + list.join(xs, \",\") + \"]\",
  err(e) => \"err:\" + e,
}
fn so(r: Result[String?, String]) -> String = match r {
  ok(o) => \"ok:\" + (o ?? \"<none>\"),
  err(e) => \"err:\" + e,
}
fn sn(r: Result[Int, String]) -> String = match r {
  ok(n) => \"ok:\" + int.to_string(n),
  err(e) => \"err:\" + e,
}
fn su(r: Result[Unit, String]) -> String = match r {
  ok(_) => \"ok:unit\",
  err(e) => \"err:\" + e,
}
";

const PARSE_ERR: &str = "err:invalid digit found in string\n";

fn expect_try(body: &str, expected: &str) {
    expect_out(&format!("{}{}", SHOW, main_print(body)), expected);
}

#[test]
fn try_map_ok_and_first_err() {
    expect_try(
        "  println(si(list.map([\"1\", \"2\"], (s) => int.parse(s)!)))",
        "ok[1,2]\n",
    );
    expect_try(
        "  println(si(list.map([\"1\", \"zz\", \"3\"], (s) => int.parse(s)!)))",
        PARSE_ERR,
    );
}

#[test]
fn try_filter_ok_and_first_err() {
    expect_try(
        "  println(ss(list.filter([\"1\", \"2\"], (s) => int.parse(s)! > 1)))",
        "ok[2]\n",
    );
    expect_try(
        "  println(ss(list.filter([\"1\", \"zz\"], (s) => int.parse(s)! > 1)))",
        PARSE_ERR,
    );
}

#[test]
fn try_filter_map_ok_and_first_err() {
    expect_try(
        "  println(ss(list.filter_map([\"1\", \"2\"], (s) => if int.parse(s)! > 1 then some(s) else none)))",
        "ok[2]\n",
    );
    expect_try(
        "  println(ss(list.filter_map([\"zz\"], (s) => if int.parse(s)! > 1 then some(s) else none)))",
        PARSE_ERR,
    );
}

#[test]
fn try_flat_map_ok_and_first_err() {
    expect_try(
        "  println(si(list.flat_map([\"1\", \"2\"], (s) => [int.parse(s)!, 0])))",
        "ok[1,0,2,0]\n",
    );
    expect_try(
        "  println(si(list.flat_map([\"1\", \"zz\"], (s) => [int.parse(s)!, 0])))",
        PARSE_ERR,
    );
}

#[test]
fn try_find_hit_stops_before_a_later_error() {
    // The HIT ends the traversal, so the trailing "zz" is NEVER parsed: the
    // short-circuit is on the find, not only on the failure.
    expect_try(
        "  println(so(list.find([\"1\", \"2\", \"zz\"], (s) => int.parse(s)! == 2)))",
        "ok:2\n",
    );
    expect_try(
        "  println(so(list.find([\"1\"], (s) => int.parse(s)! == 9)))",
        "ok:<none>\n",
    );
    expect_try(
        "  println(so(list.find([\"zz\", \"1\"], (s) => int.parse(s)! == 1)))",
        PARSE_ERR,
    );
}

#[test]
fn try_fold_ok_and_first_err() {
    expect_try(
        "  println(sn(list.fold([\"1\", \"2\"], 0, (acc, s) => acc + int.parse(s)!)))",
        "ok:3\n",
    );
    expect_try(
        "  println(sn(list.fold([\"1\", \"zz\"], 0, (acc, s) => acc + int.parse(s)!)))",
        PARSE_ERR,
    );
}

#[test]
fn try_each_ok_and_first_err() {
    // `each` is the effect-only member, so the short-circuit is visible as the
    // MISSING "e3" line: the tail element is never reached.
    expect_try(
        "  println(su(list.each([\"1\", \"2\"], (s) => println(\"e\" + int.to_string(int.parse(s)!)))))",
        "e1\ne2\nok:unit\n",
    );
    expect_try(
        "  println(su(list.each([\"zz\", \"3\"], (s) => println(\"e\" + int.to_string(int.parse(s)!)))))",
        PARSE_ERR,
    );
}

// ── The effect-fn Result carrier (#1366) ─────────────────────────────────────
//
// An `effect fn f() -> T` has ABI return type `Result[T, String]`, and BOTH
// backends materialize that carrier. The interpreter used to hand the success
// value back BARE while modelling the failure channel as `Result(Err(..))`, so
// the subject of `match <effect call> { ok(v) => .., err(e) => .. }` was a
// plain scalar, no arm matched, and the run aborted with "non-exhaustive
// match" — a WRONG third vote against two agreeing backends on one of
// ADR-0008's sanctioned consumption spellings, which is worse than an honest
// skip. Found by the 3-way oracle while building #1341's fixture.

#[test]
fn matching_an_effect_call_sees_the_ok_carrier() {
    let (exit, out, err) = run(
        "effect fn pick(xs: List[Int]) -> Int = {\n\
         \x20 let r: Result[Int, String] = match list.get(xs, 0) {\n\
         \x20   some(a) => ok(a + 1),\n\
         \x20   none => err(\"no first\"),\n\
         \x20 }\n\
         \x20 r!\n\
         }\n\
         effect fn main() -> Unit = {\n\
         \x20 match pick([10]) {\n\
         \x20   ok(v) => println(\"sum \" + int.to_string(v)),\n\
         \x20   err(e) => println(\"fail \" + e),\n\
         \x20 }\n\
         }\n",
    );
    assert_eq!((exit, out.as_str()), (0, "sum 11\n"), "stderr: {err}");
}

#[test]
fn matching_an_effect_call_sees_the_err_carrier() {
    let (exit, out, err) = run(
        "effect fn pick(xs: List[Int]) -> Int = {\n\
         \x20 let r: Result[Int, String] = match list.get(xs, 0) {\n\
         \x20   some(a) => ok(a + 1),\n\
         \x20   none => err(\"no first\"),\n\
         \x20 }\n\
         \x20 r!\n\
         }\n\
         effect fn main() -> Unit = {\n\
         \x20 match pick([]) {\n\
         \x20   ok(v) => println(\"sum \" + int.to_string(v)),\n\
         \x20   err(e) => println(\"fail \" + e),\n\
         \x20 }\n\
         }\n",
    );
    assert_eq!((exit, out.as_str()), (0, "fail no first\n"), "stderr: {err}");
}

/// The carrier must NOT be applied twice. A declared-Result effect fn emits
/// `Result<T, E>` (spec §3: no double wrap), so `!` on its call still yields
/// the payload rather than a nested `Ok(Ok(..))`.
#[test]
fn a_declared_result_effect_fn_is_not_double_wrapped() {
    let (exit, out, err) = run(
        "effect fn twice(n: Int) -> Result[Int, String] =\n\
         \x20 if n < 0 then err(\"neg\") else ok(n * 2)\n\
         effect fn main() -> Unit = {\n\
         \x20 println(int.to_string(twice(21)!))\n\
         \x20 match twice(3) { ok(v) => println(\"ok \" + int.to_string(v)), err(e) => println(e) }\n\
         }\n",
    );
    assert_eq!((exit, out.as_str()), (0, "42\nok 6\n"), "stderr: {err}");
}
/// The NEIGHBOUR of the effect-fn gap, measured rather than assumed: a fallible
/// LAMBDA (ADR-0009 — its `!` falls into the lambda's own `Result[T, String]`
/// channel) already hands the carrier back, so `match` over its call was never
/// broken. `Closure` carries no return type, so the fix above could not have
/// covered this path even if it had needed covering — pinning it here records
/// that it does not.
#[test]
fn matching_a_fallible_lambda_call_already_sees_the_carrier() {
    let (exit, out, err) = run(
        "effect fn main() -> Unit = {\n\
         \x20 let g = (s: String) => int.parse(s)! * 2\n\
         \x20 match g(\"21\") {\n\
         \x20   ok(v) => println(\"ok \" + int.to_string(v)),\n\
         \x20   err(e) => println(\"err \" + e),\n\
         \x20 }\n\
         }\n",
    );
    assert_eq!((exit, out.as_str()), (0, "ok 42\n"), "stderr: {err}");
}

/// The carrier must not disturb the OTHER sanctioned ways to consume an effect
/// call's Result. Wrapping the success value changes what every consumption
/// site sees, so each is exercised rather than read: `!` propagation, `??`
/// fallback, `?` to Option, the value in an interpolation, and passing the
/// unwrapped payload as an argument.
#[test]
fn every_consumption_site_still_sees_the_payload() {
    let (exit, out, err) = run(
        "effect fn half(n: Int) -> Int = if n < 0 then err(\"neg\") else ok(n / 2)\n\
         fn twice(n: Int) -> Int = n * 2\n\
         effect fn main() -> Unit = {\n\
         \x20 println(int.to_string(half(10)!))\n\
         \x20 println(int.to_string(half(-1) ?? 99))\n\
         \x20 println(int.to_string(half(8) ?? 99))\n\
         \x20 println(\"interp \" + int.to_string(half(6)!))\n\
         \x20 println(int.to_string(twice(half(4)!)))\n\
         }\n",
    );
    assert_eq!(
        (exit, out.as_str()),
        (0, "5\n99\n4\ninterp 3\n4\n"),
        "stderr: {err}"
    );
}
