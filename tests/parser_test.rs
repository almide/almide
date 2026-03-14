use almide::lexer::Lexer;
use almide::parser::Parser;
use almide::ast::*;

fn parse(input: &str) -> Program {
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens);
    parser.parse().expect("parse failed")
}

/// Parse with file name, return collected diagnostics as display strings.
fn parse_errors(input: &str) -> Vec<String> {
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens).with_file("test.almd");
    let _ = parser.parse();
    parser.errors.iter().map(|d| d.display()).collect()
}

fn parse_expr(input: &str) -> Expr {
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens);
    parser.parse_single_expr().expect("parse failed")
}

// ---- Basic declarations ----

#[test]
fn parse_module() {
    // module declaration is parsed and kept in decls (deprecated, emits warning)
    let prog = parse("module app\nfn f() -> Int = 1");
    assert_eq!(prog.decls.len(), 2); // Module + Fn
    assert!(matches!(&prog.decls[0], Decl::Module { .. }));
    assert!(matches!(&prog.decls[1], Decl::Fn { .. }));
}

#[test]
fn parse_import() {
    let prog = parse("import fs");
    assert_eq!(prog.imports.len(), 1);
    if let Decl::Import { path, .. } = &prog.imports[0] {
        assert_eq!(path, &["fs"]);
    } else {
        panic!("expected import decl");
    }
}

#[test]
fn parse_simple_fn() {
    let prog = parse("fn add(a: Int, b: Int) -> Int = a + b");
    assert_eq!(prog.decls.len(), 1);
    if let Decl::Fn { name, params, .. } = &prog.decls[0] {
        assert_eq!(name, "add");
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "a");
        assert_eq!(params[1].name, "b");
    } else {
        panic!("expected fn decl");
    }
}

#[test]
fn parse_effect_fn() {
    let prog = parse("effect fn main(args: List[String]) -> Result[Unit, String] = ok(())");
    if let Decl::Fn { name, effect, .. } = &prog.decls[0] {
        assert_eq!(name, "main");
        assert_eq!(*effect, Some(true));
    } else {
        panic!("expected effect fn");
    }
}

#[test]
fn parse_test_decl() {
    let prog = parse("test \"basic\" {\n  assert(true)\n}");
    assert_eq!(prog.decls.len(), 1);
    if let Decl::Test { name, .. } = &prog.decls[0] {
        assert_eq!(name, "basic");
    } else {
        panic!("expected test decl");
    }
}

// ---- Type expressions ----

#[test]
fn parse_tuple_type() {
    let prog = parse("fn foo() -> (Int, String) = (1, \"x\")");
    if let Decl::Fn { return_type, .. } = &prog.decls[0] {
        if let TypeExpr::Tuple { elements } = return_type {
            assert_eq!(elements.len(), 2);
        } else {
            panic!("expected tuple type, got {:?}", return_type);
        }
    } else {
        panic!("expected fn decl");
    }
}

#[test]
fn parse_fn_type() {
    let prog = parse("fn apply(f: fn(Int) -> Int, x: Int) -> Int = f(x)");
    if let Decl::Fn { params, .. } = &prog.decls[0] {
        if let TypeExpr::Fn { params: fp, .. } = &params[0].ty {
            assert_eq!(fp.len(), 1);
        } else {
            panic!("expected fn type");
        }
    } else {
        panic!("expected fn decl");
    }
}

#[test]
fn parse_generic_type() {
    let prog = parse("fn foo(xs: List[Int]) -> Option[Int] = list.get(xs, 0)");
    if let Decl::Fn { params, return_type, .. } = &prog.decls[0] {
        if let TypeExpr::Generic { name, args } = &params[0].ty {
            assert_eq!(name, "List");
            assert_eq!(args.len(), 1);
        } else {
            panic!("expected generic type");
        }
        if let TypeExpr::Generic { name, .. } = return_type {
            assert_eq!(name, "Option");
        } else {
            panic!("expected Option type");
        }
    } else {
        panic!("expected fn decl");
    }
}

// ---- Variant types ----

