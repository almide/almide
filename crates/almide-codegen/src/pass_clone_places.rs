//! Ownership at field, index and map projections.
use almide_ir::*;
use almide_base::{Span, Sym};
use almide_lang::types::Ty;
use super::pass_clone::{CloneCtx, insert_clones_live, needs_clone};

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
        if ctx.owned.contains(&id) && !ctx.always.contains(&id) && !ctx.in_loop
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
