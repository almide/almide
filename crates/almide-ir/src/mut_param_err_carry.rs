//! The err arm of a CAN-ERR effect fn's C-132 move-mode carrier (#2917,
//! #1871 ruling (B): write visible).
//!
//! Native passes a `mut` param as `&mut`, so a write the callee makes BEFORE
//! it errs stays visible to the caller. The move-mode convention returns the
//! buffer by value instead, so the err arm has to carry it too. A can-err
//! effect fn `f(mut b: Buf, ..) -> T` (the rewrite already returns `(T, Buf)`)
//! is declared
//!
//!   f(b, ..) -> Result[(T, Buf), (String, Buf)]
//!
//! (was-Unit: `Result[Buf, (String, Buf)]`; several `mut` params: every
//! buffer in both tuples). Every raising exit pairs the buffer it holds at
//! that point with the error:
//!
//!   err(m) tail / raise leaf   → err((m, b))!
//!   x!  (x : Result[A, String]) → match x { ok(v) => v, err(e) => err((e, b))! }
//!   guard c else err(m)         → guard c else err((m, b))
//!   guard c else v              → guard c else ok(<v paired>)
//!
//! and the tail `(ret, b)` becomes `ok((ret, b))`.
//!
//! A call site writes back on BOTH arms, then yields the Result the source
//! declared:
//!
//!   { let (__mp_r, b') = match f(p, ..) {
//!       ok((r, b)) => (ok(r), b),
//!       err((e, b)) => (err(e), b),
//!     };
//!     p = b';
//!     __mp_r }                                       : Result[T, String]
//!
//! A `!` site writes back, then propagates. Where the place is one of the
//! calling fn's own carried buffers, the err arm writes back and re-raises —
//! a raising exit of the caller, so it pairs the caller's buffer AFTER the
//! write-back and a partial write travels up any number of `!` levels:
//!
//!   match f(p, ..) { ok((r, b)) => { p = b; r }, err((e, b)) => { p = b; err(e)! } }
//!
//! Any other place dies with the propagation, so its err arm only re-raises:
//!
//!   { let (r, b) = match f(p, ..) { ok(x) => x, err((e, _)) => err(e)! }; p = b; r }
//!
//! A fn whose raises cannot all be paired keeps the #1576 form (ok arm
//! only); its unpropagated call sites stay walled. That is an Option-typed
//! `!` or a typed error in the body, or a guard exiting with a Result value
//! that is not an `err(..)` constructor.

use std::collections::HashSet;

use almide_base::intern::sym;
use almide_lang::types::Ty;

use crate::visit::IrVisitor;
use crate::visit_mut::{walk_expr_mut, walk_stmt_mut, IrMutVisitor};
use crate::*;

/// Is `t` an err type a raise can carry as the `String` first half of the
/// err tuple? (`Unknown` is the checker's placeholder on an `ok(..)`-only
/// Result; the channel is `String`.)
fn string_channel(t: &Ty) -> bool {
    matches!(t, Ty::String | Ty::Unknown)
}

/// Can every raising exit of `body` be paired with the buffer?
fn pairable(body: &IrExpr) -> bool {
    struct Scan(bool);
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            if !self.0 || matches!(e.kind, IrExprKind::Lambda { .. }) {
                return;
            }
            if let IrExprKind::Unwrap { expr } | IrExprKind::Try { expr } = &e.kind {
                let raises_err = matches!(through_empty_blocks(expr).kind, IrExprKind::ResultErr { .. });
                if !raises_err && !expr.ty.result_err_ty().is_some_and(|t| string_channel(&t)) {
                    self.0 = false;
                    return;
                }
            }
            crate::visit::walk_expr(self, e);
        }
        fn visit_stmt(&mut self, s: &IrStmt) {
            if let IrStmtKind::Guard { else_, .. } = &s.kind {
                // A raising exit is an `err(..)` constructor, bare or under
                // `!`; a value exit is neither a Result nor a propagation.
                let exit = through_empty_blocks(else_);
                let ok = match &exit.kind {
                    IrExprKind::ResultErr { .. } => true,
                    IrExprKind::Unwrap { expr } | IrExprKind::Try { expr } => {
                        matches!(through_empty_blocks(expr).kind, IrExprKind::ResultErr { .. })
                    }
                    _ => exit.ty.result_ok_ty().is_none(),
                };
                if !ok {
                    self.0 = false;
                    return;
                }
            }
            crate::visit::walk_stmt(self, s);
        }
    }
    let mut s = Scan(true);
    s.visit_expr(body);
    s.0
}

