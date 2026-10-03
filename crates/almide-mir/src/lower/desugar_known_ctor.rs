// ── case-of-known-constructor (#3121) ──

/// The constructor a `Result` literal subject is built with, and its payload.
enum KnownCtor<'a> {
    Ok(&'a IrExpr),
    Err(&'a IrExpr),
}

fn known_ctor(e: &IrExpr) -> Option<KnownCtor<'_>> {
    match &e.kind {
        IrExprKind::ResultOk { expr } => Some(KnownCtor::Ok(expr)),
        IrExprKind::ResultErr { expr } => Some(KnownCtor::Err(expr)),
        _ => None,
    }
}

/// What one arm pattern does with a known constructor: `Ok(binder)` when it matches
/// (the binder is the payload's — `None` for a `_` payload), `Err(())` when it cannot
/// match, and `None` when the arm is not decidable here (a refining payload pattern).
#[allow(clippy::result_unit_err)]
fn known_ctor_arm(ctor: &KnownCtor<'_>, pat: &IrPattern) -> Option<Result<Option<VarId>, ()>> {
    let inner = match (ctor, pat) {
        (_, IrPattern::Wildcard) => return Some(Ok(None)),
        (KnownCtor::Ok(_), IrPattern::Ok { inner }) | (KnownCtor::Err(_), IrPattern::Err { inner }) => {
            inner
        }
        (_, IrPattern::Ok { .. } | IrPattern::Err { .. }) => return Some(Err(())),
        _ => return None,
    };
    match &**inner {
        IrPattern::Wildcard => Some(Ok(None)),
        IrPattern::Bind { var, .. } => Some(Ok(Some(*var))),
        _ => None,
    }
}

/// An expression with no effect and no allocation of its own that dropping
/// unevaluated would skip — a variable, a scalar literal, or a tuple of them.
fn known_ctor_payload_inert(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::Var { .. }
        | IrExprKind::LitInt { .. }
        | IrExprKind::LitBool { .. }
        | IrExprKind::LitStr { .. }
        | IrExprKind::Unit => true,
        IrExprKind::Tuple { elements } => elements.iter().all(known_ctor_payload_inert),
        _ => false,
    }
}

/// May the binder be replaced by the payload itself (no `let`)? A literal (or a
/// tuple of literals) re-evaluates to the same value anywhere. Any other inert
/// payload only when the arm IS a constructor of the binder (`err(x)`, `x`), so
/// nothing runs between the subject and the one use.
fn known_ctor_payload_substitutable(p: &IrExpr, body: &IrExpr, var: VarId) -> bool {
    fn literal(e: &IrExpr) -> bool {
        match &e.kind {
            IrExprKind::LitInt { .. }
            | IrExprKind::LitBool { .. }
            | IrExprKind::LitStr { .. }
            | IrExprKind::Unit => true,
            IrExprKind::Tuple { elements } => elements.iter().all(literal),
            _ => false,
        }
    }
    if literal(p) {
        return true;
    }
    let is_binder = |e: &IrExpr| matches!(e.kind, IrExprKind::Var { id } if id == var);
    known_ctor_payload_inert(p)
        && match &body.kind {
            IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr } => is_binder(expr),
            _ => is_binder(body),
        }
}

/// `{ (); e }` → `e`: the substituted binder can leave an effect-free statement
/// behind (a Unit payload read as `x` on its own line).
fn drop_inert_expr_stmts(mut e: IrExpr) -> IrExpr {
    let IrExprKind::Block { stmts, expr } = &mut e.kind else { return e };
    stmts.retain(|s| {
        !matches!(&s.kind, almide_ir::IrStmtKind::Expr { expr } if known_ctor_payload_inert(expr))
    });
    if stmts.is_empty() {
        if let Some(t) = expr.take() {
            return *t;
        }
    }
    e
}

