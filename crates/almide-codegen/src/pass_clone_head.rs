//! Head reads that borrow (#3453).
//!
//! Problem: on native, `match list.get(xs, i)` and `xs[i]` borrow the
//! element (#2070), but the sibling spellings of the same read — a field of
//! `list.get(xs, i) ?? d`, a `let` of it, `list.first`, a list-pattern head
//! `[h, ..]`, `option.map` over the head — copied the whole element per read
//! (`almide_rt_list_get` / `.clone()`), so their cost grew with the element.
//!
//! References: Lean 4's `InferBorrow` treats `Array.get!` / `uget` results
//! as projections of the array — owned only when a consumer forces it
//! (`forwardProjectionProp`), and `ExplicitRC` adds the `inc` at that
//! consumer. Swift's `_read` accessors yield a borrow for the duration of
//! the access instead of returning an owned copy (Ownership Manifesto,
//! "Generalized accessors"). Rust itself spells the same thing as
//! `xs.get(i)` returning `Option<&T>` and a clone only at the escape. All
//! three put the copy at the CONSUMER, not at the read.
//!
//! Approach: rewrite each sibling shape into the form the proven
//! match-binder rule (`pass_clone_projection::match_binders`) and the borrow
//! lowering already borrow, and only when every use of the element reads it
//! through a site the walker renders identically for a `&T`
//! (`reads_binding`) — so what escapes is cloned there, exactly once:
//!
//! - `(src ?? d).f`             → `match src { some(v) => v.f, none => d.f }`
//! - `f(&(src ?? d).f)`, `f(src ?? d)` with a place `d` → the arms yield
//!   `&v.f` / `&d.f` (`v` / `&d`), no sibling argument naming the list or `d`
//! - `option.map(src, (h) => e)` → `match src { some(h) => some(e), none => none }`
//! - `let h = src ?? d; rest`   → `let h: _ = match get_ref { Some(v) => v, None => &d }`
//!   (a `let` extends the fallback temporary through the match arm)
//! - `let h = xs[i]; rest`      → `let h: _ = &xs[i]` — the list-pattern head
//!   binder `[h, ..]` lowers to this
//!
//! `src` is `list.get(xs, i)` with a variable or literal index, or
//! `list.first(xs)` (index 0). A `let` rewrite also needs the list and the
//! fallback to stay put over `rest` (`stays_put`: no move, write, capture).
//!
//! Alternatives not taken: a `Cow`-typed binding (needs a type the IR
//! cannot spell and a deref at every generic slot); hoisting a borrowed `??`
//! into a statement-level `let` (reorders the fallback's evaluation against
//! its siblings); flipping an owned param that only the fallback consumed to
//! borrowed (the call sites are already lowered — that is the borrow
//! inference's job). A bare owned param as the fallback is instead MOVED
//! into a local the `none` side reads — inside the arm for a value read,
//! before the `let` for a binding, at fn entry for a borrowed argument when
//! the param is named nowhere else — so the param stays consumed exactly as
//! the `??` consumed it; any other owned-param fallback keeps its copy (see
//! `owned_params`).
//!
//! Compatibility: none observable — the same values on every path, the
//! fallback still evaluated only on the `none` side, out-of-bounds index
//! reads still abort; only the native copies go. Wasm does not run this pass.
//!
//! A `let` binder rewritten this way is returned: it is bound by reference,
//! so it is not an owned value of the clone walk and its annotation is `_`.
use std::collections::HashSet;
use almide_ir::*;
use almide_ir::visit_mut::{IrMutVisitor, walk_expr_mut};
use almide_lang::types::{Ty, constructor::TypeConstructorId};
use super::pass_clone_projection::{reads_binding, root};

