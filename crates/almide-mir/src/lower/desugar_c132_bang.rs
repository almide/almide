// The `!` call sites of a CAN-ERR `mut`-param callee (C-132, #2917), respelled into the
// shapes the lowering executes. `include!`d from mod_c_tail.rs; run per statement list by
// `normalize_stmt_lists`.
//
// The err-carry rewrite (almide_ir::mut_param_err_carry) gives such a callee the carrier
// `Result[(T, Buf..), (String, Buf..)]` (was-Unit: `Result[Buf.., (String, Buf..)]`) and
// emits three call-site forms. Its non-`!` form, the WRITE-VISIBLE one,
//
//   let (r, b') = match f(p) { ok((t, b)) => (ok(t), b), err((m, b)) => (err(m), b) }
//
// lowers, and so does a let-bound `!` over the Result it yields. Its two `!` forms put a
// raising arm (`err(m)!`) in a match over the effect call itself, which walls. Both are
// rewritten here into the write-visible match followed by the `!`, with the write-backs
// exactly where the original runs them:
//
//   non-carried place:  let P = match f(p) { ok(x) => x, err((m, _)) => err(m)! }
//                   ≡   let (r, b') = <write-visible>; let t = r!; let P = (t, b')
//     (the `!` raises BEFORE `P` binds, so on err nothing is written back, as before);
//
//   carried place:      match f(p) { ok((t, b)) => { p = b; t }, err((m, b)) => { p = b; err((m, p))! } }
//                   ≡   let (r, b') = <write-visible>; p = b';
//                       let q = match r { ok(v) => ok(v), err(m') => err((m', p)) }; q!
//     (both arms wrote back before the raise, so the write-back moves before it).
//
// Anything else is left alone.

/// Rewrite the C-132 `!` call sites in one statement list.
fn rewrite_c132_bang_sites(stmts: &mut Vec<almide_ir::IrStmt>, vt: &mut almide_ir::VarTable) {
    let mut i = 0;
    while i < stmts.len() {
        let replacement = c132_noncarried_site(&stmts[i], vt)
            .or_else(|| c132_carried_site(&stmts[i], vt))
            .or_else(|| c132_paired_assign_site(&stmts[i], vt));
        match replacement {
            Some(new) => {
                let n = new.len();
                stmts.splice(i..=i, new);
                i += n;
            }
            None => i += 1,
        }
    }
}

fn c132_var(id: almide_ir::VarId, ty: &Ty) -> IrExpr {
    IrExpr { kind: IrExprKind::Var { id }, ty: ty.clone(), span: None, def_id: None }
}

fn c132_bind(var: almide_ir::VarId, ty: &Ty) -> almide_ir::IrPattern {
    almide_ir::IrPattern::Bind { var, ty: ty.clone() }
}

fn c132_tuple(elements: Vec<IrExpr>) -> IrExpr {
    let ty = Ty::Tuple(elements.iter().map(|e| e.ty.clone()).collect());
    IrExpr { kind: IrExprKind::Tuple { elements }, ty, span: None, def_id: None }
}

fn c132_result_ty(ok: &Ty, err: &Ty) -> Ty {
    Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, vec![ok.clone(), err.clone()])
}

/// The carrier's err half `(String, Buf..)`: the buffer types, when `ty` is
/// `Result[_, (String, Buf..)]`.
fn c132_err_buffers(ty: &Ty) -> Option<(Ty, Vec<Ty>)> {
    use almide_lang::types::constructor::TypeConstructorId;
    let Ty::Applied(TypeConstructorId::Result, a) = ty else { return None };
    let [ok, Ty::Tuple(err)] = a.as_slice() else { return None };
    let (first, bufs) = err.split_first()?;
    (matches!(first, Ty::String) && !bufs.is_empty()).then(|| (ok.clone(), bufs.to_vec()))
}

/// The value tail of an arm: `err(e)!` → `Some(e)`.
fn c132_raised(e: &IrExpr) -> Option<&IrExpr> {
    let IrExprKind::Unwrap { expr } = &e.kind else { return None };
    let IrExprKind::ResultErr { expr: m } = &expr.kind else { return None };
    Some(m)
}

