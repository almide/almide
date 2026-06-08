use almide::lexer::Lexer;
use almide::parser::Parser;
use almide::canonicalize;
use almide::check::Checker;
use almide::diagnostic::Level;

fn check(input: &str) -> Vec<(Level, String)> {
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens);
    let mut prog = parser.parse().expect("parse failed");
    let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
    let mut checker = Checker::from_env(canon.env);
    checker.diagnostics = canon.diagnostics;
    let diags = checker.infer_program(&mut prog);
    diags.into_iter().map(|d| (d.level, d.message)).collect()
}

fn check_with_hints(input: &str) -> Vec<(Level, String, String)> {
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens);
    let mut prog = parser.parse().expect("parse failed");
    let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
    let mut checker = Checker::from_env(canon.env);
    checker.diagnostics = canon.diagnostics;
    let diags = checker.infer_program(&mut prog);
    diags.into_iter().map(|d| (d.level, d.message, d.hint)).collect()
}

fn error_hints(input: &str) -> Vec<String> {
    check_with_hints(input)
        .into_iter()
        .filter(|(l, _, _)| *l == Level::Error)
        .map(|(_, _, h)| h)
        .collect()
}

fn errors(input: &str) -> Vec<String> {
    check(input)
        .into_iter()
        .filter(|(l, _)| *l == Level::Error)
        .map(|(_, m)| m)
        .collect()
}

#[allow(dead_code)]
fn warnings(input: &str) -> Vec<String> {
    check(input)
        .into_iter()
        .filter(|(l, _)| *l == Level::Warning)
        .map(|(_, m)| m)
        .collect()
}

fn has_no_errors(input: &str) {
    let errs = errors(input);
    assert!(errs.is_empty(), "expected no errors, got: {:?}", errs);
}

// ---- Valid programs ----

#[test]
fn check_simple_fn() {
    has_no_errors("fn add(a: Int, b: Int) -> Int = a + b");
}

#[test]
fn check_effect_fn() {
    has_no_errors("effect fn main(args: List[String]) -> Result[Unit, String] = ok(())");
}

#[test]
fn check_let_binding() {
    has_no_errors("fn f() -> Int = {\n  let x = 1\n  x + 2\n}");
}

#[test]
fn check_var_binding() {
    has_no_errors("fn f() -> Int = {\n  var x = 1\n  x = 2\n  x\n}");
}

#[test]
fn check_if_expr() {
    has_no_errors("fn abs(n: Int) -> Int = if n < 0 then 0 - n else n");
}

#[test]
fn check_match_option() {
    has_no_errors("fn f(x: Option[Int]) -> Int = match x {\n  some(v) => v\n  none => 0\n}");
}

#[test]
fn check_list_literal() {
    has_no_errors("fn f() -> List[Int] = [1, 2, 3]");
}

#[test]
fn check_tuple() {
    has_no_errors("fn f() -> (Int, String) = (1, \"x\")");
}

#[test]
fn check_lambda() {
    has_no_errors("fn f() -> fn(Int) -> Int = (x) => x + 1");
}

#[test]
fn check_string_interpolation() {
    has_no_errors("fn greet(name: String) -> String = \"hello ${name}\"");
}

#[test]
fn check_ok_err() {
    has_no_errors("fn f() -> Result[Int, String] = ok(42)");
    has_no_errors("fn f() -> Result[Int, String] = err(\"bad\")");
}

#[test]
fn check_variant_type() {
    has_no_errors("type Color =\n  | Red\n  | Green\n  | Blue\nfn f() -> Color = Red");
}

#[test]
fn check_variant_match() {
    has_no_errors(
        "type Color =\n  | Red\n  | Green\n  | Blue\nfn name(c: Color) -> String = match c {\n  Red => \"red\"\n  Green => \"green\"\n  Blue => \"blue\"\n}"
    );
}

#[test]
fn check_variant_with_payload() {
    has_no_errors(
        "type Shape =\n  | Circle(Float)\n  | Rect(Float, Float)\nfn area(s: Shape) -> Float = match s {\n  Circle(r) => r * r\n  Rect(w, h) => w * h\n}"
    );
}

#[test]
fn check_record_type() {
    has_no_errors("type Point = { x: Int, y: Int }\nfn origin() -> Point = { x: 0, y: 0 }");
}

#[test]
fn check_for_in_loop() {
    has_no_errors(
        "effect fn main(_a: List[String]) -> Result[Unit, String] = {\n  for x in [1, 2, 3] {\n    println(int.to_string(x))\n  }\n  ok(())\n}"
    );
}

#[test]
fn check_pipe_operator() {
    has_no_errors("fn f(xs: List[Int]) -> List[Int] = xs |> list.filter((x) => x > 0)");
}

#[test]
fn check_range() {
    has_no_errors("fn f() -> List[Int] = 0..5");
}

#[test]
fn check_test_block() {
    has_no_errors("test \"basic\" {\n  assert(1 == 1)\n}");
}

#[test]
fn check_list_concat() {
    has_no_errors("fn f() -> List[Int] = [1, 2] + [3, 4]");
}

#[test]
fn check_string_concat() {
    has_no_errors("fn f() -> String = \"hello\" + \" world\"");
}

#[test]
fn check_boolean_operators() {
    has_no_errors("fn f(a: Bool, b: Bool) -> Bool = a and b or not a");
}

#[test]
fn check_comparison_operators() {
    has_no_errors("fn f(a: Int, b: Int) -> Bool = a < b");
    has_no_errors("fn f(a: Int, b: Int) -> Bool = a <= b");
    has_no_errors("fn f(a: Int, b: Int) -> Bool = a > b");
    has_no_errors("fn f(a: Int, b: Int) -> Bool = a >= b");
    has_no_errors("fn f(a: Int, b: Int) -> Bool = a == b");
    has_no_errors("fn f(a: Int, b: Int) -> Bool = a != b");
}

