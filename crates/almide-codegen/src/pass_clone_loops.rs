//! The loop arms of the clone pass (`ForIn` / `While`) and the two facts a
//! loop knows that the flat last-use count does not (#1673):
//!
//! 1. A `List` iterable renders as `xs.iter().cloned()` — a SHARED borrow for
//!    the loop's duration — so a `Clone` under it only ever produced a
//!    throwaway copy of the whole list. It is stripped unless the body writes
//!    the list, where the temporary copy is what keeps the body's `&mut` legal.
//! 2. The loop's own binders and every `let` in its body — at any depth, not
//!    just the top level (#2316) — are rebound on every iteration that reaches
//!    them, so their last use in the body is a move even inside the loop
//!    (`CloneCtx::fresh`). A `let` under an `if` or a `match` arm cannot
//!    outlive the iteration either: its scope ends with that block.
//!
//! Split out of `pass_clone.rs` to keep that file under the `max-lines` limit.

use std::collections::HashSet;
use almide_ir::*;
use almide_lang::types::{Ty, TypeConstructorId};
use super::pass_clone::{CloneCtx, insert_clones_live, insert_clone_stmts_live};
use super::use_kind::{element_reads_only, ExplicitBorrows, Site, Use, UseSites};

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
    /// Every `for` binder seen so far, and every element binder of a chain
    /// whose source is a borrow: fresh per iteration like a body-level
    /// `let`, but possibly bound `&T` (see `borrowed`), so a FIELD of one is
    /// never moved out — a lambda's own params and lets may be.
    pub binders: HashSet<VarId>,
    /// Chain binders a filter-family adapter hands `&&T` (`.iter()` source,
    /// `.filter(|x| ..)`): rebound `let x = *x` with an inferred annotation
    /// (`infer_binding_tys`) so the body reads one `&T` like every other step.
    pub infer: HashSet<VarId>,
}

/// Before a borrowed chain's lambdas are walked (#2287): its source element
/// binders join `binders`, so no field is moved out of what may become a
/// `&T` binding. A consumed source (`.into_iter()`) hands owned elements and
/// keeps the lambda rule (params are fresh, fields may move).
pub(crate) fn note_chain_element_binders(chain: &IrExpr, loops: &mut LoopMarks) {
    let IrExprKind::IterChain { source, consume, steps, collector } = &chain.kind else { return };
    if *consume || !borrowed_element_is_heap(&source.ty) {
        return;
    }
    if let Some(receivers) = almide_ir::source_element_receivers(steps, collector) {
        loops.binders.extend(receivers.iter().map(|(v, _)| *v));
    }
}

/// After the walk: when EVERY lambda the source element reaches only borrows
/// its binder (`element_reads_only` over the final bodies — the same rule the
/// borrow verdict applied, now on the IR with every borrow spelled), the
/// binders are bound `&T` off `xs.iter()` (`borrowed`) and no element is
/// cloned. One consuming lambda keeps `.iter().cloned()` for all of them:
/// the source iterates one way. A filter-family lambda (prepared with a
/// `let x = x.clone()` rebinding for the `&T` Rust hands it) is rebound
/// `let x = *x` instead: off `.iter()` it receives `&&T`, and one deref is
/// the `&T` the rest of the body reads.
pub(crate) fn mark_chain_element_binders(chain: &mut IrExpr, loops: &mut LoopMarks) {
    let IrExprKind::IterChain { source, consume, steps, collector } = &mut chain.kind else { return };
    if *consume {
        return;
    }
    let Some(elem) = list_elem(&source.ty).cloned() else { return };
    if !super::pass_clone::needs_clone(&elem) {
        return;
    }
    let Some(receivers) = almide_ir::source_element_receivers(steps, collector) else { return };
    let reads_only = receivers.iter().all(|(binder, lambda)| match &lambda.kind {
        IrExprKind::Lambda { body, .. } => element_reads_only(&UseSites::of_expr(body, Site::Result, &ExplicitBorrows), *binder, &elem, true),
        _ => false,
    });
    if !reads_only {
        return;
    }
    let binders: HashSet<VarId> = receivers.iter().map(|(v, _)| *v).collect();
    loops.borrowed.extend(binders.iter().copied());
    // Only a filter-family lambda that receives the SOURCE element sits on
    // `&&T`; one after a `map` receives that step's owned output as `&U`,
    // and its `let x = x.clone()` stays.
    for step in steps.iter_mut() {
        if let IterStep::Filter { lambda } = step && let Some(v) = deref_rebinding(lambda, &binders) {
            loops.infer.insert(v);
        }
    }
    if let IterCollector::Count { lambda } = collector && let Some(v) = deref_rebinding(lambda, &binders) {
        loops.infer.insert(v);
    }
}

/// `let x = x.clone()` at the head of a filter-family lambda body whose
/// param is one of `binders` → `let x = *x`.
fn deref_rebinding(lambda: &mut IrExpr, binders: &HashSet<VarId>) -> Option<VarId> {
    let IrExprKind::Lambda { params, body, .. } = &mut lambda.kind else { return None };
    let (param, _) = params.first()?;
    if !binders.contains(param) {
        return None;
    }
    let IrExprKind::Block { stmts, .. } = &mut body.kind else { return None };
    let IrStmtKind::Bind { var, value, .. } = &mut stmts.first_mut()?.kind else { return None };
    if var != param || !matches!(&value.kind, IrExprKind::Clone { expr } if matches!(expr.kind, IrExprKind::Var { id } if id == *var)) {
        return None;
    }
    let IrExprKind::Clone { expr } = std::mem::replace(&mut value.kind, IrExprKind::Unit) else { unreachable!("matched above") };
    value.kind = IrExprKind::Deref { expr };
    Some(*var)
}