/// Reduce one `match <ctor literal> { … }` to the arm the constructor selects.
fn reduce_known_ctor_match(subject: &IrExpr, arms: &[IrMatchArm], ty: &Ty) -> Option<IrExpr> {
    use almide_ir::{IrStmt, IrStmtKind, Mutability};
    // Only a match whose VALUE is a Result: then an `err(e)` arm is that value. A
    // match of another type (a Unit `main`'s `!` desugar) still has an err arm
    // whose meaning is its position — the abort-line rewrite keys on it there.
    if !ty.is_result() || arms.iter().any(|a| &a.body.ty != ty) {
        return None;
    }
    let ctor = known_ctor(subject)?;
    // Only an inert payload (a variable, a literal, a tuple of them): the rewrite
    // then never moves an allocation or an effect out of the subject position.
    let (KnownCtor::Ok(payload) | KnownCtor::Err(payload)) = ctor;
    if !known_ctor_payload_inert(payload) {
        return None;
    }
    for arm in arms {
        match known_ctor_arm(&ctor, &arm.pattern)? {
            Err(()) => continue,
            Ok(_) if arm.guard.is_some() => return None,
            // A `_` payload: the inert payload has nothing to evaluate.
            Ok(None) => return Some(arm.body.clone()),
            Ok(Some(var)) if known_ctor_payload_substitutable(payload, &arm.body, var) => {
                let body = almide_ir::substitute::substitute_var_in_expr(&arm.body, var, payload);
                return Some(drop_inert_expr_stmts(body));
            }
            Ok(Some(var)) => {
                let bind = IrStmt {
                    kind: IrStmtKind::Bind {
                        var,
                        mutability: Mutability::Let,
                        ty: payload.ty.clone(),
                        value: payload.clone(),
                    },
                    span: payload.span.clone(),
                };
                return Some(IrExpr {
                    kind: IrExprKind::Block {
                        stmts: vec![bind],
                        expr: Some(Box::new(arm.body.clone())),
                    },
                    ty: ty.clone(),
                    span: arm.body.span.clone(),
                    def_id: None,
                });
            }
        }
    }
    None
}

/// `{ …; let v = <ctor literal>; match v { … } }` with `v` used only as that
/// subject — the let-bound spelling the heap-branch desugar distributes into each
/// arm of a raising `if` (`let r = if c then err(m) else ok(()); r!`). The bind
/// moves into the subject (one evaluation either way) and the match reduces.
fn reduce_let_bound_known_ctor_match(e: &mut IrExpr) -> bool {
    use almide_ir::{IrStmtKind, Mutability};
    let IrExprKind::Block { stmts, expr: Some(tail) } = &e.kind else { return false };
    let Some(last) = stmts.last() else { return false };
    let IrStmtKind::Bind { var, mutability: Mutability::Let, value, .. } = &last.kind else {
        return false;
    };
    if known_ctor(value).is_none() {
        return false;
    }
    let IrExprKind::Match { subject, arms } = &tail.kind else { return false };
    if !matches!(subject.kind, IrExprKind::Var { id } if id == *var)
        || count_var_uses(tail, *var) != 1
    {
        return false;
    }
    // A non-inert Unit payload (`let r = ok(println(..))`) is first bound on its own
    // at the same point, `let t = println(..); let r = ok(t)` — it is evaluated
    // exactly where it was — so the subject's payload is the inert `t`.
    let (hoisted, ctor) = match known_ctor(value) {
        Some(KnownCtor::Ok(p) | KnownCtor::Err(p))
            if !known_ctor_payload_inert(p) && matches!(p.ty, Ty::Unit) =>
        {
            let t = VarId(crate::lower::desugar_var_seed());
            let bind = almide_ir::IrStmt {
                kind: IrStmtKind::Bind { var: t, mutability: Mutability::Let, ty: p.ty.clone(), value: p.clone() },
                span: p.span.clone(),
            };
            let t_ref = IrExpr { kind: IrExprKind::Var { id: t }, ty: p.ty.clone(), span: p.span.clone(), def_id: None };
            let kind = match &value.kind {
                IrExprKind::ResultOk { .. } => IrExprKind::ResultOk { expr: Box::new(t_ref) },
                _ => IrExprKind::ResultErr { expr: Box::new(t_ref) },
            };
            (Some(bind), IrExpr { kind, ..value.clone() })
        }
        _ => (None, value.clone()),
    };
    let Some(reduced) = reduce_known_ctor_match(&ctor, arms, &tail.ty) else { return false };
    let IrExprKind::Block { stmts, expr } = &mut e.kind else { return false };
    stmts.pop();
    stmts.extend(hoisted);
    *expr = Some(Box::new(reduced));
    true
}

