use almide::lexer::Lexer;
use almide::parser::Parser;
use almide::emit_ts;

fn parse_and_emit_js(input: &str) -> String {
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens);
    let prog = parser.parse().expect("parse failed");
    // Type-check and lower to IR for codegen
    let mut checker = almide::check::Checker::new();
    checker.check_program(&mut prog.clone());
    let ir = almide::lower::lower_program(&prog, &checker.expr_types, &checker.env);
    emit_ts::emit_js_with_modules(&ir)
}

fn parse_and_emit_ts(input: &str) -> String {
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens);
    let prog = parser.parse().expect("parse failed");
    let mut checker = almide::check::Checker::new();
    checker.check_program(&mut prog.clone());
    let ir = almide::lower::lower_program(&prog, &checker.expr_types, &checker.env);
    emit_ts::emit_with_modules(&ir)
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
    let out = parse_and_emit_js("module app\nfn foo(xs: List[Int]) -> List[Int] = xs |> list.filter((x) => x > 0)");
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

// ---- Records ----

#[test]
fn emit_record_type_js() {
    let out = parse_and_emit_js("module app\ntype Point = { x: Int, y: Int }\nfn origin() -> Point = { x: 0, y: 0 }");
    let code = user_code(&out);
    assert!(code.contains("x:") || code.contains("\"x\""));
}

#[test]
fn emit_record_member_access() {
    let out = parse_and_emit_js("module app\ntype Point = { x: Int, y: Int }\nfn getx(p: Point) -> Int = p.x");
    let code = user_code(&out);
    assert!(code.contains(".x"), "should contain member access .x");
}

// ---- Record types in TS ----

#[test]
fn emit_record_type_ts() {
    let out = parse_and_emit_ts("module app\ntype Point = { x: Int, y: Int }\nfn origin() -> Point = { x: 0, y: 0 }");
    let code = user_code(&out);
    assert!(code.contains("number"), "TS should emit number types");
}

// ---- Variant match ----

#[test]
fn emit_variant_match_js() {
    let out = parse_and_emit_js("module app\ntype Color =\n  | Red\n  | Green\n  | Blue\nfn name(c: Color) -> String = match c {\n  Red => \"red\"\n  Green => \"green\"\n  Blue => \"blue\"\n}");
    let code = user_code(&out);
    assert!(code.contains("tag"), "should check tag field");
}

// ---- Ok/Err ----

#[test]
fn emit_ok_expr_js() {
    let out = parse_and_emit_js("module app\nfn f() -> Result[Int, String] = ok(42)");
    let code = user_code(&out);
    // In TS target, ok(x) -> x (result erasure)
    assert!(code.contains("42"));
}

#[test]
fn emit_err_expr_js() {
    let out = parse_and_emit_js("module app\nfn f() -> Result[Int, String] = err(\"bad\")");
    let code = user_code(&out);
    assert!(code.contains("Error") || code.contains("throw") || code.contains("err") || code.contains("bad"),
        "err should produce error-related code, got:\n{}", code);
}

// ---- Do-block with guard ----

#[test]
fn emit_do_guard_ok_unit_js() {
    let out = parse_and_emit_js(
        "module app\nfn f() -> Unit = {\n  var i = 0\n  do {\n    guard i < 5 else ok(())\n    i = i + 1\n  }\n}",
    );
    let code = user_code(&out);
    assert!(code.contains("while"), "do-block should emit while loop");
    assert!(code.contains("break"), "guard else ok(()) should emit break");
}

#[test]
fn emit_do_guard_ok_value_js() {
    let out = parse_and_emit_js(
        "module app\neffect fn f() -> Result[Int, String] = {\n  var count = 0\n  do {\n    guard count < 10 else ok(count)\n    count = count + 1\n  }\n}",
    );
    let code = user_code(&out);
    assert!(code.contains("while"), "do-block should emit while loop");
    assert!(
        code.contains("return count") || code.contains("return (count)"),
        "guard else ok(count) should emit return count, got:\n{}",
        code
    );
    // Must NOT emit break for non-unit ok value
    let guard_line = code.lines().find(|l| l.contains("if (!("));
    if let Some(line) = guard_line {
        assert!(
            !line.contains("break"),
            "guard else ok(count) must NOT emit break, got: {}",
            line
        );
    }
}

#[test]
fn emit_do_guard_err_js() {
    let out = parse_and_emit_js(
        "module app\neffect fn f() -> Result[Int, String] = {\n  var i = 0\n  do {\n    guard i < 10 else err(\"too many\")\n    i = i + 1\n  }\n}",
    );
    let code = user_code(&out);
    assert!(
        code.contains("throw") || code.contains("Error"),
        "guard else err should emit throw, got:\n{}",
        code
    );
}

#[test]
fn emit_do_guard_break_js() {
    let out = parse_and_emit_js(
        "module app\nfn f() -> Unit = {\n  var i = 0\n  do {\n    guard i < 5 else break\n    i = i + 1\n  }\n}",
    );
    let code = user_code(&out);
    assert!(code.contains("break"), "guard else break should emit break");
}

#[test]
fn emit_do_guard_ok_value_ts() {
    let out = parse_and_emit_ts(
        "module app\neffect fn f() -> Result[Int, String] = {\n  var count = 0\n  do {\n    guard count < 10 else ok(count)\n    count = count + 1\n  }\n}",
    );
    let code = user_code(&out);
    assert!(
        code.contains("return count") || code.contains("return (count)"),
        "TS: guard else ok(count) should emit return count, got:\n{}",
        code
    );
}

// ---- Unit variant as value (no parentheses) ----