#[test]
fn parse_variant_type() {
    let prog = parse("type Color =\n  | Red\n  | Green\n  | Blue\n  | Custom(Int, Int, Int)");
    if let Decl::Type { name, ty, .. } = &prog.decls[0] {
        assert_eq!(name, "Color");
        if let TypeExpr::Variant { cases } = ty {
            assert_eq!(cases.len(), 4);
            assert!(matches!(&cases[0], VariantCase::Unit { name } if name == "Red"));
            assert!(matches!(&cases[3], VariantCase::Tuple { name, fields } if name == "Custom" && fields.len() == 3));
        } else {
            panic!("expected variant type");
        }
    } else {
        panic!("expected type decl");
    }
}

// ---- Expressions ----

#[test]
fn parse_if_expr() {
    let expr = parse_expr("if x then 1 else 2");
    assert!(matches!(expr, Expr::If { .. }));
}

#[test]
fn parse_match_expr() {
    let expr = parse_expr("match x {\n  some(v) => v\n  none => 0\n}");
    if let Expr::Match { arms, .. } = &expr {
        assert_eq!(arms.len(), 2);
        assert!(matches!(&arms[0].pattern, Pattern::Some { .. }));
        assert!(matches!(&arms[1].pattern, Pattern::None));
    } else {
        panic!("expected match expr");
    }
}

#[test]
fn parse_lambda() {
    let expr = parse_expr("fn(x) => x + 1");
    if let Expr::Lambda { params, .. } = &expr {
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "x");
    } else {
        panic!("expected lambda");
    }
}

#[test]
fn parse_pipe() {
    let expr = parse_expr("xs |> list.map(fn(x) => x + 1)");
    assert!(matches!(expr, Expr::Pipe { .. }));
}

#[test]
fn parse_tuple_expr() {
    let expr = parse_expr("(1, 2, 3)");
    if let Expr::Tuple { elements, .. } = &expr {
        assert_eq!(elements.len(), 3);
    } else {
        panic!("expected tuple, got {:?}", expr);
    }
}

#[test]
fn parse_range_exclusive() {
    let expr = parse_expr("0..5");
    if let Expr::Range { inclusive, .. } = &expr {
        assert!(!inclusive);
    } else {
        panic!("expected range");
    }
}

#[test]
fn parse_range_inclusive() {
    let expr = parse_expr("1..=10");
    if let Expr::Range { inclusive, .. } = &expr {
        assert!(inclusive);
    } else {
        panic!("expected inclusive range");
    }
}

#[test]
fn parse_string_interpolation() {
    let expr = parse_expr("\"hello ${name}\"");
    assert!(matches!(expr, Expr::InterpolatedString { .. }));
}

#[test]
fn parse_list_literal() {
    let expr = parse_expr("[1, 2, 3]");
    if let Expr::List { elements, .. } = &expr {
        assert_eq!(elements.len(), 3);
    } else {
        panic!("expected list");
    }
}

#[test]
fn parse_record_literal() {
    let expr = parse_expr("{ name: \"Alice\", age: 30 }");
    if let Expr::Record { fields, .. } = &expr {
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "name");
        assert_eq!(fields[1].name, "age");
    } else {
        panic!("expected record");
    }
}

#[test]
fn parse_for_in() {
    let prog = parse("effect fn main(_a: List[String]) -> Result[Unit, String] = {\n  for x in xs {\n    println(x)\n  }\n  ok(())\n}");
    assert_eq!(prog.decls.len(), 1);
}

// ---- Patterns ----

#[test]
fn parse_tuple_pattern() {
    let expr = parse_expr("match p {\n  (a, b) => a + b\n}");
    if let Expr::Match { arms, .. } = &expr {
        if let Pattern::Tuple { elements } = &arms[0].pattern {
            assert_eq!(elements.len(), 2);
        } else {
            panic!("expected tuple pattern, got {:?}", arms[0].pattern);
        }
    } else {
        panic!("expected match");
    }
}