/// The write-visible match over `subject`: each arm yields `(ok(t) | err(m), buffers..)`.
/// `ok_arm` is the Ok arm's (pattern, Ok value, buffer reads); the Err arm binds fresh
/// `(m, b..)` itself. Returns the destructuring `let (r, b'..) = <match>` and `(r, b'..)`.
fn c132_write_visible(
    subject: &IrExpr,
    ok_arm: (almide_ir::IrPattern, IrExpr, Vec<IrExpr>),
    t_ty: &Ty,
    buf_tys: &[Ty],
    span: Option<almide_base::Span>,
    vt: &mut almide_ir::VarTable,
) -> (almide_ir::IrStmt, almide_ir::VarId, Vec<almide_ir::VarId>) {
    use almide_ir::{IrMatchArm, IrPattern, IrStmt, IrStmtKind, Mutability};
    let fresh = |vt: &mut almide_ir::VarTable, name: &str, ty: &Ty| {
        vt.alloc(almide_base::intern::sym(name), ty.clone(), Mutability::Let, None)
    };
    let res_ty = c132_result_ty(t_ty, &Ty::String);
    let (ok_pat, ok_val, ok_bufs) = ok_arm;
    let mut ok_elems = vec![IrExpr {
        kind: IrExprKind::ResultOk { expr: Box::new(ok_val) },
        ty: res_ty.clone(),
        span: None,
        def_id: None,
    }];
    ok_elems.extend(ok_bufs);
    let m = fresh(vt, "__c132_msg", &Ty::String);
    let err_bufs: Vec<almide_ir::VarId> = buf_tys.iter().map(|t| fresh(vt, "__c132_ebuf", t)).collect();
    let mut err_pats = vec![c132_bind(m, &Ty::String)];
    err_pats.extend(err_bufs.iter().zip(buf_tys).map(|(v, t)| c132_bind(*v, t)));
    let mut err_elems = vec![IrExpr {
        kind: IrExprKind::ResultErr { expr: Box::new(c132_var(m, &Ty::String)) },
        ty: res_ty.clone(),
        span: None,
        def_id: None,
    }];
    err_elems.extend(err_bufs.iter().zip(buf_tys).map(|(v, t)| c132_var(*v, t)));
    let ok_body = c132_tuple(ok_elems);
    let pair_ty = ok_body.ty.clone();
    let arms = vec![
        IrMatchArm { pattern: IrPattern::Ok { inner: Box::new(ok_pat) }, guard: None, body: ok_body },
        IrMatchArm {
            pattern: IrPattern::Err { inner: Box::new(IrPattern::Tuple { elements: err_pats }) },
            guard: None,
            body: c132_tuple(err_elems),
        },
    ];
    let r = fresh(vt, "__c132_res", &res_ty);
    let bufs: Vec<almide_ir::VarId> = buf_tys.iter().map(|t| fresh(vt, "__c132_buf", t)).collect();
    let mut pats = vec![c132_bind(r, &res_ty)];
    pats.extend(bufs.iter().zip(buf_tys).map(|(v, t)| c132_bind(*v, t)));
    let value = IrExpr {
        kind: IrExprKind::Match { subject: Box::new(subject.clone()), arms },
        ty: pair_ty,
        span: span.clone(),
        def_id: None,
    };
    let stmt = IrStmt { kind: IrStmtKind::BindDestructure { pattern: IrPattern::Tuple { elements: pats }, value }, span };
    (stmt, r, bufs)
}

/// The non-carried `!` site's match (`match f(p) { ok(x) => x, err((m, _..)) => err(m)! }`)
/// as `(the bound value, the call)`, or `None` for any other statement.
fn c132_noncarried_match(stmt: &almide_ir::IrStmt) -> Option<(&IrExpr, &IrExpr)> {
    use almide_ir::{IrPattern, IrStmtKind};
    let (IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }) = &stmt.kind else {
        return None;
    };
    let IrExprKind::Match { subject, arms } = &value.kind else { return None };
    let [ok, err] = arms.as_slice() else { return None };
    if !matches!(subject.kind, IrExprKind::Call { .. }) || ok.guard.is_some() || err.guard.is_some() {
        return None;
    }
    let IrPattern::Ok { inner } = &ok.pattern else { return None };
    let IrPattern::Bind { var: x, .. } = inner.as_ref() else { return None };
    let IrPattern::Err { inner } = &err.pattern else { return None };
    let IrPattern::Tuple { elements } = inner.as_ref() else { return None };
    let (IrPattern::Bind { var: m, .. }, rest) = elements.split_first()? else { return None };
    let ok_is_x = matches!(ok.body.kind, IrExprKind::Var { id } if id == *x);
    let raises_m = matches!(c132_raised(&err.body).map(|e| &e.kind), Some(IrExprKind::Var { id }) if id == m);
    (ok_is_x && raises_m && rest.iter().all(|p| matches!(p, IrPattern::Wildcard))).then_some((value, &**subject))
}

