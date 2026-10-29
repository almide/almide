//! Phase 3b of the C-132 move-mode rewrite (#3662, #3664): fold the
//! write-back of a tail call that forwards the `mut` params into the tail
//! leaves. A fn with one `mut` param and a non-Unit return was rewritten to
//!
//!   fn go(s, i) -> (T, Buf) = {
//!     let __mp_ret = if i == 0 then s
//!                    else { …; { let (__mp_res, __mp_buf) = go(s, i - 1); s = __mp_buf; __mp_res } };
//!     (__mp_ret, s)
//!   }
//!
//! so the recursive call is followed by its write-back and the final pairing:
//! it is not a tail call, and deep recursion keeps one frame per level on
//! wasm (native's TCO loop does not). Folding pushes the pairing into every
//! leaf of the tail `if` / `match` tree:
//!
//!   fn go(s, i) -> (T, Buf) = if i == 0 then (s, s) else { …; go(s, i - 1) }
//!
//! A write-back leaf whose callee returns the SAME tuple becomes the bare
//! call — the call's tuple is exactly the fn's own after the write-back — and
//! any other leaf `x` becomes the tail the rewrite built with `x` in the
//! value slot (`(x, s)`), the order the old `let __mp_ret = x; (__mp_ret, s)`
//! evaluated in. The call is then in tail position with the fn's own return
//! type, so the emitter takes its self-loop / `return_call` route (C-178).
//!
//! The same fold covers every move-mode form (#3664):
//! * several `mut` params — the tuple holds every buffer, `(ret, b1, …, bN)`,
//!   and the write-back assigns each in order;
//! * a was-Unit fn — the tail holds the buffers only (`{ <body>; (b1, …) }`,
//!   or `{ <body>; b }` for one param when phase 3 did not fold it), a leaf
//!   `x` becomes `{ x; (b1, …) }`;
//! * a never-err effect fn — the call is `f(..)!`, which propagates exactly
//!   as the folded tail `f(..)!` does;
//! * a can-err effect fn that carries its buffers on the err arm
//!   (`mut_param_err_carry`) — the tail is `ok(<tuple>)`, and the write-back
//!   is the arm-wise `match f(..) { ok(..) => { <wb>; r }, err((e, ..)) =>
//!   { <wb>; err((e, p..))! } }`, which yields exactly the callee's Result.
//!
//! NARROW by construction: only a body that is the exact block phase 1 built,
//! and only write-back leaves the call-site rewriter built for these same
//! params, in order, with a callee of the same tuple (and carrier). Anything
//! else is left untouched.

use crate::mut_param::{mut_call_spelling, scope_key, MutFns};
use crate::*;
use almide_lang::types::Ty;

pub(crate) fn fold_value_tail_writebacks(program: &mut IrProgram, mut_fns: &MutFns) {
    for func in program.functions.iter_mut() {
        fold_one_fn(func, "", mut_fns);
    }
    for m in program.modules.iter_mut() {
        let scope = m.name.to_string();
        for func in m.functions.iter_mut() {
            fold_one_fn(func, &scope, mut_fns);
        }
    }
}

/// What a leaf fold needs: the fn's buffers (in slot order), its value type
/// (None = was-Unit), the tuple it pairs them in, and the err-carrying
/// carrier when it has one.
struct Shape<'a> {
    bufs: Vec<(VarId, Ty)>,
    orig: Option<Ty>,
    pair: Ty,
    carrier: Option<Ty>,
    scope: &'a str,
    mut_fns: &'a MutFns,
}