fn list_elem(ty: &Ty) -> Option<&Ty> {
    match ty {
        Ty::Applied(TypeConstructorId::List, args) => args.first(),
        _ => None,
    }
}

fn borrowed_element_is_heap(ty: &Ty) -> bool {
    list_elem(ty).is_some_and(super::pass_clone::needs_clone)
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
    ctx.loops.binders.insert(var);
    ctx.loops.binders.extend(var_tuple.iter().flatten().copied());
    let mut loop_ctx = CloneCtx { always: ctx.always, eligible: ctx.eligible, remaining: ctx.remaining, in_loop: true, memo: ctx.memo, fresh: &fresh, owned: ctx.owned, loops: ctx.loops, captured: ctx.captured };
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
    let mut loop_ctx = CloneCtx { always: ctx.always, eligible: ctx.eligible, remaining: ctx.remaining, in_loop: true, memo: ctx.memo, fresh: &fresh, owned: ctx.owned, loops: ctx.loops, captured: ctx.captured };
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
    let mut w = FreshBinds { fresh: &mut fresh };
    for s in body {
        // `visit_stmt`, not `walk_stmt`: the walk recurses into a statement's
        // CHILDREN, so calling it directly skips the top-level statement and
        // drops exactly the binds the old code collected.
        almide_ir::visit::IrVisitor::visit_stmt(&mut w, s);
    }
    fresh
}

/// Every `Bind` reachable in the loop body, not just the ones at its top
/// level (#2316). A `let` inside an `if` or a `match` arm is rebound on every
/// iteration that reaches it, exactly like a top-level one, and its scope ends
/// with that block — so it can never survive into the next iteration and its
/// last use in the body is a move. Collecting only the top level made the same
/// statements clone under a branch and move without one, which is the whole of
/// #2316.
///
/// Lambda bodies are NOT descended into: a closure's binds belong to its own
/// invocation and `insert_clones_live` builds them a separate `fresh` set when
/// it walks the lambda. Folding them in here would mark a binding fresh in the
/// wrong frame.
struct FreshBinds<'a> {
    fresh: &'a mut HashSet<VarId>,
}

impl almide_ir::visit::IrVisitor for FreshBinds<'_> {
    fn visit_expr(&mut self, expr: &IrExpr) {
        if matches!(expr.kind, IrExprKind::Lambda { .. }) {
            return;
        }
        almide_ir::visit::walk_expr(self, expr);
    }
    fn visit_stmt(&mut self, stmt: &IrStmt) {
        if let IrStmtKind::Bind { var, .. } = &stmt.kind {
            self.fresh.insert(*var);
        }
        almide_ir::visit::walk_stmt(self, stmt);
    }
}

/// Does any statement of `body` (at any depth, lambdas included) write `v`:
/// reassign it, write an element/field/key of it, or `&mut`-borrow it or a
/// projection of it (the form `list.push(v, …)` / `list.push(r.items, …)`
/// takes after `BorrowInsertionPass`)?
fn body_writes_var(body: &[IrStmt], v: VarId) -> bool {
    UseSites::of_stmts(body, &ExplicitBorrows).of(v).any(|u| Use::is_write(u, true))
}

/// The union of every loop's FRESH binder set over one function body.
///
/// [`loop_fresh_vars`] answers "which binders are fresh for THIS loop"; this
/// answers "which binders in this function are fresh for the loop they sit in".
/// VarIds are unique after lowering (no shadowing), so the union cannot
/// conflate two bindings, and a var in it is bound inside some loop body.
///
/// Two consumers, deliberately one copy (#2410): the capture-move rule
/// (`pass_capture_clone_bindings::holds_last_occurrence`) and the ownership
/// certifier's C3. Both previously tested a bare `in_loop`, which abstains from
/// a binding that is rebound every iteration and therefore CAN be moved — the
/// #2316 shape. Note the trade: the certifier is otherwise independent of the
/// passes, and sharing this notion means a wrong `loop_fresh_vars` would be
/// invisible to both. It is shared because the alternative — two copies of one
/// rule — is the failure this repo keeps finding.
pub(crate) fn loop_fresh_union(body: &IrExpr) -> HashSet<VarId> {
    struct Union {
        fresh: HashSet<VarId>,
    }
    impl almide_ir::visit::IrVisitor for Union {
        fn visit_expr(&mut self, expr: &IrExpr) {
            match &expr.kind {
                // A closure's binds belong to its own invocation frame, exactly
                // as in `FreshBinds`.
                IrExprKind::Lambda { .. } => return,
                IrExprKind::ForIn { var, var_tuple, body, .. } => {
                    self.fresh.extend(loop_fresh_vars(Some(*var), var_tuple.as_deref(), body));
                }
                IrExprKind::While { body, .. } => {
                    self.fresh.extend(loop_fresh_vars(None, None, body));
                }
                _ => {}
            }
            almide_ir::visit::walk_expr(self, expr);
        }
    }
    let mut v = Union { fresh: HashSet::new() };
    almide_ir::visit::IrVisitor::visit_expr(&mut v, body);
    v.fresh
}
