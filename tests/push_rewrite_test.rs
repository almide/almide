//! #2186 step 1, island (c): `RustLoweringPass` rewrites `xs = xs + [v]` to
//! `xs.push(v)`. On a var whose lvalue is not a direct Rust place — a
//! closure-captured cell or a mutable top-let — the push would land on a
//! discarded clone and the write would vanish (#501); the rewrite is guarded
//! only by the `collect_assign_exempt_vars` allowlist. This file pins, on
//! hand-built IR, that after the pass the var that is pushed is the var that
//! is subsequently read, that every exempt shape keeps its `Assign`, and
//! that the pass's postcondition (`verify_push_targets_are_places`) refuses
//! a push on an exempt var — then runs the shapes end to end.

use std::process::Command;

use almide::codegen::pass::{verify_postconditions, NanoPass, Target};
use almide::codegen::pass_rust_lowering::{verify_push_targets_are_places, RustLoweringPass};
use almide::ir::*;
use almide::types::{Ty, TypeConstructorId};
use almide_base::intern::sym;

fn e(kind: IrExprKind, ty: Ty) -> IrExpr {
    IrExpr { kind, ty, span: None, def_id: None }
}

fn list_ty() -> Ty {
    Ty::Applied(TypeConstructorId::List, vec![Ty::Int])
}

fn var(id: VarId) -> IrExpr {
    e(IrExprKind::Var { id }, list_ty())
}

fn lit(v: i64) -> IrExpr {
    e(IrExprKind::LitInt { value: v }, Ty::Int)
}

/// `xs = xs + [v]` (the operand cloned, as CloneInsertion leaves it).
fn append(xs: VarId, v: i64) -> IrStmt {
    let left = e(IrExprKind::Clone { expr: Box::new(var(xs)) }, list_ty());
    let right = e(IrExprKind::List { elements: vec![lit(v)] }, list_ty());
    let value = e(IrExprKind::BinOp { op: BinOp::ConcatList, left: Box::new(left), right: Box::new(right) }, list_ty());
    IrStmt { kind: IrStmtKind::Assign { var: xs, value }, span: None }
}

/// `len(&xs)` as a statement — the read that follows the write.
fn read(xs: VarId) -> IrStmt {
    let arg = e(IrExprKind::Borrow { expr: Box::new(var(xs)), as_str: false, mutable: false }, Ty::Unit);
    let call = e(IrExprKind::Call { target: CallTarget::Named { name: sym("len") }, args: vec![arg], type_args: vec![] }, Ty::Int);
    IrStmt { kind: IrStmtKind::Expr { expr: call }, span: None }
}

fn func(name: &str, stmts: Vec<IrStmt>) -> IrFunction {
    IrFunction {
        name: sym(name),
        params: vec![],
        ret_ty: Ty::Unit,
        body: e(IrExprKind::Block { stmts, expr: None }, Ty::Unit),
        is_effect: false,
        is_test: false,
        generics: None,
        extern_attrs: vec![],
        export_attrs: vec![],
        attrs: vec![],
        visibility: IrVisibility::Public,
        doc: None,
        blank_lines_before: 0,
        def_id: None,
        mutated_params: vec![],
        module_origin: None,
    }
}

fn top_let(xs: VarId) -> IrTopLet {
    IrTopLet {
        var: xs,
        ty: list_ty(),
        value: e(IrExprKind::List { elements: vec![] }, list_ty()),
        kind: TopLetKind::Lazy,
        mutable: true,
        doc: None,
        blank_lines_before: 0,
        def_id: None,
    }
}

/// The fn's statements after the pass.
fn stmts_of(program: &IrProgram, name: &str) -> Vec<IrStmt> {
    let f = program.functions.iter().chain(program.modules.iter().flat_map(|m| m.functions.iter()))
        .find(|f| f.name.as_str() == name).expect("fn present");
    let IrExprKind::Block { stmts, .. } = &f.body.kind else { panic!("block body") };
    stmts.clone()
}

/// The var a `xs.push(..)` statement targets, if the statement is one.
fn push_target(stmt: &IrStmt) -> Option<VarId> {
    let IrStmtKind::Expr { expr } = &stmt.kind else { return None };
    let IrExprKind::Call { target: CallTarget::Method { object, method }, .. } = &expr.kind else { return None };
    (method.as_str() == "push").then(|| match &object.kind {
        IrExprKind::Var { id } => *id,
        other => panic!("push object is not a var: {other:?}"),
    })
}

fn read_var(stmt: &IrStmt) -> VarId {
    let IrStmtKind::Expr { expr } = &stmt.kind else { panic!("read stmt") };
    let IrExprKind::Call { args, .. } = &expr.kind else { panic!("read call") };
    let IrExprKind::Borrow { expr, .. } = &args[0].kind else { panic!("borrowed arg") };
    let IrExprKind::Var { id } = &expr.kind else { panic!("var arg") };
    *id
}

fn run(program: IrProgram) -> IrProgram {
    let out = RustLoweringPass.run(program, Target::Rust).program;
    let violations = verify_postconditions("RustLowering", &out, &RustLoweringPass.postconditions());
    assert!(violations.is_empty(), "the pass violated its own postcondition: {violations:?}");
    out
}

