//! `return EXPR` is reported once and the parse continues with EXPR.
//!
//! `return` is not Almide (the last expression is the value), and the hint
//! says so. But the parser used to stop at it: the rest of the statement was
//! dropped, so `fn f(x: Int) -> Int = { return x + 1 }` also reported
//! `expected Int but got Unit`. Inside a loop it lost its place entirely:
//! `Expected top-level declaration … (got None 'none')`. Three errors for one
//! mistake send a model repairing the program after the wrong two. The
//! `return` is now skipped after its diagnostic, and EXPR is parsed as the
//! value it was meant to be.
use std::process::Command;

fn errors(src: &str) -> Vec<String> {
    let (ok, errs) = check(src);
    assert!(!ok, "a `return` is still an error");
    errs
}

fn check(src: &str) -> (bool, Vec<String>) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("ret.almd");
    std::fs::write(&file, src).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg("check").arg(&file).output().expect("check");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text.lines().filter(|l| l.starts_with("error")).map(str::to_string).collect())
}

const MAIN: &str = "\neffect fn main() -> Unit = println(int.to_string(f(3)))\n";

#[test]
fn a_tail_return_is_one_error() {
    let errs = errors(&format!("fn f(x: Int) -> Int = {{\n  return x + 1\n}}\n{MAIN}"));
    assert_eq!(errs.len(), 1, "{errs:#?}");
    assert!(errs[0].contains("'return' is not needed"), "{errs:#?}");
}

#[test]
fn a_return_in_a_branch_is_one_error() {
    let errs = errors(&format!("fn f(x: Int) -> Int = {{\n  if x > 0 then return 1 else return 2\n}}\n{MAIN}"));
    assert_eq!(errs.len(), 2, "one per `return`, nothing else: {errs:#?}");
    assert!(errs.iter().all(|e| e.contains("'return' is not needed")), "{errs:#?}");
}

#[test]
fn a_return_in_a_loop_does_not_lose_the_parse() {
    let errs = errors(
        "fn first_hit(xs: List[String], f: (String) -> Bool) -> Option[String] = {\n  for x in xs {\n    if f(x) then return some(x)\n  }\n  none\n}\n\neffect fn main() -> Unit = println(first_hit([\"a\"], (s) => true) ?? \"-\")\n",
    );
    assert!(errs[0].contains("'return' is not needed"), "{errs:#?}");
    assert!(!errs.iter().any(|e| e.contains("Expected top-level declaration")), "the parser keeps its place: {errs:#?}");
}

/// `let mut x = …` (Rust) is reported once and binds as the `var` it means,
/// so a later read of `x` is not also `undefined variable 'x'`.
#[test]
fn let_mut_is_one_error_and_binds_the_var() {
    let errs = errors(&format!("fn f(x: Int) -> Int = {{\n  let mut y = x\n  y = y + 1\n  y\n}}\n{MAIN}"));
    assert_eq!(errs.len(), 1, "{errs:#?}");
    assert!(errs[0].contains("'let mut' is not valid"), "{errs:#?}");
}

/// A brace-style `if` (`if (c) { a } else { b }`, C / Rust / JS) is reported
/// once — Almide's `if` takes `then` — and the braces parse as the branches,
/// instead of losing the parse at `else` and also failing the fn's type.
#[test]
fn a_brace_style_if_is_one_error() {
    for body in ["if (x > 0) { 1 } else { 2 }", "if x > 0 { 1 } else { 2 }"] {
        let errs = errors(&format!("fn f(x: Int) -> Int = {{\n  {body}\n}}\n{MAIN}"));
        assert_eq!(errs.len(), 1, "{body}: {errs:#?}");
        assert!(errs[0].contains("then"), "{body}: the error names `then`: {errs:#?}");
    }
}

