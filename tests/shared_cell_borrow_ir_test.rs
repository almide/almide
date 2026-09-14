//! #2186 step 1, island (b): `SharedCellBorrowPass` marks a captured-cell
//! read `Borrow { Deref { Var cell } }` when its statement proves every use
//! is a shared call-arg read; the walker renders the marker as
//! `&*cell.borrow_proven("<name>")`. A wrong proof is a `RefCell` panic at
//! run time that rustc cannot see — so the panic now names the pass and the
//! Almide variable (`tests/shared_cell_borrow_test.rs` pins the spelling),
//! and this file pins the verdict on hand-built IR: the shape the pass
//! accepts, and the shapes it must decline.

use almide::codegen::pass::{NanoPass, Target};
use almide::codegen::pass_shared_cell_borrow::SharedCellBorrowPass;
use almide::ir::visit::{walk_expr, IrVisitor};
use almide::ir::*;
use almide::types::{Ty, TypeConstructorId};
use almide_base::intern::sym;

fn e(kind: IrExprKind, ty: Ty) -> IrExpr {
    IrExpr { kind, ty, span: None, def_id: None }
}

fn map_ty() -> Ty {
    Ty::Applied(TypeConstructorId::Map, vec![Ty::String, Ty::Int])
}

fn var(id: VarId, ty: Ty) -> IrExpr {
    e(IrExprKind::Var { id }, ty)
}

fn borrow(id: VarId, ty: Ty, mutable: bool) -> IrExpr {
    e(IrExprKind::Borrow { expr: Box::new(var(id, ty)), as_str: false, mutable }, Ty::Unit)
}

fn call(name: &str, args: Vec<IrExpr>, ty: Ty) -> IrExpr {
    e(IrExprKind::Call { target: CallTarget::Named { name: sym(name) }, args, type_args: vec![] }, ty)
}

fn bind(var: VarId, ty: Ty, value: IrExpr) -> IrStmt {
    IrStmt { kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value }, span: None }
}

/// A program with one closure-body fn `bump(k)` whose body is `stmts`, and
/// one captured cell `stats: Map[String, Int]` (a free var of the fn, as the
/// closure-conversion leaves it).
struct Shape {
    program: IrProgram,
    stats: VarId,
}

fn shape(body: impl FnOnce(&mut VarTable, VarId, VarId) -> Vec<IrStmt>) -> Shape {
    let mut vt = VarTable::new();
    let stats = vt.alloc(sym("stats"), map_ty(), Mutability::Var, None);
    let k = vt.alloc(sym("k"), Ty::String, Mutability::Let, None);
    let stmts = body(&mut vt, stats, k);
    let f = IrFunction {
        name: sym("bump"),
        params: vec![IrParam { var: k, ty: Ty::String, name: sym("k"), borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![] }],
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
    };
    let mut program = IrProgram { functions: vec![f], var_table: vt, ..Default::default() };
    program.codegen_annotations.shared_mut_vars.insert(stats);
    Shape { program, stats }
}

/// Every `Borrow { Deref { Var id } }` marker in the program, by var.
fn markers(program: &IrProgram) -> Vec<VarId> {
    struct Find(Vec<VarId>);
    impl IrVisitor for Find {
        fn visit_expr(&mut self, x: &IrExpr) {
            if let IrExprKind::Borrow { expr, mutable: false, .. } = &x.kind
                && let IrExprKind::Deref { expr: inner } = &expr.kind
                && let IrExprKind::Var { id } = &inner.kind
            {
                self.0.push(*id);
            }
            walk_expr(self, x);
        }
    }
    let mut f = Find(Vec::new());
    for func in &program.functions {
        f.visit_expr(&func.body);
    }
    f.0
}

fn run(s: Shape) -> (Vec<VarId>, VarId, bool) {
    let stats = s.stats;
    let result = SharedCellBorrowPass.run(s.program, Target::Rust);
    (markers(&result.program), stats, result.changed)
}

#[test]
fn accepts_a_statement_whose_only_uses_are_shared_call_arg_reads() {
    // let cur = map_get(&stats, k)
    let s = shape(|vt, stats, k| {
        let cur = vt.alloc(sym("cur"), Ty::Int, Mutability::Let, None);
        vec![bind(cur, Ty::Int, call("map_get", vec![borrow(stats, map_ty(), false), var(k, Ty::String)], Ty::Int))]
    });
    let (marked, stats, changed) = run(s);
    assert!(changed, "the qualifying read must be marked");
    assert_eq!(marked, vec![stats], "exactly the cell read carries the marker");
}

