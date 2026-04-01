use almide::lexer::Lexer;
use almide::parser::Parser;
use almide::canonicalize;
use almide::check::Checker;
use almide::lower::lower_program;
use almide::ir::*;

fn lower(input: &str) -> IrProgram {
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens);
    let mut prog = parser.parse().expect("parse failed");
    let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
    let mut checker = Checker::from_env(canon.env);
    checker.diagnostics = canon.diagnostics;
    checker.infer_program(&mut prog);
    lower_program(&prog, &checker.env, &checker.type_map)
}

// ---- Basic function lowering ----

#[test]
fn lower_simple_fn() {
    let ir = lower("fn add(a: Int, b: Int) -> Int = a + b");
    assert_eq!(ir.functions.len(), 1);
    assert_eq!(ir.functions[0].name, "add");
    assert_eq!(ir.functions[0].params.len(), 2);
    assert!(!ir.functions[0].is_effect);
    assert!(!ir.functions[0].is_test);
}

#[test]
fn lower_effect_fn() {
    let ir = lower("effect fn main(args: List[String]) -> Result[Unit, String] = ok(())");
    assert_eq!(ir.functions.len(), 1);
    assert!(ir.functions[0].is_effect);
    assert_eq!(ir.functions[0].name, "main");
}

#[test]
fn lower_test_block() {
    let ir = lower("test \"basic\" {\n  assert(true)\n}");
    assert_eq!(ir.functions.len(), 1);
    assert!(ir.functions[0].is_test);
    assert_eq!(ir.functions[0].name, "basic");
}

#[test]
fn lower_multiple_functions() {
    let ir = lower("fn a() -> Int = 1\nfn b() -> Int = 2\nfn c() -> Int = 3");
    assert_eq!(ir.functions.len(), 3);
}

// ---- Literals ----

#[test]
fn lower_int_literal() {
    let ir = lower("fn f() -> Int = 42");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::LitInt { value: 42 }));
}

#[test]
fn lower_float_literal() {
    let ir = lower("fn f() -> Float = 3.14");
    if let IrExprKind::LitFloat { value } = &ir.functions[0].body.kind {
        assert!((value - 3.14).abs() < f64::EPSILON);
    } else {
        panic!("expected LitFloat");
    }
}

#[test]
fn lower_string_literal() {
    let ir = lower("fn f() -> String = \"hello\"");
    assert!(matches!(&ir.functions[0].body.kind, IrExprKind::LitStr { value } if value == "hello"));
}

#[test]
fn lower_bool_literal() {
    let ir = lower("fn f() -> Bool = true");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::LitBool { value: true }));
}

#[test]
fn lower_unit_literal() {
    let ir = lower("fn f() -> Unit = ()");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::Unit));
}

// ---- Operators ----

#[test]
fn lower_int_addition() {
    let ir = lower("fn f(a: Int, b: Int) -> Int = a + b");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::BinOp { op: BinOp::AddInt, .. }));
}

#[test]
fn lower_float_addition() {
    let ir = lower("fn f(a: Float, b: Float) -> Float = a + b");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::BinOp { op: BinOp::AddFloat, .. }));
}

#[test]
fn lower_int_subtraction() {
    let ir = lower("fn f(a: Int, b: Int) -> Int = a - b");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::BinOp { op: BinOp::SubInt, .. }));
}

#[test]
fn lower_int_multiplication() {
    let ir = lower("fn f(a: Int, b: Int) -> Int = a * b");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::BinOp { op: BinOp::MulInt, .. }));
}

#[test]
fn lower_int_division() {
    let ir = lower("fn f(a: Int, b: Int) -> Int = a / b");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::BinOp { op: BinOp::DivInt, .. }));
}

#[test]
fn lower_int_modulo() {
    let ir = lower("fn f(a: Int, b: Int) -> Int = a % b");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::BinOp { op: BinOp::ModInt, .. }));
}

#[test]
fn lower_string_concat() {
    let ir = lower("fn f(a: String, b: String) -> String = a + b");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::BinOp { op: BinOp::ConcatStr, .. }));
}

#[test]
fn lower_list_concat() {
    let ir = lower("fn f(a: List[Int], b: List[Int]) -> List[Int] = a + b");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::BinOp { op: BinOp::ConcatList, .. }));
}