/// The carrier Ok payload against its buffers: `(T, Buf..)` → `(T, false)`, a was-Unit
/// `Buf` / `(Buf..)` → `(Unit, true)`.
fn c132_ok_split(ok_ty: &Ty, buf_tys: &[Ty]) -> Option<(Ty, bool)> {
    let n = buf_tys.len();
    match ok_ty {
        Ty::Tuple(e) if e.len() == n + 1 && e[1..] == buf_tys[..] => Some((e[0].clone(), false)),
        Ty::Tuple(e) if n > 1 && e[..] == buf_tys[..] => Some((Ty::Unit, true)),
        t if n == 1 && *t == buf_tys[0] => Some((Ty::Unit, true)),
        _ => None,
    }
}

/// The write-visible Ok arm for the non-carried site: its pattern over the payload, the Ok
/// value (`t`, or `()` for a was-Unit callee) and the buffer reads.
fn c132_noncarried_ok_arm(
    t_ty: &Ty,
    unit: bool,
    buf_tys: &[Ty],
    vt: &mut almide_ir::VarTable,
) -> (almide_ir::IrPattern, IrExpr, Vec<IrExpr>) {
    use almide_ir::{IrPattern, Mutability};
    let fresh = |vt: &mut almide_ir::VarTable, name: &str, ty: &Ty| {
        vt.alloc(almide_base::intern::sym(name), ty.clone(), Mutability::Let, None)
    };
    let ob: Vec<almide_ir::VarId> = buf_tys.iter().map(|t| fresh(vt, "__c132_obuf", t)).collect();
    let reads: Vec<IrExpr> = ob.iter().zip(buf_tys).map(|(v, t)| c132_var(*v, t)).collect();
    let mut pats: Vec<IrPattern> = ob.iter().zip(buf_tys).map(|(v, t)| c132_bind(*v, t)).collect();
    if unit {
        let pat = if pats.len() == 1 { pats.remove(0) } else { IrPattern::Tuple { elements: pats } };
        return (pat, IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None }, reads);
    }
    let ot = fresh(vt, "__c132_ok", t_ty);
    let mut all = vec![c132_bind(ot, t_ty)];
    all.extend(pats);
    (IrPattern::Tuple { elements: all }, c132_var(ot, t_ty), reads)
}

/// `let (x, b..) = …` with plain binders: the `!` binds straight into `x` (its read-shape
/// is then the Ok payload's own) and each buffer binder from its write-visible slot.
fn c132_direct_binders(stmt: &almide_ir::IrStmt, unit: bool, n: usize) -> Option<Vec<almide_ir::IrPattern>> {
    use almide_ir::{IrPattern, IrStmtKind};
    let IrStmtKind::BindDestructure { pattern: IrPattern::Tuple { elements }, .. } = &stmt.kind else {
        return None;
    };
    let plain = elements.iter().all(|p| matches!(p, IrPattern::Bind { .. } | IrPattern::Wildcard));
    (!unit && elements.len() == n + 1 && plain).then(|| elements.clone())
}