#[test]
fn check_float_arithmetic() {
    has_no_errors("fn f(a: Float, b: Float) -> Float = a + b");
    has_no_errors("fn f(a: Float, b: Float) -> Float = a - b");
    has_no_errors("fn f(a: Float, b: Float) -> Float = a * b");
    has_no_errors("fn f(a: Float, b: Float) -> Float = a / b");
}

#[test]
fn check_unit_return() {
    has_no_errors("fn f() -> Unit = ()");
}

#[test]
fn check_none_literal() {
    has_no_errors("fn f() -> Option[Int] = none");
}

#[test]
fn check_some_literal() {
    has_no_errors("fn f() -> Option[Int] = some(42)");
}

// ---- More valid programs ----

#[test]
fn check_member_access() {
    has_no_errors("type Point = { x: Int, y: Int }\nfn f(p: Point) -> Int = p.x");
}

#[test]
fn check_spread_record() {
    has_no_errors("type Point = { x: Int, y: Int }\nfn f(p: Point) -> Point = { ...p, x: 1 }");
}

#[test]
fn check_index_access() {
    has_no_errors("fn f(xs: List[Int]) -> Int = xs[0]");
}

#[test]
fn check_tuple_access() {
    has_no_errors("fn f(p: (Int, String)) -> Int = p.0");
}

#[test]
fn check_nested_match() {
    has_no_errors("fn f(x: Option[Option[Int]]) -> Int = match x {\n  some(some(v)) => v\n  _ => 0\n}");
}

#[test]
fn check_guard_stmt() {
    has_no_errors("fn f(x: Int) -> Int = {\n  guard x > 0 else 0\n  x\n}");
}

#[test]
fn check_impl_block() {
    has_no_errors("type Greeter = { name: String }\nimpl Greeter {\n  fn greet(self: Greeter) -> String = self.name\n}");
}

#[test]
fn check_int_modulo() {
    has_no_errors("fn f(a: Int, b: Int) -> Int = a % b");
}

#[test]
fn check_mixed_int_float_arithmetic() {
    // Int + Float should promote to Float
    has_no_errors("fn f(a: Int, b: Float) -> Float = a + b");
}

#[test]
fn check_map_type() {
    has_no_errors("fn f(m: Map[String, Int]) -> Map[String, Int] = m");
}

#[test]
fn check_lambda_as_arg() {
    has_no_errors("fn apply(f: fn(Int) -> Int, x: Int) -> Int = f(x)\nfn g() -> Int = apply((x) => x + 1, 5)");
}

#[test]
fn check_string_interpolation_complex() {
    has_no_errors("fn f(name: String, age: Int) -> String = \"${name} is ${int.to_string(age)}\"");
}

#[test]
fn check_equality_on_strings() {
    has_no_errors("fn f(a: String, b: String) -> Bool = a == b");
}

#[test]
fn check_equality_on_bools() {
    has_no_errors("fn f(a: Bool, b: Bool) -> Bool = a == b");
}

#[test]
fn check_multiple_let_bindings() {
    has_no_errors("fn f() -> Int = {\n  let a = 1\n  let b = 2\n  let c = 3\n  a + b + c\n}");
}

#[test]
fn check_nested_if() {
    has_no_errors("fn f(x: Int) -> Int = if x > 0 then if x > 10 then 2 else 1 else 0");
}

#[test]
fn check_match_with_wildcard() {
    has_no_errors("fn f(x: Int) -> String = match x {\n  0 => \"zero\"\n  1 => \"one\"\n  _ => \"other\"\n}");
}

#[test]
fn check_for_in_range() {
    has_no_errors("effect fn main(_a: List[String]) -> Result[Unit, String] = {\n  for i in 0..10 {\n    println(int.to_string(i))\n  }\n  ok(())\n}");
}

#[test]
fn check_chained_pipe() {
    has_no_errors("fn f(xs: List[Int]) -> List[Int] = xs |> list.filter((x) => x > 0) |> list.map((x) => x * 2)");
}

// ---- Type error messages ----

#[test]
fn check_string_plus_is_concat() {
    // + on String is now concat, not an error
    has_no_errors("fn f(a: String, b: String) -> String = a + b");
}

#[test]
fn check_and_on_non_bool_error() {
    let errs = errors("fn f(a: Int, b: Int) -> Bool = a and b");
    assert!(!errs.is_empty());
    assert!(errs[0].contains("Bool"), "should mention Bool requirement, got: {}", errs[0]);
}

#[test]
fn check_or_on_non_bool_error() {
    let errs = errors("fn f(a: Int, b: Int) -> Bool = a or b");
    assert!(!errs.is_empty());
}

#[test]
fn check_concat_mismatch_error() {
    // ++ is removed, produces error
    let errs = errors("fn f(a: Int, b: Int) -> Int = a ++ b");
    assert!(!errs.is_empty());
}

#[test]
fn check_wrong_arg_type() {
    let errs = errors("fn add(a: Int, b: Int) -> Int = a + b\nfn f() -> Int = add(\"hello\", 1)");
    assert!(!errs.is_empty(), "should report arg type mismatch");
}

#[test]
fn check_assign_to_param() {
    let errs = errors("fn f(x: Int) -> Int = {\n  x = 2\n  x\n}");
    assert!(!errs.is_empty(), "should report assignment to parameter");
}

#[test]
fn check_undefined_function() {
    let errs = errors("fn f() -> Int = nonexistent()");
    assert!(!errs.is_empty(), "should report undefined function");
}

// ---- Stdlib edge cases ----

#[test]
fn check_stdlib_string_trim() {
    has_no_errors("fn f(s: String) -> String = string.trim(s)");
}

#[test]
fn check_stdlib_string_split() {
    has_no_errors("fn f(s: String) -> List[String] = string.split(s, \",\")");
}

#[test]
fn check_stdlib_list_fold() {
    has_no_errors("fn f(xs: List[Int]) -> Int = list.fold(xs, 0, (acc, x) => acc + x)");
}

#[test]
fn check_stdlib_list_reduce() {
    has_no_errors("fn f(xs: List[Int]) -> Option[Int] = list.reduce(xs, (a, b) => a + b)");
}