#[test]
fn lower_equality() {
    let ir = lower("fn f(a: Int, b: Int) -> Bool = a == b");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::BinOp { op: BinOp::Eq, .. }));
}

#[test]
fn lower_inequality() {
    let ir = lower("fn f(a: Int, b: Int) -> Bool = a != b");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::BinOp { op: BinOp::Neq, .. }));
}

#[test]
fn lower_comparison_lt() {
    let ir = lower("fn f(a: Int, b: Int) -> Bool = a < b");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::BinOp { op: BinOp::Lt, .. }));
}

#[test]
fn lower_comparison_gt() {
    let ir = lower("fn f(a: Int, b: Int) -> Bool = a > b");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::BinOp { op: BinOp::Gt, .. }));
}

#[test]
fn lower_comparison_lte() {
    let ir = lower("fn f(a: Int, b: Int) -> Bool = a <= b");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::BinOp { op: BinOp::Lte, .. }));
}

#[test]
fn lower_comparison_gte() {
    let ir = lower("fn f(a: Int, b: Int) -> Bool = a >= b");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::BinOp { op: BinOp::Gte, .. }));
}

#[test]
fn lower_boolean_and() {
    let ir = lower("fn f(a: Bool, b: Bool) -> Bool = a and b");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::BinOp { op: BinOp::And, .. }));
}

#[test]
fn lower_boolean_or() {
    let ir = lower("fn f(a: Bool, b: Bool) -> Bool = a or b");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::BinOp { op: BinOp::Or, .. }));
}

#[test]
fn lower_not_operator() {
    let ir = lower("fn f(a: Bool) -> Bool = not a");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::UnOp { op: UnOp::Not, .. }));
}

// ---- Control flow ----

#[test]
fn lower_if_expr() {
    let ir = lower("fn f(x: Int) -> Int = if x > 0 then x else 0 - x");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::If { .. }));
}

#[test]
fn lower_match_expr() {
    let ir = lower("fn f(x: Option[Int]) -> Int = match x {\n  some(v) => v\n  none => 0\n}");
    if let IrExprKind::Match { arms, .. } = &ir.functions[0].body.kind {
        assert_eq!(arms.len(), 2);
    } else {
        panic!("expected Match expression");
    }
}

// ---- Collections ----

#[test]
fn lower_list_literal() {
    let ir = lower("fn f() -> List[Int] = [1, 2, 3]");
    if let IrExprKind::List { elements } = &ir.functions[0].body.kind {
        assert_eq!(elements.len(), 3);
    } else {
        panic!("expected List");
    }
}

#[test]
fn lower_tuple_literal() {
    let ir = lower("fn f() -> (Int, String) = (1, \"x\")");
    if let IrExprKind::Tuple { elements } = &ir.functions[0].body.kind {
        assert_eq!(elements.len(), 2);
    } else {
        panic!("expected Tuple");
    }
}

#[test]
fn lower_range() {
    let ir = lower("fn f() -> List[Int] = 0..5");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::Range { inclusive: false, .. }));
}

#[test]
fn lower_range_inclusive() {
    let ir = lower("fn f() -> List[Int] = 1..=10");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::Range { inclusive: true, .. }));
}

// ---- Result/Option ----

#[test]
fn lower_ok() {
    let ir = lower("fn f() -> Result[Int, String] = ok(42)");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::ResultOk { .. }));
}

#[test]
fn lower_err() {
    let ir = lower("fn f() -> Result[Int, String] = err(\"bad\")");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::ResultErr { .. }));
}

#[test]
fn lower_some() {
    let ir = lower("fn f() -> Option[Int] = some(42)");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::OptionSome { .. }));
}

#[test]
fn lower_none() {
    let ir = lower("fn f() -> Option[Int] = none");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::OptionNone));
}

// ---- Block with let ----

#[test]
fn lower_block_with_let() {
    let ir = lower("fn f() -> Int = {\n  let x = 1\n  x + 2\n}");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::Block { .. }));
}

// ---- Lambda ----

#[test]
fn lower_lambda() {
    let ir = lower("fn f() -> fn(Int) -> Int = (x) => x + 1");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::Lambda { .. }));
}

// ---- For-in ----