#[test]
fn parse_constructor_pattern() {
    let expr = parse_expr("match c {\n  Custom(r, g, b) => r\n  _ => 0\n}");
    if let Expr::Match { arms, .. } = &expr {
        if let Pattern::Constructor { name, args } = &arms[0].pattern {
            assert_eq!(name, "Custom");
            assert_eq!(args.len(), 3);
        } else {
            panic!("expected constructor pattern");
        }
    } else {
        panic!("expected match");
    }
}

// ---- Edge cases ----

#[test]
fn parse_empty_program() {
    let prog = parse("");
    assert!(prog.module.is_none());
    assert!(prog.decls.is_empty());
}

#[test]
fn parse_multiple_fns() {
    let prog = parse("fn a() -> Int = 1\nfn b() -> Int = 2\nfn c() -> Int = 3");
    assert_eq!(prog.decls.len(), 3);
}

#[test]
fn parse_binary_operators() {
    let expr = parse_expr("1 + 2 * 3");
    // Should parse as 1 + (2 * 3) due to precedence
    if let Expr::Binary { op, .. } = &expr {
        assert_eq!(op, "+");
    } else {
        panic!("expected binary");
    }
}

#[test]
fn parse_unit_literal() {
    let expr = parse_expr("()");
    assert!(matches!(expr, Expr::Unit { .. }));
}

#[test]
fn parse_ok_err() {
    let ok = parse_expr("ok(42)");
    assert!(matches!(ok, Expr::Ok { .. }));
    let err = parse_expr("err(\"bad\")");
    assert!(matches!(err, Expr::Err { .. }));
}

// ---- Multi-error recovery ----

#[test]
fn parse_recovers_from_errors() {
    let input = "fn good() -> Int = 1\nfn bad( -> Int = 2\nfn also_good() -> Int = 3";
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens);
    let prog = parser.parse().expect("should return partial program");
    assert!(prog.decls.len() >= 2, "expected at least 2 decls, got {}", prog.decls.len());
    assert!(!parser.errors.is_empty(), "expected parse errors to be collected");
}

#[test]
fn parse_multiple_errors() {
    let input = "fn a( -> Int = 1\nfn b( -> Int = 2\nfn c() -> Int = 3";
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens);
    let prog = parser.parse().expect("should return partial program");
    assert!(parser.errors.len() >= 2, "expected at least 2 errors, got {}", parser.errors.len());
    assert!(!prog.decls.is_empty(), "expected at least 1 valid decl");
}

#[test]
fn parse_no_errors_for_valid_program() {
    let tokens = Lexer::tokenize("fn a() -> Int = 1\nfn b() -> Int = 2");
    let mut parser = Parser::new(tokens);
    let _prog = parser.parse().expect("parse failed");
    assert!(parser.errors.is_empty());
}

// ---- Declarations: TopLet ----

#[test]
fn parse_top_let() {
    let prog = parse("let pi = 3");
    assert_eq!(prog.decls.len(), 1);
    assert!(matches!(&prog.decls[0], Decl::TopLet { name, .. } if name == "pi"));
}

#[test]
fn parse_top_let_with_type() {
    let prog = parse("let pi: Float = 3.14");
    if let Decl::TopLet { name, ty, .. } = &prog.decls[0] {
        assert_eq!(name, "pi");
        assert!(ty.is_some());
    } else {
        panic!("expected TopLet");
    }
}

// ---- Declarations: Record type ----

#[test]
fn parse_record_type() {
    let prog = parse("type Point = { x: Int, y: Int }");
    if let Decl::Type { name, ty, .. } = &prog.decls[0] {
        assert_eq!(name, "Point");
        assert!(matches!(ty, TypeExpr::Record { fields, .. } if fields.len() == 2));
    } else {
        panic!("expected type decl");
    }
}

// ---- Declarations: Variant with record fields ----

#[test]
fn parse_variant_record_case() {
    let prog = parse("type Msg =\n  | Click { x: Int, y: Int }\n  | Key(String)");
    if let Decl::Type { ty: TypeExpr::Variant { cases }, .. } = &prog.decls[0] {
        assert_eq!(cases.len(), 2);
        assert!(matches!(&cases[0], VariantCase::Record { name, fields } if name == "Click" && fields.len() == 2));
        assert!(matches!(&cases[1], VariantCase::Tuple { name, fields } if name == "Key" && fields.len() == 1));
    } else {
        panic!("expected variant type");
    }
}

