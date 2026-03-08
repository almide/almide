use almide::lexer::Lexer;
use almide::parser::Parser;
use almide::fmt;

fn roundtrip(input: &str) -> String {
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens);
    let prog = parser.parse().expect("parse failed");
    fmt::format_program(&prog)
}

#[test]
fn fmt_simple_fn() {
    let out = roundtrip("module app\nfn add(a: Int, b: Int) -> Int = a + b");
    assert!(out.contains("fn add(a: Int, b: Int) -> Int ="));
    assert!(out.contains("a + b"));
}

#[test]
fn fmt_variant_type() {
    let out = roundtrip("module app\ntype Color =\n  | Red\n  | Green\n  | Blue");
    assert!(out.contains("Red"));
    assert!(out.contains("Green"));
    assert!(out.contains("Blue"));
}

#[test]
fn fmt_tuple_type() {
    let out = roundtrip("module app\nfn pair() -> (Int, String) = (1, \"x\")");
    assert!(out.contains("(Int, String)"));
}

#[test]
fn fmt_if_expr() {
    let out = roundtrip("module app\nfn f(x: Int) -> Int = if x > 0 then x else 0 - x");
    assert!(out.contains("if x > 0 then"));
    assert!(out.contains("else"));
}

#[test]
fn fmt_match() {
    let out = roundtrip("module app\nfn f(x: Option[Int]) -> Int = match x {\n  some(v) => v\n  none => 0\n}");
    assert!(out.contains("match x {"));
    assert!(out.contains("some(v) =>"));
    assert!(out.contains("none =>"));
}

#[test]
fn fmt_preserves_module_and_imports() {
    let out = roundtrip("import fs\nimport http\nfn f() -> Int = 1");
    assert!(out.contains("import fs\n"));
    assert!(out.contains("import http\n"));
}

#[test]
fn fmt_test_decl() {
    let out = roundtrip("module app\ntest \"basic\" {\n  assert(true)\n}");
    assert!(out.contains("test \"basic\""));
}

#[test]
fn fmt_lambda() {
    let out = roundtrip("module app\nfn f() -> fn(Int) -> Int = fn(x) => x + 1");
    assert!(out.contains("fn(x) => x + 1"));
}

#[test]
fn fmt_for_in() {
    let out = roundtrip("module app\neffect fn main(_a: List[String]) -> Result[Unit, String] = {\n  for x in xs {\n    println(x)\n  }\n  ok(())\n}");
    assert!(out.contains("for x in xs {"));
}

#[test]
fn fmt_tuple_pattern() {
    let out = roundtrip("module app\nfn f(p: (Int, Int)) -> Int = match p {\n  (a, b) => a + b\n}");
    assert!(out.contains("(a, b) =>"));
}

// ---- Comment preservation ----

#[test]
fn fmt_top_level_comments() {
    let out = roundtrip("// file header\nmodule app\n// a utility\nfn f() -> Int = 1");
    assert!(out.contains("// file header"));
    assert!(out.contains("// a utility"));
}

#[test]
fn fmt_inline_block_comments() {
    let out = roundtrip("module app\nfn f() -> Int = {\n  // step 1\n  let x = 1\n  // step 2\n  x + 1\n}");
    assert!(out.contains("// step 1"));
    assert!(out.contains("// step 2"));
}

#[test]
fn fmt_match_arm_comments() {
    let out = roundtrip("module app\nfn f(x: Option[Int]) -> Int = match x {\n  // handle value\n  some(v) => v\n  // handle empty\n  none => 0\n}");
    assert!(out.contains("// handle value"));
    assert!(out.contains("// handle empty"));
}

#[test]
fn fmt_for_in_comments() {
    let out = roundtrip("module app\neffect fn main(_a: List[String]) -> Result[Unit, String] = {\n  for x in xs {\n    // process item\n    println(x)\n  }\n  ok(())\n}");
    assert!(out.contains("// process item"));
}
