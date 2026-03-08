use almide::lexer::Lexer;
use almide::parser::Parser;
use almide::ast::*;

fn parse(input: &str) -> Program {
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens);
    parser.parse().expect("parse failed")
}

fn parse_expr(input: &str) -> Expr {
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens);
    parser.parse_single_expr().expect("parse failed")
}

// ---- Basic declarations ----

#[test]
fn parse_module() {
    let prog = parse("module app");
    assert!(prog.module.is_some());
    if let Some(Decl::Module { path }) = &prog.module {
        assert_eq!(path, &["app"]);
    } else {
        panic!("expected module decl");
    }
}

#[test]
fn parse_import() {
    let prog = parse("module app\nimport fs");
    assert_eq!(prog.imports.len(), 1);
    if let Decl::Import { path, .. } = &prog.imports[0] {
        assert_eq!(path, &["fs"]);
    } else {
        panic!("expected import decl");
    }
}

#[test]
fn parse_simple_fn() {
    let prog = parse("module app\nfn add(a: Int, b: Int) -> Int = a + b");
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
    let prog = parse("module app\neffect fn main(args: List[String]) -> Result[Unit, String] = ok(())");
    if let Decl::Fn { name, effect, .. } = &prog.decls[0] {
        assert_eq!(name, "main");
        assert_eq!(*effect, Some(true));
    } else {
        panic!("expected effect fn");
    }
}

#[test]
fn parse_test_decl() {
    let prog = parse("module app\ntest \"basic\" {\n  assert(true)\n}");
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
    let prog = parse("module app\nfn foo() -> (Int, String) = (1, \"x\")");
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
    let prog = parse("module app\nfn apply(f: fn(Int) -> Int, x: Int) -> Int = f(x)");
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
    let prog = parse("module app\nfn foo(xs: List[Int]) -> Option[Int] = list.get(xs, 0)");
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
    let prog = parse("module app\ntype Color =\n  | Red\n  | Green\n  | Blue\n  | Custom(Int, Int, Int)");
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
    if let Expr::Tuple { elements } = &expr {
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
    if let Expr::List { elements } = &expr {
        assert_eq!(elements.len(), 3);
    } else {
        panic!("expected list");
    }
}

#[test]
fn parse_record_literal() {
    let expr = parse_expr("{ name: \"Alice\", age: 30 }");
    if let Expr::Record { fields } = &expr {
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "name");
        assert_eq!(fields[1].name, "age");
    } else {
        panic!("expected record");
    }
}

#[test]
fn parse_for_in() {
    let prog = parse("module app\neffect fn main(_a: List[String]) -> Result[Unit, String] = {\n  for x in xs {\n    println(x)\n  }\n  ok(())\n}");
    // Just verify it parses without error
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
    let prog = parse("module app\nfn a() -> Int = 1\nfn b() -> Int = 2\nfn c() -> Int = 3");
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
    assert!(matches!(expr, Expr::Unit));
}

#[test]
fn parse_ok_err() {
    let ok = parse_expr("ok(42)");
    assert!(matches!(ok, Expr::Ok { .. }));
    let err = parse_expr("err(\"bad\")");
    assert!(matches!(err, Expr::Err { .. }));
}
