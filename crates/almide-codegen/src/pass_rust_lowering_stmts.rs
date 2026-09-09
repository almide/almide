//! Rust statement rewrites: list pushes, borrow-index lifting, and stored closures.
use std::collections::HashSet;
use almide_ir::*;
use almide_base::intern::sym;
use super::pass_rust_lowering::box_closure_value;

/// Element type of `List[E]`.
fn list_elem_ty(ty: &almide_lang::types::Ty) -> Option<&almide_lang::types::Ty> {
    use almide_lang::types::{Ty, TypeConstructorId};
    if let Ty::Applied(TypeConstructorId::List, args) = ty { args.first() } else { None }
}

/// Value type of `Map[K, V]`.
fn map_value_ty(ty: &almide_lang::types::Ty) -> Option<&almide_lang::types::Ty> {
    use almide_lang::types::{Ty, TypeConstructorId};
    if let Ty::Applied(TypeConstructorId::Map, args) = ty { args.get(1) } else { None }
}

/// Walk all stmts in expressions recursively (Rust push/index peepholes).
pub(super) fn rewrite_stmts_in_expr(expr: &mut IrExpr, vt: &mut VarTable, shared: &HashSet<VarId>) -> bool {
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => rewrite_stmts_in_block(stmts, tail, vt, shared),
        IrExprKind::If { cond, then, else_ } => rewrite_stmts_in_if(cond, then, else_, vt, shared),
        IrExprKind::Match { subject, arms } => rewrite_stmts_in_match(subject, arms, vt, shared),
        IrExprKind::ForIn { iterable, body, .. } => rewrite_stmts_in_for_in(iterable, body, vt, shared),
        IrExprKind::While { cond, body } => rewrite_stmts_in_while(cond, body, vt, shared),
        IrExprKind::Lambda { body, .. } => rewrite_stmts_in_expr(body, vt, shared),
        IrExprKind::RuntimeCall { args, .. } => rewrite_stmts_in_runtime_call_args(args, vt, shared),
        // No nested statements to rewrite — listed explicitly so a new
        // statement-bearing IrExprKind is a compile error, not a silent miss.
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::Var { .. }
        | IrExprKind::FnRef { .. } | IrExprKind::BinOp { .. } | IrExprKind::UnOp { .. }
        | IrExprKind::Fan { .. } | IrExprKind::Break | IrExprKind::Continue
        | IrExprKind::Call { .. } | IrExprKind::TailCall { .. } | IrExprKind::List { .. }
        | IrExprKind::MapLiteral { .. } | IrExprKind::EmptyMap | IrExprKind::Record { .. }
        | IrExprKind::SpreadRecord { .. } | IrExprKind::Tuple { .. } | IrExprKind::Range { .. }
        | IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. } | IrExprKind::IndexAccess { .. }
        | IrExprKind::MapAccess { .. } | IrExprKind::StringInterp { .. } | IrExprKind::ResultOk { .. }
        | IrExprKind::ResultErr { .. } | IrExprKind::OptionSome { .. } | IrExprKind::OptionNone
        | IrExprKind::Try { .. } | IrExprKind::Unwrap { .. } | IrExprKind::UnwrapOr { .. }
        | IrExprKind::ToOption { .. } | IrExprKind::OptionalChain { .. }
        | IrExprKind::Clone { .. } | IrExprKind::Deref { .. } | IrExprKind::Borrow { .. }
        | IrExprKind::BoxNew { .. } | IrExprKind::RcWrap { .. } | IrExprKind::RustMacro { .. }
        | IrExprKind::ToVec { .. } | IrExprKind::RenderedCall { .. } | IrExprKind::InlineRust { .. }
        | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. } | IrExprKind::IterChain { .. }
        | IrExprKind::Hole | IrExprKind::Todo { .. } => false,
    }
}