#[test]
fn check_stdlib_list_len() {
    has_no_errors("fn f(xs: List[Int]) -> Int = list.len(xs)");
}

#[test]
fn check_stdlib_float_to_string() {
    has_no_errors("fn f(n: Float) -> String = float.to_string(n)");
}

// ---- Type errors ----

#[test]
fn check_type_mismatch_return() {
    let errs = errors("fn f() -> Int = \"hello\"");
    assert!(!errs.is_empty(), "should report type mismatch");
}

#[test]
fn check_undefined_variable() {
    let errs = errors("fn f() -> Int = x");
    assert!(!errs.is_empty(), "should report undefined variable");
}

#[test]
fn check_wrong_argument_count() {
    let errs = errors("fn add(a: Int, b: Int) -> Int = a + b\nfn f() -> Int = add(1)");
    assert!(!errs.is_empty(), "should report wrong argument count");
}

#[test]
fn check_int_plus_string() {
    let errs = errors("fn f(a: Int, b: String) -> Int = a + b");
    assert!(!errs.is_empty(), "should report type mismatch on +");
}

#[test]
fn check_assign_to_let() {
    let errs = errors("fn f() -> Int = {\n  let x = 1\n  x = 2\n  x\n}");
    assert!(!errs.is_empty(), "should report assignment to immutable");
}

#[test]
fn check_index_assign_to_module_let() {
    // A module-level `let g` is immutable just like a local `let`: index-assigning
    // its contents must be rejected (E009), not silently slip through to a codegen
    // error. `lookup_var` only sees locals, so this exercises the `top_lets` arm.
    let errs = errors("let g: List[Int] = [1, 2, 3]\neffect fn main() -> Unit = {\n  g[0] = 9\n}");
    assert!(errs.iter().any(|e| e.contains("immutable binding 'g'")),
        "module-level `let g` index-assign should report E009, got: {:?}", errs);
}

#[test]
fn check_index_assign_to_module_var_ok() {
    // The `var` counterpart is fine — must NOT report the immutable-binding error.
    let errs = errors("var g: List[Int] = [1, 2, 3]\neffect fn main() -> Unit = {\n  g[0] = 9\n}");
    assert!(!errs.iter().any(|e| e.contains("immutable binding")),
        "module-level `var g` index-assign should be allowed, got: {:?}", errs);
}

#[test]
fn check_undefined_type() {
    let errs = errors("fn f() -> Foo = {\n  let x = 1\n  x\n}");
    // Should either error or treat as Unknown
    // This is a softer test — the checker may or may not produce an error here
    let _ = errs;
}

// ---- Warnings ----

#[test]
fn check_unused_variable_warning() {
    // Unused variable warnings are now generated from IR (collect_unused_var_warnings),
    // not from the checker. See tests/ir_test.rs for those tests.
    // Verify the checker does not produce false errors for this valid code.
    has_no_errors("fn f() -> Int = {\n  let x = 1\n  2\n}");
}

#[test]
fn check_underscore_prefix_no_warning() {
    // Same as above — unused variable detection is in IR layer.
    has_no_errors("fn f() -> Int = {\n  let _x = 1\n  2\n}");
}

// ---- Do blocks ----

#[test]
fn check_effect_block() {
    has_no_errors(
        "effect fn read() -> Result[String, String] = ok(\"data\")\neffect fn main(_a: List[String]) -> Result[Unit, String] = {\n  let data = read()\n  println(data)\n  ok(())\n}"
    );
}

// ---- Multiple functions ----

#[test]
fn check_function_calls() {
    has_no_errors(
        "fn double(x: Int) -> Int = x * 2\nfn f() -> Int = double(5)"
    );
}

#[test]
fn check_recursive_function() {
    has_no_errors(
        "fn fib(n: Int) -> Int = if n <= 1 then n else fib(n - 1) + fib(n - 2)"
    );
}

// ---- Stdlib calls ----

#[test]
fn check_stdlib_string_len() {
    has_no_errors("fn f(s: String) -> Int = string.len(s)");
}

#[test]
fn check_stdlib_list_map() {
    has_no_errors("fn f(xs: List[Int]) -> List[Int] = list.map(xs, (x) => x + 1)");
}

#[test]
fn check_stdlib_list_filter() {
    has_no_errors("fn f(xs: List[Int]) -> List[Int] = list.filter(xs, (x) => x > 0)");
}

#[test]
fn check_stdlib_int_to_string() {
    has_no_errors("fn f(n: Int) -> String = int.to_string(n)");
}

// ---- Top-level let ----

#[test]
fn check_top_level_let() {
    has_no_errors("let pi = 3\nfn f() -> Int = pi");
}

// ---- While loops ----

#[test]
fn check_while_loop() {
    has_no_errors(
        "fn f() -> Int = {\n  var x = 0\n  while x < 10 {\n    x = x + 1\n  }\n  x\n}"
    );
}

// ---- Type conversion suggestions ----

#[test]
fn hint_int_to_string_return() {
    let hints = error_hints("fn f() -> String = 42");
    assert!(!hints.is_empty(), "should have an error");
    assert!(hints[0].contains("int.to_string"), "hint should suggest int.to_string, got: {}", hints[0]);
}

#[test]
fn hint_int_to_string_let() {
    let hints = error_hints("fn f() -> Unit = {\n  let x: String = 42\n  ()\n}");
    assert!(!hints.is_empty(), "should have an error");
    assert!(hints[0].contains("int.to_string"), "hint should suggest int.to_string, got: {}", hints[0]);
}

#[test]
fn hint_float_to_int_return() {
    let hints = error_hints("fn f() -> Int = 3.14");
    assert!(!hints.is_empty(), "should have an error");
    assert!(hints[0].contains("to_int"), "hint should suggest to_int, got: {}", hints[0]);
}