/// The non-carried `!` site: `let P = match f(p) { ok(x) => x, err((m, _..)) => err(m)! }`.
fn c132_noncarried_site(stmt: &almide_ir::IrStmt, vt: &mut almide_ir::VarTable) -> Option<Vec<almide_ir::IrStmt>> {
    use almide_ir::{IrPattern, IrStmt, IrStmtKind, Mutability};
    let (value, subject) = c132_noncarried_match(stmt)?;
    let (ok_ty, buf_tys) = c132_err_buffers(&subject.ty)?;
    let (t_ty, unit) = c132_ok_split(&ok_ty, &buf_tys)?;
    let ok_arm = c132_noncarried_ok_arm(&t_ty, unit, &buf_tys, vt);
    let span = stmt.span.clone();
    let (wv, r, bufs) = c132_write_visible(subject, ok_arm, &t_ty, &buf_tys, span.clone(), vt);
    let res_ty = c132_result_ty(&t_ty, &Ty::String);
    let direct = c132_direct_binders(stmt, unit, buf_tys.len());
    let t = match direct.as_ref().map(|e| &e[0]) {
        Some(IrPattern::Bind { var, .. }) => *var,
        _ => vt.alloc(almide_base::intern::sym("__c132_val"), t_ty.clone(), Mutability::Let, None),
    };
    let unwrap = IrExpr {
        kind: IrExprKind::Unwrap { expr: Box::new(c132_var(r, &res_ty)) },
        ty: t_ty.clone(),
        span: value.span.clone(),
        def_id: None,
    };
    let bang = IrStmt { kind: IrStmtKind::Bind { var: t, mutability: Mutability::Let, ty: t_ty.clone(), value: unwrap }, span: span.clone() };
    let mut out = vec![wv, bang];
    if let Some(elements) = direct {
        for ((p, b), ty) in elements[1..].iter().zip(&bufs).zip(&buf_tys) {
            if let IrPattern::Bind { var, .. } = p {
                let value = c132_var(*b, ty);
                out.push(IrStmt { kind: IrStmtKind::Bind { var: *var, mutability: Mutability::Let, ty: ty.clone(), value }, span: span.clone() });
            }
        }
        return Some(out);
    }
    // Otherwise the original binding takes the rebuilt Ok payload `(t, b'..)` / `b'` / `(b'..)`.
    let mut parts: Vec<IrExpr> = if unit { Vec::new() } else { vec![c132_var(t, &t_ty)] };
    parts.extend(bufs.iter().zip(&buf_tys).map(|(v, ty)| c132_var(*v, ty)));
    let payload = if parts.len() == 1 { parts.remove(0) } else { c132_tuple(parts) };
    let mut rebound = stmt.clone();
    if let IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. } = &mut rebound.kind {
        *value = IrExpr { ty: ok_ty, ..payload };
    }
    out.push(rebound);
    Some(out)
}