fn through_empty_blocks(mut e: &IrExpr) -> &IrExpr {
    while let IrExprKind::Block { stmts, expr: Some(inner) } = &e.kind
        && stmts.is_empty()
    {
        e = inner;
    }
    e
}

fn through_empty_blocks_mut(e: &mut IrExpr) -> &mut IrExpr {
    if matches!(&e.kind, IrExprKind::Block { stmts, expr: Some(_) } if stmts.is_empty()) {
        let IrExprKind::Block { expr: Some(inner), .. } = &mut e.kind else { unreachable!() };
        return through_empty_blocks_mut(inner);
    }
    e
}

/// Every `MutFns` key of a can-err effect move-mode fn whose raises can all
/// be paired: effect, not never-err, [`pairable`].
pub(crate) fn carry_keys(
    program: &IrProgram,
    is_mut_key: impl Fn(&str) -> bool,
    never_err: &HashSet<String>,
) -> HashSet<String> {
    let mut keys = HashSet::new();
    let mut admit = |module: Option<&str>, f: &IrFunction| {
        let scoped = crate::mut_param::scope_key(module.unwrap_or(""), f.name.as_str());
        if !f.is_effect || !is_mut_key(&scoped) || never_err.contains(&scoped) || !pairable(&f.body) {
            return;
        }
        let spellings = crate::mut_param_unpropagated::fn_spellings(module, f.name.as_str());
        keys.extend(spellings.into_iter().filter(|k| is_mut_key(k)));
    };
    for f in &program.functions {
        admit(None, f);
    }
    for m in &program.modules {
        for f in &m.functions {
            admit(Some(m.name.as_str()), f);
        }
    }
    keys
}

fn var(id: VarId, ty: Ty) -> IrExpr {
    IrExpr { kind: IrExprKind::Var { id }, ty, span: None, def_id: None }
}

fn node(kind: IrExprKind, ty: Ty) -> IrExpr {
    IrExpr { kind, ty, span: None, def_id: None }
}

fn placeholder() -> IrExpr {
    node(IrExprKind::Unit, Ty::Unit)
}

/// The err payload a raise carries: `(String, Buf1, …, BufN)`.
pub(crate) fn err_payload_ty(buf_tys: &[Ty]) -> Ty {
    Ty::Tuple(std::iter::once(Ty::String).chain(buf_tys.iter().cloned()).collect())
}