/// `===` / `!==` (JavaScript) lexes as `==` / `!=` and a stray `=`; the stray
/// `=` was reported as "Assignments return Unit", which sends the reader to
/// look for an assignment, and the comparison was lost. It is now one error
/// naming `==` / `!=`, and the comparison parses.
#[test]
fn a_triple_equals_is_one_error_naming_double_equals() {
    for (op, want) in [("===", "=="), ("!==", "!=")] {
        let errs = errors(&format!("fn f(x: Int) -> Int = {{\n  if x {op} 3 then 1 else 2\n}}\n{MAIN}"));
        assert_eq!(errs.len(), 1, "{op}: {errs:#?}");
        assert!(errs[0].contains(&format!("'{want}'")), "{op}: names {want}: {errs:#?}");
        assert!(!errs[0].contains("Assignments"), "{op}: {errs:#?}");
    }
}

/// `elif` / `elsif` / `elseif` (Python / Ruby / PHP) is reported once and read
/// as the `else if` it means — before, the chain stopped there, the `then`
/// branch's type had no `else` to agree with, and the fn's type failed too.
#[test]
fn an_elif_is_one_error_and_reads_as_else_if() {
    for kw in ["elif", "elsif", "elseif"] {
        let errs = errors(&format!("fn f(x: Int) -> Int = {{\n  if x > 0 then 1 {kw} x < 0 then 2 else 3\n}}\n{MAIN}"));
        assert_eq!(errs.len(), 1, "{kw}: {errs:#?}");
        assert!(errs[0].contains("else if"), "{kw}: names `else if`: {errs:#?}");
    }
}

/// `c ? a : b` (C / JS / Java) is reported once and read as
/// `if c then a else b` — before, `?` took the last operand as a Result→Option
/// conversion, the parse stopped at the `a`, and the fn's type failed too.
#[test]
fn a_ternary_is_one_error_and_reads_as_if() {
    let errs = errors(&format!("fn f(x: Int) -> Int = x > 0 ? 1 : 2\n{MAIN}"));
    assert_eq!(errs.len(), 1, "{errs:#?}");
    assert!(errs[0].contains("if") && errs[0].contains("then"), "names `if … then … else`: {errs:#?}");
    // the postfix `?` that IS Almide keeps its meaning
    let (ok, errs) = check(&format!("fn f(x: Int) -> Int = int.parse(int.to_string(x))? ?? 0\n{MAIN}"));
    assert!(ok && errs.is_empty(), "{errs:#?}");
}

/// `&&`, `||` and prefix `!` (C / JS / Rust) are reported once each and read
/// as `and` / `or` / `not` — before, the parse stopped at the operator and
/// the fn's type failed too.
#[test]
fn c_style_boolean_operators_are_one_error_and_read_as_words() {
    for (body, op) in [
        ("if x > 0 && x < 9 then 1 else 0", "&&"),
        ("if x > 0 || x < -9 then 1 else 0", "||"),
        ("if !(x > 0) then 1 else 0", "!"),
    ] {
        let errs = errors(&format!("fn f(x: Int) -> Int = {{\n  {body}\n}}\n{MAIN}"));
        assert_eq!(errs.len(), 1, "{op}: {errs:#?}");
        assert!(errs[0].contains(&format!("'{op}' is not valid")), "{op}: {errs:#?}");
    }
}

/// A match arm written `p -> e` (Haskell / OCaml / Kotlin `when`), `p: e`
/// (Python dicts, YAML) or `case p => e` (Scala / Swift) is reported once per
/// arm — each is a place to edit — and parsed as `p => e`. Before, the parse
/// stopped at the first arm and the fn's type failed too.
#[test]
fn a_foreign_match_arm_is_one_error_and_reads_as_fat_arrow() {
    for (body, needle) in [
        ("match x { 1 -> 2, _ -> 3 }", "=>"),
        ("match x {\n    1: 2,\n    _: 3,\n  }", "=>"),
        ("match x { case 1 => 2, case _ => 3 }", "case"),
    ] {
        let errs = errors(&format!("fn f(x: Int) -> Int = {{\n  {body}\n}}\n{MAIN}"));
        assert_eq!(errs.len(), 2, "one per arm: {body}: {errs:#?}");
        assert!(errs.iter().all(|e| e.contains(needle)), "{body}: names `{needle}`: {errs:#?}");
    }
}

