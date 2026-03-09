use almide::lexer::Lexer;
use almide::parser::Parser;
use almide::emit_ts;

fn parse_and_emit_js(input: &str) -> String {
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens);
    let prog = parser.parse().expect("parse failed");
    emit_ts::emit_js_with_modules(&prog, &[])
}

fn parse_and_emit_ts(input: &str) -> String {
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens);
    let prog = parser.parse().expect("parse failed");
    emit_ts::emit_with_modules(&prog, &[])
}

/// Strip the runtime preamble, return only user code
fn user_code(output: &str) -> &str {
    if let Some(pos) = output.find("// ---- End Runtime ----") {
        output[pos + "// ---- End Runtime ----".len()..].trim()
    } else {
        output.trim()
    }
}

// ---- Function declarations ----

#[test]
fn emit_simple_fn_js() {
    let out = parse_and_emit_js("module app\nfn add(a: Int, b: Int) -> Int = a + b");
    let code = user_code(&out);
    assert!(code.contains("function add(a, b)"));
    assert!(code.contains("(a + b)"));
}

#[test]
fn emit_simple_fn_ts() {
    let out = parse_and_emit_ts("module app\nfn add(a: Int, b: Int) -> Int = a + b");
    let code = user_code(&out);
    assert!(code.contains("function add(a: number, b: number): number"));
}

// ---- Type declarations ----

#[test]
fn emit_variant_unit_const() {
    let out = parse_and_emit_js("module app\ntype Color =\n  | Red\n  | Green\n  | Blue");
    let code = user_code(&out);
    // Unit variants should be const, not functions
    assert!(code.contains("const Red = { tag: \"Red\" }"));
    assert!(code.contains("const Green = { tag: \"Green\" }"));
    assert!(code.contains("const Blue = { tag: \"Blue\" }"));
}

#[test]
fn emit_variant_tuple_constructor() {
    let out = parse_and_emit_js("module app\ntype Shape =\n  | Circle(Float)\n  | Rect(Float, Float)");
    let code = user_code(&out);
    assert!(code.contains("function Circle(_0)"));
    assert!(code.contains("function Rect(_0, _1)"));
}

// ---- Expressions ----

#[test]
fn emit_if_expr() {
    let out = parse_and_emit_js("module app\nfn abs(n: Int) -> Int = if n < 0 then 0 - n else n");
    let code = user_code(&out);
    assert!(code.contains("(n < 0)"));
}

#[test]
fn emit_match() {
    let out = parse_and_emit_js("module app\nfn check(x: Option[Int]) -> Int = match x {\n  some(v) => v\n  none => 0\n}");
    let code = user_code(&out);
    assert!(code.contains("!== null"));
}

#[test]
fn emit_string_interpolation() {
    let out = parse_and_emit_js("module app\nfn greet(name: String) -> String = \"hello ${name}\"");
    let code = user_code(&out);
    assert!(code.contains("`hello ${name}`"));
}

#[test]
fn emit_list_concat() {
    let out = parse_and_emit_js("module app\nfn foo() -> List[Int] = [1, 2] ++ [3, 4]");
    let code = user_code(&out);
    assert!(code.contains("__concat"));
}

#[test]
fn emit_pipe() {
    let out = parse_and_emit_js("module app\nfn foo(xs: List[Int]) -> List[Int] = xs |> list.filter(fn(x) => x > 0)");
    let code = user_code(&out);
    assert!(code.contains("__almd_list.filter"));
}

// ---- Range ----

#[test]
fn emit_range_exclusive() {
    let out = parse_and_emit_js("module app\nfn foo() -> List[Int] = 0..5");
    let code = user_code(&out);
    assert!(code.contains("Array.from"));
}

#[test]
fn emit_for_in_range() {
    let out = parse_and_emit_js("module app\neffect fn main(_a: List[String]) -> Result[Unit, String] = {\n  for i in 0..10 {\n    println(int.to_string(i))\n  }\n  ok(())\n}");
    let code = user_code(&out);
    // For-in with range should use optimized loop
    assert!(code.contains("for (let"));
}

// ---- Tuples ----

#[test]
fn emit_tuple_literal() {
    let out = parse_and_emit_js("module app\nfn pair() -> (Int, String) = (1, \"x\")");
    let code = user_code(&out);
    assert!(code.contains("[1, \"x\"]"));
}

#[test]
fn emit_tuple_type_ts() {
    let out = parse_and_emit_ts("module app\nfn pair() -> (Int, String) = (1, \"x\")");
    let code = user_code(&out);
    assert!(code.contains("[number, string]"));
}

// ---- Tests ----

#[test]
fn emit_test_js() {
    let out = parse_and_emit_js("module app\ntest \"basic\" {\n  assert(true)\n}");
    let code = user_code(&out);
    assert!(code.contains("test basic ... ok"));
    assert!(code.contains("test basic ... FAILED"));
    assert!(code.contains("process.exitCode = 1"));
}

#[test]
fn emit_test_ts() {
    let out = parse_and_emit_ts("module app\ntest \"basic\" {\n  assert(true)\n}");
    let code = user_code(&out);
    assert!(code.contains("Deno.test(\"basic\""));
}

// ---- Module calls ----

#[test]
fn emit_stdlib_module_call() {
    let out = parse_and_emit_js("module app\nfn foo(s: String) -> String = string.trim(s)");
    let code = user_code(&out);
    assert!(code.contains("__almd_string.trim(s)"));
}

// ---- Entry point ----

#[test]
fn emit_main_entry() {
    let out = parse_and_emit_js("module app\neffect fn main(args: List[String]) -> Result[Unit, String] = {\n  println(\"hi\")\n  ok(())\n}");
    let code = user_code(&out);
    assert!(code.contains("// ---- Entry Point ----"));
    assert!(code.contains("main("));
}

// ---- Runtime presence ----

#[test]
fn emit_js_has_runtime() {
    let out = parse_and_emit_js("module app");
    assert!(out.contains("// ---- Almide Runtime (JS) ----"));
    assert!(out.contains("__almd_string"));
    assert!(out.contains("__almd_list"));
    assert!(out.contains("__almd_http"));
}

#[test]
fn emit_ts_has_runtime() {
    let out = parse_and_emit_ts("module app");
    assert!(out.contains("// ---- Almide Runtime ----"));
    assert!(out.contains("__almd_string"));
    assert!(out.contains("__almd_http"));
}