/// A site block's binders: the buffers the write-backs read, and the
/// callee's value `__mp_res` when it returns one (the block's tail reads it).
fn site_binders(block: &IrExpr) -> Option<(Vec<(VarId, Ty)>, Option<(VarId, Ty)>)> {
    let IrExprKind::Block { stmts, expr: Some(tail) } = &block.kind else { return None };
    match &stmts.first()?.kind {
        IrStmtKind::Bind { var, ty, .. } => Some((vec![(*var, ty.clone())], None)),
        IrStmtKind::BindDestructure { pattern: IrPattern::Tuple { elements }, .. } => {
            let mut binds = Vec::new();
            for el in elements {
                let IrPattern::Bind { var, ty } = el else { return None };
                binds.push((*var, ty.clone()));
            }
            match &tail.kind {
                IrExprKind::Var { id } if binds.first().is_some_and(|(v, _)| v == id) => {
                    let r = binds.remove(0);
                    Some((binds, Some(r)))
                }
                IrExprKind::Unit => Some((binds, None)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// An unpropagated site (bound as a Result, `??`, `match`, `let _ =`): turn
/// the rewriter's block `{ let <pat> = call; <write-backs>; <tail> }` into
///
///   { let (__mp_r, b'..) = match call {
///       ok((r, b..)) => (ok(r), b..),
///       err((e, b..)) => (err(e), b..),
///     };
///     <write-backs of b'..>;
///     __mp_r }                                  : Result[<tail>, String]
///
/// The write-backs stay straight-line after the match (an Assign inside a
/// value-position arm is not a shape the lowering releases correctly).
pub(crate) fn to_result_block(expr: &mut IrExpr, vt: &mut VarTable) {
    let Some((buf_vars, res)) = site_binders(expr) else { return };
    let IrExprKind::Block { stmts, expr: Some(tail) } = &mut expr.kind else { return };
    let span = expr.span;
    let t_ty = tail.ty.clone();
    let res_ty = Ty::result(t_ty, Ty::String);
    let buf_tys: Vec<Ty> = buf_vars.iter().map(|(_, t)| t.clone()).collect();
    let err_ty = err_payload_ty(&buf_tys);
    let pair_ty = Ty::Tuple(std::iter::once(res_ty.clone()).chain(buf_tys.iter().cloned()).collect());

    let first = stmts.remove(0);
    let (mut call, ok_ty) = match first.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. } => {
            let ok_ty = value.ty.clone();
            (value, ok_ty)
        }
        _ => unreachable!("site_binders matched a bind"),
    };
    call.ty = Ty::result(ok_ty, err_ty);

    // ok((r, b..)) => (ok(r), b..)
    let o_res = res.as_ref().map(|(_, t)| (vt.alloc(sym("__mp_ores"), t.clone(), Mutability::Let, None), t.clone()));
    let o_bufs: Vec<(VarId, Ty)> =
        buf_tys.iter().map(|t| (vt.alloc(sym("__mp_obuf"), t.clone(), Mutability::Let, None), t.clone())).collect();
    let mut ok_binds: Vec<IrPattern> = Vec::new();
    if let Some((v, t)) = &o_res {
        ok_binds.push(IrPattern::Bind { var: *v, ty: t.clone() });
    }
    ok_binds.extend(o_bufs.iter().map(|(v, t)| IrPattern::Bind { var: *v, ty: t.clone() }));
    let ok_inner = if ok_binds.len() == 1 { ok_binds.remove(0) } else { IrPattern::Tuple { elements: ok_binds } };
    let ok_val = match &o_res {
        Some((v, t)) => var(*v, t.clone()),
        None => placeholder(),
    };
    let ok_body = node(
        IrExprKind::Tuple {
            elements: std::iter::once(node(IrExprKind::ResultOk { expr: Box::new(ok_val) }, res_ty.clone()))
                .chain(o_bufs.iter().map(|(v, t)| var(*v, t.clone())))
                .collect(),
        },
        pair_ty.clone(),
    );

    // err((e, b..)) => (err(e), b..)
    let e = vt.alloc(sym("__mp_err"), Ty::String, Mutability::Let, None);
    let e_bufs: Vec<(VarId, Ty)> =
        buf_tys.iter().map(|t| (vt.alloc(sym("__mp_ebuf"), t.clone(), Mutability::Let, None), t.clone())).collect();
    let err_inner = IrPattern::Tuple {
        elements: std::iter::once(IrPattern::Bind { var: e, ty: Ty::String })
            .chain(e_bufs.iter().map(|(v, t)| IrPattern::Bind { var: *v, ty: t.clone() }))
            .collect(),
    };
    let err_body = node(
        IrExprKind::Tuple {
            elements: std::iter::once(node(IrExprKind::ResultErr { expr: Box::new(var(e, Ty::String)) }, res_ty.clone()))
                .chain(e_bufs.iter().map(|(v, t)| var(*v, t.clone())))
                .collect(),
        },
        pair_ty.clone(),
    );

    let matched = IrExpr {
        kind: IrExprKind::Match {
            subject: Box::new(call),
            arms: vec![
                IrMatchArm { pattern: IrPattern::Ok { inner: Box::new(ok_inner) }, guard: None, body: ok_body },
                IrMatchArm { pattern: IrPattern::Err { inner: Box::new(err_inner) }, guard: None, body: err_body },
            ],
        },
        ty: pair_ty,
        span,
        def_id: None,
    };
    let r = vt.alloc(sym("__mp_r"), res_ty.clone(), Mutability::Let, None);
    let outer = IrPattern::Tuple {
        elements: std::iter::once(IrPattern::Bind { var: r, ty: res_ty.clone() })
            .chain(buf_vars.iter().map(|(v, t)| IrPattern::Bind { var: *v, ty: t.clone() }))
            .collect(),
    };
    stmts.insert(0, IrStmt { kind: IrStmtKind::BindDestructure { pattern: outer, value: matched }, span });
    **tail = var(r, res_ty.clone());
    expr.ty = res_ty;
}


/// Callee side: give every fn in `carry` the err-carrying signature.
pub(crate) fn carry_bodies(program: &mut IrProgram, mut_fns: &crate::mut_param::MutFns, carry: &HashSet<String>) {
    if carry.is_empty() {
        return;
    }
    let vt = &mut program.var_table;
    for f in program.functions.iter_mut() {
        carry_fn(f, "", mut_fns, carry, vt);
    }
    for m in program.modules.iter_mut() {
        let scope = m.name.to_string();
        let mvt = &mut m.var_table;
        for f in m.functions.iter_mut() {
            carry_fn(f, &scope, mut_fns, carry, mvt);
        }
    }
}

fn carry_fn(
    func: &mut IrFunction,
    scope: &str,
    mut_fns: &crate::mut_param::MutFns,
    carry: &HashSet<String>,
    vt: &mut VarTable,
) {
    let key = crate::mut_param::scope_key(scope, func.name.as_str());
    if !carry.contains(&key) {
        return;
    }
    let Some((idx, mut_ty, _, _, extra)) = mut_fns.get(&key).cloned() else { return };
    let Some(p) = func.params.get(idx) else { return };
    let mut bufs = vec![(p.var, mut_ty)];
    for (i, t) in &extra {
        let Some(q) = func.params.get(*i) else { return };
        bufs.push((q.var, t.clone()));
    }
    let buf_tys: Vec<Ty> = bufs.iter().map(|(_, t)| t.clone()).collect();
    let err_ty = err_payload_ty(&buf_tys);
    let ret_ty = func.ret_ty.clone();
    let carrier = Ty::result(ret_ty.clone(), err_ty.clone());
    let IrExprKind::Block { stmts, expr: Some(tail) } = &mut func.body.kind else { return };
    // The original body is the first statement (`let __mp_ret = <body>` or
    // `<body>;`). An effect fn declared `-> T` may still yield a whole
    // Result, or a Result in some tails (`if c then err(..) else ok(n)`, a
    // `none => err(..)` arm): strip every tail to the raw payload so its
    // `err(..)` tails are raise leaves.
    match stmts.first_mut().map(|s| &mut s.kind) {
        Some(IrStmtKind::Bind { value, ty, .. }) if ty.result_ok_ty().is_none() => {
            let ty = ty.clone();
            crate::mut_param::strip_ok_layer(value, &ty);
        }
        Some(IrStmtKind::Expr { expr }) => {
            crate::mut_param::strip_ok_layer(expr, &Ty::Unit);
        }
        _ => {}
    }
    let old_tail = std::mem::replace(tail.as_mut(), placeholder());
    **tail = node(IrExprKind::ResultOk { expr: Box::new(old_tail) }, carrier.clone());
    func.body.ty = carrier.clone();
    func.ret_ty = carrier.clone();
    let mut pairer = Pairer { bufs: &bufs, err_ty, carrier, vt };
    pairer.visit_expr_mut(&mut func.body);
}

struct Pairer<'a> {
    bufs: &'a [(VarId, Ty)],
    err_ty: Ty,
    /// The fn's return type, `Result[R, (String, Buf..)]` — a guard's exit.
    carrier: Ty,
    vt: &'a mut VarTable,
}

impl Pairer<'_> {
    /// `(m, b1, …, bN)` — the buffers as they are at this point.
    fn paired(&self, m: IrExpr) -> IrExpr {
        let elements = std::iter::once(m).chain(self.bufs.iter().map(|(v, t)| var(*v, t.clone()))).collect();
        node(IrExprKind::Tuple { elements }, self.err_ty.clone())
    }

    /// `err((m, b..))!` typed `ty`.
    fn raise(&self, m: IrExpr, ty: &Ty) -> IrExpr {
        let err = node(IrExprKind::ResultErr { expr: Box::new(self.paired(m)) }, Ty::result(ty.clone(), self.err_ty.clone()));
        node(IrExprKind::Unwrap { expr: Box::new(err) }, ty.clone())
    }
}

impl IrMutVisitor for Pairer<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        if matches!(e.kind, IrExprKind::Lambda { .. }) {
            return;
        }
        walk_expr_mut(self, e);
        let span = e.span;
        match &mut e.kind {
            IrExprKind::Unwrap { expr: inner } | IrExprKind::Try { expr: inner } => {
                let target = through_empty_blocks_mut(inner);
                if let IrExprKind::ResultErr { expr: m } = &mut target.kind {
                    // `err(m)!`: pair the payload in place.
                    let m = std::mem::replace(m.as_mut(), placeholder());
                    let IrExprKind::ResultErr { expr: slot } = &mut target.kind else { unreachable!() };
                    **slot = self.paired(m);
                    target.ty = Ty::result(e.ty.clone(), self.err_ty.clone());
                    inner.ty = target_ty_of(inner);
                    return;
                }
                if inner.ty.is_option() {
                    return;
                }
                // `x!` → match x { ok(v) => v, err(e) => err((e, b..))! }
                let ty = e.ty.clone();
                let mut x = std::mem::replace(inner.as_mut(), placeholder());
                // A rotated move-mode call (`let b = f(r)!`, a never-err
                // callee) is typed with its raw payload; its value is still
                // the lifted `Result[_, String]` carrier.
                if x.ty.result_ok_ty().is_none() {
                    x.ty = Ty::result(x.ty.clone(), Ty::String);
                }
                let ok_ty = x.ty.result_ok_ty().filter(|t| !matches!(t, Ty::Unknown)).unwrap_or_else(|| ty.clone());
                let v = self.vt.alloc(sym("__mp_v"), ok_ty.clone(), Mutability::Let, None);
                let er = self.vt.alloc(sym("__mp_e"), Ty::String, Mutability::Let, None);
                let raise = self.raise(var(er, Ty::String), &ty);
                *e = IrExpr {
                    kind: IrExprKind::Match {
                        subject: Box::new(x),
                        arms: vec![
                            IrMatchArm {
                                pattern: IrPattern::Ok { inner: Box::new(IrPattern::Bind { var: v, ty: ok_ty.clone() }) },
                                guard: None,
                                body: var(v, ok_ty),
                            },
                            IrMatchArm {
                                pattern: IrPattern::Err { inner: Box::new(IrPattern::Bind { var: er, ty: Ty::String }) },
                                guard: None,
                                body: raise,
                            },
                        ],
                    },
                    ty,
                    span,
                    def_id: None,
                };
            }
            // A raise leaf: an `err(m)` typed with the raw payload (a stripped tail).
            IrExprKind::ResultErr { expr: m } if e.ty.result_ok_ty().is_none() => {
                let m = std::mem::replace(m.as_mut(), placeholder());
                let ty = e.ty.clone();
                *e = self.raise(m, &ty);
                e.span = span;
            }
            _ => {}
        }
    }

    fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) {
        walk_stmt_mut(self, stmt);
        let IrStmtKind::Guard { else_, .. } = &mut stmt.kind else { return };
        let exit = through_empty_blocks_mut(else_);
        let paired_payload = match &mut exit.kind {
            // Already paired by the walk: `err((m, b..))!`.
            IrExprKind::Unwrap { expr } | IrExprKind::Try { expr } => match &mut through_empty_blocks_mut(expr).kind {
                IrExprKind::ResultErr { expr: payload } => Some(std::mem::replace(payload.as_mut(), placeholder())),
                _ => None,
            },
            IrExprKind::ResultErr { expr: m } => {
                let m = std::mem::replace(m.as_mut(), placeholder());
                Some(self.paired(m))
            }
            _ => None,
        };
        let span = else_.span;
        let new_exit = match paired_payload {
            Some(p) => node(IrExprKind::ResultErr { expr: Box::new(p) }, self.carrier.clone()),
            // A value exit (already paired with the buffers by the #2907
            // guard pairing): the fn's ok.
            None => {
                let v = std::mem::replace(else_, placeholder());
                node(IrExprKind::ResultOk { expr: Box::new(v) }, self.carrier.clone())
            }
        };
        *else_ = IrExpr { span, ..new_exit };
    }
}