/// Does `body` hold a `match` over a constructor literal, directly or through a
/// let-bound subject? A cheap read-only scan, so the pass clones only on a hit.
fn has_known_ctor_subject(body: &IrExpr) -> bool {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct Scan(bool);
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            if self.0 {
                return;
            }
            match &e.kind {
                IrExprKind::Match { subject, .. } if known_ctor(subject).is_some() => self.0 = true,
                IrExprKind::Block { stmts, expr: Some(tail) } => {
                    let bound_ctor = stmts.last().is_some_and(|s| {
                        matches!(&s.kind, almide_ir::IrStmtKind::Bind { value, .. } if known_ctor(value).is_some())
                    });
                    if bound_ctor && matches!(tail.kind, IrExprKind::Match { .. }) {
                        self.0 = true;
                    }
                }
                _ => {}
            }
            walk_expr(self, e);
        }
    }
    let mut s = Scan(false);
    s.visit_expr(body);
    s.0
}

/// Case-of-known-constructor: a `match` whose subject is a `Result` LITERAL
/// (`ok(e)` / `err(e)`) over an inert payload selects its arm statically,
/// so it is `{ let x = e; <that arm> }`. The C-132 err carrier (#2917) makes this
/// shape out of every raise inside a `!` continuation: `err((m, b))!` desugars to
/// `match err((m, b)) { ok(v) => <continuation>, err(x) => err(x) }`, whose dead
/// Ok arm still holds the continuation and walled the heap-result match. The
/// payload is evaluated exactly once on both sides (the subject evaluates it
/// before any arm runs); only a dead arm is removed. An arm that refines the
/// payload, or a guard on the selected arm, declines. Runs in the shared chain.
pub fn desugar_known_ctor_match(body: &IrExpr) -> Option<IrExpr> {
    if !has_known_ctor_subject(body) {
        return None;
    }
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    struct V {
        changed: bool,
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            if let IrExprKind::Match { subject, arms } = &e.kind {
                if let Some(r) = reduce_known_ctor_match(subject, arms, &e.ty) {
                    *e = r;
                    self.changed = true;
                }
            } else if reduce_let_bound_known_ctor_match(e) {
                self.changed = true;
            }
        }
    }
    let mut v = V { changed: false };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    v.changed.then_some(out)
}

/// Is `s` a statement that reads a Unit value and does nothing else (`()` or a
/// Unit-typed variable on its own line)?
fn is_unit_read_stmt(s: &almide_ir::IrStmt) -> bool {
    matches!(&s.kind, almide_ir::IrStmtKind::Expr { expr }
        if matches!(expr.ty, Ty::Unit)
            && matches!(expr.kind, IrExprKind::Unit | IrExprKind::Var { .. }))
}

/// Drop every statement that only reads a Unit value — `{ (); ok(()) }` is
/// `ok(())`, and `{ let u = println(..); u; ok(()) }` is `{ let u = println(..);
/// ok(()) }` — and unwrap a block left with no statements. Reading a Unit value
/// has no effect and owns nothing. A Unit effect fn's `()` tail and the `!`
/// desugar's `ok(u) => { u; ok(()) }` arm leave these inside a heap-result `if`
/// arm (#3121), where the arm lowering keys on the concrete tail kind.
pub fn desugar_drop_unit_read_stmts(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::visit::{walk_expr, IrVisitor};
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    struct Scan(bool);
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            if self.0 {
                return;
            }
            if let IrExprKind::Block { stmts, expr: Some(_) } = &e.kind {
                if stmts.iter().any(is_unit_read_stmt) {
                    self.0 = true;
                    return;
                }
            }
            walk_expr(self, e);
        }
    }
    let mut scan = Scan(false);
    scan.visit_expr(body);
    if !scan.0 {
        return None;
    }
    struct V;
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Block { stmts, expr: Some(tail) } = &mut e.kind else { return };
            stmts.retain(|s| !is_unit_read_stmt(s));
            if stmts.is_empty() {
                let t = (**tail).clone();
                *e = t;
            }
        }
    }
    let mut out = body.clone();
    V.visit_expr_mut(&mut out);
    Some(out)
}