#[test]
fn emit_unit_variant_no_parens_js() {
    let out = parse_and_emit_js(
        "module app\ntype Token =\n  | Heading(Int, String)\n  | Divider\nfn f() -> Token = if true then Divider else Heading(1, \"hi\")",
    );
    let code = user_code(&out);
    // Divider is a const, should NOT be called as Divider()
    assert!(
        !code.contains("Divider()"),
        "unit variant should not have parens, got:\n{}",
        code
    );
    assert!(
        code.contains("Divider"),
        "should reference Divider, got:\n{}",
        code
    );
}

#[test]
fn emit_tuple_variant_with_parens_js() {
    let out = parse_and_emit_js(
        "module app\ntype Token =\n  | Heading(Int, String)\n  | Divider\nfn f() -> Token = Heading(1, \"hi\")",
    );
    let code = user_code(&out);
    assert!(
        code.contains("Heading(1"),
        "tuple variant should have parens, got:\n{}",
        code
    );
}

// ---- Some/None ----

#[test]
fn emit_some_js() {
    let out = parse_and_emit_js("module app\nfn f() -> Option[Int] = some(42)");
    let code = user_code(&out);
    assert!(code.contains("42"));
}

#[test]
fn emit_none_js() {
    let out = parse_and_emit_js("module app\nfn f() -> Option[Int] = none");
    let code = user_code(&out);
    assert!(code.contains("null"));
}

// ---- Let/Var ----

#[test]
fn emit_let_binding_js() {
    let out = parse_and_emit_js("module app\nfn f() -> Int = {\n  let x = 1\n  x + 2\n}");
    let code = user_code(&out);
    assert!(code.contains("const x") || code.contains("let x") || code.contains("x =") || code.contains("x;"),
        "should emit let binding, got:\n{}", code);
}

#[test]
fn emit_var_binding_js() {
    let out = parse_and_emit_js("module app\nfn f() -> Int = {\n  var x = 1\n  x = 2\n  x\n}");
    let code = user_code(&out);
    assert!(code.contains("let x"), "var should emit 'let' in JS");
}

// ---- While ----

#[test]
fn emit_while_js() {
    let out = parse_and_emit_js("module app\nfn f() -> Int = {\n  var x = 0\n  while x < 10 {\n    x = x + 1\n  }\n  x\n}");
    let code = user_code(&out);
    assert!(code.contains("while"), "should contain while loop");
}

// ---- Boolean operators ----

#[test]
fn emit_boolean_and_js() {
    let out = parse_and_emit_js("module app\nfn f(a: Bool, b: Bool) -> Bool = a and b");
    let code = user_code(&out);
    assert!(code.contains("&&"), "'and' should emit &&");
}

#[test]
fn emit_boolean_or_js() {
    let out = parse_and_emit_js("module app\nfn f(a: Bool, b: Bool) -> Bool = a or b");
    let code = user_code(&out);
    assert!(code.contains("||"), "'or' should emit ||");
}

#[test]
fn emit_not_js() {
    let out = parse_and_emit_js("module app\nfn f(a: Bool) -> Bool = not a");
    let code = user_code(&out);
    assert!(code.contains("!"), "'not' should emit !");
}

// ---- Equality ----

#[test]
fn emit_equality_js() {
    let out = parse_and_emit_js("module app\nfn f(a: Int, b: Int) -> Bool = a == b");
    let code = user_code(&out);
    assert!(code.contains("__deep_eq") || code.contains("===") || code.contains("almide_eq"),
        "== should use deep equality");
}

// ---- Empty list ----

#[test]
fn emit_empty_list_js() {
    let out = parse_and_emit_js("module app\nfn f() -> List[Int] = []");
    let code = user_code(&out);
    assert!(code.contains("[]"), "empty list should emit []");
}

// ---- Lambda ----

#[test]
fn emit_lambda_js() {
    let out = parse_and_emit_js("module app\nfn f() -> fn(Int) -> Int = (x) => x + 1");
    let code = user_code(&out);
    assert!(code.contains("=>") || code.contains("function"), "should emit lambda/arrow function");
}

// ---- Spread record ----

#[test]
fn emit_spread_record_js() {
    let out = parse_and_emit_js("module app\ntype Point = { x: Int, y: Int }\nfn f(p: Point) -> Point = { ...p, x: 1 }");
    let code = user_code(&out);
    assert!(code.contains("..."), "should contain spread operator");
}

// ---- TS type annotations ----

#[test]
fn emit_ts_fn_return_type() {
    let out = parse_and_emit_ts("module app\nfn f(x: Int) -> Bool = x > 0");
    let code = user_code(&out);
    assert!(code.contains(": boolean"), "should annotate return type as boolean");
}

#[test]
fn emit_ts_list_type() {
    let out = parse_and_emit_ts("module app\nfn f() -> List[Int] = [1, 2, 3]");
    let code = user_code(&out);
    assert!(code.contains("number[]") || code.contains("Array<number>"), "should annotate list type");
}

#[test]
fn emit_ts_option_type() {
    let out = parse_and_emit_ts("module app\nfn f() -> Option[Int] = none");
    let code = user_code(&out);
    assert!(code.contains("null") || code.contains("number | null"), "should handle Option type");
}

// ---- Todo ----

#[test]
fn emit_todo_js() {
    let out = parse_and_emit_js("module app\nfn f() -> Int = todo(\"not done\")");
    let code = user_code(&out);
    assert!(code.contains("throw") || code.contains("Error") || code.contains("todo"),
        "todo should throw, got:\n{}", code);
}

// ---- Top-level let ----

#[test]
fn emit_top_let_js() {
    let out = parse_and_emit_js("module app\nlet pi = 3\nfn f() -> Int = pi");
    let code = user_code(&out);
    assert!(code.contains("const pi") || code.contains("pi"), "should emit top-level constant");
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