/// The type an empty-block chain carries after its innermost node was retyped.
fn target_ty_of(e: &IrExpr) -> Ty {
    through_empty_blocks(e).ty.clone()
}

/// The buffers `func` carries on its own err arm, when it is a carry fn.
pub(crate) fn own_buffers(
    func: &IrFunction,
    scope: &str,
    mut_fns: &crate::mut_param::MutFns,
    carry: &HashSet<String>,
) -> HashSet<VarId> {
    let key = crate::mut_param::scope_key(scope, func.name.as_str());
    let Some((idx, _, _, _, extra)) = carry.contains(&key).then(|| mut_fns.get(&key)).flatten() else {
        return HashSet::new();
    };
    std::iter::once(*idx).chain(extra.iter().map(|(i, _)| *i)).filter_map(|i| func.params.get(i).map(|p| p.var)).collect()
}

/// The key a site block is recorded under: the first var its first statement
/// binds, while that statement still binds the bare call.
pub(crate) fn site_key(block: &IrExpr) -> Option<VarId> {
    let IrExprKind::Block { stmts, expr: Some(_) } = &block.kind else { return None };
    match &stmts.first()?.kind {
        IrStmtKind::Bind { var, value, .. } if matches!(value.kind, IrExprKind::Call { .. }) => Some(*var),
        IrStmtKind::BindDestructure { pattern: IrPattern::Tuple { elements }, value }
            if matches!(value.kind, IrExprKind::Call { .. }) =>
        {
            match elements.first() {
                Some(IrPattern::Bind { var, .. }) => Some(*var),
                _ => None,
            }
        }
        _ => None,
    }
}

