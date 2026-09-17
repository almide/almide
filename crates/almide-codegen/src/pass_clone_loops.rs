//! The loop arms of the clone pass (`ForIn` / `While`) and the two facts a
//! loop knows that the flat last-use count does not (#1673):
//!
//! 1. A `List` iterable renders as `xs.iter().cloned()` — a SHARED borrow for
//!    the loop's duration — so a `Clone` under it only ever produced a
//!    throwaway copy of the whole list. It is stripped unless the body writes
//!    the list, where the temporary copy is what keeps the body's `&mut` legal.
//! 2. The loop's own binders and its body's top-level `let`s are rebound on
//!    every iteration, so their last use in the body is a move even inside
//!    the loop (`CloneCtx::fresh`).
//!
//! Split out of `pass_clone.rs` to keep that file under the `max-lines` limit.

use std::collections::HashSet;
use almide_ir::*;
use almide_lang::types::{Ty, TypeConstructorId};
use super::pass_clone::{CloneCtx, insert_clones_live, insert_clone_stmts_live};
use super::use_kind::{ExplicitBorrows, Site, Use, UseSites};

/// The loop binders the clone walk classified (#1673), drained into
/// `CodegenAnnotations` by the pass's `run`.
#[derive(Default)]
pub(crate) struct LoopMarks {
    /// Binders whose bodies only borrow them: the walker iterates `xs.iter()`
    /// and binds `&T`.
    pub borrowed: HashSet<VarId>,
    /// Binders over a list field whose owned root is dead after the head:
    /// the walker iterates `into_iter()` and moves each element.
    pub consumed: HashSet<VarId>,
}

fn owns_final_field_read(iterable: &IrExpr, body: &[IrStmt], ctx: &CloneCtx) -> bool {
    if ctx.in_loop || !matches!(iterable.kind, IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }) {
        return false;
    }
    let Some(root) = iterable_root(iterable) else { return false; };
    ctx.owned.contains(&root) && !ctx.always.contains(&root) && ctx.remaining.get(&root).copied().unwrap_or(1) <= 1
        && !body_writes_var(body, root)
}

/// `ForIn { var, var_tuple, iterable, body }` arm of [`insert_clones_live`]:
/// the iterable is NOT in the loop, the body IS.
pub(crate) fn insert_clones_for_in(var: VarId, var_tuple: Option<Vec<VarId>>, iterable: IrExpr, body: Vec<IrStmt>, ctx: &mut CloneCtx) -> IrExprKind {
    let owns_field = owns_final_field_read(&iterable, &body, ctx);
    let new_iterable = strip_list_iterable_clone(insert_clones_live(iterable, ctx), &body);
    let fresh = loop_fresh_vars(Some(var), var_tuple.as_deref(), &body);
    let mut loop_ctx = CloneCtx { always: ctx.always, eligible: ctx.eligible, remaining: ctx.remaining, in_loop: true, memo: ctx.memo, fresh: &fresh, owned: ctx.owned, loops: ctx.loops };
    let new_body = insert_clone_stmts_live(body, &mut loop_ctx);
    // With the body's clones and moves placed, a binder every use of which
    // sits under a shared borrow / field read / clone never needs an owned
    // element at all: iterate `xs.iter()` and bind `&T` (#1673). A List head
    // only — a Map loop consumes its pairs by value. An inline `Range` head
    // is typed as a list but renders as the bare `start..end` and binds an
    // OWNED scalar (#2256): it is never a by-reference binder.
    let is_list = matches!(&new_iterable.ty, Ty::Applied(TypeConstructorId::List, _));
    let by_ref_head = is_list && !matches!(new_iterable.kind, IrExprKind::Range { .. });
    if by_ref_head && var_tuple.is_none() && only_borrowed_uses(&new_body, var) {
        ctx.loops.borrowed.insert(var);
    } else if is_list && owns_field {
        ctx.loops.consumed.insert(var);
    }
    IrExprKind::ForIn { var, var_tuple, iterable: Box::new(new_iterable), body: new_body }
}

