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

use std::cell::RefCell;
use std::collections::HashSet;
use almide_ir::*;
use almide_lang::types::{Ty, TypeConstructorId};
use super::pass_clone::{CloneCtx, insert_clones_live, insert_clone_stmts_live};

thread_local! {
    /// Loop binders whose bodies only borrow them — collected per program by
    /// [`insert_clones_for_in`], drained into `CodegenAnnotations::borrowed_loop_vars`
    /// by the pass's `run` (see [`take_borrowed_loop_vars`]).
    static BORROWED_LOOP_VARS: RefCell<HashSet<VarId>> = RefCell::new(HashSet::new());
}

/// Hand the collected set to the pass and reset for the next program.
pub(crate) fn take_borrowed_loop_vars() -> HashSet<VarId> {
    BORROWED_LOOP_VARS.with(|c| std::mem::take(&mut *c.borrow_mut()))
}

/// `ForIn { var, var_tuple, iterable, body }` arm of [`insert_clones_live`]:
/// the iterable is NOT in the loop, the body IS.
pub(crate) fn insert_clones_for_in(var: VarId, var_tuple: Option<Vec<VarId>>, iterable: IrExpr, body: Vec<IrStmt>, ctx: &mut CloneCtx) -> IrExprKind {
    let new_iterable = strip_list_iterable_clone(insert_clones_live(iterable, ctx), &body);
    let fresh = loop_fresh_vars(Some(var), var_tuple.as_deref(), &body);
    let mut loop_ctx = CloneCtx { always: ctx.always, eligible: ctx.eligible, remaining: ctx.remaining, in_loop: true, memo: ctx.memo, fresh: &fresh };
    let new_body = insert_clone_stmts_live(body, &mut loop_ctx);
    // With the body's clones and moves placed, a binder every use of which
    // sits under a shared borrow / field read / clone never needs an owned
    // element at all: iterate `xs.iter()` and bind `&T` (#1673). A List head
    // only — a Map loop consumes its pairs by value.
    let is_list = matches!(&new_iterable.ty, Ty::Applied(TypeConstructorId::List, _));
    if is_list && var_tuple.is_none() && only_borrowed_uses(&new_body, var) {
        BORROWED_LOOP_VARS.with(|c| { c.borrow_mut().insert(var); });
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
    struct Scan { v: VarId, ok: bool }
    impl almide_ir::visit::IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            if !self.ok { return; }
            let is_v = |x: &IrExpr| matches!(&x.kind, IrExprKind::Var { id } if *id == self.v);
            match &e.kind {
                IrExprKind::Borrow { expr, mutable: false, .. } if is_v(expr) => return,
                IrExprKind::Member { object, .. } if is_v(object) => return,
                IrExprKind::Clone { expr } if is_v(expr) => return,
                IrExprKind::Var { id } if *id == self.v => { self.ok = false; return; }
                IrExprKind::Lambda { .. } | IrExprKind::IterChain { .. } => {
                    if uses_var_anywhere(e, self.v) { self.ok = false; }
                    return;
                }
                _ => {}
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    use almide_ir::visit::IrVisitor;
    let mut s = Scan { v, ok: true };
    for st in body { s.visit_stmt(st); }
    s.ok
}

/// Does `v` occur anywhere under `e` (closure bodies included)?
fn uses_var_anywhere(e: &IrExpr, v: VarId) -> bool {
    struct U { v: VarId, hit: bool }
    impl almide_ir::visit::IrVisitor for U {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(&e.kind, IrExprKind::Var { id } if *id == self.v) { self.hit = true; }
            if !self.hit { almide_ir::visit::walk_expr(self, e); }
        }
    }
    use almide_ir::visit::IrVisitor;
    let mut u = U { v, hit: false };
    u.visit_expr(e);
    u.hit
}

/// `While { cond, body }` arm of [`insert_clones_live`]: cond and body are
/// both in the loop.
pub(crate) fn insert_clones_while(cond: IrExpr, body: Vec<IrStmt>, ctx: &mut CloneCtx) -> IrExprKind {
    let fresh = loop_fresh_vars(None, None, &body);
    let mut loop_ctx = CloneCtx { always: ctx.always, eligible: ctx.eligible, remaining: ctx.remaining, in_loop: true, memo: ctx.memo, fresh: &fresh };
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
            if is_list && matches!(&inner.kind, IrExprKind::Var { id } if !body_writes_var(body, *id)) =>
        {
            *inner
        }
        kind => IrExpr { kind, ..iterable },
    }
}

/// The vars a loop rebinds on every iteration: its own binders plus the
/// top-level `let`s of its body. Deliberately shallow — a `let` inside a
/// nested loop or lambda belongs to THAT scope's freshness, and a `let`
/// inside an `if`/`match` arm is left conservative (cloned) rather than
/// reasoned about here.
fn loop_fresh_vars(var: Option<VarId>, var_tuple: Option<&[VarId]>, body: &[IrStmt]) -> HashSet<VarId> {
    let mut fresh: HashSet<VarId> = var.into_iter().collect();
    fresh.extend(var_tuple.into_iter().flatten().copied());
    for s in body {
        if let IrStmtKind::Bind { var, .. } = &s.kind { fresh.insert(*var); }
    }
    fresh
}

/// Does any statement of `body` (at any depth, lambdas included) write `v`:
/// reassign it, write an element/field/key of it, or `&mut`-borrow it (the
/// form `list.push(v, …)` takes after `BorrowInsertionPass`)?
fn body_writes_var(body: &[IrStmt], v: VarId) -> bool {
    struct W { v: VarId, hit: bool }
    impl almide_ir::visit::IrVisitor for W {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Borrow { expr: inner, mutable: true, .. } = &e.kind
                && matches!(&inner.kind, IrExprKind::Var { id } if *id == self.v)
            {
                self.hit = true;
            }
            almide_ir::visit::walk_expr(self, e);
        }
        fn visit_stmt(&mut self, s: &IrStmt) {
            match &s.kind {
                IrStmtKind::Assign { var, .. } if *var == self.v => self.hit = true,
                IrStmtKind::IndexAssign { target, .. }
                | IrStmtKind::MapInsert { target, .. }
                | IrStmtKind::FieldAssign { target, .. } if *target == self.v => self.hit = true,
                _ => {}
            }
            almide_ir::visit::walk_stmt(self, s);
        }
    }
    use almide_ir::visit::IrVisitor;
    let mut w = W { v, hit: false };
    for s in body { w.visit_stmt(s); }
    w.hit
}