/// A `!` over a recorded site block. When the block writes back into one of
/// the walked fn's OWN carried buffers, the write-back is observable after
/// the err (the fn's raise carries that buffer), so both arms write back and
/// the err arm then re-raises ([`write_back_armwise`]). Any other place dies
/// with the propagation, so the err arm only re-raises the message:
///
///   { let <pat> = match f(p, ..) { ok(x) => x, err((e, _)) => err(e)! }; p = b; <tail> }
///
/// Returns true when `expr` was such a site.
pub(crate) fn propagate_site(
    expr: &mut IrExpr,
    pending: &mut HashSet<VarId>,
    vt: &mut VarTable,
    own_bufs: &HashSet<VarId>,
) -> bool {
    let (IrExprKind::Unwrap { expr: inner } | IrExprKind::Try { expr: inner }) = &mut expr.kind else {
        return false;
    };
    let Some(key) = site_key(inner).filter(|k| pending.contains(k)) else { return false };
    pending.remove(&key);
    let ty = expr.ty.clone();
    let span = expr.span;
    let mut block = std::mem::replace(inner.as_mut(), placeholder());
    let IrExprKind::Block { stmts, .. } = &block.kind else { unreachable!("site_key matched a block") };
    let writes_own = stmts[1..].iter().any(|s| match &s.kind {
        IrStmtKind::Assign { var, .. } => own_bufs.contains(var),
        IrStmtKind::FieldAssign { target, .. } => own_bufs.contains(target),
        _ => false,
    });
    if writes_own {
        write_back_armwise(&mut block, vt, &ty, true);
    } else {
        reraise_only(&mut block, vt);
    }
    block.ty = ty;
    block.span = span;
    *expr = block;
    true
}