impl Shape<'_> {
    /// The fn's return type: the carrier, or the bare tuple.
    fn ret(&self) -> &Ty {
        self.carrier.as_ref().unwrap_or(&self.pair)
    }

    /// The tail phase 1 built around the value `x`: `(x, b1, …)`, the bare
    /// buffer for a was-Unit single param, under `ok(..)` for a carrier.
    fn pack(&self, x: Option<IrExpr>) -> IrExpr {
        let mut elements: Vec<IrExpr> = x.into_iter().collect();
        elements.extend(self.bufs.iter().map(|(v, t)| var(*v, t.clone())));
        let packed = if elements.len() == 1 {
            elements.remove(0)
        } else {
            node(IrExprKind::Tuple { elements }, self.pair.clone())
        };
        match &self.carrier {
            Some(c) => node(IrExprKind::ResultOk { expr: Box::new(packed) }, c.clone()),
            None => packed,
        }
    }

    /// Is `e` the tail [`Self::pack`] builds around `ret`?
    fn is_pack(&self, e: &IrExpr, ret: Option<VarId>) -> bool {
        let e = match (&self.carrier, &e.kind) {
            (Some(_), IrExprKind::ResultOk { expr }) => &**expr,
            (Some(_), _) => return false,
            (None, _) => e,
        };
        let want: Vec<VarId> = ret.into_iter().chain(self.bufs.iter().map(|(v, _)| *v)).collect();
        match (&e.kind, want.as_slice()) {
            (IrExprKind::Var { id }, [only]) => id == only,
            (IrExprKind::Tuple { elements }, _) => vars_are(elements, &want),
            _ => false,
        }
    }

    /// Does the call `target(args)` forward exactly these buffers, in slot
    /// order, to a move-mode fn of the same shape?
    fn forwards(&self, target: &CallTarget, args: &[IrExpr]) -> bool {
        let Some(name) = mut_call_spelling(target) else { return false };
        let Some((idx, buf_ty, was_unit, orig, extra)) =
            self.mut_fns.get(&name).or_else(|| self.mut_fns.get(&scope_key(self.scope, &name)))
        else {
            return false;
        };
        if *was_unit != self.orig.is_none() || (!*was_unit && Some(orig) != self.orig.as_ref()) {
            return false;
        }
        let slots: Vec<(usize, &Ty)> = std::iter::once((*idx, buf_ty)).chain(extra.iter().map(|(i, t)| (*i, t))).collect();
        slots.len() == self.bufs.len()
            && slots.iter().zip(&self.bufs).all(|(&(i, t), (p, pt))| {
                t == pt && matches!(args.get(i).map(|a| &a.kind), Some(IrExprKind::Var { id }) if id == p)
            })
    }
}

fn fold_one_fn(func: &mut IrFunction, scope: &str, mut_fns: &MutFns) {
    let Some(shape) = shape_of(func, scope, mut_fns) else { return };
    // The phase-1 body: `{ let __mp_ret = E; <pack(__mp_ret)> }`, or
    // `{ E; <pack()> }` for a was-Unit fn.
    let IrExprKind::Block { stmts, expr: Some(tail) } = &func.body.kind else { return };
    let (value, ret_var) = match (stmts.as_slice(), &shape.orig) {
        ([IrStmt { kind: IrStmtKind::Bind { var, value, .. }, .. }], Some(_)) => (value, Some(*var)),
        ([IrStmt { kind: IrStmtKind::Expr { expr }, .. }], None) => (expr, None),
        _ => return,
    };
    if !shape.is_pack(tail, ret_var) {
        return;
    }
    let mut folded = value.clone();
    let mut calls = 0;
    if !fold_leaf(&mut folded, &shape, &mut calls) || calls == 0 {
        return;
    }
    func.body = folded;
}

/// The fold's view of `func`, when it is a rewritten move-mode fn whose
/// return is the plain tuple or the err-carrying carrier.
fn shape_of<'a>(func: &IrFunction, scope: &'a str, mut_fns: &'a MutFns) -> Option<Shape<'a>> {
    let (idx, buf, was_unit, orig, extra) = mut_fns.get(&scope_key(scope, func.name.as_str()))?;
    let mut bufs = vec![(func.params.get(*idx)?.var, buf.clone())];
    for (i, t) in extra {
        bufs.push((func.params.get(*i)?.var, t.clone()));
    }
    let orig = (!was_unit).then(|| orig.clone());
    let elems: Vec<Ty> = orig.iter().cloned().chain(bufs.iter().map(|(_, t)| t.clone())).collect();
    let pair = if elems.len() == 1 { elems[0].clone() } else { Ty::Tuple(elems) };
    let buf_tys: Vec<Ty> = bufs.iter().map(|(_, t)| t.clone()).collect();
    let carried = Ty::result(pair.clone(), crate::mut_param_err_carry::err_payload_ty(&buf_tys));
    let carrier = if func.ret_ty == pair {
        None
    } else if func.ret_ty == carried {
        Some(carried)
    } else {
        return None;
    };
    Some(Shape { bufs, orig, pair, carrier, scope, mut_fns })
}

