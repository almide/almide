//! Phase 3b of the C-132 move-mode rewrite (#3662): the VALUE-returning twin
//! of `fold_tail_writebacks`. A fn with one `mut` param and a non-Unit
//! return was rewritten to
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
//! A write-back leaf `{ let (r, b) = f(.., s, ..); s = b; r }` whose callee
//! returns the SAME `(T, Buf)` pair becomes the bare call — the call's pair is
//! exactly `(r, s-after-write-back)` — and any other leaf `x` becomes
//! `(x, s)`, the order the old `let __mp_ret = x; (__mp_ret, s)` evaluated in.
//! The call is now in tail position with the fn's own return type, so the
//! emitter takes its self-loop / `return_call` route (C-178).
//!
//! NARROW by construction: a non-effect fn (no err-carry, no lifted carrier,
//! no `!` around the call) with a single `mut` param, whose body is the
//! exact block `rewrite_value_body` built, and only write-back leaves the
//! call-site rewriter built for that same param with a callee of the same
//! pair type. Anything else is left untouched.

use crate::mut_param::{call_spelling, scope_key, MutFns};
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

/// What a leaf fold needs: the mut param, the fn's pair type and its halves.
struct Shape<'a> {
    p: VarId,
    pair: Ty,
    orig: Ty,
    buf: Ty,
    scope: &'a str,
    mut_fns: &'a MutFns,
}

fn fold_one_fn(func: &mut IrFunction, scope: &str, mut_fns: &MutFns) {
    if func.is_effect {
        return;
    }
    let Some((idx, buf, was_unit, orig, extra)) = mut_fns.get(&scope_key(scope, func.name.as_str())) else { return };
    if *was_unit || !extra.is_empty() {
        return;
    }
    let Some(p) = func.params.get(*idx).map(|prm| prm.var) else { return };
    let pair = Ty::Tuple(vec![orig.clone(), buf.clone()]);
    if func.ret_ty != pair {
        return;
    }
    // The phase-1 body: `{ let __mp_ret = E; (__mp_ret, p) }`.
    let IrExprKind::Block { stmts, expr: Some(tail) } = &func.body.kind else { return };
    let [IrStmt { kind: IrStmtKind::Bind { var: ret_var, value, .. }, .. }] = stmts.as_slice() else { return };
    let IrExprKind::Tuple { elements } = &tail.kind else { return };
    let [IrExpr { kind: IrExprKind::Var { id: r }, .. }, IrExpr { kind: IrExprKind::Var { id: b }, .. }] =
        elements.as_slice()
    else {
        return;
    };
    if r != ret_var || *b != p {
        return;
    }
    let shape = Shape { p, pair: pair.clone(), orig: orig.clone(), buf: buf.clone(), scope, mut_fns };
    let mut folded = value.clone();
    let mut calls = 0;
    if !fold_leaf(&mut folded, &shape, &mut calls) || calls == 0 {
        return;
    }
    func.body = folded;
}

/// Fold one tail position. `calls` counts the write-back leaves turned into
/// tail calls; a fold that finds none is dropped by the caller.
fn fold_leaf(e: &mut IrExpr, s: &Shape<'_>, calls: &mut usize) -> bool {
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
            let leaf = std::mem::replace(e, var(s.p, s.buf.clone()));
            *e = IrExpr {
                kind: IrExprKind::Tuple { elements: vec![leaf, var(s.p, s.buf.clone())] },
                ty: s.pair.clone(),
                span,
                def_id: None,
            };
            return true;
        }
    }
    e.ty = s.pair.clone();
    true
}

fn var(id: VarId, ty: Ty) -> IrExpr {
    IrExpr { kind: IrExprKind::Var { id }, ty, span: None, def_id: None }
}

/// A block ending in the call-site rewriter's value write-back for `p` —
/// `…; let (r, b) = f(.., p, ..); p = b; r` with `f` a single-`mut` fn of the
/// same pair type whose `mut` argument IS `p` — pops the two statements and
/// returns the call.
fn take_writeback_call(stmts: &mut Vec<IrStmt>, tail: &IrExpr, s: &Shape<'_>) -> Option<IrExpr> {
    let n = stmts.len();
    if n < 2 {
        return None;
    }
    let IrExprKind::Var { id: res_tail } = tail.kind else { return None };
    let IrStmtKind::Assign { var: assigned, value: assigned_from } = &stmts[n - 1].kind else { return None };
    let IrExprKind::Var { id: from } = assigned_from.kind else { return None };
    let IrStmtKind::BindDestructure { pattern: IrPattern::Tuple { elements }, value: call } = &stmts[n - 2].kind else {
        return None;
    };
    let [IrPattern::Bind { var: res, .. }, IrPattern::Bind { var: buf, .. }] = elements.as_slice() else { return None };
    if *assigned != s.p || from != *buf || res_tail != *res || call.ty != s.pair {
        return None;
    }
    let IrExprKind::Call { target, args, .. } = &call.kind else { return None };
    let name = call_spelling(target)?;
    let (idx, buf_ty, was_unit, orig, extra) =
        s.mut_fns.get(&name).or_else(|| s.mut_fns.get(&scope_key(s.scope, &name)))?;
    let forwards_p = matches!(args.get(*idx).map(|a| &a.kind), Some(IrExprKind::Var { id }) if *id == s.p);
    if *was_unit || !extra.is_empty() || *buf_ty != s.buf || *orig != s.orig || !forwards_p {
        return None;
    }
    stmts.pop();
    let Some(IrStmt { kind: IrStmtKind::BindDestructure { value, .. }, .. }) = stmts.pop() else {
        unreachable!("checked the destructure");
    };
    Some(value)
}
