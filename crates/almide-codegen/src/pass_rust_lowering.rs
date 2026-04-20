//! RustLoweringPass: Rust-specific IR rewrites that keep the walker target-agnostic.
//!
//! 1. **List push**: `xs = xs + [v]` → `Expr(Call(xs.push, [v]))`.
//!    Avoids a full list clone + concat for single-element append.
//!
//! 2. **Borrow index lift**: `xs[f(xs)] = v` → `{ let __idx = f(xs); xs[__idx] = v; }`
//!    Resolves Rust simultaneous mutable+immutable borrow conflicts in IndexAssign.

use almide_ir::*;
use almide_base::intern::sym;
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct RustLoweringPass;

impl NanoPass for RustLoweringPass {
    fn name(&self) -> &str { "RustLowering" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }
    fn depends_on(&self) -> Vec<&'static str> { vec!["CloneInsertion"] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut changed = false;
        let IrProgram { functions, top_lets, modules, var_table, .. } = &mut program;
        for func in functions.iter_mut() {
            if rewrite_stmts_in_expr(&mut func.body, var_table) { changed = true; }
        }
        for tl in top_lets.iter_mut() {
            if rewrite_stmts_in_expr(&mut tl.value, var_table) { changed = true; }
        }
        for module in modules.iter_mut() {
            for func in module.functions.iter_mut() {
                if rewrite_stmts_in_expr(&mut func.body, var_table) { changed = true; }
            }
        }
        PassResult { program, changed }
    }
}

/// Walk all stmts in expressions recursively. Also rewrites UnwrapOr
/// fallback lambdas for List[Fn] contexts with RcWrap.
fn rewrite_stmts_in_expr(expr: &mut IrExpr, vt: &mut VarTable) -> bool {
    let mut changed = false;
    // UnwrapOr with Fn fallback lambda: wrap in RcWrap for List[Fn] compatibility
    if let IrExprKind::UnwrapOr { expr: inner, fallback } = &mut expr.kind {
        if matches!(&fallback.kind, IrExprKind::Lambda { .. })
            && matches!(&expr.ty, almide_lang::types::Ty::Fn { .. })
        {
            if let Some(inner_fn_ty) = inner.ty.option_inner() {
                if matches!(inner_fn_ty, almide_lang::types::Ty::Fn { .. }) {
                    let old_fallback = std::mem::replace(fallback.as_mut(), IrExpr {
                        kind: IrExprKind::Unit, ty: almide_lang::types::Ty::Unit, span: None,
                    });
                    *fallback.as_mut() = IrExpr {
                        ty: old_fallback.ty.clone(),
                        span: old_fallback.span,
                        kind: IrExprKind::RcWrap {
                            expr: Box::new(old_fallback),
                            cast_ty: Some(Box::new(inner_fn_ty.clone())),
                        },
                    };
                    changed = true;
                }
            }
        }
    }
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            for s in stmts.iter_mut() {
                if rewrite_stmt(s, vt) { changed = true; }
                rewrite_stmts_in_stmt(s, vt, &mut changed);
            }
            if let Some(e) = tail { if rewrite_stmts_in_expr(e, vt) { changed = true; } }
        }
        IrExprKind::If { cond, then, else_ } => {
            if rewrite_stmts_in_expr(cond, vt) { changed = true; }
            if rewrite_stmts_in_expr(then, vt) { changed = true; }
            if rewrite_stmts_in_expr(else_, vt) { changed = true; }
        }
        IrExprKind::Match { subject, arms } => {
            if rewrite_stmts_in_expr(subject, vt) { changed = true; }
            for arm in arms {
                if let Some(g) = &mut arm.guard { rewrite_stmts_in_expr(g, vt); }
                if rewrite_stmts_in_expr(&mut arm.body, vt) { changed = true; }
            }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            if rewrite_stmts_in_expr(iterable, vt) { changed = true; }
            for s in body.iter_mut() {
                if rewrite_stmt(s, vt) { changed = true; }
                rewrite_stmts_in_stmt(s, vt, &mut changed);
            }
        }
        IrExprKind::While { cond, body } => {
            if rewrite_stmts_in_expr(cond, vt) { changed = true; }
            for s in body.iter_mut() {
                if rewrite_stmt(s, vt) { changed = true; }
                rewrite_stmts_in_stmt(s, vt, &mut changed);
            }
        }
        IrExprKind::Lambda { body, .. } => {
            if rewrite_stmts_in_expr(body, vt) { changed = true; }
        }
        _ => {}
    }
    changed
}

fn rewrite_stmts_in_stmt(stmt: &mut IrStmt, vt: &mut VarTable, changed: &mut bool) {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => {
            if rewrite_stmts_in_expr(value, vt) { *changed = true; }
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            if rewrite_stmts_in_expr(index, vt) { *changed = true; }
            if rewrite_stmts_in_expr(value, vt) { *changed = true; }
        }
        IrStmtKind::Guard { cond, else_ } => {
            if rewrite_stmts_in_expr(cond, vt) { *changed = true; }
            if rewrite_stmts_in_expr(else_, vt) { *changed = true; }
        }
        IrStmtKind::Expr { expr } => {
            if rewrite_stmts_in_expr(expr, vt) { *changed = true; }
        }
        _ => {}
    }
}

