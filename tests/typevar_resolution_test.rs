/// Regression tests: verify that inference TypeVars (?N) are fully resolved
/// before reaching codegen. These patterns previously caused WASM codegen
/// to produce incorrect code due to unresolved TypeVars.

use almide::lexer::Lexer;
use almide::parser::Parser;
use almide::canonicalize;
use almide::check::Checker;
use almide::types::Ty;

/// Parse, check, and lower a program. Returns the IR VarTable for inspection.
fn lower(input: &str) -> almide::ir::IrProgram {
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens);
    let mut prog = parser.parse().expect("parse failed");
    let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
    let mut checker = Checker::from_env(canon.env);
    checker.diagnostics = canon.diagnostics;
    checker.infer_program(&mut prog);
    almide::lower::lower_program(&prog, &checker.env)
}

fn has_inference_typevar(ty: &Ty) -> bool {
    match ty {
        Ty::TypeVar(name) => name.starts_with('?'),
        Ty::Applied(_, args) => args.iter().any(has_inference_typevar),
        Ty::Tuple(elems) => elems.iter().any(has_inference_typevar),
        Ty::Fn { params, ret } => params.iter().any(has_inference_typevar) || has_inference_typevar(ret),
        Ty::Named(_, args) => args.iter().any(has_inference_typevar),
        Ty::Record { fields } | Ty::OpenRecord { fields } => fields.iter().any(|(_, t)| has_inference_typevar(t)),
        _ => false,
    }
}

fn assert_no_inference_typevars(program: &almide::ir::IrProgram) {
    for i in 0..program.var_table.len() {
        let info = program.var_table.get(almide::ir::VarId(i as u32));
        assert!(
            !has_inference_typevar(&info.ty),
            "var {} '{}' has unresolved TypeVar: {:?}",
            i, info.name, info.ty
        );
    }
    for func in &program.functions {
        assert!(
            !has_inference_typevar(&func.ret_ty),
            "fn {} ret_ty has unresolved TypeVar: {:?}",
            func.name, func.ret_ty
        );
    }
}

#[test]
fn fold_with_list_accumulator() {
    let program = lower(r#"
        test "x" {
          let xs = [10, 20]
          let doubled = list.fold(xs, [], (acc, x) => acc + [x * 2])
          let total = list.fold(doubled, 0, (acc, x) => acc + x)
          assert_eq(total, 60)
        }
    "#);
    assert_no_inference_typevars(&program);
}

#[test]
fn none_compared_with_some() {
    let program = lower(r#"
        test "x" {
          assert_eq(some(1) != none, true)
        }
    "#);
    assert_no_inference_typevars(&program);
}

#[test]
fn err_compared_with_ok() {
    let program = lower(r#"
        test "x" {
          let a: Result[Int, String] = ok(1)
          assert_eq(a != err("fail"), true)
        }
    "#);
    assert_no_inference_typevars(&program);
}

#[test]
fn generic_variant_constructor() {
    let program = lower(r#"
        type Wrapper[T] = | Wrapped(T) | Empty
        fn unwrap_or[T](w: Wrapper[T], default: T) -> T = match w {
          Wrapped(v) => v,
          Empty => default,
        }
        test "x" {
          let w = Wrapped(99)
          assert_eq(unwrap_or(w, 0), 99)
        }
    "#);
    assert_no_inference_typevars(&program);
}

#[test]
fn recursive_generic_variant() {
    let program = lower(r#"
        type Tree[T] = | Leaf(T) | Node(Tree[T], Tree[T])
        test "x" {
          let t1 = Node(Leaf(1), Leaf(2))
          let t2 = Node(Leaf(1), Leaf(2))
          assert_eq(t1 == t2, true)
        }
    "#);
    assert_no_inference_typevars(&program);
}

#[test]
fn closure_with_record_field_access() {
    let program = lower(r#"
        type Row = { id: Int, name: String }
        test "x" {
          let p = Row { id: 1, name: "a" }
          let get_id = (r) => r.id
          assert_eq(get_id(p), 1)
        }
    "#);
    assert_no_inference_typevars(&program);
}