#[test]
fn accepts_two_shared_reads_in_one_statement() {
    // let cur = add(map_get(&stats, k), map_len(&stats))
    let s = shape(|vt, stats, k| {
        let cur = vt.alloc(sym("cur"), Ty::Int, Mutability::Let, None);
        let a = call("map_get", vec![borrow(stats, map_ty(), false), var(k, Ty::String)], Ty::Int);
        let b = call("map_len", vec![borrow(stats, map_ty(), false)], Ty::Int);
        vec![bind(cur, Ty::Int, call("add", vec![a, b], Ty::Int))]
    });
    let (marked, stats, _) = run(s);
    assert_eq!(marked, vec![stats, stats], "overlapping shared guards are admissible");
}

#[test]
fn declines_a_statement_holding_a_lambda() {
    // let cur = f(&stats, (x) => x): a closure aliasing the cell could run
    // (and take a mut borrow) while the guard is live.
    let s = shape(|vt, stats, _k| {
        let cur = vt.alloc(sym("cur"), Ty::Int, Mutability::Let, None);
        let x = vt.alloc(sym("x"), Ty::Int, Mutability::Let, None);
        let lam = e(IrExprKind::Lambda { params: vec![(x, Ty::Int)], body: Box::new(var(x, Ty::Int)), lambda_id: None }, Ty::Unit);
        vec![bind(cur, Ty::Int, call("f", vec![borrow(stats, map_ty(), false), lam], Ty::Int))]
    });
    let (marked, _, changed) = run(s);
    assert!(marked.is_empty() && !changed, "a lambda in the statement must decline the mark: {marked:?}");
}

#[test]
fn declines_a_statement_with_a_use_outside_call_arg_position() {
    // let cur = f(&stats, stats): the bare read is not a shared call-arg borrow.
    let s = shape(|vt, stats, _k| {
        let cur = vt.alloc(sym("cur"), Ty::Int, Mutability::Let, None);
        vec![bind(cur, Ty::Int, call("f", vec![borrow(stats, map_ty(), false), var(stats, map_ty())], Ty::Int))]
    });
    let (marked, _, _) = run(s);
    assert!(marked.is_empty(), "a use outside the safe shape must decline: {marked:?}");
}

#[test]
fn declines_a_mutable_borrow() {
    // insert(&mut stats, k): a mut guard is not a shared read.
    let s = shape(|_vt, stats, k| {
        vec![IrStmt { kind: IrStmtKind::Expr { expr: call("insert", vec![borrow(stats, map_ty(), true), var(k, Ty::String)], Ty::Unit) }, span: None }]
    });
    let (marked, _, _) = run(s);
    assert!(marked.is_empty(), "a mutable borrow must not be marked: {marked:?}");
}

#[test]
fn declines_a_statement_that_writes_the_cell() {
    // stats = g(&stats): the write ends the statement's shared-only story.
    let s = shape(|_vt, stats, _k| {
        vec![IrStmt { kind: IrStmtKind::Assign { var: stats, value: call("g", vec![borrow(stats, map_ty(), false)], map_ty()) }, span: None }]
    });
    let (marked, _, _) = run(s);
    assert!(marked.is_empty(), "a statement writing the cell must decline: {marked:?}");
}

#[test]
fn declines_a_for_loop_head_read() {
    // for x in keys(&stats) { insert(&mut stats, x) }: the head's temporaries
    // live for the whole loop, across the body's mut borrow.
    let s = shape(|vt, stats, _k| {
        let x = vt.alloc(sym("x"), Ty::String, Mutability::Let, None);
        let body = vec![IrStmt {
            kind: IrStmtKind::Expr { expr: call("insert", vec![borrow(stats, map_ty(), true), var(x, Ty::String)], Ty::Unit) },
            span: None,
        }];
        let iterable = call("keys", vec![borrow(stats, map_ty(), false)], Ty::Applied(TypeConstructorId::List, vec![Ty::String]));
        vec![IrStmt {
            kind: IrStmtKind::Expr { expr: e(IrExprKind::ForIn { var: x, var_tuple: None, iterable: Box::new(iterable), body }, Ty::Unit) },
            span: None,
        }]
    });
    let (marked, _, _) = run(s);
    assert!(marked.is_empty(), "a for-head read must keep the owned snapshot: {marked:?}");
}