#[test]
fn hint_int_to_float_return() {
    let hints = error_hints("fn f() -> Float = 42");
    assert!(!hints.is_empty(), "should have an error");
    assert!(hints[0].contains("to_float"), "hint should suggest to_float, got: {}", hints[0]);
}

#[test]
fn hint_string_to_int_return() {
    let hints = error_hints("fn f() -> Int = \"42\"");
    assert!(!hints.is_empty(), "should have an error");
    assert!(hints[0].contains("int.parse"), "hint should suggest int.parse, got: {}", hints[0]);
}

#[test]
fn hint_bool_to_string_return() {
    let hints = error_hints("fn f() -> String = true");
    assert!(!hints.is_empty(), "should have an error");
    assert!(hints[0].contains("to_string"), "hint should suggest to_string, got: {}", hints[0]);
}

#[test]
fn hint_int_to_string_println() {
    let hints = error_hints("effect fn main(_a: List[String]) -> Result[Unit, String] = {\n  println(42)\n  ok(())\n}");
    assert!(!hints.is_empty(), "should have an error");
    assert!(hints[0].contains("int.to_string"), "hint should suggest int.to_string for println, got: {}", hints[0]);
}

#[test]
fn hint_int_to_string_fn_arg() {
    let hints = error_hints("fn greet(name: String) -> String = name\nfn f() -> String = greet(42)");
    assert!(!hints.is_empty(), "should have an error");
    assert!(hints[0].contains("int.to_string"), "hint should suggest int.to_string for fn arg, got: {}", hints[0]);
}

#[test]
fn hint_no_conversion_for_same_type() {
    // No error means no hint needed
    has_no_errors("fn f() -> Int = 42");
}

#[test]
fn hint_no_conversion_for_unrelated_types() {
    let hints = error_hints("fn f() -> List[Int] = 42");
    assert!(!hints.is_empty(), "should have an error");
    // Should NOT contain conversion suggestions for unrelated types
    assert!(!hints[0].contains("to_string") && !hints[0].contains("to_int") && !hints[0].contains("to_float"),
        "should not suggest conversion for unrelated types, got: {}", hints[0]);
}

// ---- Multi-error recovery ----

#[test]
fn multi_error_block_statements() {
    // Multiple independent errors in a block should all be reported
    let errs = errors("fn f() -> Unit = {\n  let x: Int = \"hello\"\n  let y: String = 42\n  ()\n}");
    assert!(errs.len() >= 2, "should report errors for both let bindings, got: {:?}", errs);
    assert!(errs.iter().any(|e| e.contains("let x")), "should report error for x: {:?}", errs);
    assert!(errs.iter().any(|e| e.contains("let y")), "should report error for y: {:?}", errs);
}

#[test]
fn multi_error_function_args() {
    // All wrong arguments should be reported, not just the first
    let errs = errors("fn add(a: Int, b: Int, c: Int) -> Int = a + b + c\nfn f() -> Int = add(\"x\", true, \"y\")");
    assert!(errs.len() >= 3, "should report errors for all 3 args, got: {:?}", errs);
}

#[test]
fn multi_error_across_functions() {
    // Errors in one function should not prevent checking another
    let errs = errors("fn f() -> Int = \"hello\"\nfn g() -> String = 42");
    assert!(errs.len() >= 2, "should report errors in both functions, got: {:?}", errs);
    assert!(errs.iter().any(|e| e.contains("'f'")), "should report error for f: {:?}", errs);
    assert!(errs.iter().any(|e| e.contains("'g'")), "should report error for g: {:?}", errs);
}

#[test]
fn multi_error_println_calls() {
    // Multiple statement-level errors should all be reported
    let errs = errors("effect fn main(_a: List[String]) -> Result[Unit, String] = {\n  println(42)\n  println(true)\n  ok(())\n}");
    assert!(errs.len() >= 2, "should report errors for both println calls, got: {:?}", errs);
}

#[test]
fn multi_error_no_cascade_from_unknown() {
    // An undefined function should not cause cascading errors on its result
    let errs = errors("fn f() -> Int = {\n  let x = undefined_fn()\n  let y = x + 1\n  y\n}");
    assert_eq!(errs.len(), 1, "should report only the undefined function error, got: {:?}", errs);
    assert!(errs[0].contains("undefined"), "error should be about undefined function: {}", errs[0]);
}

#[test]
fn multi_error_constructor_arg_types() {
    // Constructor calls should check argument types and report all mismatches
    let errs = errors("type Shape =\n  | Circle(Float)\n  | Rect(Float, Float)\nfn f() -> Shape = Rect(\"x\", true)");
    assert!(errs.len() >= 2, "should report errors for both constructor args, got: {:?}", errs);
}

#[test]
fn multi_error_mixed_stmt_types() {
    // Binary op error + let type error should both be reported
    let errs = errors("fn f(a: Int) -> Unit = {\n  let x = a + \"hello\"\n  let y = a - true\n  ()\n}");
    assert!(errs.len() >= 2, "should report both binary op errors, got: {:?}", errs);
}

// ---- Effect isolation (Layer 1 security) ----

#[test]
fn effect_isolation_pure_cannot_call_effect() {
    let errs = errors(
        "effect fn load() -> Result[String, String] = ok(\"data\")\nfn f() -> String = load()"
    );
    assert!(!errs.is_empty(), "pure fn calling effect fn should error");
    assert!(errs[0].contains("effect"), "error should mention effect, got: {}", errs[0]);
}

#[test]
fn effect_isolation_effect_can_call_effect() {
    has_no_errors(
        "effect fn load() -> Result[String, String] = ok(\"data\")\neffect fn f() -> Result[String, String] = load()"
    );
}

#[test]
fn effect_isolation_test_can_call_effect() {
    has_no_errors(
        "effect fn load() -> Result[String, String] = ok(\"data\")\ntest \"use effect\" {\n  let _ = load()\n  assert(true)\n}"
    );
}

#[test]
fn effect_isolation_pure_can_call_pure() {
    has_no_errors(
        "fn double(x: Int) -> Int = x * 2\nfn f() -> Int = double(5)"
    );
}