// ---- Declarations: Impl ----

#[test]
fn parse_type_and_fn() {
    let prog = parse("type Point = { x: Int, y: Int }\nfn origin() -> Point = { x: 0, y: 0 }");
    assert_eq!(prog.decls.len(), 2);
    assert!(matches!(&prog.decls[0], Decl::Type { .. }));
    assert!(matches!(&prog.decls[1], Decl::Fn { .. }));
}

// ---- Statements ----

#[test]
fn parse_let_stmt_in_block() {
    let prog = parse("fn f() -> Int = {\n  let x = 1\n  x\n}");
    assert_eq!(prog.decls.len(), 1);
}

#[test]
fn parse_var_stmt() {
    let prog = parse("fn f() -> Int = {\n  var x = 1\n  x = 2\n  x\n}");
    assert_eq!(prog.decls.len(), 1);
}

#[test]
fn parse_guard_stmt() {
    let prog = parse("fn f(x: Int) -> Int = {\n  guard x > 0 else 0\n  x\n}");
    assert_eq!(prog.decls.len(), 1);
}

// ---- Expressions: while ----

#[test]
fn parse_while_loop() {
    let prog = parse("fn f() -> Int = {\n  var x = 0\n  while x < 10 {\n    x = x + 1\n  }\n  x\n}");
    assert_eq!(prog.decls.len(), 1);
}

// ---- Expressions: do block ----

#[test]
fn parse_do_block() {
    let prog = parse("effect fn f() -> Result[Int, String] = do {\n  let x = ok(1)\n  ok(x)\n}");
    assert_eq!(prog.decls.len(), 1);
}

// ---- Expressions: try ----

#[test]
fn parse_try_expr() {
    let prog = parse("effect fn f(x: Result[Int, String]) -> Result[Int, String] = {\n  let v = try x\n  ok(v)\n}");
    assert_eq!(prog.decls.len(), 1);
}

// ---- Expressions: todo ----

#[test]
fn parse_todo_expr() {
    let expr = parse_expr("todo(\"not implemented\")");
    assert!(matches!(expr, Expr::Todo { .. }));
}

// ---- Expressions: member access ----

#[test]
fn parse_member_access() {
    let expr = parse_expr("point.x");
    assert!(matches!(expr, Expr::Member { .. }));
}

// ---- Expressions: index access ----

#[test]
fn parse_index_access() {
    let expr = parse_expr("xs[0]");
    assert!(matches!(expr, Expr::IndexAccess { .. }));
}

// ---- Expressions: method call ----

#[test]
fn parse_method_call() {
    let expr = parse_expr("xs.len()");
    if let Expr::Call { callee, .. } = &expr {
        assert!(matches!(callee.as_ref(), Expr::Member { .. }));
    } else {
        panic!("expected call, got {:?}", expr);
    }
}

// ---- Expressions: spread record ----

#[test]
fn parse_spread_record() {
    let expr = parse_expr("{ ...base, x: 1 }");
    assert!(matches!(expr, Expr::SpreadRecord { .. }));
}

// ---- Expressions: unary negation ----

#[test]
fn parse_unary_negation() {
    let expr = parse_expr("-x");
    if let Expr::Unary { op, .. } = &expr {
        assert_eq!(op, "-");
    } else {
        panic!("expected unary, got {:?}", expr);
    }
}

// ---- Expressions: not ----

#[test]
fn parse_not_expr() {
    let expr = parse_expr("not true");
    if let Expr::Unary { op, .. } = &expr {
        assert_eq!(op, "not");
    } else {
        panic!("expected unary not, got {:?}", expr);
    }
}

// ---- Expressions: empty list ----

#[test]
fn parse_empty_list() {
    let expr = parse_expr("[]");
    if let Expr::List { elements, .. } = &expr {
        assert!(elements.is_empty());
    } else {
        panic!("expected list");
    }
}

// ---- Expressions: none literal ----

#[test]
fn parse_none_literal() {
    let expr = parse_expr("none");
    assert!(matches!(expr, Expr::None { .. }));
}