#[test]
fn lower_for_in() {
    let ir = lower("effect fn main(_a: List[String]) -> Result[Unit, String] = {\n  for x in [1, 2, 3] {\n    println(int.to_string(x))\n  }\n  ok(())\n}");
    // Body should be a block containing ForIn
    if let IrExprKind::Block { stmts, .. } = &ir.functions[0].body.kind {
        let has_for = stmts.iter().any(|s| matches!(s.kind, IrStmtKind::Expr { ref expr } if matches!(expr.kind, IrExprKind::ForIn { .. })));
        assert!(has_for, "should contain ForIn statement");
    } else {
        panic!("expected Block");
    }
}

// ---- Module calls ----

#[test]
fn lower_stdlib_call() {
    let ir = lower("fn f(s: String) -> String = string.trim(s)");
    if let IrExprKind::Call { target: CallTarget::Module { module, func }, .. } = &ir.functions[0].body.kind {
        assert_eq!(module, "string");
        assert_eq!(func, "trim");
    } else {
        panic!("expected Module call, got {:?}", ir.functions[0].body.kind);
    }
}

// ---- Pipe desugaring ----

#[test]
fn lower_pipe_desugars_to_call() {
    let ir = lower("fn f(xs: List[Int]) -> List[Int] = xs |> list.filter((x) => x > 0)");
    // Pipe should be desugared into a call
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::Call { .. }));
}

// ---- Top-level let ----

#[test]
fn lower_top_level_let() {
    let ir = lower("let pi = 3\nfn f() -> Int = pi");
    assert_eq!(ir.top_lets.len(), 1);
}

// ---- String interpolation ----

#[test]
fn lower_string_interpolation() {
    let ir = lower("fn greet(name: String) -> String = \"hello ${name}\"");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::StringInterp { .. }));
}

// ---- VarTable ----

#[test]
fn lower_var_table_populated() {
    let ir = lower("fn f(x: Int) -> Int = x + 1");
    assert!(ir.var_table.len() > 0, "var table should contain at least the parameter");
}

// ---- Variant type ----

#[test]
fn lower_variant_constructor() {
    let ir = lower("type Color =\n  | Red\n  | Green\nfn f() -> Color = Red");
    let body = &ir.functions[0].body;
    // Should lower to a call or constructor expression
    assert!(matches!(body.kind, IrExprKind::Call { target: CallTarget::Named { .. }, .. }));
}

// ---- While loop ----

#[test]
fn lower_while() {
    let ir = lower("fn f() -> Int = {\n  var x = 0\n  while x < 10 {\n    x = x + 1\n  }\n  x\n}");
    if let IrExprKind::Block { stmts, .. } = &ir.functions[0].body.kind {
        let has_while = stmts.iter().any(|s| matches!(&s.kind, IrStmtKind::Expr { expr } if matches!(expr.kind, IrExprKind::While { .. })));
        assert!(has_while, "should contain While");
    }
}

// ---- Record literal ----

#[test]
fn lower_record_literal() {
    let ir = lower("type Point = { x: Int, y: Int }\nfn f() -> Point = { x: 1, y: 2 }");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::Record { .. }));
}

// ---- Todo ----

#[test]
fn lower_todo() {
    let ir = lower("fn f() -> Int = todo(\"not implemented\")");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::Todo { .. }));
}

// ---- Member access ----

#[test]
fn lower_member_access() {
    let ir = lower("type Point = { x: Int, y: Int }\nfn f(p: Point) -> Int = p.x");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::Member { .. }));
}

// ---- Tuple index ----

#[test]
fn lower_tuple_index() {
    let ir = lower("fn f(p: (Int, String)) -> Int = p.0");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::TupleIndex { index: 0, .. }));
}

// ---- Index access ----

#[test]
fn lower_index_access() {
    let ir = lower("fn f(xs: List[Int]) -> Int = xs[0]");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::IndexAccess { .. }));
}

// ---- Spread record ----

#[test]
fn lower_spread_record() {
    let ir = lower("type Point = { x: Int, y: Int }\nfn f(p: Point) -> Point = { ...p, x: 1 }");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::SpreadRecord { .. }));
}

// ---- Var binding (mutable) ----

#[test]
fn lower_var_binding() {
    let ir = lower("fn f() -> Int = {\n  var x = 0\n  x = 1\n  x\n}");
    if let IrExprKind::Block { stmts, .. } = &ir.functions[0].body.kind {
        // First stmt should be a Bind with Var mutability
        assert!(matches!(&stmts[0].kind, IrStmtKind::Bind { mutability: Mutability::Var, .. }));
        // Second stmt should be Assign
        assert!(matches!(&stmts[1].kind, IrStmtKind::Assign { .. }));
    } else {
        panic!("expected Block");
    }
}