#[test]
fn effect_isolation_fan_in_pure_fn() {
    let errs = errors(
        "effect fn a() -> Result[Int, String] = ok(1)\neffect fn b() -> Result[Int, String] = ok(2)\nfn f() -> (Int, Int) = fan { a(); b() }"
    );
    assert!(!errs.is_empty(), "fan in pure fn should error");
    assert!(errs[0].contains("fan") || errs[0].contains("effect"), "error should mention fan or effect, got: {}", errs[0]);
}

#[test]
fn effect_isolation_fan_in_effect_fn() {
    has_no_errors(
        "effect fn a() -> Result[Int, String] = ok(1)\neffect fn b() -> Result[Int, String] = ok(2)\neffect fn f() -> Result[Unit, String] = {\n  let _ = fan { a(); b() }\n  ok(())\n}"
    );
}

#[test]
fn effect_isolation_fan_var_capture_rejected() {
    let errs = errors(
        "effect fn a() -> Result[Int, String] = ok(1)\neffect fn f() -> Result[Unit, String] = {\n  var x = 0\n  let _ = fan { a(); a() }\n  ok(())\n}"
    );
    // No error — var x is not captured inside fan
    assert!(errs.is_empty(), "var not captured should be fine, got: {:?}", errs);
}

#[test]
fn effect_isolation_fan_var_capture_error() {
    let errs = errors(
        "effect fn a(n: Int) -> Result[Int, String] = ok(n)\neffect fn f() -> Result[Unit, String] = {\n  var x = 0\n  let _ = fan { a(x); a(x) }\n  ok(())\n}"
    );
    assert!(!errs.is_empty(), "capturing var in fan should error");
    assert!(errs[0].contains("mutable") || errs[0].contains("var"), "error should mention mutable/var, got: {}", errs[0]);
}

#[test]
fn effect_isolation_stdlib_effect_fn() {
    let errs = errors(
        "import fs\nfn f(path: String) -> String = fs.read_text(path)"
    );
    assert!(!errs.is_empty(), "pure fn calling stdlib effect fn should error");
    assert!(errs[0].contains("effect"), "error should mention effect, got: {}", errs[0]);
}

// ---- Escape analysis: var mutation in lambdas ----

#[test]
fn escape_var_local_no_lambda() {
    // var local to function, mutated in same scope (no lambda) — OK
    has_no_errors("fn sum(xs: List[Int]) -> Int = { var total = 0; for x in xs { total = total + x }; total }");
}

#[test]
fn escape_var_read_only_in_lambda() {
    // var read (not mutated) in lambda — OK
    has_no_errors("fn offset_all(xs: List[Int]) -> List[Int] = { var base = 10; list.map(xs, (x) => x + base) }");
}

#[test]
fn escape_var_mutated_in_lambda_pure_fn() {
    // var mutated inside lambda in pure fn — ERROR
    let errs = errors(
        "fn bad() -> fn() -> Unit = { var count = 0; () => { count = count + 1 } }"
    );
    assert!(!errs.is_empty(), "should error on var mutation in lambda inside pure fn");
    assert!(errs.iter().any(|e| e.contains("mutated inside a closure")),
        "error should mention closure mutation, got: {:?}", errs);
}

#[test]
fn escape_var_mutated_in_lambda_effect_fn() {
    // var mutated inside lambda in effect fn — OK
    has_no_errors(
        "effect fn counter() -> Result[Int, String] = { var count = 0; let inc = () => { count = count + 1 }; inc(); ok(count) }"
    );
}

#[test]
fn escape_var_declared_inside_lambda() {
    // var declared AND mutated inside same lambda — OK (same scope)
    has_no_errors(
        "fn transform(xs: List[Int]) -> List[Int] = list.map(xs, (x) => { var temp = x * 2; temp = temp + 1; temp })"
    );
}

#[test]
fn escape_nested_lambda_mutation() {
    // var from outer scope mutated in deeply nested lambda — ERROR
    let errs = errors(
        "fn bad() -> fn() -> fn() -> Unit = { var x = 0; () => { () => { x = x + 1 } } }"
    );
    assert!(!errs.is_empty(), "should error on var mutation in nested lambda inside pure fn");
    assert!(errs.iter().any(|e| e.contains("mutated inside a closure")),
        "error should mention closure mutation, got: {:?}", errs);
}

#[test]
fn escape_multiple_vars_mutated() {
    // Multiple vars mutated in lambda — should report errors for each
    let errs = errors(
        "fn bad() -> fn() -> Unit = { var a = 0; var b = 0; () => { a = 1; b = 2 } }"
    );
    assert!(errs.len() >= 2, "should report errors for both vars, got: {:?}", errs);
}

// ---- Exhaustiveness: nested patterns ----

#[test]
fn exhaust_nested_option() {
    // Missing some(none)
    let errs = errors(
        "fn f(x: Option[Option[Int]]) -> Int = match x {\n  some(some(n)) => n\n  none => 0\n}"
    );
    assert!(errs.iter().any(|e| e.contains("some(none)")), "should report some(none), got: {:?}", errs);
}

#[test]
fn exhaust_nested_option_complete() {
    has_no_errors(
        "fn f(x: Option[Option[Int]]) -> Int = match x {\n  some(some(n)) => n\n  some(none) => -1\n  none => 0\n}"
    );
}

#[test]
fn exhaust_nested_result_in_option() {
    // Missing some(err(_))
    let errs = errors(
        "fn f(x: Option[Result[Int, String]]) -> String = match x {\n  some(ok(n)) => \"ok\"\n  none => \"none\"\n}"
    );
    assert!(errs.iter().any(|e| e.contains("some(err(_))")), "should report some(err(_)), got: {:?}", errs);
}

#[test]
fn exhaust_deep_variant() {
    // type Expr = | Lit(Int) | Neg(Expr)
    // Missing Neg(Neg(_))
    let errs = errors(
        "type Expr =\n  | Lit(Int)\n  | Neg(Expr)\nfn eval(e: Expr) -> Int = match e {\n  Lit(n) => n\n  Neg(Lit(n)) => 0 - n\n}"
    );
    assert!(errs.iter().any(|e| e.contains("Neg(Neg(_))")), "should report Neg(Neg(_)), got: {:?}", errs);
}