// ---- Expressions: some literal ----

#[test]
fn parse_some_literal() {
    let expr = parse_expr("some(42)");
    assert!(matches!(expr, Expr::Some { .. }));
}

// ---- Expressions: boolean literals ----

#[test]
fn parse_bool_literals() {
    assert!(matches!(parse_expr("true"), Expr::Bool { value: true, .. }));
    assert!(matches!(parse_expr("false"), Expr::Bool { value: false, .. }));
}

// ---- Expressions: string literal ----

#[test]
fn parse_string_literal() {
    let expr = parse_expr("\"hello\"");
    assert!(matches!(expr, Expr::String { .. }));
}

// ---- Expressions: float literal ----

#[test]
fn parse_float_literal() {
    let expr = parse_expr("3.14");
    assert!(matches!(expr, Expr::Float { .. }));
}

// ---- Expressions: call with no args ----

#[test]
fn parse_call_no_args() {
    let expr = parse_expr("f()");
    if let Expr::Call { args, .. } = &expr {
        assert!(args.is_empty());
    } else {
        panic!("expected call");
    }
}

// ---- Expressions: nested call ----

#[test]
fn parse_nested_calls() {
    let expr = parse_expr("f(g(x))");
    assert!(matches!(expr, Expr::Call { .. }));
}

// ---- Expressions: lambda with typed param ----

#[test]
fn parse_typed_lambda() {
    let expr = parse_expr("fn(x: Int) => x + 1");
    if let Expr::Lambda { params, .. } = &expr {
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "x");
    } else {
        panic!("expected lambda");
    }
}

// ---- Patterns: wildcard ----

#[test]
fn parse_wildcard_pattern() {
    let expr = parse_expr("match x {\n  _ => 0\n}");
    if let Expr::Match { arms, .. } = &expr {
        assert!(matches!(&arms[0].pattern, Pattern::Wildcard));
    } else {
        panic!("expected match");
    }
}

// ---- Patterns: literal ----

#[test]
fn parse_literal_pattern() {
    let expr = parse_expr("match x {\n  0 => \"zero\"\n  _ => \"other\"\n}");
    if let Expr::Match { arms, .. } = &expr {
        assert!(matches!(&arms[0].pattern, Pattern::Literal { .. }));
    } else {
        panic!("expected match");
    }
}

// ---- Patterns: ok/err ----

#[test]
fn parse_ok_err_pattern() {
    let expr = parse_expr("match r {\n  ok(v) => v\n  err(e) => 0\n}");
    if let Expr::Match { arms, .. } = &expr {
        assert!(matches!(&arms[0].pattern, Pattern::Ok { .. }));
        assert!(matches!(&arms[1].pattern, Pattern::Err { .. }));
    } else {
        panic!("expected match");
    }
}

// ---- Operator precedence ----

#[test]
fn parse_precedence_mul_over_add() {
    let expr = parse_expr("1 + 2 * 3");
    if let Expr::Binary { op, right, .. } = &expr {
        assert_eq!(op, "+");
        assert!(matches!(right.as_ref(), Expr::Binary { op, .. } if op == "*"));
    } else {
        panic!("expected binary");
    }
}

#[test]
fn parse_precedence_comparison() {
    let expr = parse_expr("a + b > c + d");
    if let Expr::Binary { op, .. } = &expr {
        assert_eq!(op, ">");
    } else {
        panic!("expected comparison");
    }
}

#[test]
fn parse_precedence_and_or() {
    let expr = parse_expr("a and b or c");
    // or has lower precedence than and
    if let Expr::Binary { op, .. } = &expr {
        assert_eq!(op, "or");
    } else {
        panic!("expected or");
    }
}

// ---- Chained pipes ----

#[test]
fn parse_chained_pipe() {
    let expr = parse_expr("xs |> f() |> g()");
    assert!(matches!(expr, Expr::Pipe { .. }));
}

// ---- Import with alias ----

#[test]
fn parse_import_alias() {
    let prog = parse("import json as j");
    if let Decl::Import { path, alias, .. } = &prog.imports[0] {
        assert_eq!(path, &["json"]);
        assert_eq!(alias.as_deref(), Some("j"));
    } else {
        panic!("expected import");
    }
}