#[test]
fn a_local_list_is_pushed_in_place_and_the_read_sees_the_same_var() {
    let mut vt = VarTable::new();
    let xs = vt.alloc(sym("xs"), list_ty(), Mutability::Var, None);
    let program = IrProgram { functions: vec![func("f", vec![append(xs, 1), read(xs)])], var_table: vt, ..Default::default() };
    let out = run(program);
    let stmts = stmts_of(&out, "f");
    assert_eq!(push_target(&stmts[0]), Some(xs), "the append must become a push on `xs` itself");
    assert_eq!(read_var(&stmts[1]), xs, "the following read must observe the pushed var");
}

#[test]
fn a_closure_captured_cell_keeps_its_assign() {
    let mut vt = VarTable::new();
    let xs = vt.alloc(sym("xs"), list_ty(), Mutability::Var, None);
    let mut program = IrProgram { functions: vec![func("f", vec![append(xs, 1), read(xs)])], var_table: vt, ..Default::default() };
    program.codegen_annotations.shared_mut_vars.insert(xs);
    let out = run(program);
    let stmts = stmts_of(&out, "f");
    assert!(matches!(stmts[0].kind, IrStmtKind::Assign { var, .. } if var == xs), "a shared cell must keep the cell write: {:?}", stmts[0].kind);
}

#[test]
fn a_mutable_top_let_keeps_its_assign() {
    let mut vt = VarTable::new();
    let xs = vt.alloc(sym("xs"), list_ty(), Mutability::Var, None);
    let program = IrProgram {
        functions: vec![func("f", vec![append(xs, 1), read(xs)])],
        top_lets: vec![top_let(xs)],
        var_table: vt,
        ..Default::default()
    };
    let out = run(program);
    let stmts = stmts_of(&out, "f");
    assert!(matches!(stmts[0].kind, IrStmtKind::Assign { var, .. } if var == xs), "a mutable top-let must keep the ModuleRc write: {:?}", stmts[0].kind);
}

#[test]
fn a_module_mutable_top_let_keeps_its_assign() {
    let mut vt = VarTable::new();
    let xs = vt.alloc(sym("xs"), list_ty(), Mutability::Var, None);
    let module = IrModule {
        name: sym("m"),
        versioned_name: None,
        type_decls: vec![],
        functions: vec![func("m_f", vec![append(xs, 1), read(xs)])],
        top_lets: vec![top_let(xs)],
        var_table: VarTable::new(),
        exports: vec![],
        imports: vec![],
    };
    let program = IrProgram { modules: vec![module], var_table: vt, ..Default::default() };
    let out = run(program);
    let stmts = stmts_of(&out, "m_f");
    assert!(matches!(stmts[0].kind, IrStmtKind::Assign { var, .. } if var == xs), "a module's mutable top-let must keep its write: {:?}", stmts[0].kind);
}

#[test]
fn the_postcondition_refuses_a_push_on_an_exempt_var() {
    // Forge the shape the allowlist exists to prevent: a push whose object
    // is a shared cell. The witness must name the var.
    let mut vt = VarTable::new();
    let xs = vt.alloc(sym("xs"), list_ty(), Mutability::Var, None);
    let push = e(IrExprKind::Call { target: CallTarget::Method { object: Box::new(var(xs)), method: sym("push") }, args: vec![lit(1)], type_args: vec![] }, Ty::Unit);
    let mut program = IrProgram { functions: vec![func("f", vec![IrStmt { kind: IrStmtKind::Expr { expr: push }, span: None }])], var_table: vt, ..Default::default() };
    assert!(verify_push_targets_are_places(&program).is_empty(), "a push on a plain local is a place");
    program.codegen_annotations.shared_mut_vars.insert(xs);
    let violations = verify_push_targets_are_places(&program);
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(violations[0].contains("`xs.push(..)`") && violations[0].contains("#501"), "{violations:?}");
}

// ── end to end: the write must be observable ─────────────────────────

const CAPTURED_CELL: &str = r#"
effect fn main() -> Unit = {
  var xs: List[Int] = []
  let add = (v: Int) => {
    xs = xs + [v]
  }
  add(1)
  add(2)
  println("n=${list.len(xs)} last=${list.get(xs, 1) ?? 0}")
}
"#;

const TOP_LET: &str = r#"
var log: List[Int] = []

fn record(v: Int) -> Unit = {
  log = log + [v]
}

effect fn main() -> Unit = {
  record(4)
  record(5)
  println("n=${list.len(log)} last=${list.get(log, 1) ?? 0}")
}
"#;

const LOCAL: &str = r#"
effect fn main() -> Unit = {
  var xs: List[Int] = []
  for v in 0..<3 {
    xs = xs + [v * 2]
  }
  println("n=${list.len(xs)} last=${list.get(xs, 2) ?? 0}")
}
"#;

#[test]
fn the_pushed_write_is_observed_on_native() {
    let dir = tempfile::tempdir().expect("tempdir");
    for (label, program, expected) in [
        ("captured_cell", CAPTURED_CELL, "n=2 last=2\n"),
        ("top_let", TOP_LET, "n=2 last=5\n"),
        ("local", LOCAL, "n=3 last=4\n"),
    ] {
        let source = dir.path().join(format!("{label}.almd"));
        std::fs::write(&source, program).expect("source");
        let out = Command::new(env!("CARGO_BIN_EXE_almide"))
            .args(["run", source.to_str().expect("path")])
            .output()
            .expect("almide run");
        assert!(out.status.success(), "{label}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout), expected, "{label}: a write was lost");
    }
}