/// The light `!` form of [`propagate_site`]: the bound call becomes
/// `match call { ok(x) => x, err((e, _..)) => err(e)! }`.
fn reraise_only(block: &mut IrExpr, vt: &mut VarTable) {
    let Some((buf_vars, _)) = site_binders(block) else { return };
    let bufs: Vec<Ty> = buf_vars.into_iter().map(|(_, t)| t).collect();
    let IrExprKind::Block { stmts, .. } = &mut block.kind else { return };
    let value = match &mut stmts[0].kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. } => value,
        _ => return,
    };
    let span = value.span;
    let mut call = std::mem::replace(value, placeholder());
    let ok_ty = call.ty.clone();
    call.ty = Ty::result(ok_ty.clone(), err_payload_ty(&bufs));
    let x = vt.alloc(sym("__mp_ok"), ok_ty.clone(), Mutability::Let, None);
    let e = vt.alloc(sym("__mp_err"), Ty::String, Mutability::Let, None);
    let raise = node(
        IrExprKind::Unwrap {
            expr: Box::new(node(
                IrExprKind::ResultErr { expr: Box::new(var(e, Ty::String)) },
                Ty::result(ok_ty.clone(), Ty::String),
            )),
        },
        ok_ty.clone(),
    );
    let err_inner = IrPattern::Tuple {
        elements: std::iter::once(IrPattern::Bind { var: e, ty: Ty::String })
            .chain(bufs.iter().map(|_| IrPattern::Wildcard))
            .collect(),
    };
    *value = IrExpr {
        kind: IrExprKind::Match {
            subject: Box::new(call),
            arms: vec![
                IrMatchArm {
                    pattern: IrPattern::Ok { inner: Box::new(IrPattern::Bind { var: x, ty: ok_ty.clone() }) },
                    guard: None,
                    body: var(x, ok_ty.clone()),
                },
                IrMatchArm { pattern: IrPattern::Err { inner: Box::new(err_inner) }, guard: None, body: raise },
            ],
        },
        ty: ok_ty,
        span,
        def_id: None,
    };
}

