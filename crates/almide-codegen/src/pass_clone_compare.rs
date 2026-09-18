//! Stable string operands can be compared as str views, without materializing copies.
use almide_ir::*;
use almide_lang::types::Ty;
use super::pass_clone::{CloneCtx, insert_clones_live};

fn can_borrow(op: BinOp, left: &IrExpr, right: &IrExpr) -> bool {
    matches!(op, BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Lte | BinOp::Gt | BinOp::Gte)
        && left.ty == Ty::String && right.ty == Ty::String
        && stable(left) && stable(right)
}

// Keep snapshot semantics when evaluating either operand can mutate the other.
// Indexing, calls, blocks, and owned temporaries retain the existing value path.
fn stable(expr: &IrExpr) -> bool {
    match &expr.kind {
        IrExprKind::Var { .. } | IrExprKind::LitStr { .. } => true,
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => stable(object),
        _ => false,
    }
}

fn borrow(expr: IrExpr, ctx: &mut CloneCtx) -> Box<IrExpr> {
    let ty = expr.ty.clone();
    let span = expr.span;
    // Reuse the ordinary Borrow path: it counts the use, removes the redundant
    // value clone, and leaves literals as str rather than allocating Strings.
    Box::new(insert_clones_live(IrExpr {
        kind: IrExprKind::Borrow { expr: Box::new(expr), as_str: true, mutable: false },
        ty, span, def_id: None,
    }, ctx))
}

pub(super) fn rewrite(expr: &mut IrExpr, ctx: &mut CloneCtx) -> bool {
    let IrExprKind::BinOp { op, left, right } = &mut expr.kind else { return false; };
    if !can_borrow(*op, left, right) { return false; }
    *left = borrow(std::mem::take(left.as_mut()), ctx);
    *right = borrow(std::mem::take(right.as_mut()), ctx);
    true
}