// ---- Import basic ----

#[test]
fn parse_import_basic() {
    let prog = parse("import math");
    assert!(!prog.imports.is_empty(), "should parse import");
}

// ---- Span information ----

#[test]
fn decl_spans_are_present() {
    let prog = parse("module app\nimport fs\nfn add(a: Int, b: Int) -> Int = a + b\ntype Color =\n  | Red\n  | Blue\ntest \"basic\" {\n  assert(true)\n}");
    // module (now in decls[0])
    if let Decl::Module { span, .. } = &prog.decls[0] {
        let s = span.expect("module should have span");
        assert_eq!(s.line, 1);
    } else {
        panic!("expected module decl at decls[0]");
    }
    // import
    if let Decl::Import { span, .. } = &prog.imports[0] {
        let s = span.expect("import should have span");
        assert_eq!(s.line, 2);
    }
    // fn
    if let Decl::Fn { span, .. } = &prog.decls[1] {
        let s = span.expect("fn should have span");
        assert_eq!(s.line, 3);
    }
    // type
    if let Decl::Type { span, .. } = &prog.decls[2] {
        let s = span.expect("type should have span");
        assert_eq!(s.line, 4);
    }
    // test
    if let Decl::Test { span, .. } = &prog.decls[3] {
        let s = span.expect("test should have span");
        assert_eq!(s.line, 7);
    }
}

// ---- Parser error snapshot tests ----

#[test]
fn error_unclosed_paren_in_call() {
    let errors = parse_errors("fn f() -> Int = add(1, 2");
    assert!(!errors.is_empty());
    let msg = &errors[0];
    assert!(msg.contains("Expected ')'"), "should mention missing ')': {}", msg);
    assert!(msg.contains("function call"), "should mention context: {}", msg);
    assert!(msg.contains("opened at line"), "should reference opening position: {}", msg);
}

#[test]
fn error_unclosed_bracket_in_list() {
    let errors = parse_errors("fn f() -> Int = [1, 2, 3");
    assert!(!errors.is_empty());
    let msg = &errors[0];
    assert!(msg.contains("Expected ']'"), "should mention missing ']': {}", msg);
    assert!(msg.contains("list literal"), "should mention context: {}", msg);
}

#[test]
fn error_unclosed_brace_in_block() {
    let errors = parse_errors("fn f() -> Int = {\n  let x = 1\n  x");
    assert!(!errors.is_empty());
    let msg = &errors[0];
    assert!(msg.contains("Expected '}'"), "should mention missing '}}': {}", msg);
    assert!(msg.contains("block"), "should mention context: {}", msg);
}

#[test]
fn error_unclosed_match_brace() {
    // Match without closing } — parser reaches EOF while looking for next pattern
    let errors = parse_errors("fn f(x: Int) -> Int = match x {\n  0 => 1\n  _ => 2");
    assert!(!errors.is_empty(), "should report an error for unclosed match");
}

#[test]
fn error_unclosed_lambda_params() {
    let errors = parse_errors("fn f() -> Int = {\n  let g = fn(x => x\n  g(1)\n}");
    assert!(!errors.is_empty());
    let msg = &errors[0];
    assert!(msg.contains("Expected ')'"), "should mention missing ')': {}", msg);
    assert!(msg.contains("lambda parameters"), "should mention lambda context: {}", msg);
}

#[test]
fn error_unclosed_fn_params() {
    let errors = parse_errors("fn f(x: Int -> Int = x");
    assert!(!errors.is_empty());
    let msg = &errors[0];
    assert!(msg.contains("Expected ')'"), "should mention missing ')': {}", msg);
    assert!(msg.contains("function parameters"), "should mention fn params context: {}", msg);
}

#[test]
fn error_keyword_typo_function() {
    let errors = parse_errors("function add(a: Int) -> Int = a");
    assert!(!errors.is_empty());
    let msg = &errors[0];
    assert!(msg.contains("fn"), "should hint about 'fn': {}", msg);
}

