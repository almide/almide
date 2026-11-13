// An `err(..)` arm of a `let`-bound match on the fn-body tail chain (#4640),
// on the certified native render. include!-spliced from mod_c_tail.rs.
//
// The checker joins a match arm written `err(e)` as `Never`: in
// `let x = match s { some(v) => v, none => err("no x") }` the second arm is an
// early return of the enclosing fn. On the fn-body tail chain that return is
// expressible with no new control flow, as `desugar_fn_body_guards` does for
// `guard`: the rest of the block moves into the one arm that continues.
//
// ```text
// { s1; let x = match s { p => v, q => err(e) }; rest; tail }
//   ≡  { s1; match s { p => { let x = v; rest; tail }, q => err(e) } }
// ```
//
// Exact because the continuation of a fn-tail bind is the fn tail, and only
// when exactly one arm continues (the rest is moved, never copied). Nothing
// is allocated: the earlier `!` form of the same match built an `ok(v)` block
// per call and ran slower than the standard codegen. The rewrite recurses
// into the continuation and into tail `if`/`match` arms, never into loops,
// nested statements or lambdas, whose `err(..)` arm leaves a different scope.

/// Restructure the fn-tail err-arm binds of `body`, a fn declared `ret`.
pub fn desugar_err_arm_continuations(body: &IrExpr, ret: &Ty) -> Option<IrExpr> {
    if !crate::lower::owned_tco_slots() || !ret.is_result() {
        return None;
    }
    let mut changed = false;
    let out = err_arm_tail(body.clone(), ret, &mut changed);
    changed.then_some(out)
}

fn err_arm_tail(e: IrExpr, ret: &Ty, changed: &mut bool) -> IrExpr {
    // A tail typed other than the declared Result (a lifted effect fn's
    // payload-typed tail) has no `err(..)` value of its own type.
    if e.ty != *ret {
        return e;
    }
    let IrExpr { kind, ty, span, def_id } = e;
    let kind = match kind {
        IrExprKind::Block { stmts, expr } => return err_arm_block(stmts, expr, IrExpr { kind: IrExprKind::Unit, ty, span, def_id }, ret, changed),
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond,
            then: Box::new(err_arm_tail(*then, ret, changed)),
            else_: Box::new(err_arm_tail(*else_, ret, changed)),
        },
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject,
            arms: arms
                .into_iter()
                .map(|a| almide_ir::IrMatchArm { body: err_arm_tail(a.body, ret, changed), ..a })
                .collect(),
        },
        other => other,
    };
    IrExpr { kind, ty, span, def_id }
}

/// One fn-tail block (`shell` carries its type, span and def id).
fn err_arm_block(stmts: Vec<IrStmt>, expr: Option<Box<IrExpr>>, shell: IrExpr, ret: &Ty, changed: &mut bool) -> IrExpr {
    let block = |stmts, expr| IrExpr { kind: IrExprKind::Block { stmts, expr }, ..shell.clone() };
    let mut before = Vec::with_capacity(stmts.len());
    let mut iter = stmts.into_iter();
    while let Some(stmt) = iter.next() {
        let Some(k) = continuing_arm(&stmt) else {
            before.push(stmt);
            continue;
        };
        let IrStmtKind::Bind { var, mutability, ty: bind_ty, value } = stmt.kind else { unreachable!() };
        let IrExprKind::Match { subject, arms } = value.kind else { unreachable!() };
        *changed = true;
        let rest: Vec<IrStmt> = iter.collect();
        let arms = arms
            .into_iter()
            .enumerate()
            .map(|(i, a)| {
                let body = if i == k {
                    let bind = IrStmt {
                        kind: IrStmtKind::Bind { var, mutability, ty: bind_ty.clone(), value: a.body },
                        span: stmt.span,
                    };
                    let cont = block(std::iter::once(bind).chain(rest.iter().cloned()).collect(), expr.clone());
                    err_arm_tail(cont, ret, changed)
                } else {
                    // The `err(..)` arm, now the block's own value.
                    IrExpr { ty: ret.clone(), ..a.body }
                };
                almide_ir::IrMatchArm { body, ..a }
            })
            .collect();
        let matched = IrExpr {
            kind: IrExprKind::Match { subject, arms },
            ty: ret.clone(),
            span: value.span,
            def_id: None,
        };
        return block(before, Some(Box::new(matched)));
    }
    let expr = expr.map(|t| Box::new(err_arm_tail(*t, ret, changed)));
    block(before, expr)
}

/// The index of the one arm that continues, when `stmt` is a non-Result `let`
/// of a guard-free match whose every other arm is a bare `err(..)`.
fn continuing_arm(stmt: &IrStmt) -> Option<usize> {
    let IrStmtKind::Bind { ty, value, .. } = &stmt.kind else { return None };
    let IrExprKind::Match { arms, .. } = &value.kind else { return None };
    if ty.is_result() || arms.iter().any(|a| a.guard.is_some()) {
        return None;
    }
    let mut cont = arms.iter().enumerate().filter(|(_, a)| !matches!(a.body.kind, IrExprKind::ResultErr { .. }));
    let (k, _) = cont.next()?;
    (cont.next().is_none() && arms.len() > 1).then_some(k)
}