/// `Block { stmts, expr: tail }` arm of [`rewrite_stmts_in_expr`].
fn rewrite_stmts_in_block(stmts: &mut [IrStmt], tail: &mut Option<Box<IrExpr>>, vt: &mut VarTable, shared: &HashSet<VarId>) -> bool {
    let mut changed = false;
    for s in stmts.iter_mut() {
        if rewrite_stmt(s, vt, shared) { changed = true; }
        rewrite_stmts_in_stmt(s, vt, shared, &mut changed);
    }
    if let Some(e) = tail { if rewrite_stmts_in_expr(e, vt, shared) { changed = true; } }
    changed
}

/// `If { cond, then, else_ }` arm of [`rewrite_stmts_in_expr`].
fn rewrite_stmts_in_if(cond: &mut IrExpr, then: &mut IrExpr, else_: &mut IrExpr, vt: &mut VarTable, shared: &HashSet<VarId>) -> bool {
    let mut changed = false;
    if rewrite_stmts_in_expr(cond, vt, shared) { changed = true; }
    if rewrite_stmts_in_expr(then, vt, shared) { changed = true; }
    if rewrite_stmts_in_expr(else_, vt, shared) { changed = true; }
    changed
}

/// `Match { subject, arms }` arm of [`rewrite_stmts_in_expr`].
fn rewrite_stmts_in_match(subject: &mut IrExpr, arms: &mut [IrMatchArm], vt: &mut VarTable, shared: &HashSet<VarId>) -> bool {
    let mut changed = false;
    if rewrite_stmts_in_expr(subject, vt, shared) { changed = true; }
    for arm in arms {
        if let Some(g) = &mut arm.guard { rewrite_stmts_in_expr(g, vt, shared); }
        if rewrite_stmts_in_expr(&mut arm.body, vt, shared) { changed = true; }
    }
    changed
}

/// `ForIn { iterable, body, .. }` arm of [`rewrite_stmts_in_expr`].
fn rewrite_stmts_in_for_in(iterable: &mut IrExpr, body: &mut [IrStmt], vt: &mut VarTable, shared: &HashSet<VarId>) -> bool {
    let mut changed = false;
    if rewrite_stmts_in_expr(iterable, vt, shared) { changed = true; }
    for s in body.iter_mut() {
        if rewrite_stmt(s, vt, shared) { changed = true; }
        rewrite_stmts_in_stmt(s, vt, shared, &mut changed);
    }
    changed
}

/// `While { cond, body }` arm of [`rewrite_stmts_in_expr`].
fn rewrite_stmts_in_while(cond: &mut IrExpr, body: &mut [IrStmt], vt: &mut VarTable, shared: &HashSet<VarId>) -> bool {
    let mut changed = false;
    if rewrite_stmts_in_expr(cond, vt, shared) { changed = true; }
    for s in body.iter_mut() {
        if rewrite_stmt(s, vt, shared) { changed = true; }
        rewrite_stmts_in_stmt(s, vt, shared, &mut changed);
    }
    changed
}

/// `RuntimeCall { args, .. }` arm of [`rewrite_stmts_in_expr`].
fn rewrite_stmts_in_runtime_call_args(args: &mut [IrExpr], vt: &mut VarTable, shared: &HashSet<VarId>) -> bool {
    let mut changed = false;
    for a in args.iter_mut() { if rewrite_stmts_in_expr(a, vt, shared) { changed = true; } }
    changed
}

/// `IndexAssign { index, value, target }` arm of [`rewrite_stmts_in_stmt`]:
/// `xs[i] = closure` into a `List[Fn]` boxes the stored closure (the
/// expr-level boxing pass can't reach a statement value).
fn rewrite_stmts_in_index_assign(index: &mut IrExpr, value: &mut IrExpr, target: VarId, vt: &mut VarTable, shared: &HashSet<VarId>, changed: &mut bool) {
    if rewrite_stmts_in_expr(index, vt, shared) { *changed = true; }
    if rewrite_stmts_in_expr(value, vt, shared) { *changed = true; }
    let ety = list_elem_ty(&vt.get(target).ty).cloned();
    if let Some(et) = ety {
        if matches!(&et, almide_lang::types::Ty::Fn { .. }) && box_closure_value(value, &et) { *changed = true; }
    }
}

