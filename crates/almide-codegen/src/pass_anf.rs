//! ANF (A-Normal Form) Pass: lift heap-typed sub-expressions to let bindings.
//!
//! Transforms nested expressions like `"[" + s + "]"` into:
//!   let __anf_0 = "[" + s
//!   __anf_0 + "]"
//!
//! This ensures every heap allocation has a VarId, so PerceusPass can
//! insert RcDec for it. Without ANF, intermediate heap values in nested
//! calls/binops are invisible to Perceus and leak in WASM.
//!
//! Correctness: `perceus_all_heap_freed` proves that after perceusTransform,
//! every VDecl with heap type gets Dec'd. ANF guarantees every heap alloc
//! IS a VDecl, closing the gap between the proof and the implementation.

use almide_ir::*;
use almide_lang::types::Ty;
use super::pass::{NanoPass, PassResult, Target};

fn is_heap_type(ty: &Ty) -> bool {
    matches!(ty, Ty::String | Ty::Applied(_, _) | Ty::Record { .. } | Ty::Unknown | Ty::Fn { .. })
}

/// Does this expression produce a new heap allocation that should be lifted?
fn needs_lift(expr: &IrExpr) -> bool {
    if !is_heap_type(&expr.ty) { return false; }
    matches!(&expr.kind,
        IrExprKind::Call { .. }
        | IrExprKind::RuntimeCall { .. }
        | IrExprKind::BinOp { .. }
        | IrExprKind::If { .. }
        | IrExprKind::Match { .. }
        | IrExprKind::Block { .. }
    )
}

/// Replace a sub-expression with a Var reference, returning the original
/// expression and a new VarId for the let binding.
fn lift_one(
    expr: &mut IrExpr,
    var_table: &mut VarTable,
    counter: &mut u32,
) -> Option<(VarId, Ty, IrExpr)> {
    if !needs_lift(expr) { return None; }
    let ty = expr.ty.clone();
    let name = almide_base::intern::sym(&format!("__anf_{}", counter));
    *counter += 1;
    let var = var_table.alloc(name, ty.clone(), Mutability::Let, None);
    let original = std::mem::replace(expr, IrExpr {
        kind: IrExprKind::Var { id: var },
        ty: ty.clone(),
        span: None,
        def_id: None,
    });
    Some((var, ty, original))
}

/// Wrap an expression in a Block with preceding let bindings.
fn wrap_with_lets(expr: IrExpr, lifted: Vec<IrStmt>) -> IrExpr {
    if lifted.is_empty() { return expr; }
    let result_ty = expr.ty.clone();
    IrExpr {
        kind: IrExprKind::Block {
            stmts: lifted,
            expr: Some(Box::new(expr)),
        },
        ty: result_ty,
        span: None,
        def_id: None,
    }
}

/// ANF-transform an expression tree. Lifts heap sub-expressions to let bindings.
fn anf_expr(expr: &mut IrExpr, var_table: &mut VarTable, counter: &mut u32) {
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            for stmt in stmts.iter_mut() {
                match &mut stmt.kind {
                    IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } =>
                        anf_expr(value, var_table, counter),
                    IrStmtKind::Expr { expr } =>
                        anf_expr(expr, var_table, counter),
                    IrStmtKind::Guard { cond, else_ } => {
                        anf_expr(cond, var_table, counter);
                        anf_expr(else_, var_table, counter);
                    }
                    _ => {}
                }
            }
            if let Some(t) = tail { anf_expr(t, var_table, counter); }
        }
        IrExprKind::If { cond, then, else_ } => {
            anf_expr(cond, var_table, counter);
            anf_expr(then, var_table, counter);
            anf_expr(else_, var_table, counter);
        }
        IrExprKind::Match { subject, arms } => {
            anf_expr(subject, var_table, counter);
            for arm in arms { anf_expr(&mut arm.body, var_table, counter); }
        }
        IrExprKind::While { cond, body } => {
            anf_expr(cond, var_table, counter);
            for stmt in body.iter_mut() {
                match &mut stmt.kind {
                    IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } =>
                        anf_expr(value, var_table, counter),
                    IrStmtKind::Expr { expr } => anf_expr(expr, var_table, counter),
                    _ => {}
                }
            }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            anf_expr(iterable, var_table, counter);
            for stmt in body.iter_mut() {
                match &mut stmt.kind {
                    IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } =>
                        anf_expr(value, var_table, counter),
                    IrStmtKind::Expr { expr } => anf_expr(expr, var_table, counter),
                    _ => {}
                }
            }
        }
        IrExprKind::Lambda { body, .. } => { anf_expr(body, var_table, counter); }
        _ => {
            // For Call, RuntimeCall, BinOp: lift heap sub-args
            anf_lift_children(expr, var_table, counter);
        }
    }
}

/// Lift heap-typed children of Call/BinOp expressions.
fn anf_lift_children(expr: &mut IrExpr, var_table: &mut VarTable, counter: &mut u32) {
    // Take the expression out, process it, put it back
    let mut owned = std::mem::replace(expr, IrExpr {
        kind: IrExprKind::LitInt { value: 0 }, ty: Ty::Unit, span: None, def_id: None,
    });
    let mut lifted = Vec::new();

    match &mut owned.kind {
        IrExprKind::Call { args, .. } | IrExprKind::RuntimeCall { args, .. } => {
            for arg in args.iter_mut() {
                anf_expr(arg, var_table, counter);
                if let Some((var, ty, original)) = lift_one(arg, var_table, counter) {
                    lifted.push(IrStmt {
                        kind: IrStmtKind::Bind { var, ty, mutability: Mutability::Let, value: original },
                        span: None,
                    });
                }
            }
        }
        IrExprKind::BinOp { left, right, .. } => {
            anf_expr(left, var_table, counter);
            anf_expr(right, var_table, counter);
            if let Some((var, ty, original)) = lift_one(left, var_table, counter) {
                lifted.push(IrStmt {
                    kind: IrStmtKind::Bind { var, ty, mutability: Mutability::Let, value: original },
                    span: None,
                });
            }
            if let Some((var, ty, original)) = lift_one(right, var_table, counter) {
                lifted.push(IrStmt {
                    kind: IrStmtKind::Bind { var, ty, mutability: Mutability::Let, value: original },
                    span: None,
                });
            }
        }
        _ => {}
    }

    *expr = wrap_with_lets(owned, lifted);
}

#[derive(Debug)]
pub struct AnfPass;

impl NanoPass for AnfPass {
    fn name(&self) -> &str { "ANF" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Wasm]) }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut counter = 0u32;
        for func in &mut program.functions {
            if func.is_test { continue; }
            anf_expr(&mut func.body, &mut program.var_table, &mut counter);
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                anf_expr(&mut func.body, &mut program.var_table, &mut counter);
            }
        }
        PassResult { program, changed: counter > 0 }
    }
}