/// The carried `!` site: `[let x =] match f(p) { ok(Q) => { p = b..; t }, err((m, b'..)) => { p = b'..; err(E)! } }`.
fn c132_carried_site(stmt: &almide_ir::IrStmt, vt: &mut almide_ir::VarTable) -> Option<Vec<almide_ir::IrStmt>> {
    use almide_ir::{IrMatchArm, IrPattern, IrStmt, IrStmtKind, Mutability};
    let value = match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::Expr { expr: value } => value,
        _ => return None,
    };
    let IrExprKind::Match { subject, arms } = &value.kind else { return None };
    let [ok, err] = arms.as_slice() else { return None };
    if !matches!(subject.kind, IrExprKind::Call { .. }) || ok.guard.is_some() || err.guard.is_some() {
        return None;
    }
    let (_, buf_tys) = c132_err_buffers(&subject.ty)?;
    let IrPattern::Ok { inner: ok_pat } = &ok.pattern else { return None };
    let IrExprKind::Block { stmts: ok_wb, expr: Some(ok_tail) } = &ok.body.kind else { return None };
    let IrPattern::Err { inner } = &err.pattern else { return None };
    let IrPattern::Tuple { elements } = inner.as_ref() else { return None };
    let (IrPattern::Bind { var: m, .. }, err_bufs) = elements.split_first()? else { return None };
    let IrExprKind::Block { stmts: err_wb, expr: Some(err_tail) } = &err.body.kind else { return None };
    let raised = c132_raised(err_tail)?;
    // Each arm writes the same places back, in order, from its own buffer binders.
    let writes = |wb: &[IrStmt]| -> Option<Vec<(almide_ir::VarId, almide_ir::VarId)>> {
        wb.iter()
            .map(|s| match &s.kind {
                IrStmtKind::Assign { var, value } => match value.kind {
                    IrExprKind::Var { id } => Some((*var, id)),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    };
    let ok_w = writes(ok_wb)?;
    let err_w = writes(err_wb)?;
    let places: Vec<almide_ir::VarId> = ok_w.iter().map(|(p, _)| *p).collect();
    if ok_w.len() != buf_tys.len()
        || err_w.iter().map(|(p, _)| *p).collect::<Vec<_>>() != places
        || err_bufs.len() != err_w.len()
        || !err_bufs.iter().zip(&err_w).all(|(pat, (_, b))| matches!(pat, IrPattern::Bind { var, .. } if var == b))
        || c132_mentions(ok_tail, &places)
    {
        return None;
    }
    let t_ty = ok_tail.ty.clone();
    let ok_reads: Vec<IrExpr> = ok_w.iter().zip(&buf_tys).map(|((_, b), t)| c132_var(*b, t)).collect();
    let span = stmt.span.clone();
    let (wv, r, bufs) =
        c132_write_visible(subject, ((**ok_pat).clone(), (**ok_tail).clone(), ok_reads), &t_ty, &buf_tys, span.clone(), vt);
    let mut out = vec![wv];
    for ((p, ty), b) in places.iter().zip(&buf_tys).zip(&bufs) {
        out.push(IrStmt { kind: IrStmtKind::Assign { var: *p, value: c132_var(*b, ty) }, span: span.clone() });
    }
    // After the write-backs: `let q = match r { ok(v) => ok(v), err(m') => err(E[m := m']) }`,
    // whose Err is the carrier's own error, then the statement over `q!` — a let-bound `!`,
    // the shape the propagation lowering places.
    let res_ty = c132_result_ty(&t_ty, &Ty::String);
    let carrier_err = raised.ty.clone();
    let q_ty = c132_result_ty(&t_ty, &carrier_err);
    let v = vt.alloc(almide_base::intern::sym("__c132_v"), t_ty.clone(), Mutability::Let, None);
    let m2 = vt.alloc(almide_base::intern::sym("__c132_m"), Ty::String, Mutability::Let, None);
    let e = almide_ir::substitute::substitute_var_in_expr(raised, *m, &c132_var(m2, &Ty::String));
    let ctor = |kind: IrExprKind| IrExpr { kind, ty: q_ty.clone(), span: None, def_id: None };
    let arms = vec![
        IrMatchArm {
            pattern: IrPattern::Ok { inner: Box::new(c132_bind(v, &t_ty)) },
            guard: None,
            body: ctor(IrExprKind::ResultOk { expr: Box::new(c132_var(v, &t_ty)) }),
        },
        IrMatchArm {
            pattern: IrPattern::Err { inner: Box::new(c132_bind(m2, &Ty::String)) },
            guard: None,
            body: ctor(IrExprKind::ResultErr { expr: Box::new(e) }),
        },
    ];
    let q = vt.alloc(almide_base::intern::sym("__c132_paired"), q_ty.clone(), Mutability::Let, None);
    let paired = IrExpr {
        kind: IrExprKind::Match { subject: Box::new(c132_var(r, &res_ty)), arms },
        ty: q_ty.clone(),
        span: value.span.clone(),
        def_id: None,
    };
    out.push(IrStmt { kind: IrStmtKind::Bind { var: q, mutability: Mutability::Let, ty: q_ty.clone(), value: paired }, span: span.clone() });
    let bang = IrExpr { kind: IrExprKind::Unwrap { expr: Box::new(c132_var(q, &q_ty)) }, ty: t_ty.clone(), span: value.span.clone(), def_id: None };
    let last = match &stmt.kind {
        IrStmtKind::Bind { var, mutability, ty, .. } => {
            IrStmt { kind: IrStmtKind::Bind { var: *var, mutability: *mutability, ty: ty.clone(), value: bang }, span }
        }
        _ => {
            let t = vt.alloc(almide_base::intern::sym("__c132_unit"), t_ty.clone(), Mutability::Let, None);
            IrStmt { kind: IrStmtKind::Bind { var: t, mutability: Mutability::Let, ty: t_ty, value: bang }, span }
        }
    };
    out.push(last);
    Some(out)
}

/// Whether `e` reads any of `vars`.
fn c132_mentions(e: &IrExpr, vars: &[almide_ir::VarId]) -> bool {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct M<'a>(&'a [almide_ir::VarId], bool);
    impl IrVisitor for M<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Var { id } = &e.kind {
                self.1 |= self.0.contains(id);
            }
            walk_expr(self, e);
        }
    }
    let mut m = M(vars, false);
    m.visit_expr(e);
    m.1
}

