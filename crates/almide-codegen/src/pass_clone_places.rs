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
        if ctx.owned.contains(&id) && !ctx.always.contains(&id)
            && (!ctx.in_loop || (ctx.fresh.contains(&id) && !ctx.loops.binders.contains(&id)))
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

/// `Assign { var: x, value }` arm of `insert_clone_stmts_live` (#3404): when
/// `value` reads `x` exactly once, that read is `x`'s last use before the
/// statement overwrites it, so it moves even inside a loop — `b = add(b, i)`
/// hands `b` over instead of copying it, and the next iteration (or any later
/// read) sees the new value. The read must be the only occurrence of `x` in
/// `value`, outside every closure, loop body, iterator chain, guard and
/// `&mut` (each of which may run it again or keep it borrowed), and `x` must
/// be an owned, uncaptured, non-always-clone binding: a borrowed param or a
/// global cannot be moved from at all.
pub(super) fn insert_clones_reassign(var: VarId, value: IrExpr, ctx: &mut CloneCtx) -> IrStmtKind {
    let local = ctx.owned.contains(&var) && !ctx.always.contains(&var) && !ctx.captured.contains(&var);
    let value = if local { interp_as_append(value, var, None) } else { value };
    let movable = local && reads_once_plainly(&value, var);
    let mut value = insert_clones_live(value, ctx);
    if movable {
        strip_var_clone(&mut value, var);
    }
    IrStmtKind::Assign { var, value }
}

/// `FieldAssign { target: b, field, value }` arm (#3454): the same
/// interpolation-as-append rewrite as [`insert_clones_reassign`], keyed on the
/// place `b.field`. The `Clone` the walk then puts on that read (it does not
/// know the write kills the old value) is the borrow lowering's to replace
/// with `std::mem::take(&mut b.field)`.
pub(super) fn insert_clones_field_reassign(target: VarId, field: Sym, value: IrExpr, ctx: &mut CloneCtx) -> IrExpr {
    let local = ctx.owned.contains(&target) && !ctx.always.contains(&target) && !ctx.captured.contains(&target);
    let value = if local { interp_as_append(value, target, Some(field)) } else { value };
    insert_clones_live(value, ctx)
}

/// #3454: `s = "${s}…"` → `s = s + "…"`, when the interpolation's FIRST piece
/// is the place being overwritten (`s`, or `b.f` when `field` is set) and no
/// later piece mentions its root. The two spell the same bytes — a `String`
/// piece formats as itself — but the concat's left operand is a value the
/// reassignment can move (or take) and extend in place, where `format!`
/// rebuilds the whole accumulated string on every step. Any later piece
/// reading the root keeps the interpolation, and with it the copy.
fn interp_as_append(value: IrExpr, root: VarId, field: Option<Sym>) -> IrExpr {
    let IrExprKind::StringInterp { parts } = &value.kind else { return value };
    let is_place = |e: &IrExpr| match (&e.kind, field) {
        (IrExprKind::Var { id }, None) => *id == root,
        (IrExprKind::Member { object, field: f }, Some(want)) => *f == want
            && matches!(object.kind, IrExprKind::Var { id } if id == root),
        _ => false,
    };
    let first_is_place = matches!(parts.first(), Some(IrStringPart::Expr { expr }) if expr.ty == Ty::String && is_place(expr));
    let rest_free = parts.iter().skip(1).all(|p| match p {
        IrStringPart::Lit { .. } => true,
        IrStringPart::Expr { expr } => !almide_ir::free_vars::free_vars(expr, &Default::default()).contains(&root),
    });
    if !first_is_place || parts.len() < 2 || !rest_free {
        return value;
    }
    let IrExprKind::StringInterp { mut parts } = value.kind else { unreachable!() };
    let rest: Vec<IrStringPart> = parts.split_off(1);
    let Some(IrStringPart::Expr { expr: head }) = parts.pop() else { unreachable!() };
    let tail_kind = match rest.as_slice() {
        [IrStringPart::Lit { value: lit }] => IrExprKind::LitStr { value: lit.clone() },
        _ => IrExprKind::StringInterp { parts: rest },
    };
    let tail = IrExpr { kind: tail_kind, ty: Ty::String, span: value.span, def_id: None };
    let kind = IrExprKind::BinOp { op: BinOp::ConcatStr, left: Box::new(head), right: Box::new(tail) };
    IrExpr { kind, ty: Ty::String, span: value.span, def_id: None }
}

fn reads_once_plainly(value: &IrExpr, var: VarId) -> bool {
    use super::use_kind::{ExplicitBorrows, Site, UseSites};
    let sites = UseSites::of_expr(value, Site::Assigned, &ExplicitBorrows);
    let mut uses = sites.of(var);
    let Some(u) = uses.next() else { return false };
    uses.next().is_none() && u.is_node() && u.depth == 0 && !u.in_chain && !u.in_loop && !u.in_mut
        && !u.in_guard && !u.guard_forced
}

/// `Clone(Var x)` → `Var x` (the single occurrence `reads_once_plainly` found).
fn strip_var_clone(e: &mut IrExpr, var: VarId) {
    use almide_ir::visit_mut::{IrMutVisitor, walk_expr_mut};
    struct Strip(VarId);
    impl IrMutVisitor for Strip {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            if let IrExprKind::Clone { expr } = &e.kind
                && matches!(expr.kind, IrExprKind::Var { id } if id == self.0)
            {
                e.kind = IrExprKind::Var { id: self.0 };
                return;
            }
            walk_expr_mut(self, e);
        }
    }
    Strip(var).visit_expr_mut(e);
}