struct Heads<'a> {
    vt: &'a mut VarTable,
    top_lets: &'a HashSet<VarId>,
    /// The fn's heap params rendered owned. The signature is decided before
    /// this pass, so one whose only consumption is the `??` fallback (or a
    /// capture by the inlined callback) would be left owned and never
    /// consumed — every caller paying a clone (the certifier's C4). Those
    /// reads keep their copy.
    owned_params: HashSet<VarId>,
    /// Every param: a borrowed one handed on whole as the `None` side's
    /// reference lowers to the bare param, which reads as a consumption.
    params: HashSet<VarId>,
    /// Owned params named exactly once in the body, outside every closure:
    /// such a param may be moved into a local at entry, which a borrowed
    /// `none` side then points at (`entry_moves`).
    single_use: HashSet<VarId>,
    entry_moves: Vec<IrStmt>,
    ref_lets: HashSet<VarId>,
}

/// Rewrite every head read of `func`'s body; returns the `let` binders now
/// bound by reference.
pub(super) fn rewrite(func: &mut IrFunction, vt: &mut VarTable, top_lets: &HashSet<VarId>) -> HashSet<VarId> {
    use super::use_kind::{ExplicitBorrows, Site, UseSites};
    let owned_params: HashSet<VarId> = func.params.iter()
        .filter(|p| p.borrow == ParamBorrow::Own && super::pass_clone::needs_clone(&p.ty))
        .map(|p| p.var).collect();
    let sites = UseSites::of_expr(&func.body, Site::Result, &ExplicitBorrows);
    let single_use = owned_params.iter().copied()
        .filter(|&p| { let mut uses = sites.of(p); uses.next().is_some_and(|u| u.depth == 0 && !u.in_chain) && uses.next().is_none() })
        .collect();
    let params = func.params.iter().map(|p| p.var).collect();
    let mut heads = Heads { vt, top_lets, owned_params, params, single_use, entry_moves: Vec::new(), ref_lets: HashSet::new() };
    heads.visit_expr_mut(&mut func.body);
    if !heads.entry_moves.is_empty() {
        let body = std::mem::replace(&mut func.body, mk(IrExprKind::Unit, Ty::Unit, None));
        let (ty, span) = (body.ty.clone(), body.span);
        func.body = match body.kind {
            IrExprKind::Block { mut stmts, expr } => {
                stmts.splice(0..0, heads.entry_moves);
                mk(IrExprKind::Block { stmts, expr }, ty, span)
            }
            _ => mk(IrExprKind::Block { stmts: heads.entry_moves, expr: Some(Box::new(body)) }, ty, span),
        };
    }
    heads.ref_lets
}

fn mk(kind: IrExprKind, ty: Ty, span: Option<almide_base::Span>) -> IrExpr {
    IrExpr { kind, ty, span, def_id: None }
}

/// The list a head source reads: `almide_rt_list_get(xs, i)` with a variable
/// or literal index, or `almide_rt_list_first(xs)`.
pub(super) fn head_source(e: &IrExpr) -> Option<VarId> {
    match &e.kind {
        IrExprKind::RuntimeCall { symbol, args } => match (symbol.as_str(), args.as_slice()) {
            ("almide_rt_list_get", [xs, i]) if super::pass_clone_projection::borrowable_index(i) => root(xs),
            ("almide_rt_list_first", [xs]) => root(xs),
            _ => None,
        },
        _ => None,
    }
}

/// An element worth borrowing: a heap value whose `&T` the walker spells
/// like the value at every `reads_binding` site.
fn heap_element(ty: &Ty) -> bool {
    let shape = matches!(ty, Ty::String | Ty::Named(..) | Ty::Record { .. } | Ty::Variant { .. } | Ty::Tuple(_)
        | Ty::Applied(TypeConstructorId::List | TypeConstructorId::Map | TypeConstructorId::Set | TypeConstructorId::Option, _));
    shape && super::pass_clone::needs_clone(ty)
}

fn option_payload(ty: &Ty) -> Option<&Ty> {
    match ty {
        Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => Some(&args[0]),
        _ => None,
    }
}