/// A top-level `def` / `function` / `func` / `fun` (Python / JS / Go /
/// Kotlin) heading a function, or `const` / `val` heading a constant, is
/// reported once and the declaration parses as the `fn` / `let` it means —
/// before, the declaration was dropped and every use of it failed too.
#[test]
fn a_foreign_declaration_keyword_is_one_error_and_keeps_the_declaration() {
    for src in [
        "def f(x: Int) -> Int = x + 1",
        "function f(x: Int) -> Int = x + 1",
        "func f(x: Int) -> Int = x + 1",
        "fun f(x: Int) -> Int = x + 1",
        "const K = 3\nfn f(x: Int) -> Int = x + K",
        "val K = 3\nfn f(x: Int) -> Int = x + K",
    ] {
        let errs = errors(&format!("{src}\n{MAIN}"));
        assert_eq!(errs.len(), 1, "{src}: {errs:#?}");
        assert!(errs[0].contains("is not a keyword in Almide"), "{src}: {errs:#?}");
    }
}

/// Inside a body, `const y = …` / `val y = …` is reported once and binds `y`
/// as the `let` it means — before, `const` read as an undefined variable and
/// `y` as an assignment to an undefined binding, three errors in all.
#[test]
fn a_local_const_is_one_error_and_binds() {
    for kw in ["const", "val"] {
        let errs = errors(&format!("fn f(x: Int) -> Int = {{\n  {kw} y = x + 1\n  y\n}}\n{MAIN}"));
        assert_eq!(errs.len(), 1, "{kw}: {errs:#?}");
        assert!(errs[0].contains(kw) && errs[0].contains("let"), "{kw}: names `let`: {errs:#?}");
    }
}

/// A Rust macro call (`println!(…)`, `format!(…)`, `panic!(…)`) is reported
/// once, naming the plain call — before, the `!` read as error propagation
/// on a function value and three errors followed.
#[test]
fn a_rust_macro_call_is_one_error_and_reads_as_the_call() {
    let src = "effect fn main() -> Unit = {\n  println!(\"hi\")\n}\n";
    let errs = errors(src);
    assert_eq!(errs.len(), 1, "{errs:#?}");
    assert!(errs[0].contains("println("), "names the plain call: {errs:#?}");
}

/// A lowercase or Rust-style builtin type (`int`, `i64`, `string`, `bool`,
/// `f64`, `u8`) is reported once, naming the Almide type, and read as it —
/// before, a param's type error dropped the fn and a let's dropped the bind,
/// so every use failed too.
#[test]
fn a_foreign_builtin_type_is_one_error_naming_the_almide_type() {
    for (src, almide) in [
        ("fn f(x: int) -> int = x + 1", "Int"),
        ("fn f(x: i64) -> Int = x + 1", "Int"),
        ("fn f(x: Int) -> Int = {\n  let y: int = x\n  y\n}", "Int"),
        ("fn g(s: string) -> Bool = s == \"\"\nfn f(x: Int) -> Int = x", "String"),
        ("fn g(b: bool) -> Bool = b\nfn f(x: Int) -> Int = x", "Bool"),
        ("fn g(v: f64) -> Float = v\nfn f(x: Int) -> Int = x", "Float"),
        ("fn g(v: u8) -> UInt8 = v\nfn f(x: Int) -> Int = x", "UInt8"),
    ] {
        let errs = errors(&format!("{src}\n{MAIN}"));
        let want = if src.contains("int) -> int") { 2 } else { 1 };
        assert_eq!(errs.len(), want, "{src}: one per spelling: {errs:#?}");
        assert!(errs.iter().all(|e| e.contains(&format!("'{almide}'"))), "{src}: names `{almide}`: {errs:#?}");
    }
}