// ---- Guard statement ----

#[test]
fn lower_guard() {
    let ir = lower("fn f(x: Int) -> Int = {\n  guard x > 0 else 0\n  x\n}");
    if let IrExprKind::Block { stmts, .. } = &ir.functions[0].body.kind {
        let has_guard = stmts.iter().any(|s| matches!(s.kind, IrStmtKind::Guard { .. }));
        assert!(has_guard, "should contain Guard statement");
    } else {
        panic!("expected Block");
    }
}

// ---- Multiple module calls ----

#[test]
fn lower_list_map_call() {
    let ir = lower("fn f(xs: List[Int]) -> List[Int] = list.map(xs, (x) => x + 1)");
    if let IrExprKind::Call { target: CallTarget::Module { module, func }, .. } = &ir.functions[0].body.kind {
        assert_eq!(module, "list");
        assert_eq!(func, "map");
    } else {
        panic!("expected Module call");
    }
}

#[test]
fn lower_int_to_string_call() {
    let ir = lower("fn f(n: Int) -> String = int.to_string(n)");
    if let IrExprKind::Call { target: CallTarget::Module { module, func }, .. } = &ir.functions[0].body.kind {
        assert_eq!(module, "int");
        assert_eq!(func, "to_string");
    } else {
        panic!("expected Module call");
    }
}

// ---- Chained pipe desugaring ----

#[test]
fn lower_chained_pipe() {
    let ir = lower("fn f(xs: List[Int]) -> List[Int] = xs |> list.filter((x) => x > 0) |> list.map((x) => x * 2)");
    // Outermost should be a Call (the second pipe)
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::Call { .. }));
}

// ---- Empty list ----

#[test]
fn lower_empty_list() {
    let ir = lower("fn f() -> List[Int] = []");
    if let IrExprKind::List { elements } = &ir.functions[0].body.kind {
        assert!(elements.is_empty());
    } else {
        panic!("expected List");
    }
}

// ---- Nested match ----

#[test]
fn lower_nested_match() {
    let ir = lower("fn f(x: Option[Option[Int]]) -> Int = match x {\n  some(some(v)) => v\n  _ => 0\n}");
    if let IrExprKind::Match { arms, .. } = &ir.functions[0].body.kind {
        assert_eq!(arms.len(), 2);
    } else {
        panic!("expected Match");
    }
}

// ---- Match with wildcard ----

#[test]
fn lower_match_wildcard() {
    let ir = lower("fn f(x: Int) -> String = match x {\n  0 => \"zero\"\n  _ => \"other\"\n}");
    if let IrExprKind::Match { arms, .. } = &ir.functions[0].body.kind {
        assert_eq!(arms.len(), 2);
        // Wildcard pattern may lower as Wildcard or Bind depending on parser
        assert!(matches!(arms[1].pattern, IrPattern::Wildcard | IrPattern::Bind { .. }));
    } else {
        panic!("expected Match");
    }
}

// ---- Impl methods ----

#[test]
fn lower_impl_methods() {
    // NOTE: impl blocks are not yet lowered to IR; verify it doesn't crash
    let ir = lower("type Greeter = { name: String }\nimpl Greeter {\n  fn greet(self: Greeter) -> String = self.name\n}");
    // Just verify lowering completes without panic
    let _ = ir;
}

// ---- Multiple top-level lets ----

#[test]
fn lower_multiple_top_lets() {
    let ir = lower("let a = 1\nlet b = 2\nfn f() -> Int = a + b");
    assert_eq!(ir.top_lets.len(), 2);
}

// ---- Float operators ----

#[test]
fn lower_float_subtraction() {
    let ir = lower("fn f(a: Float, b: Float) -> Float = a - b");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::BinOp { op: BinOp::SubFloat, .. }));
}

#[test]
fn lower_float_multiplication() {
    let ir = lower("fn f(a: Float, b: Float) -> Float = a * b");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::BinOp { op: BinOp::MulFloat, .. }));
}

#[test]
fn lower_float_division() {
    let ir = lower("fn f(a: Float, b: Float) -> Float = a / b");
    assert!(matches!(ir.functions[0].body.kind, IrExprKind::BinOp { op: BinOp::DivFloat, .. }));
}