/// A lambda body that may run inline in the enclosing fn: nothing in it
/// leaves the lambda (`!`, a loop exit, a tail call).
fn inlinable(body: &IrExpr) -> bool {
    struct Exits(bool);
    impl almide_ir::visit::IrVisitor for Exits {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e.kind, IrExprKind::Try { .. } | IrExprKind::Break | IrExprKind::Continue | IrExprKind::TailCall { .. }) {
                self.0 = true;
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    let mut exits = Exits(false);
    almide_ir::visit::IrVisitor::visit_expr(&mut exits, body);
    !exits.0
}

/// Does every occurrence of `v` in `rest` leave it in place — no move (a
/// heap field projected into a consuming position included), no write, no
/// capture — so a reference into it stays valid across `rest`?
fn stays_put(rest: &IrExpr, v: VarId) -> bool {
    use super::use_kind::{ExplicitBorrows, Site, SlotMode, UseSites, element_top_consumes};
    UseSites::of_expr(rest, Site::Result, &ExplicitBorrows).of(v).all(|u| {
        u.depth == 0 && !u.in_chain && !u.in_mut && !u.is_write(true) && match u.site {
            Site::Borrow { mutable: false } | Site::Arg(SlotMode::Borrow) | Site::Clone | Site::Index => true,
            Site::Member | Site::TupleIndex => u.chain.is_some_and(|c| !(c.heap && element_top_consumes(c.top))),
            _ => false,
        }
    })
}

/// The variables a borrowed read chain over `src ?? d` holds: the list and
/// the fallback's place.
fn held_by(chain: &IrExpr) -> Option<Vec<VarId>> {
    let IrExprKind::UnwrapOr { expr: src, fallback } = &unwrap_or_base(chain)?.kind else { return None };
    Some(std::iter::once(head_source(src)?).chain(root(fallback)).collect())
}

/// The `??` a chain of field / tuple reads projects.
fn unwrap_or_base(e: &IrExpr) -> Option<&IrExpr> {
    match &e.kind {
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => unwrap_or_base(object),
        IrExprKind::UnwrapOr { .. } => Some(e),
        _ => None,
    }
}

/// The read chain `e` rebuilt over `base` in place of its `??`.
fn reproject(e: &IrExpr, base: IrExpr) -> IrExpr {
    let kind = match &e.kind {
        IrExprKind::Member { object, field } => IrExprKind::Member { object: Box::new(reproject(object, base)), field: *field },
        IrExprKind::TupleIndex { object, index } => IrExprKind::TupleIndex { object: Box::new(reproject(object, base)), index: *index },
        _ => return base,
    };
    mk(kind, e.ty.clone(), e.span)
}

fn some_arm(var: VarId, ty: &Ty, body: IrExpr) -> IrMatchArm {
    IrMatchArm { pattern: IrPattern::Some { inner: Box::new(IrPattern::Bind { var, ty: ty.clone() }) }, guard: None, body }
}

fn none_arm(body: IrExpr) -> IrMatchArm {
    IrMatchArm { pattern: IrPattern::None, guard: None, body }
}

impl Heads<'_> {
    fn is_param(&self, e: &IrExpr) -> bool {
        matches!(e.kind, IrExprKind::Var { id } if self.params.contains(&id))
    }

    fn fresh(&mut self, name: &str, ty: &Ty) -> VarId {
        self.vt.alloc(almide_base::intern::sym(name), ty.clone(), Mutability::Let, None)
    }

    /// `(src ?? d).f` reads one field of the element: push the read chain
    /// into both arms of an explicit match the binder proof borrows.
    fn project(&mut self, e: &mut IrExpr) {
        if !matches!(e.kind, IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }) { return; }
        if let Some(m) = self.head_match(e, false) { *e = m; }
    }

    /// A call argument `&(src ?? d).f` (or `&(src ?? d)`) over a place
    /// fallback borrows the element's field in place. No sibling argument
    /// may name the list or the fallback: the reference lives through the call.
    fn borrow_args(&mut self, e: &mut IrExpr) {
        let (IrExprKind::Call { args, .. } | IrExprKind::RuntimeCall { args, .. }) = &mut e.kind else { return };
        for i in 0..args.len() {
            let IrExprKind::Borrow { expr: chain, as_str: false, mutable: false } = &args[i].kind else { continue };
            let Some(held) = held_by(chain) else { continue };
            if args.iter().enumerate().any(|(j, a)| j != i && held.iter().any(|&v| super::pass_clone_projection::mentions(a, v))) { continue; }
            let Some(m) = self.head_match(chain, true) else { continue };
            args[i] = IrExpr { ty: args[i].ty.clone(), ..m };
        }
    }

    /// The match a read chain over `src ?? d` becomes.
    fn head_match(&mut self, chain: &IrExpr, borrowed: bool) -> Option<IrExpr> {
        let IrExprKind::UnwrapOr { expr: src, fallback } = &unwrap_or_base(chain)?.kind else { return None };
        let (xs, elem) = (head_source(src)?, option_payload(&src.ty)?.clone());
        let place = root(fallback);
        let whole = matches!(chain.kind, IrExprKind::UnwrapOr { .. });
        if !heap_element(&elem) || !self.fallback_admits(fallback, xs, borrowed, whole) { return None; }
        let moved;
        let fallback = if borrowed && self.is_param(fallback) && place.is_some_and(|p| self.owned_params.contains(&p)) {
            moved = self.move_at_entry(fallback, &elem);
            &moved
        } else { fallback };
        let v = self.fresh("__head", &elem);
        let var = mk(IrExprKind::Var { id: v }, elem.clone(), chain.span);
        let borrow = |e: IrExpr| mk(IrExprKind::Borrow { expr: Box::new(e), as_str: false, mutable: false }, chain.ty.clone(), chain.span);
        let (some, none, subject) = match (borrowed, whole) {
            (false, _) => (reproject(chain, var), self.consumed_fallback(chain, fallback, &elem), (**src).clone()),
            (true, true) => (var, borrow((**fallback).clone()), super::pass_clone_projection::borrowed_subject((**src).clone())),
            (true, false) => (borrow(reproject(chain, var)), borrow(reproject(chain, (**fallback).clone())),
                super::pass_clone_projection::borrowed_subject((**src).clone())),
        };
        let arms = vec![some_arm(v, &elem, some), none_arm(none)];
        // A value read the binder proof would not borrow stays a copy.
        if !borrowed && super::pass_clone_projection::match_binders(&subject, &arms).is_none() { return None; }
        Some(mk(IrExprKind::Match { subject: Box::new(subject), arms }, chain.ty.clone(), chain.span))
    }

    /// May `fallback` stand on the `none` side of a head read of `xs`? It
    /// may not name the list; a borrowed result needs a place to borrow; a
    /// borrowed param handed on whole lowers to the bare param, which reads
    /// as a consumption; a place inside an owned param is never consumed
    /// any more (C4) — except the bare owned param of a value read, which
    /// [`Self::consumed_fallback`] still moves.
    fn fallback_admits(&self, fallback: &IrExpr, xs: VarId, borrowed: bool, whole: bool) -> bool {
        if super::pass_clone_projection::mentions(fallback, xs) { return false; }
        let Some(place) = root(fallback) else { return !borrowed };
        if self.owned_params.contains(&place) {
            return self.is_param(fallback) && (!borrowed || self.single_use.contains(&place));
        }
        !(borrowed && whole && self.is_param(fallback))
    }

    /// A single-use owned param moved into a local at fn entry; the local's
    /// read in its place.
    fn move_at_entry(&mut self, param: &IrExpr, ty: &Ty) -> Box<IrExpr> {
        let local = self.fresh("__fallback", ty);
        self.entry_moves.push(IrStmt { kind: IrStmtKind::Bind { var: local, mutability: Mutability::Let, ty: ty.clone(), value: param.clone() }, span: param.span });
        Box::new(mk(IrExprKind::Var { id: local }, ty.clone(), param.span))
    }

    /// The `none` side of a value read: the read chain over the fallback. A
    /// bare owned param is moved into a local first, so the param stays
    /// consumed exactly as the `??` consumed it.
    fn consumed_fallback(&mut self, chain: &IrExpr, fallback: &IrExpr, elem: &Ty) -> IrExpr {
        if !matches!(fallback.kind, IrExprKind::Var { id } if self.owned_params.contains(&id)) {
            return reproject(chain, fallback.clone());
        }
        let local = self.fresh("__fallback", elem);
        let bind = IrStmt { kind: IrStmtKind::Bind { var: local, mutability: Mutability::Let, ty: elem.clone(), value: fallback.clone() }, span: chain.span };
        let read = reproject(chain, mk(IrExprKind::Var { id: local }, elem.clone(), chain.span));
        mk(IrExprKind::Block { stmts: vec![bind], expr: Some(Box::new(read)) }, chain.ty.clone(), chain.span)
    }

    /// `option.map(src, (h) => body)` over a head source whose callback only
    /// reads `h`: the callback runs inline in a `some` arm.
    fn map_inline(&mut self, e: &mut IrExpr) {
        let IrExprKind::RuntimeCall { symbol, args } = &e.kind else { return };
        if symbol.as_str() != "almide_rt_option_map" || args.len() != 2 { return; }
        let IrExprKind::Lambda { params, body, .. } = &args[1].kind else { return };
        let (Some(_), Some(elem), [(h, _)]) = (head_source(&args[0]), option_payload(&args[0].ty), params.as_slice()) else { return };
        let arms = [some_arm(*h, elem, (**body).clone()), none_arm(mk(IrExprKind::OptionNone, e.ty.clone(), e.span))];
        let captures_owned = almide_ir::free_vars::free_vars(body, &HashSet::from([*h])).iter().any(|v| self.owned_params.contains(v));
        if !heap_element(elem) || !inlinable(body) || captures_owned
            || super::pass_clone_projection::match_binders(&args[0], &arms).is_none() { return; }
        let IrExprKind::RuntimeCall { args, .. } = std::mem::replace(&mut e.kind, IrExprKind::Unit) else { unreachable!() };
        let Some(src) = args.into_iter().next() else { unreachable!() };
        let [some, none] = arms;
        let some = IrMatchArm { body: mk(IrExprKind::OptionSome { expr: Box::new(some.body) }, e.ty.clone(), e.span), ..some };
        e.kind = IrExprKind::Match { subject: Box::new(src), arms: vec![some, none] };
    }

    /// `let h = <head read>` whose binder the rest of the block only reads,
    /// over a list the rest only reads: bind `h` by reference.
    fn bind_ref(&mut self, stmts: &mut Vec<IrStmt>, tail: Option<&IrExpr>) {
        let mut i = 0;
        while i < stmts.len() {
            if let Some(Some(moved)) = self.bind_ref_at(stmts, i, tail) {
                stmts.insert(i, moved);
                i += 1;
            }
            i += 1;
        }
    }

    /// The `let` at `stmts[i]`, rewritten when it qualifies. A bare owned
    /// param as the fallback is first moved into a local the reference can
    /// point at (returned, to go before the `let`): the param stays consumed,
    /// as the `??` consumed it, and the rest must not name it again.
    fn bind_ref_at(&mut self, stmts: &mut [IrStmt], i: usize, tail: Option<&IrExpr>) -> Option<Option<IrStmt>> {
        let IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value } = &stmts[i].kind else { return None };
        let (h, ty) = (*var, ty.clone());
        let (xs, fallback) = self.let_source(value)?;
        let owned_param = fallback.and_then(|d| match d.kind { IrExprKind::Var { id } if self.owned_params.contains(&id) => Some(id), _ => None });
        if fallback.is_some_and(|d| super::pass_clone_projection::mentions(d, xs)
                || (owned_param.is_none() && (self.is_param(d) || root(d).is_some_and(|p| self.owned_params.contains(&p)))))
            || !heap_element(&ty) || xs == h || self.top_lets.contains(&xs) || self.vt.get(xs).mutability != Mutability::Let { return None; }
        let rest = mk(IrExprKind::Block { stmts: stmts[i + 1..].to_vec(), expr: tail.map(|t| Box::new(t.clone())) }, Ty::Unit, None);
        let held: Vec<VarId> = fallback.iter().flat_map(|d| almide_ir::free_vars::free_vars(d, &HashSet::new())).collect();
        if !super::pass_clone_projection::mentions(&rest, h) || !reads_binding(&rest, h)
            || owned_param.is_some_and(|p| super::pass_clone_projection::mentions(&rest, p))
            || !std::iter::once(xs).chain(held.into_iter().filter(|v| Some(*v) != owned_param)).all(|v| stays_put(&rest, v)) { return None; }
        let span = stmts[i].span;
        let IrStmtKind::Bind { value, .. } = &mut stmts[i].kind else { unreachable!() };
        let mut value = std::mem::replace(value, mk(IrExprKind::Unit, Ty::Unit, None));
        let moved = owned_param.map(|p| {
            let local = self.fresh("__fallback", &ty);
            if let IrExprKind::UnwrapOr { fallback, .. } = &mut value.kind { fallback.kind = IrExprKind::Var { id: local }; }
            IrStmt { kind: IrStmtKind::Bind { var: local, mutability: Mutability::Let, ty: ty.clone(), value: mk(IrExprKind::Var { id: p }, ty.clone(), span) }, span }
        });
        let IrStmtKind::Bind { value: slot, .. } = &mut stmts[i].kind else { unreachable!() };
        *slot = self.borrowed_value(value, &ty);
        self.ref_lets.insert(h);
        Some(moved)
    }

    /// The list a `let`'s head read borrows, and its `??` fallback if any.
    fn let_source<'e>(&self, value: &'e IrExpr) -> Option<(VarId, Option<&'e IrExpr>)> {
        match &value.kind {
            IrExprKind::UnwrapOr { expr: src, fallback } if option_payload(&src.ty).is_some() =>
                head_source(src).map(|xs| (xs, Some(fallback.as_ref()))),
            IrExprKind::Clone { expr: inner } => self.let_source(inner).filter(|(_, d)| d.is_none()),
            IrExprKind::IndexAccess { object, index }
                if matches!(object.ty, Ty::Applied(TypeConstructorId::List, _))
                    && super::pass_clone_projection::borrowable_index(index) =>
                root(object).map(|xs| (xs, None)),
            _ => None,
        }
    }

    fn borrowed_value(&mut self, value: IrExpr, ty: &Ty) -> IrExpr {
        let span = value.span;
        let borrow = |e: IrExpr| mk(IrExprKind::Borrow { expr: Box::new(e), as_str: false, mutable: false }, ty.clone(), span);
        match value.kind {
            IrExprKind::UnwrapOr { expr: src, fallback } => {
                let v = self.fresh("__head", ty);
                let subject = super::pass_clone_projection::borrowed_subject(*src);
                let some = some_arm(v, ty, mk(IrExprKind::Var { id: v }, ty.clone(), span));
                mk(IrExprKind::Match { subject: Box::new(subject), arms: vec![some, none_arm(borrow(*fallback))] }, ty.clone(), span)
            }
            IrExprKind::Clone { expr: inner } => borrow(*inner),
            kind => borrow(mk(kind, ty.clone(), span)),
        }
    }
}

impl IrMutVisitor for Heads<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        // Outermost first: a read chain is rewritten whole, and a borrowed
        // argument before its chain is seen as a value read.
        self.borrow_args(e);
        self.project(e);
        walk_expr_mut(self, e);
        match &mut e.kind {
            IrExprKind::RuntimeCall { .. } => self.map_inline(e),
            IrExprKind::Block { stmts, expr } => self.bind_ref(stmts, expr.as_deref()),
            IrExprKind::ForIn { body, .. } | IrExprKind::While { body, .. } => self.bind_ref(body, None),
            _ => {}
        }
    }
}
