//! Ownership at field, index and map projections.
use almide_ir::*;
use almide_base::{Span, Sym};
use almide_lang::types::Ty;
use super::pass_clone::{CloneCtx, insert_clones_live, needs_clone};

/// `MapInsert { key, value }` (#2157): may the VALUE be evaluated before the
/// KEY? Yes when the key is a plain variable (optionally already Clone-wrapped)
/// that the value never assigns — reading a var has no observable effect, so
/// the order cannot be told apart from the key-first order the wasm leg
/// evaluates. The clone pass and the walker BOTH consult this: the pass then
/// visits the value first, which makes the key position the var's last use
/// (`m[w] = f(&w)` moves `w` in instead of cloning it), and the walker binds
/// the value to a temporary before the insert so rustc sees the same order.
/// Any other key shape — a call, an index read, a field — keeps key-first.
pub(crate) fn map_insert_value_first(key: &IrExpr, value: &IrExpr) -> bool {
    let inner = match &key.kind {
        IrExprKind::Clone { expr } => &expr.kind,
        other => other,
    };
    let IrExprKind::Var { id } = inner else { return false };
    let mut assigned = std::collections::HashSet::new();
    collect_assigned_vars(value, &mut assigned);
    !assigned.contains(&id.0)
}

/// `IndexAccess { object, index }` arm of [`insert_clones_live`]: borrow the
/// container, clone the element.
pub(super) fn insert_clones_index_access(object: IrExpr, index: IrExpr, ty: Ty, span: Option<Span>, ctx: &mut CloneCtx) -> IrExpr {
    let mut processed_object = insert_clones_live(object, ctx);
    // Strip top-level Clone from container (indexing borrows)
    if let IrExprKind::Clone { expr } = processed_object.kind {
        processed_object = *expr;
    }
    let processed_index = insert_clones_live(index, ctx);
    let access = IrExpr {
        kind: IrExprKind::IndexAccess {
            object: Box::new(processed_object),
            index: Box::new(processed_index),
        },
        ty: ty.clone(), span, def_id: None,
    };
    if needs_clone(&ty) {
        return IrExpr { kind: IrExprKind::Clone { expr: Box::new(access) }, ty, span, def_id: None };
    }
    access
}

/// `MapAccess { object, key }` arm of [`insert_clones_live`]: borrow the
/// container, clone the element.
pub(super) fn insert_clones_map_access(object: IrExpr, key: IrExpr, ty: Ty, span: Option<Span>, ctx: &mut CloneCtx) -> IrExpr {
    let mut processed_object = insert_clones_live(object, ctx);
    if let IrExprKind::Clone { expr } = processed_object.kind {
        processed_object = *expr;
    }
    let processed_key = insert_clones_live(key, ctx);
    let access = IrExpr {
        kind: IrExprKind::MapAccess {
            object: Box::new(processed_object),
            key: Box::new(processed_key),
        },
        ty: ty.clone(), span, def_id: None,
    };
    if needs_clone(&ty) {
        return IrExpr { kind: IrExprKind::Clone { expr: Box::new(access) }, ty, span, def_id: None };
    }
    access
}

/// `Member { object, field }` arm of [`insert_clones_live`]. Mirrors
/// IndexAccess/MapAccess: the container is borrowed (Record may be a `&T`
/// after BorrowInference), and a heap-typed field can't be moved out
/// through the reference. Wrap the access in Clone when the field itself
/// needs cloning.
pub(super) fn insert_clones_member(object: IrExpr, field: Sym, ty: Ty, span: Option<Span>, ctx: &mut CloneCtx) -> IrExpr {
    let can_move = matches!(object.kind, IrExprKind::Var { id }
        if ctx.owned.contains(&id) && !ctx.always.contains(&id) && (!ctx.in_loop || ctx.fresh.contains(&id))
            && ctx.remaining.get(&id).copied().unwrap_or(1) <= 1);
    let object = if let IrExprKind::IndexAccess { object: base, index } = &object.kind
        && super::pass_clone_projection::root(base).is_some()
        && matches!(index.kind, IrExprKind::Var { .. } | IrExprKind::LitInt { .. })
    {
        IrExpr { ty: object.ty.clone(), span: object.span, def_id: None,
            kind: IrExprKind::Borrow { expr: Box::new(object), as_str: false, mutable: false } }
    } else { object };
    let mut processed_object = insert_clones_live(object, ctx);
    if let IrExprKind::Clone { expr } = processed_object.kind {
        processed_object = *expr;
    }
    let access = IrExpr {
        kind: IrExprKind::Member {
            object: Box::new(processed_object),
            field,
        },
        ty: ty.clone(), span, def_id: None,
    };
    if needs_clone(&ty) && !can_move {
        return IrExpr { kind: IrExprKind::Clone { expr: Box::new(access) }, ty, span, def_id: None };
    }
    access
}