/// Fold one tail position. `calls` counts the write-back leaves turned into
/// tail calls; a fold that finds none is dropped by the caller.
fn fold_leaf(e: &mut IrExpr, s: &Shape<'_>, calls: &mut usize) -> bool {
    if let Some(call) = carried_writeback_call(e, s) {
        *e = call;
        *calls += 1;
        return true;
    }
    match &mut e.kind {
        IrExprKind::If { then, else_, .. } => {
            if !(fold_leaf(then, s, calls) && fold_leaf(else_, s, calls)) {
                return false;
            }
        }
        IrExprKind::Match { arms, .. } => {
            if !arms.iter_mut().all(|arm| fold_leaf(&mut arm.body, s, calls)) {
                return false;
            }
        }
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            if let Some(call) = take_writeback_call(stmts, tail, s) {
                **tail = call;
                *calls += 1;
            } else if !fold_leaf(tail, s, calls) {
                return false;
            }
        }
        // A diverging leaf never reaches the pairing.
        _ if matches!(e.ty, Ty::Never) => return true,
        _ => {
            let span = e.span;
            let leaf = std::mem::replace(e, node(IrExprKind::Unit, Ty::Unit));
            *e = pack_leaf(leaf, s);
            e.span = span;
            return true;
        }
    }
    e.ty = s.ret().clone();
    true
}

/// A non-call leaf: the value takes the value slot; a was-Unit leaf runs
/// first, then the buffers are read.
fn pack_leaf(leaf: IrExpr, s: &Shape<'_>) -> IrExpr {
    if s.orig.is_some() {
        return s.pack(Some(leaf));
    }
    if matches!(leaf.kind, IrExprKind::Unit) {
        return s.pack(None);
    }
    node(
        IrExprKind::Block { stmts: vec![IrStmt { kind: IrStmtKind::Expr { expr: leaf }, span: None }], expr: Some(Box::new(s.pack(None))) },
        s.ret().clone(),
    )
}

fn var(id: VarId, ty: Ty) -> IrExpr {
    node(IrExprKind::Var { id }, ty)
}

fn node(kind: IrExprKind, ty: Ty) -> IrExpr {
    IrExpr { kind, ty, span: None, def_id: None }
}

fn vars_are(elements: &[IrExpr], want: &[VarId]) -> bool {
    elements.len() == want.len()
        && elements.iter().zip(want).all(|(e, w)| matches!(e.kind, IrExprKind::Var { id } if id == *w))
}