/// `MapInsert { key, value, target }` arm of [`rewrite_stmts_in_stmt`]:
/// `m[k] = closure` / `m = map.set(m,k,closure)` (lowered to MapInsert)
/// into a closure-valued map boxes the stored closure, same reasoning as
/// [`rewrite_stmts_in_index_assign`].
fn rewrite_stmts_in_map_insert(key: &mut IrExpr, value: &mut IrExpr, target: VarId, vt: &mut VarTable, shared: &HashSet<VarId>, changed: &mut bool) {
    if rewrite_stmts_in_expr(key, vt, shared) { *changed = true; }
    if rewrite_stmts_in_expr(value, vt, shared) { *changed = true; }
    let vty = map_value_ty(&vt.get(target).ty).cloned();
    if let Some(vt_) = vty {
        if matches!(&vt_, almide_lang::types::Ty::Fn { .. }) && box_closure_value(value, &vt_) { *changed = true; }
    }
}

fn rewrite_stmts_in_stmt(stmt: &mut IrStmt, vt: &mut VarTable, shared: &HashSet<VarId>, changed: &mut bool) {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => {
            if rewrite_stmts_in_expr(value, vt, shared) { *changed = true; }
        }
        IrStmtKind::IndexAssign { index, value, target } => rewrite_stmts_in_index_assign(index, value, *target, vt, shared, changed),
        IrStmtKind::MapInsert { key, value, target } => rewrite_stmts_in_map_insert(key, value, *target, vt, shared, changed),
        IrStmtKind::Guard { cond, else_ } => {
            if rewrite_stmts_in_expr(cond, vt, shared) { *changed = true; }
            if rewrite_stmts_in_expr(else_, vt, shared) { *changed = true; }
        }
        IrStmtKind::Expr { expr } => {
            if rewrite_stmts_in_expr(expr, vt, shared) { *changed = true; }
        }
        // No statement value to descend — listed explicitly so a new
        // expr-bearing IrStmtKind is a compile error, not a silent miss.
        IrStmtKind::Comment { .. } | IrStmtKind::RcDec { .. } | IrStmtKind::RcInc { .. }
        | IrStmtKind::ListCopySlice { .. } | IrStmtKind::ListReverse { .. }
        | IrStmtKind::ListRotateLeft { .. } | IrStmtKind::ListSwap { .. } => {}
    }
}

/// Try to rewrite a single statement.
fn rewrite_stmt(stmt: &mut IrStmt, vt: &mut VarTable, shared: &HashSet<VarId>) -> bool {
    let span = stmt.span;
    // (1) xs = xs + [v] → xs.push(v) — but not for a shared cell (see run()).
    if let IrStmtKind::Assign { var, value } = &stmt.kind {
        if let Some(push_stmt) = try_rewrite_push(*var, value, span, shared) {
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
                span: None, def_id: None,
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
                    span: None, def_id: None,
                },
            };
            return true;
        }
    }
    false
}

/// Rewrite `xs = xs + [v]` → `Expr(Call(xs.push, [v]))`.
fn try_rewrite_push(var: VarId, value: &IrExpr, span: Option<almide_base::Span>, shared: &HashSet<VarId>) -> Option<IrStmt> {
    // A shared-cell var keeps its `Assign` so the walker writes through the cell
    // (`xs.set(…)`); rewriting to `xs.push(v)` would push onto a discarded clone.
    if shared.contains(&var) { return None; }
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
                    span: None, def_id: None,
                }),
                method: sym("push"),
            },
            args: vec![elements[0].clone()],
            type_args: vec![],
        },
        ty: almide_lang::types::Ty::Unit,
        span: None, def_id: None,
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
            call_target_references_var(target, var) || args.iter().any(|a| expr_references_var(a, var))
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

fn call_target_references_var(target: &CallTarget, var: VarId) -> bool {
    match target {
        CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } =>
            expr_references_var(object, var),
        _ => false,
    }
}