/// Try to rewrite a single statement.
fn rewrite_stmt(stmt: &mut IrStmt, vt: &mut VarTable) -> bool {
    let span = stmt.span;
    // (0) List[Fn] binding: wrap lambda elements in RcWrap
    if let IrStmtKind::Bind { ty, value, .. } = &mut stmt.kind {
        if let almide_lang::types::Ty::Applied(almide_lang::types::TypeConstructorId::List, args) = ty {
            if let Some(fn_ty) = args.first().cloned() {
                if matches!(&fn_ty, almide_lang::types::Ty::Fn { .. }) {
                    if let IrExprKind::List { elements } = &mut value.kind {
                        let cast = Some(Box::new(fn_ty));
                        for elem in elements.iter_mut() {
                            let inner = std::mem::replace(elem, IrExpr {
                                kind: IrExprKind::Unit, ty: almide_lang::types::Ty::Unit, span: None,
                            });
                            *elem = IrExpr {
                                ty: inner.ty.clone(),
                                span: inner.span,
                                kind: IrExprKind::RcWrap { expr: Box::new(inner), cast_ty: cast.clone() },
                            };
                        }
                        // Change type to mark it (walker will render Rc type from the RcWrap nodes)
                        return true;
                    }
                }
            }
        }
    }
    // (1) xs = xs + [v] → xs.push(v)
    if let IrStmtKind::Assign { var, value } = &stmt.kind {
        if let Some(push_stmt) = try_rewrite_push(*var, value, span) {
            *stmt = push_stmt;
            return true;
        }
    }
    // (2) xs[f(xs)] = v → { let __idx = f(xs); xs[__idx] = v; }
    if let IrStmtKind::IndexAssign { target, index, value } = &stmt.kind {
        if expr_references_var(index, *target) {
            let idx_var = vt.alloc(sym("__idx"), almide_lang::types::Ty::Int, Mutability::Let, None);
            let idx_bind = IrStmt {
                kind: IrStmtKind::Bind {
                    var: idx_var,
                    mutability: Mutability::Let,
                    ty: almide_lang::types::Ty::Int,
                    value: index.clone(),
                },
                span,
            };
            let idx_ref = IrExpr {
                kind: IrExprKind::Var { id: idx_var },
                ty: almide_lang::types::Ty::Int,
                span: None,
            };
            let new_assign = IrStmt {
                kind: IrStmtKind::IndexAssign {
                    target: *target,
                    index: idx_ref,
                    value: value.clone(),
                },
                span,
            };
            // Wrap in a Block statement
            stmt.kind = IrStmtKind::Expr {
                expr: IrExpr {
                    kind: IrExprKind::Block {
                        stmts: vec![idx_bind, new_assign],
                        expr: None,
                    },
                    ty: almide_lang::types::Ty::Unit,
                    span: None,
                },
            };
            return true;
        }
    }
    false
}

/// Rewrite `xs = xs + [v]` → `Expr(Call(xs.push, [v]))`.
fn try_rewrite_push(var: VarId, value: &IrExpr, span: Option<almide_base::Span>) -> Option<IrStmt> {
    let IrExprKind::BinOp { op: BinOp::ConcatList, left, right } = &value.kind else { return None; };
    let IrExprKind::List { elements } = &right.kind else { return None; };
    if elements.len() != 1 { return None; }
    let is_self = match &left.kind {
        IrExprKind::Var { id } => *id == var,
        IrExprKind::Clone { expr } => matches!(&expr.kind, IrExprKind::Var { id } if *id == var),
        _ => false,
    };
    if !is_self { return None; }
    let push_call = IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Method {
                object: Box::new(IrExpr {
                    kind: IrExprKind::Var { id: var },
                    ty: left.ty.clone(),
                    span: None,
                }),
                method: sym("push"),
            },
            args: vec![elements[0].clone()],
            type_args: vec![],
        },
        ty: almide_lang::types::Ty::Unit,
        span: None,
    };
    Some(IrStmt {
        kind: IrStmtKind::Expr { expr: push_call },
        span,
    })
}

/// Check if expr references the given variable (for borrow conflict detection).
fn expr_references_var(expr: &IrExpr, var: VarId) -> bool {
    match &expr.kind {
        IrExprKind::Var { id } => *id == var,
        IrExprKind::BinOp { left, right, .. } => {
            expr_references_var(left, var) || expr_references_var(right, var)
        }
        IrExprKind::UnOp { operand, .. } => expr_references_var(operand, var),
        IrExprKind::Call { target, args, .. } => {
            let t = match target {
                CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } => expr_references_var(object, var),
                _ => false,
            };
            t || args.iter().any(|a| expr_references_var(a, var))
        }
        IrExprKind::RuntimeCall { args, .. } => {
            args.iter().any(|a| expr_references_var(a, var))
        }
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            expr_references_var(object, var) || expr_references_var(index, var)
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            expr_references_var(object, var)
        }
        IrExprKind::Clone { expr: e } | IrExprKind::Borrow { expr: e, .. }
        | IrExprKind::Deref { expr: e } | IrExprKind::ToVec { expr: e }
        | IrExprKind::OptionSome { expr: e } | IrExprKind::Try { expr: e }
        | IrExprKind::Unwrap { expr: e } | IrExprKind::ToOption { expr: e } => {
            expr_references_var(e, var)
        }
        IrExprKind::UnwrapOr { expr: e, fallback: f } => {
            expr_references_var(e, var) || expr_references_var(f, var)
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            elements.iter().any(|e| expr_references_var(e, var))
        }
        IrExprKind::If { cond, then, else_ } => {
            expr_references_var(cond, var) || expr_references_var(then, var) || expr_references_var(else_, var)
        }
        _ => false,
    }
}