/// Convert every recorded site block no `!` reached to the Result-valued form.
pub(crate) fn settle_sites<'b>(
    bodies: impl Iterator<Item = &'b mut IrExpr>,
    pending: &HashSet<VarId>,
    vt: &mut VarTable,
) {
    if pending.is_empty() {
        return;
    }
    struct Settle<'p, 'v> {
        pending: &'p HashSet<VarId>,
        vt: &'v mut VarTable,
    }
    impl IrMutVisitor for Settle<'_, '_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            if site_key(e).is_some_and(|k| self.pending.contains(&k)) {
                to_result_block(e, self.vt);
            }
        }
    }
    let mut s = Settle { pending, vt };
    for b in bodies {
        s.visit_expr_mut(b);
    }
}

/// A site block, arm-wise: both arms write back, then the ok arm yields the
/// block's value and the err arm the error — re-raised when `propagate`
/// (`err(e)!`, typed `ty`), else as the value `err(e)` of the Result `ty`:
///
///   match f(p, ..) { ok((r, b)) => { p = b; r }, err((e, b)) => { p = b; err(e)! } }
///
/// A re-raise inside a carry fn is paired afterwards with that fn's buffers,
/// read after the write-back.
fn write_back_armwise(block: &mut IrExpr, vt: &mut VarTable, ty: &Ty, propagate: bool) {
    let Some((buf_vars, res)) = site_binders(block) else { return };
    let IrExprKind::Block { stmts, expr: Some(tail) } = std::mem::replace(&mut block.kind, IrExprKind::Unit) else {
        unreachable!()
    };
    let mut stmts = stmts;
    let first = stmts.remove(0);
    let wbs = stmts;
    let (mut call, ok_ty) = match first.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. } => {
            let t = value.ty.clone();
            (value, t)
        }
        _ => unreachable!(),
    };
    let buf_tys: Vec<Ty> = buf_vars.iter().map(|(_, t)| t.clone()).collect();
    call.ty = Ty::result(ok_ty, err_payload_ty(&buf_tys));
    // The ok arm keeps the original binders (the write-backs read them).
    let mut ok_binds: Vec<IrPattern> = Vec::new();
    if let Some((v, t)) = &res {
        ok_binds.push(IrPattern::Bind { var: *v, ty: t.clone() });
    }
    ok_binds.extend(buf_vars.iter().map(|(v, t)| IrPattern::Bind { var: *v, ty: t.clone() }));
    let ok_inner = if ok_binds.len() == 1 { ok_binds.remove(0) } else { IrPattern::Tuple { elements: ok_binds } };
    let ok_tail = if propagate { *tail } else { node(IrExprKind::ResultOk { expr: tail }, ty.clone()) };
    let ok_body =
        IrExpr { kind: IrExprKind::Block { stmts: wbs.clone(), expr: Some(Box::new(ok_tail)) }, ty: ty.clone(), span: None, def_id: None };
    // The err arm binds fresh buffers and writes them back the same way.
    let e = vt.alloc(sym("__mp_err"), Ty::String, Mutability::Let, None);
    let fresh: Vec<(VarId, VarId, Ty)> = buf_vars
        .iter()
        .map(|(v, t)| (*v, vt.alloc(sym("__mp_ebuf"), t.clone(), Mutability::Let, None), t.clone()))
        .collect();
    let renamed = |x: &IrExpr| -> IrExpr {
        match &x.kind {
            IrExprKind::Var { id } => fresh.iter().find(|(o, _, _)| o == id).map_or_else(|| x.clone(), |(_, n, t)| var(*n, t.clone())),
            _ => x.clone(),
        }
    };
    let err_wbs: Vec<IrStmt> = wbs
        .iter()
        .map(|s| IrStmt {
            kind: match &s.kind {
                IrStmtKind::Assign { var: v, value } => IrStmtKind::Assign { var: *v, value: renamed(value) },
                IrStmtKind::FieldAssign { target, field, value } => {
                    IrStmtKind::FieldAssign { target: *target, field: *field, value: renamed(value) }
                }
                other => other.clone(),
            },
            span: s.span,
        })
        .collect();
    let err_inner = IrPattern::Tuple {
        elements: std::iter::once(IrPattern::Bind { var: e, ty: Ty::String })
            .chain(fresh.iter().map(|(_, n, t)| IrPattern::Bind { var: *n, ty: t.clone() }))
            .collect(),
    };
    let raise = if propagate {
        node(
            IrExprKind::Unwrap {
                expr: Box::new(node(
                    IrExprKind::ResultErr { expr: Box::new(var(e, Ty::String)) },
                    Ty::result(ty.clone(), Ty::String),
                )),
            },
            ty.clone(),
        )
    } else {
        node(IrExprKind::ResultErr { expr: Box::new(var(e, Ty::String)) }, ty.clone())
    };
    let err_body = IrExpr { kind: IrExprKind::Block { stmts: err_wbs, expr: Some(Box::new(raise)) }, ty: ty.clone(), span: None, def_id: None };
    block.kind = IrExprKind::Match {
        subject: Box::new(call),
        arms: vec![
            IrMatchArm { pattern: IrPattern::Ok { inner: Box::new(ok_inner) }, guard: None, body: ok_body },
            IrMatchArm { pattern: IrPattern::Err { inner: Box::new(err_inner) }, guard: None, body: err_body },
        ],
    };
}