// ── block operands (#3121, #3084) ──

/// `{ stmts; r } ?? fb  ≡  { stmts; r ?? fb }` — a `??` whose operand is a block
/// absorbs into it, the way a call absorbs a block argument
/// ([`hoist_block_call_args`]). Exact: the operand's statements already ran
/// before the fallback could, and the fallback stays lazy. The C-132 call site
/// of a can-err `mut`-param callee (`f(xs) ?? d`, #3121) is that block — the
/// write-back `{ let (r, b) = match f(xs) {…}; xs = b; r }`. Returns whether
/// `e` was rewritten.
fn absorb_unwrap_or_block_operand(e: &mut IrExpr) -> bool {
    let IrExprKind::UnwrapOr { expr, .. } = &mut e.kind else { return false };
    let IrExprKind::Block { stmts, expr: Some(tail) } = &mut expr.kind else { return false };
    if stmts.is_empty() {
        return false;
    }
    let hoisted = std::mem::take(stmts);
    let tail = (**tail).clone();
    **expr = tail;
    let ty = e.ty.clone();
    let span = e.span.clone();
    let inner = std::mem::replace(
        e,
        IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None },
    );
    *e = IrExpr {
        kind: IrExprKind::Block { stmts: hoisted, expr: Some(Box::new(inner)) },
        ty,
        span,
        def_id: None,
    };
    true
}

/// The operands BEFORE a call's block argument, made safe to evaluate after
/// the block's statements, and the `let`s that do it, in order. An operand that
/// calls nothing and reads no variable the block writes stays (re-reading it
/// later is unobservable). Otherwise it is bound to a fresh `let` first —
/// `f(g(x), { s; e })` becomes `{ let t = g(x); s; f(t, e) }`, so `g` still runs
/// before the block (#3084: a nested `mut`-param call, whose write-back block
/// follows its sibling arguments), and `f(v, { v = w; e })` becomes
/// `{ let t = v; v = w; f(t, e) }`, so the operand sees `v` from BEFORE the
/// write-back (C-132, #3230) — when the operand is a scalar, a call or an
/// interpolation (a fresh value), or the written variable itself (the `let`
/// shares it, and an in-place write copies on share). Any other operand (a heap
/// field or index place, an operator) declines with `None`, leaving the call
/// untouched.
fn bind_earlier_call_operands(
    args: &mut [IrExpr],
    block: &IrExpr,
    vt: &mut almide_ir::VarTable,
) -> Option<Vec<almide_ir::IrStmt>> {
    use almide_ir::{IrStmt, IrStmtKind, Mutability};
    let written = stmts_written_vars(block);
    let mut plan = Vec::with_capacity(args.len());
    for a in args.iter() {
        let (reads_written, calls) = part_reads_and_calls(a, &written);
        let bind = match &a.kind {
            IrExprKind::LitInt { .. }
            | IrExprKind::LitFloat { .. }
            | IrExprKind::LitBool { .. }
            | IrExprKind::LitStr { .. }
            | IrExprKind::Unit => false,
            IrExprKind::Var { .. } => reads_written,
            IrExprKind::Call { .. } | IrExprKind::StringInterp { .. } => true,
            _ if !crate::lower::is_heap_ty(&a.ty) => reads_written || calls,
            _ => return None,
        };
        plan.push(bind);
    }
    let mut binds = Vec::new();
    for (a, bind) in args.iter_mut().zip(plan) {
        if !bind {
            continue;
        }
        let ty = a.ty.clone();
        let span = a.span.clone();
        let var = vt.alloc(almide_base::intern::sym("__arg"), ty.clone(), Mutability::Let, None);
        let value = std::mem::replace(
            a,
            IrExpr { kind: IrExprKind::Var { id: var }, ty: ty.clone(), span: span.clone(), def_id: None },
        );
        binds.push(IrStmt { kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value }, span });
    }
    Some(binds)
}
