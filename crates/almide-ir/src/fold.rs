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
        IrExprKind::BinOp { op, left, right } => {
            match (&left.kind, &right.kind) {
                (IrExprKind::LitInt { value: a }, IrExprKind::LitInt { value: b }) => {
                    match op {
                        BinOp::AddInt => Some(IrExprKind::LitInt { value: a.wrapping_add(*b) }),
                        BinOp::SubInt => Some(IrExprKind::LitInt { value: a.wrapping_sub(*b) }),
                        BinOp::MulInt => Some(IrExprKind::LitInt { value: a.wrapping_mul(*b) }),
                        BinOp::DivInt if *b != 0 => Some(IrExprKind::LitInt { value: a / b }),
                        BinOp::ModInt if *b != 0 => Some(IrExprKind::LitInt { value: a % b }),
                        _ => None,
                    }
                }
                (IrExprKind::LitFloat { value: a }, IrExprKind::LitFloat { value: b }) => {
                    match op {
                        BinOp::AddFloat => Some(IrExprKind::LitFloat { value: a + b }),
                        BinOp::SubFloat => Some(IrExprKind::LitFloat { value: a - b }),
                        BinOp::MulFloat => Some(IrExprKind::LitFloat { value: a * b }),
                        BinOp::DivFloat if *b != 0.0 => Some(IrExprKind::LitFloat { value: a / b }),
                        _ => None,
                    }
                }
                (IrExprKind::LitStr { value: a }, IrExprKind::LitStr { value: b }) => {
                    match op {
                        BinOp::ConcatStr => Some(IrExprKind::LitStr { value: format!("{}{}", a, b) }),
                        _ => None,
                    }
                }
                (IrExprKind::LitBool { value: a }, IrExprKind::LitBool { value: b }) => {
                    match op {
                        BinOp::And => Some(IrExprKind::LitBool { value: *a && *b }),
                        BinOp::Or => Some(IrExprKind::LitBool { value: *a || *b }),
                        _ => None,
                    }
                }
                _ => None,
            }
        }
        IrExprKind::UnOp { op, operand } => {
            match (&op, &operand.kind) {
                (UnOp::NegInt, IrExprKind::LitInt { value }) => Some(IrExprKind::LitInt { value: -value }),
                (UnOp::NegFloat, IrExprKind::LitFloat { value }) => Some(IrExprKind::LitFloat { value: -value }),
                (UnOp::Not, IrExprKind::LitBool { value }) => Some(IrExprKind::LitBool { value: !value }),
                _ => None,
            }
        }
        _ => None,
    }
}