#[test]
fn error_keyword_typo_class() {
    let errors = parse_errors("class Point = { x: Int, y: Int }");
    assert!(!errors.is_empty());
    let msg = &errors[0];
    assert!(msg.contains("type"), "should hint about 'type': {}", msg);
}

#[test]
fn error_multi_errors_in_block() {
    // Two parse errors in one block — missing identifiers after `let`
    let errors = parse_errors("fn f() -> Int = {\n  let = 1\n  let = 2\n  42\n}");
    assert!(errors.len() >= 2, "should report at least 2 errors via statement recovery, got: {:?}", errors);
}

#[test]
fn error_recovery_across_decls() {
    // Error in first fn (missing `=` before body), second fn should still parse
    let input = "fn bad() -> Int { 1 }\nfn good() -> Int = 42";
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens).with_file("test.almd");
    let prog = parser.parse().expect("should return Ok with partial AST");
    assert!(!parser.errors.is_empty(), "should have collected parse errors");
    // The good declaration fn good() should be parsed
    let has_good = prog.decls.iter().any(|d| matches!(d, Decl::Fn { name, .. } if name == "good"));
    assert!(has_good, "should have parsed fn good(): {:?}", prog.decls.iter().map(|d| match d { Decl::Fn { name, .. } => name.as_str(), _ => "?" }).collect::<Vec<_>>());
}

#[test]
fn error_recovery_multiple_bad_decls() {
    // Two broken declarations followed by a good one
    let input = "fn a() -> = 1\nfn b( -> Int = 2\nfn c() -> Int = 3";
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens).with_file("test.almd");
    let prog = parser.parse().expect("should return Ok with partial AST");
    assert!(!parser.errors.is_empty(), "should have errors");
    // The good declaration fn c() should be parsed
    let has_c = prog.decls.iter().any(|d| matches!(d, Decl::Fn { name, .. } if name == "c"));
    assert!(has_c, "should have parsed fn c(): {:?}", prog.decls.iter().map(|d| match d { Decl::Fn { name, .. } => name.as_str(), _ => "?" }).collect::<Vec<_>>());
}

#[test]
fn error_recovery_do_block() {
    // Error inside do block — missing identifier after `let`
    let input = "effect fn f() -> Result[Int, String] = do {\n  let = 1\n  let y = 2\n  ok(y)\n}";
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens).with_file("test.almd");
    let prog = parser.parse().expect("should return Ok with partial AST");
    assert_eq!(prog.decls.len(), 1, "should parse the fn declaration");
    assert!(!parser.errors.is_empty(), "should have parse errors from do block");
}

#[test]
fn error_recovery_import_then_decls() {
    // Bad import followed by valid declarations
    let input = "import { bad }\nfn f() -> Int = 1";
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens).with_file("test.almd");
    let prog = parser.parse().expect("should return Ok with partial AST");
    assert!(!parser.errors.is_empty(), "should have import error");
    assert!(!prog.decls.is_empty(), "should still parse fn declaration after bad import");
}

#[test]
fn error_recovery_stmt_error_nodes() {
    // Verify Stmt::Error nodes are inserted at failure points
    let input = "fn f() -> Int = {\n  let = 1\n  let y = 2\n  y\n}";
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens).with_file("test.almd");
    let prog = parser.parse().expect("should parse");
    if let Decl::Fn { body: Some(body), .. } = &prog.decls[0] {
        if let Expr::Block { stmts, .. } = body {
            let has_error = stmts.iter().any(|s| matches!(s, Stmt::Error { .. }));
            assert!(has_error, "block should contain Stmt::Error node, got: {:?}", stmts.iter().map(|s| std::mem::discriminant(s)).collect::<Vec<_>>());
        } else {
            panic!("expected block body");
        }
    } else {
        panic!("expected fn with body");
    }
}

#[test]
fn error_let_mut_hint() {
    let errors = parse_errors("fn f() -> Int = {\n  let mut x = 1\n  x\n}");
    assert!(!errors.is_empty());
    let msg = &errors[0];
    assert!(msg.contains("var"), "should hint about 'var': {}", msg);
}