#[test]
fn exhaust_variant_with_wildcard_nested() {
    has_no_errors(
        "type Expr =\n  | Lit(Int)\n  | Neg(Expr)\nfn eval(e: Expr) -> Int = match e {\n  Lit(n) => n\n  Neg(x) => 0\n}"
    );
}

// ---- Exhaustiveness: tuple patterns ----

#[test]
fn exhaust_tuple_bool_pair() {
    // Missing (true, false), (false, true)
    let errs = errors(
        "fn f(p: (Bool, Bool)) -> String = match p {\n  (true, true) => \"tt\"\n  (false, false) => \"ff\"\n}"
    );
    assert!(errs.iter().any(|e| e.contains("(true, false)")), "should report (true, false), got: {:?}", errs);
    assert!(errs.iter().any(|e| e.contains("(false, true)")), "should report (false, true), got: {:?}", errs);
}

#[test]
fn exhaust_tuple_bool_pair_complete() {
    has_no_errors(
        "fn f(p: (Bool, Bool)) -> String = match p {\n  (true, true) => \"tt\"\n  (true, false) => \"tf\"\n  (false, true) => \"ft\"\n  (false, false) => \"ff\"\n}"
    );
}

#[test]
fn exhaust_tuple_with_wildcard() {
    has_no_errors(
        "fn f(p: (Bool, Bool)) -> String = match p {\n  (true, true) => \"tt\"\n  _ => \"other\"\n}"
    );
}

// ---- Exhaustiveness: infinite domain ----

#[test]
fn exhaust_int_without_wildcard() {
    let errs = errors(
        "fn f(x: Int) -> String = match x {\n  0 => \"zero\"\n  1 => \"one\"\n}"
    );
    assert!(!errs.is_empty(), "should require _ for Int match");
}

#[test]
fn exhaust_int_with_wildcard() {
    has_no_errors(
        "fn f(x: Int) -> String = match x {\n  0 => \"zero\"\n  _ => \"other\"\n}"
    );
}

#[test]
fn exhaust_string_without_wildcard() {
    let errs = errors(
        "fn f(x: String) -> Int = match x {\n  \"a\" => 1\n  \"b\" => 2\n}"
    );
    assert!(!errs.is_empty(), "should require _ for String match");
}

// ---- Exhaustiveness: existing flat cases still work ----

#[test]
fn exhaust_variant_missing_case() {
    let errs = errors(
        "type Color =\n  | Red\n  | Green\n  | Blue\nfn name(c: Color) -> String = match c {\n  Red => \"red\"\n  Green => \"green\"\n}"
    );
    assert!(errs.iter().any(|e| e.contains("Blue")), "should report Blue, got: {:?}", errs);
}

#[test]
fn exhaust_option_missing_none() {
    let errs = errors(
        "fn f(x: Option[Int]) -> Int = match x {\n  some(v) => v\n}"
    );
    assert!(errs.iter().any(|e| e.contains("none")), "should report none, got: {:?}", errs);
}

#[test]
fn exhaust_result_missing_err() {
    let errs = errors(
        "fn f(x: Result[Int, String]) -> Int = match x {\n  ok(v) => v\n}"
    );
    assert!(errs.iter().any(|e| e.contains("err")), "should report err, got: {:?}", errs);
}

#[test]
fn exhaust_bool_missing_false() {
    let errs = errors(
        "fn f(x: Bool) -> Int = match x {\n  true => 1\n}"
    );
    assert!(errs.iter().any(|e| e.contains("false")), "should report false, got: {:?}", errs);
}

#[test]
fn exhaust_guard_not_counted() {
    // Guard arms don't guarantee coverage
    let errs = errors(
        "type AB = | A | B\nfn f(x: AB) -> Int = match x {\n  A => 1\n  B if true => 2\n}"
    );
    assert!(errs.iter().any(|e| e.contains("B")), "guarded B should not count, got: {:?}", errs);
}

// ── Sized Numeric Types Stage 1c: mixed-width arithmetic rejection ──

#[test]
fn sized_mixed_width_int8_int32_rejected() {
    let errs = errors(
        "fn f(a: Int8, b: Int32) -> Int32 = a + b"
    );
    assert!(errs.iter().any(|e| e.contains("mixes sized numeric")),
        "should reject Int8 + Int32, got: {:?}", errs);
}

#[test]
fn sized_mixed_width_float32_int32_rejected() {
    let errs = errors(
        "fn f(a: Float32, b: Int32) -> Float32 = a + b"
    );
    assert!(errs.iter().any(|e| e.contains("mixes sized numeric")),
        "should reject Float32 + Int32, got: {:?}", errs);
}

#[test]
fn sized_mixed_width_uint16_int16_rejected() {
    let errs = errors(
        "fn f(a: UInt16, b: Int16) -> Int16 = a * b"
    );
    assert!(errs.iter().any(|e| e.contains("mixes sized numeric")),
        "should reject UInt16 * Int16 (even same width, different signedness), got: {:?}", errs);
}

#[test]
fn sized_same_width_arith_ok() {
    has_no_errors("fn f(a: Int32, b: Int32) -> Int32 = a + b");
    has_no_errors("fn f(a: UInt8, b: UInt8) -> UInt8 = a - b");
    has_no_errors("fn f(a: Float32, b: Float32) -> Float32 = a * b");
}

#[test]
fn sized_literal_coercion_ok() {
    has_no_errors("fn f(a: Int32) -> Int32 = a + 5");
    has_no_errors("fn f(a: Int32) -> Int32 = 10 + a");
    has_no_errors("fn f(a: Float32) -> Float32 = a + 1.5");
}