/// `{ s; let x = r!; s'; u }` in STATEMENT position, a Unit block, splices into the enclosing
/// list, and so does `let y = { s; let x = r!; s'; t }` (→ `s; let x = r!; s'; let y = t`):
/// the `!` early return is placed only for a `let` of the enclosing body, and the C-132
/// rewrite above leaves one in the block a write-back sits in (`fill2(a, b, s)!` →
/// `{ let (r, a', b') = …; let _ = r!; a = a'; b = b' }`; a carrier fn's body block).
/// Exact: the statements run in the same order, every VarId is unique (nothing shadowed or
/// captured), and the block's temporaries drop at the enclosing scope's end instead.
fn flatten_bang_statement_blocks(stmts: &mut Vec<almide_ir::IrStmt>) {
    use almide_ir::IrStmtKind;
    let binds_bang = |s: &almide_ir::IrStmt| {
        matches!(&s.kind,
            IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
                if matches!(value.kind, IrExprKind::Unwrap { .. }))
    };
    let mut i = 0;
    while i < stmts.len() {
        // `let x = { s; t }` holding such a statement: `s; let x = t`.
        if let IrStmtKind::Bind { value, .. } = &mut stmts[i].kind {
            if let IrExprKind::Block { stmts: inner, expr: Some(t) } = &mut value.kind {
                if inner.iter().any(binds_bang) {
                    let hoisted = std::mem::take(inner);
                    let tail = (**t).clone();
                    *value = tail;
                    let n = hoisted.len();
                    stmts.splice(i..i, hoisted);
                    i += n + 1;
                    continue;
                }
            }
        }
        let IrStmtKind::Expr { expr } = &mut stmts[i].kind else {
            i += 1;
            continue;
        };
        let IrExprKind::Block { stmts: inner, expr: tail } = &mut expr.kind else {
            i += 1;
            continue;
        };
        let unit_tail = tail.as_ref().is_none_or(|t| matches!(t.ty, Ty::Unit));
        if !unit_tail || !inner.iter().any(binds_bang) {
            i += 1;
            continue;
        }
        let mut hoisted = std::mem::take(inner);
        // A Unit tail that does something (`io.print(..)`) stays, as the last statement.
        if let Some(t) = tail.take().filter(|t| !matches!(t.kind, IrExprKind::Unit)) {
            let span = t.span.clone();
            hoisted.push(almide_ir::IrStmt { kind: IrStmtKind::Expr { expr: *t }, span });
        }
        let n = hoisted.len();
        stmts.splice(i..=i, hoisted);
        i += n;
    }
}