/// The binders a write-back binds the callee's tuple to: `[r?, b1, …, bN]`.
fn binders(pattern: &IrPattern) -> Option<Vec<VarId>> {
    match pattern {
        IrPattern::Bind { var, .. } => Some(vec![*var]),
        IrPattern::Tuple { elements } => elements
            .iter()
            .map(|p| match p {
                IrPattern::Bind { var, .. } => Some(*var),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

/// Do `stmts` assign each buffer from its binder, in order (`p1 = b1; …`)?
fn assigns_back(stmts: &[IrStmt], s: &Shape<'_>, from: &[VarId]) -> bool {
    stmts.len() == s.bufs.len()
        && stmts.iter().zip(s.bufs.iter().zip(from)).all(|(st, ((p, _), b))| {
            matches!(&st.kind, IrStmtKind::Assign { var, value }
                if var == p && matches!(value.kind, IrExprKind::Var { id } if id == *b))
        })
}

/// Is `tail` the value a write-back block yields: `r`, or `()` was-Unit?
fn yields(tail: &IrExpr, res: Option<VarId>) -> bool {
    match res {
        Some(r) => matches!(tail.kind, IrExprKind::Var { id } if id == r),
        None => matches!(tail.kind, IrExprKind::Unit),
    }
}

/// Split `[r?, b1, …]` into the value binder and the buffer binders.
fn split_binders(all: Vec<VarId>, s: &Shape<'_>) -> Option<(Option<VarId>, Vec<VarId>)> {
    let has_res = s.orig.is_some();
    if all.len() != s.bufs.len() + usize::from(has_res) {
        return None;
    }
    let res = has_res.then(|| all[0]);
    Some((res, all[usize::from(has_res)..].to_vec()))
}

/// A plain block ending in the call-site rewriter's write-back of these
/// buffers — `…; let (r?, b1, …) = f(..)[!]; p1 = b1; …; r | ()` with `f` of
/// the same tuple forwarding them — pops the binding and the assignments and
/// returns the call (under its `!`, which propagates as the tail does).
fn take_writeback_call(stmts: &mut Vec<IrStmt>, tail: &IrExpr, s: &Shape<'_>) -> Option<IrExpr> {
    if s.carrier.is_some() {
        return None;
    }
    let n = stmts.len();
    let k = s.bufs.len();
    if n < k + 1 {
        return None;
    }
    let (pattern, call) = match &stmts[n - k - 1].kind {
        IrStmtKind::BindDestructure { pattern, value } => (pattern.clone(), value),
        IrStmtKind::Bind { var, ty, value, .. } => (IrPattern::Bind { var: *var, ty: ty.clone() }, value),
        _ => return None,
    };
    let (res, bufs) = split_binders(binders(&pattern)?, s)?;
    if call.ty != s.pair || !yields(tail, res) || !assigns_back(&stmts[n - k..], s, &bufs) {
        return None;
    }
    let inner = match &call.kind {
        IrExprKind::Unwrap { expr } | IrExprKind::Try { expr } => &**expr,
        _ => call,
    };
    let IrExprKind::Call { target, args, .. } = &inner.kind else { return None };
    if !s.forwards(target, args) {
        return None;
    }
    stmts.truncate(n - k);
    let Some(IrStmt { kind: IrStmtKind::BindDestructure { value, .. } | IrStmtKind::Bind { value, .. }, .. }) = stmts.pop()
    else {
        unreachable!("checked the binding");
    };
    Some(value)
}

/// A carrier fn's arm-wise write-back of these buffers (#2917):
///
///   match f(p1, …) { ok((r?, b1, …)) => { p1 = b1; …; r | () },
///                    err((e, c1, …)) => { p1 = c1; …; err((e, p1, …))! } }
///
/// Both arms rebuild exactly the Result `f` returned, so the match is the
/// call itself.
fn carried_writeback_call(e: &IrExpr, s: &Shape<'_>) -> Option<IrExpr> {
    let carrier = s.carrier.as_ref()?;
    let IrExprKind::Match { subject, arms } = &e.kind else { return None };
    let IrExprKind::Call { target, args, .. } = &subject.kind else { return None };
    let [ok, err] = arms.as_slice() else { return None };
    if subject.ty != *carrier || ok.guard.is_some() || err.guard.is_some() || !s.forwards(target, args) {
        return None;
    }
    (ok_arm_writes_back(ok, s) && err_arm_writes_back(err, s)).then(|| (**subject).clone())
}

fn ok_arm_writes_back(arm: &IrMatchArm, s: &Shape<'_>) -> bool {
    let IrPattern::Ok { inner } = &arm.pattern else { return false };
    let Some((res, bufs)) = binders(inner).and_then(|all| split_binders(all, s)) else { return false };
    let IrExprKind::Block { stmts, expr: Some(tail) } = &arm.body.kind else { return false };
    assigns_back(stmts, s, &bufs) && yields(tail, res)
}

fn err_arm_writes_back(arm: &IrMatchArm, s: &Shape<'_>) -> bool {
    let IrPattern::Err { inner } = &arm.pattern else { return false };
    let Some(all) = binders(inner) else { return false };
    let Some((e, bufs)) = all.split_first() else { return false };
    let IrExprKind::Block { stmts, expr: Some(tail) } = &arm.body.kind else { return false };
    if bufs.len() != s.bufs.len() || !assigns_back(stmts, s, bufs) {
        return false;
    }
    // The raise re-pairs the message with the buffers just written back.
    let IrExprKind::Unwrap { expr } = &tail.kind else { return false };
    let IrExprKind::ResultErr { expr: payload } = &expr.kind else { return false };
    let IrExprKind::Tuple { elements } = &payload.kind else { return false };
    let want: Vec<VarId> = std::iter::once(*e).chain(s.bufs.iter().map(|(v, _)| *v)).collect();
    vars_are(elements, &want)
}