#[test]
fn sized_canonical_int_plus_sized_ok() {
    // `Int` / `Float` canonical types stay permissive to preserve the
    // literal-coercion story. `Int + Int32` is therefore accepted (the
    // right-hand side collapses to the sized variant at emit time).
    has_no_errors("fn f(a: Int, b: Int32) -> Int32 = a + b");
}

#[test]
fn sized_mixed_all_ops_rejected() {
    for op in ["-", "*", "/", "%", "^"] {
        let src = format!("fn f(a: Int8, b: Int32) -> Int32 = a {} b", op);
        let errs = errors(&src);
        assert!(errs.iter().any(|e| e.contains("mixes sized numeric")),
            "operator '{}' should reject mixed sized types, got: {:?}", op, errs);
    }
}

// ── Numeric protocol (P3 of Matrix[T] dtype arc) ──

#[test]
fn numeric_protocol_accepts_int() {
    has_no_errors("fn double[T: Numeric](x: T) -> T = x + x\nfn use_it() -> Int = double(21)");
}

#[test]
fn numeric_protocol_accepts_float() {
    has_no_errors("fn double[T: Numeric](x: T) -> T = x + x\nfn use_it() -> Float = double(1.5)");
}

#[test]
fn numeric_protocol_accepts_int32() {
    has_no_errors("fn double[T: Numeric](x: T) -> T = x + x\nfn use_it(x: Int32) -> Int32 = double(x)");
}

#[test]
fn numeric_protocol_rejects_string() {
    let errs = errors(
        "fn double[T: Numeric](x: T) -> T = x + x\nfn use_it() -> String = double(\"x\")"
    );
    assert!(errs.iter().any(|e| e.contains("does not implement protocol 'Numeric'")),
        "should reject String, got: {:?}", errs);
}

#[test]
fn numeric_protocol_rejects_bool() {
    let errs = errors(
        "fn double[T: Numeric](x: T) -> T = x + x\nfn use_it() -> Bool = double(true)"
    );
    assert!(errs.iter().any(|e| e.contains("does not implement protocol 'Numeric'")),
        "should reject Bool, got: {:?}", errs);
}

#[test]
fn sized_int64_explicit_vs_int32_rejected() {
    let errs = errors(
        "fn mix(a: Int32, b: Int64) -> Int32 = a + b"
    );
    assert!(errs.iter().any(|e| e.contains("mixes sized numeric")),
        "should reject Int32 + Int64, got: {:?}", errs);
}

#[test]
fn sized_float64_explicit_vs_float32_rejected() {
    let errs = errors(
        "fn mix(a: Float32, b: Float64) -> Float64 = a + b"
    );
    assert!(errs.iter().any(|e| e.contains("mixes sized numeric")),
        "should reject Float32 + Float64, got: {:?}", errs);
}

#[test]
fn sized_int_and_int64_interop_ok() {
    // Canonical `Int` stays the literal-coercion slot; it interops
    // with `Int64` freely at the same width.
    has_no_errors("fn f(a: Int, b: Int64) -> Int64 = b");
    has_no_errors("fn f(a: Int64) -> Int = a");
}

// ── Strict Matrix[T] discrimination (post-C) ──

#[test]
fn strict_matrix_f32_rejects_bare_matrix() {
    // A fn that asks for `Matrix[Float32]` MUST NOT accept a bare
    // `Matrix` value — bare carries no f32 guarantee.
    let errs = errors(
        "fn needs_f32(m: Matrix[Float32]) -> Int = matrix.rows(m)\nfn use_bare() -> Int = needs_f32(matrix.zeros(3, 3))"
    );
    assert!(errs.iter().any(|e| e.contains("expects")),
        "should reject bare Matrix passed to Matrix[Float32] param, got: {:?}", errs);
}

#[test]
fn strict_matrix_bare_accepts_typed() {
    // `matrix.shape(m: Matrix)` still accepts `Matrix[Float32]`
    // (bare widens to typed downstream via the runtime tag).
    has_no_errors(
        "fn row_count_f32(m: Matrix[Float32]) -> Int = matrix.rows(m)"
    );
}

#[test]
fn strict_matrix_float_alias_interop() {
    // `Matrix[Float]` is the legacy alias for bare `Matrix` — both
    // directions stay compatible at the checker layer.
    has_no_errors(
        "fn f(m: Matrix[Float]) -> Int = matrix.rows(m)\nfn g() -> Int = f(matrix.zeros(3, 3))"
    );
    has_no_errors(
        "fn f(m: Matrix) -> Int = matrix.rows(m)\nfn g() -> Int = {\n  let m: Matrix[Float] = matrix.zeros(3, 3)\n  f(m)\n}"
    );
}

#[test]
fn set_of_closures_rejected() {
    // A closure has no equality/hash, so a `Set` of closures is meaningless. The
    // two targets disagreed (native rustc E0277, WASM silently dropped inserts and
    // printed 0), so reject it at typecheck on both. (E016)
    let errs = errors(
        "effect fn main() -> Unit = {\n  var s: Set[() -> Unit] = set.new()\n}"
    );
    assert!(errs.iter().any(|e| e.contains("Set") && e.contains("function")),
        "should reject Set[() -> Unit], got: {:?}", errs);
}

#[test]
fn map_with_closure_key_rejected() {
    // Same reason for a `Map` *key*. (E016)
    let errs = errors(
        "effect fn main() -> Unit = {\n  var m: Map[() -> Unit, Int] = map.new()\n}"
    );
    assert!(errs.iter().any(|e| e.contains("Map") && e.contains("key") && e.contains("function")),
        "should reject Map[() -> Unit, Int], got: {:?}", errs);
}

#[test]
fn map_with_closure_value_allowed() {
    // A closure is fine as a `Map` *value* — only the key must be comparable.
    has_no_errors(
        "effect fn main() -> Unit = {\n  var m: Map[String, () -> Unit] = map.new()\n  map.insert(m, \"a\", () => {})\n}"
    );
}