/// `x = match f(..) { ok(v) => v, err(e) => err(E)! }` — the paired `!` the err-carry
/// rewrite makes of `x = f(..)!` inside a carrier fn — writes `x` at statement level,
/// before the `!`, in the write-visible spelling, and pairs the error after it:
///
///   let s = f(..)
///   let (r, x') = match s { ok(v) => (ok(()), v), err(e) => (err(e), x) }
///   x = x'
///   let q = match r { ok(u) => ok(u), err(e') => err(E[e := e']) }
///   let _ = q!
///
/// Exact: on Ok `x` gets the payload; on Err `x` is rewritten with itself, so `E` reads the
/// same `x` it did before, and the raise leaves. Writing `x` inside the `!`'s continuation
/// instead would be a heap reassignment in a control-flow frame.
fn c132_paired_assign_site(stmt: &almide_ir::IrStmt, vt: &mut almide_ir::VarTable) -> Option<Vec<almide_ir::IrStmt>> {
    use almide_ir::{IrMatchArm, IrPattern, IrStmt, IrStmtKind, Mutability};
    let IrStmtKind::Assign { var: x, value } = &stmt.kind else { return None };
    let IrExprKind::Match { subject, arms } = &value.kind else { return None };
    let [ok, err] = arms.as_slice() else { return None };
    if !matches!(subject.kind, IrExprKind::Call { .. }) || ok.guard.is_some() || err.guard.is_some() {
        return None;
    }
    let IrPattern::Ok { inner } = &ok.pattern else { return None };
    let IrPattern::Bind { var: v, .. } = inner.as_ref() else { return None };
    let IrPattern::Err { inner: err_inner } = &err.pattern else { return None };
    let IrPattern::Bind { var: e, ty: e_ty } = err_inner.as_ref() else { return None };
    if !matches!(ok.body.kind, IrExprKind::Var { id } if id == *v) || !matches!(e_ty, Ty::String) {
        return None;
    }
    let raised = c132_raised(&err.body)?;
    let span = stmt.span.clone();
    let x_ty = value.ty.clone();
    let r_ty = c132_result_ty(&Ty::Unit, &Ty::String);
    let q_ty = c132_result_ty(&Ty::Unit, &raised.ty);
    let unit = || IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None };
    let fresh = |vt: &mut almide_ir::VarTable, name: &str, ty: &Ty| {
        vt.alloc(almide_base::intern::sym(name), ty.clone(), Mutability::Let, None)
    };
    let s = fresh(vt, "__c132_call", &subject.ty);
    let r = fresh(vt, "__c132_res", &r_ty);
    let nx = fresh(vt, "__c132_new", &x_ty);
    let u = fresh(vt, "__c132_u", &Ty::Unit);
    let e2 = fresh(vt, "__c132_m", &Ty::String);
    let q = fresh(vt, "__c132_paired", &q_ty);
    let done = fresh(vt, "__c132_unit", &Ty::Unit);
    let r_ctor = |kind: IrExprKind| IrExpr { kind, ty: r_ty.clone(), span: None, def_id: None };
    let q_ctor = |kind: IrExprKind| IrExpr { kind, ty: q_ty.clone(), span: None, def_id: None };
    let first = IrExpr {
        kind: IrExprKind::Match {
            subject: Box::new(c132_var(s, &subject.ty)),
            arms: vec![
                IrMatchArm {
                    pattern: ok.pattern.clone(),
                    guard: None,
                    body: c132_tuple(vec![r_ctor(IrExprKind::ResultOk { expr: Box::new(unit()) }), c132_var(*v, &x_ty)]),
                },
                IrMatchArm {
                    pattern: err.pattern.clone(),
                    guard: None,
                    body: c132_tuple(vec![
                        r_ctor(IrExprKind::ResultErr { expr: Box::new(c132_var(*e, &Ty::String)) }),
                        c132_var(*x, &x_ty),
                    ]),
                },
            ],
        },
        ty: Ty::Tuple(vec![r_ty.clone(), x_ty.clone()]),
        span: value.span.clone(),
        def_id: None,
    };
    let paired_err = almide_ir::substitute::substitute_var_in_expr(raised, *e, &c132_var(e2, &Ty::String));
    let paired = IrExpr {
        kind: IrExprKind::Match {
            subject: Box::new(c132_var(r, &r_ty)),
            arms: vec![
                IrMatchArm {
                    pattern: IrPattern::Ok { inner: Box::new(c132_bind(u, &Ty::Unit)) },
                    guard: None,
                    body: q_ctor(IrExprKind::ResultOk { expr: Box::new(c132_var(u, &Ty::Unit)) }),
                },
                IrMatchArm {
                    pattern: IrPattern::Err { inner: Box::new(c132_bind(e2, &Ty::String)) },
                    guard: None,
                    body: q_ctor(IrExprKind::ResultErr { expr: Box::new(paired_err) }),
                },
            ],
        },
        ty: q_ty.clone(),
        span: value.span.clone(),
        def_id: None,
    };
    let bang = IrExpr { kind: IrExprKind::Unwrap { expr: Box::new(c132_var(q, &q_ty)) }, ty: Ty::Unit, span: value.span.clone(), def_id: None };
    Some(vec![
        IrStmt { kind: IrStmtKind::Bind { var: s, mutability: Mutability::Let, ty: subject.ty.clone(), value: (**subject).clone() }, span: span.clone() },
        IrStmt {
            kind: IrStmtKind::BindDestructure {
                pattern: IrPattern::Tuple { elements: vec![c132_bind(r, &r_ty), c132_bind(nx, &x_ty)] },
                value: first,
            },
            span: span.clone(),
        },
        IrStmt { kind: IrStmtKind::Assign { var: *x, value: c132_var(nx, &x_ty) }, span: span.clone() },
        IrStmt { kind: IrStmtKind::Bind { var: q, mutability: Mutability::Let, ty: q_ty, value: paired }, span: span.clone() },
        IrStmt { kind: IrStmtKind::Bind { var: done, mutability: Mutability::Let, ty: Ty::Unit, value: bang }, span },
    ])
}