/// Is every occurrence of `v` in `body` one the Rust walker renders the same
/// for a `&T` binder as for a `T` one: `&v` (a shared `Borrow`, which coerces
/// from `&&T`), `v.field` (auto-deref), or `v.clone()` (resolves to
/// `T::clone` through the reference)? A bare occurrence (a move), a `&mut`,
/// a match subject, an interpolation, or any use inside a closure / fused
/// iterator chain (whose captures must own) says no.
fn only_borrowed_uses(body: &[IrStmt], v: VarId) -> bool {
    UseSites::of_stmts(body, &ExplicitBorrows).of(v).all(|u| {
        u.depth == 0 && !u.in_chain
            && matches!(u.site, Site::Borrow { mutable: false } | Site::Member | Site::Clone)
    })
}

/// `While { cond, body }` arm of [`insert_clones_live`]: cond and body are
/// both in the loop.
pub(crate) fn insert_clones_while(cond: IrExpr, body: Vec<IrStmt>, ctx: &mut CloneCtx) -> IrExprKind {
    let fresh = loop_fresh_vars(None, None, &body);
    let mut loop_ctx = CloneCtx { always: ctx.always, eligible: ctx.eligible, remaining: ctx.remaining, in_loop: true, memo: ctx.memo, fresh: &fresh, owned: ctx.owned, loops: ctx.loops };
    let new_cond = insert_clones_live(cond, &mut loop_ctx);
    let new_body = insert_clone_stmts_live(body, &mut loop_ctx);
    IrExprKind::While { cond: Box::new(new_cond), body: new_body }
}

/// `Clone(Var xs)` as a `List` loop head → `Var xs`, unless `body` writes
/// `xs`. The renderer iterates a List by `.iter().cloned()`, which borrows
/// `xs` for the whole loop; a clone there is a full copy of the list that no
/// one reads (#1673 — 735 ns per iteration on an 8-field object before a
/// single field was touched). A body that assigns `xs`, writes `xs[i]`, or
/// passes `xs` to an in-place op holds `&mut xs` under that borrow, and the
/// throwaway copy is the only thing keeping rustc's E0502 away — keep it.
fn strip_list_iterable_clone(iterable: IrExpr, body: &[IrStmt]) -> IrExpr {
    let is_list = matches!(&iterable.ty, Ty::Applied(TypeConstructorId::List, _));
    match iterable.kind {
        IrExprKind::Clone { expr: inner }
            if is_list && iterable_root(&inner).is_some_and(|id| !body_writes_var(body, id)) =>
        {
            *inner
        }
        kind => IrExpr { kind, ..iterable },
    }
}

/// A field projection borrows its root for the loop duration, just like a bare list.
fn iterable_root(expr: &IrExpr) -> Option<VarId> {
    match &expr.kind {
        IrExprKind::Var { id } => Some(*id),
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::Deref { expr: object } => iterable_root(object),
        _ => None,
    }
}

/// The vars a loop rebinds on every iteration: its own binders plus the
/// top-level `let`s of its body. Deliberately shallow — a `let` inside a
/// nested loop or lambda belongs to THAT scope's freshness, and a `let`
/// inside an `if`/`match` arm is left conservative (cloned) rather than
/// reasoned about here.
pub(crate) fn loop_fresh_vars(var: Option<VarId>, var_tuple: Option<&[VarId]>, body: &[IrStmt]) -> HashSet<VarId> {
    let mut fresh: HashSet<VarId> = var.into_iter().collect();
    fresh.extend(var_tuple.into_iter().flatten().copied());
    for s in body {
        if let IrStmtKind::Bind { var, .. } = &s.kind { fresh.insert(*var); }
    }
    fresh
}

/// Does any statement of `body` (at any depth, lambdas included) write `v`:
/// reassign it, write an element/field/key of it, or `&mut`-borrow it or a
/// projection of it (the form `list.push(v, …)` / `list.push(r.items, …)`
/// takes after `BorrowInsertionPass`)?
fn body_writes_var(body: &[IrStmt], v: VarId) -> bool {
    UseSites::of_stmts(body, &ExplicitBorrows).of(v).any(|u| Use::is_write(u, true))
}
