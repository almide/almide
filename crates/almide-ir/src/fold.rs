// ── Constant folding (post-pass) ─────────────────────────────────

use super::*;

/// Fold constant expressions in the IR program.
/// e.g. LitInt(1) + LitInt(2) → LitInt(3)
pub fn constant_fold(program: &mut IrProgram) {
    for f in &mut program.functions {
        fold_in_place(&mut f.body);
    }
    for tl in &mut program.top_lets {
        fold_in_place(&mut tl.value);
    }
}

/// Constant-fold `slot` in place. `fold_expr` is by-value so its recursion can
/// go through `IrExpr::map_children`; this swaps a placeholder in to take
/// ownership and writes the folded result back.
fn fold_in_place(slot: &mut IrExpr) {
    let placeholder = IrExpr { kind: IrExprKind::Unit, ty: slot.ty.clone(), span: None, def_id: None };
    let taken = std::mem::replace(slot, placeholder);
    *slot = fold_expr(taken);
}

/// Bottom-up constant fold.
///
/// Recursion goes through `IrExpr::map_children` — the single wildcard-free
/// traversal primitive (it lists every `IrExprKind`, so adding a variant is a
/// compile error there). A hand-rolled `match expr.kind { …; _ => {} }` here
/// would silently drop the children of any un-listed or future node kind — the
/// exact failure class behind the native↔WASM capture divergences (DIV2). See
/// docs/roadmap/active/codegen-traversal-totality.md.
fn fold_expr(mut expr: IrExpr) -> IrExpr {
    // 1. Fold every child first, so parents see already-folded literals.
    expr = expr.map_children(&mut |e| fold_expr(e));
    // 2. Fold this node if it has now become a constant operation.
    if let Some(kind) = try_fold(&expr) {
        expr.kind = kind;
    }
    expr
}

/// The node-level fold decision: the replacement kind, or `None` when no fold
/// applies. This is a *value* match — its `_ => None` is a legitimate "nothing
/// to fold" default, not a recursion drop.
fn try_fold(expr: &IrExpr) -> Option<IrExprKind> {
    match &expr.kind {
        IrExprKind::BinOp { op, left, right } => fold_binop(*op, &left.kind, &right.kind),
        IrExprKind::UnOp { op, operand } => fold_unop(*op, &operand.kind),
        _ => None,
    }
}

/// Fold `left op right` when BOTH operands are literals of the same
/// primitive type. Dispatches to the per-type folder; a mixed or
/// non-literal operand pair is never foldable.
fn fold_binop(op: BinOp, left: &IrExprKind, right: &IrExprKind) -> Option<IrExprKind> {
    match (left, right) {
        (IrExprKind::LitInt { value: a }, IrExprKind::LitInt { value: b }) => fold_int_binop(op, *a, *b),
        (IrExprKind::LitFloat { value: a }, IrExprKind::LitFloat { value: b }) => fold_float_binop(op, *a, *b),
        (IrExprKind::LitBool { value: a }, IrExprKind::LitBool { value: b }) => fold_bool_binop(op, *a, *b),
        (IrExprKind::LitStr { value: a }, IrExprKind::LitStr { value: b }) if op == BinOp::ConcatStr => {
            Some(IrExprKind::LitStr { value: format!("{}{}", a, b) })
        }
        _ => None,
    }
}

/// Int × Int. Arithmetic wraps to match the runtime's `i64` semantics;
/// division and modulo by zero are left un-folded so the program keeps
/// its runtime trap instead of gaining a compile-time one.
fn fold_int_binop(op: BinOp, a: i64, b: i64) -> Option<IrExprKind> {
    let value = match op {
        BinOp::AddInt => a.wrapping_add(b),
        BinOp::SubInt => a.wrapping_sub(b),
        BinOp::MulInt => a.wrapping_mul(b),
        BinOp::DivInt if b != 0 => a / b,
        BinOp::ModInt if b != 0 => a % b,
        _ => return None,
    };
    Some(IrExprKind::LitInt { value })
}

/// Float × Float. Division by zero is left un-folded — the IEEE infinity
/// it would produce is the backend's business, not the folder's.
fn fold_float_binop(op: BinOp, a: f64, b: f64) -> Option<IrExprKind> {
    let value = match op {
        BinOp::AddFloat => a + b,
        BinOp::SubFloat => a - b,
        BinOp::MulFloat => a * b,
        BinOp::DivFloat if b != 0.0 => a / b,
        _ => return None,
    };
    Some(IrExprKind::LitFloat { value })
}

/// Bool × Bool. Both operands are already literals, so short-circuiting
/// is unobservable here.
fn fold_bool_binop(op: BinOp, a: bool, b: bool) -> Option<IrExprKind> {
    let value = match op {
        BinOp::And => a && b,
        BinOp::Or => a || b,
        _ => return None,
    };
    Some(IrExprKind::LitBool { value })
}

/// Fold `op operand` when the operand is a literal of the operator's type.
fn fold_unop(op: UnOp, operand: &IrExprKind) -> Option<IrExprKind> {
    match (op, operand) {
        (UnOp::NegInt, IrExprKind::LitInt { value }) => Some(IrExprKind::LitInt { value: value.wrapping_neg() }),
        (UnOp::NegFloat, IrExprKind::LitFloat { value }) => Some(IrExprKind::LitFloat { value: -value }),
        (UnOp::Not, IrExprKind::LitBool { value }) => Some(IrExprKind::LitBool { value: !value }),
        _ => None,
    }
}
