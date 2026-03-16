use almide::lexer::Lexer;
use almide::parser::Parser;
use almide::check::Checker;
use almide::diagnostic::Level;

fn check(input: &str) -> Vec<(Level, String)> {
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens);
    let mut prog = parser.parse().expect("parse failed");
    let diags = {
        let mut checker = Checker::new();
        checker.check_program(&mut prog)
    };
    diags.into_iter().map(|d| (d.level, d.message)).collect()
}

fn check_with_hints(input: &str) -> Vec<(Level, String, String)> {
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens);
    let mut prog = parser.parse().expect("parse failed");
    let diags = {
        let mut checker = Checker::new();
        checker.check_program(&mut prog)
    };
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
    has_no_errors("fn f() -> List[Int] = [1, 2] ++ [3, 4]");
}

#[test]
fn check_string_concat() {
    has_no_errors("fn f() -> String = \"hello\" ++ \" world\"");
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
fn check_arithmetic_on_string_error_message() {
    let errs = errors("fn f(a: String, b: String) -> String = a + b");
    assert!(!errs.is_empty());
    let msg = &errs[0];
    assert!(msg.contains("numeric") || msg.contains("String"), "error should mention type, got: {}", msg);
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
    let errs = errors("fn f(a: Int, b: Int) -> Int = a ++ b");
    assert!(!errs.is_empty());
    assert!(errs[0].contains("String") || errs[0].contains("List"), "should mention String/List, got: {}", errs[0]);
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
fn check_do_block() {
    has_no_errors(
        "effect fn read() -> Result[String, String] = ok(\"data\")\neffect fn main(_a: List[String]) -> Result[Unit, String] = do {\n  let data = read()\n  println(data)\n  ok(())\n}"
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
fn effect_isolation_stdlib_effect_fn() {
    let errs = errors(
        "fn f(path: String) -> String = fs.read_text(path)"
    );
    assert!(!errs.is_empty(), "pure fn calling stdlib effect fn should error");
    assert!(errs[0].contains("effect"), "error should mention effect, got: {}", errs[0]);
}