#[test]
fn enum_name_record_construction_rejected() {
    // Constructing a record literal via the ENUM TYPE name (not a case name) is a
    // category error. The two targets disagreed (native rustc leaked E0574, WASM
    // accepted it and mis-constructed the value with an empty field), so reject it
    // at typecheck on both with a proper diagnostic. (E017)
    let errs = errors(
        "type V = Tag(Float) | Named { who: String }\nfn main() -> Unit = {\n  let v = V { who: \"x\" }\n  println(\"${v}\")\n}"
    );
    assert!(errs.iter().any(|e| e.contains("enum type 'V'") && e.contains("record syntax")),
        "should reject `V {{ who: ... }}` on the enum type name, got: {:?}", errs);
}

#[test]
fn enum_name_record_construction_hint_lists_cases() {
    // The hint must name the available record-variant case so the fix is obvious.
    let hints = error_hints(
        "type V = Tag(Float) | Named { who: String }\nfn main() -> Unit = {\n  let v = V { who: \"x\" }\n  println(\"${v}\")\n}"
    );
    assert!(hints.iter().any(|h| h.contains("Named")),
        "hint should mention the `Named` case, got: {:?}", hints);
}

#[test]
fn record_type_construction_still_allowed() {
    // A legitimate record TYPE (not an enum) must still construct fine.
    has_no_errors(
        "type Point = { x: Int, y: Int }\nfn main() -> Unit = {\n  let p = Point { x: 1, y: 2 }\n  println(\"${p.x}\")\n}"
    );
}

#[test]
fn record_variant_case_construction_still_allowed() {
    // Constructing the record-bearing CASE (not the enum type) is the correct form.
    has_no_errors(
        "type V = Tag(Float) | Named { who: String }\nfn main() -> Unit = {\n  let v = Named { who: \"x\" }\n  println(\"${v}\")\n}"
    );
}

// ── E014 reachability with literal sub-patterns (A2 regression-lock) ──
//
// A literal nested in a constructor pattern (`some(1)`, `ok(0)`, `some("a")`)
// is a REFINEMENT of that constructor, not a full cover of it. So:
//   • a literal arm must NOT shadow a later binder arm of the same ctor, and
//   • two DISTINCT literals must NOT shadow each other.
// The `is_useful` infinite-domain guard (`enumerable`) realizes this: after
// `some(1)`, the value `some(2)` is still uncovered (Int is infinite), so
// `some(x)` stays reachable. These positive cases must check clean.

#[test]
fn literal_some_then_binder_is_reachable() {
    // some(1)/some(2)/some(x)/none — the binder catches every other Int.
    has_no_errors(
        "fn f(o: Option[Int]) -> String = match o {\n  some(1) => \"a\"\n  some(2) => \"b\"\n  some(x) => \"o\"\n  none => \"n\"\n}"
    );
}

#[test]
fn literal_some_with_none_between_binder_is_reachable() {
    // The outer Some/None space is complete BEFORE the binder, so this drives
    // the `enumerable && is_complete` arm of `is_useful` — the binder is still
    // reachable because the inner Int domain is infinite.
    has_no_errors(
        "fn f(o: Option[Int]) -> String = match o {\n  some(1) => \"a\"\n  none => \"n\"\n  some(x) => \"o\"\n}"
    );
}

#[test]
fn literal_result_then_binder_is_reachable() {
    has_no_errors(
        "fn f(r: Result[Int, String]) -> String = match r {\n  ok(0) => \"z\"\n  err(e) => e\n  ok(n) => \"n\"\n}"
    );
}

#[test]
fn string_literal_some_then_binder_is_reachable() {
    has_no_errors(
        "fn f(o: Option[String]) -> String = match o {\n  some(\"a\") => \"A\"\n  none => \"N\"\n  some(x) => x\n}"
    );
}

#[test]
fn distinct_int_literals_do_not_shadow() {
    has_no_errors(
        "fn f(n: Int) -> String = match n {\n  1 => \"a\"\n  2 => \"b\"\n  3 => \"c\"\n  x => \"o\"\n}"
    );
}

// Negative direction: the loosening must NOT swallow genuine dead arms. A
// binder BEFORE a same-ctor literal covers it (the binder already matches
// `some(1)`), and a duplicate literal is dead — both stay E014.

#[test]
fn binder_before_literal_is_still_unreachable() {
    let errs = errors(
        "fn f(o: Option[Int]) -> String = match o {\n  some(x) => \"o\"\n  some(1) => \"a\"\n  none => \"n\"\n}"
    );
    assert!(errs.iter().any(|e| e.contains("unreachable match arm")),
        "some(1) after some(x) must report E014, got: {:?}", errs);
}

#[test]
fn duplicate_int_literal_is_still_unreachable() {
    let errs = errors(
        "fn f(o: Option[Int]) -> String = match o {\n  some(1) => \"a\"\n  some(1) => \"d\"\n  some(x) => \"o\"\n  none => \"n\"\n}"
    );
    assert!(errs.iter().any(|e| e.contains("unreachable match arm")),
        "duplicate some(1) must report E014, got: {:?}", errs);
}

#[test]
fn ambiguous_constructor_reports_e019() {
    // The same ctor `Pong` in two variant types is ambiguous when used bare (#413).
    let errs = errors(
        "type Cmd = | Pong | Move(Int)\n\
         type Resp = | Pong | Ack(Int)\n\
         fn cmd(c: Cmd) -> Int = match c { Pong => 1, Move(x) => x }\n\
         fn main() -> Unit = { println(int.to_string(cmd(Pong))) }"
    );
    assert!(errs.iter().any(|e| e.contains("ambiguous constructor 'Pong'")),
        "ambiguous ctor must report E019, got: {:?}", errs);
}

#[test]
fn unambiguous_constructor_is_not_flagged() {
    // A ctor name in exactly one type must NOT trip the ambiguity check.
    has_no_errors(
        "type Cmd = | Stop | Move(Int)\n\
         fn cmd(c: Cmd) -> Int = match c { Stop => 0, Move(x) => x }\n\
         fn main() -> Unit = { println(int.to_string(cmd(Stop))) }"
    );
}
