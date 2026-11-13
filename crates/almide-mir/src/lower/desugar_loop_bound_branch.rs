// the bound-branch lift of the loop flag rewrite (desugar_loop.rs), include!-spliced at
// module level beside it (the 800-line ceiling the codopsy gate holds this crate to).

/// `let v = if c then d else { pre; let t = X!; rest; tail }` in a loop body (#4715) —
/// a `!` inside an arm BLOCK of a bound branch. The flag rewrite reads a `!` only in
/// bind position, so this shape declined the whole loop and the `!` reached the bound
/// return rule, whose exit leaves the loop's own state undropped. Lift the `!` out of
/// the branch: each arm becomes a `Result[T, err_ty]` value — an arm block's
/// propagating bind turns into `match X { ok(t) => { rest; ok(tail) }, err(e) => err(e) }`,
/// a direct `X!` arm gives `X`, a `!`-free arm is `ok(arm)` — and the statement becomes
/// `let $r = <branch>; let v = $r!`, the bind-position shape the flag rewrite owns.
/// Byte-identical to the early return: the err reaches the same propagation, the ok
/// payload the same bind. Only `err_ty` errors lift (no conversion inside the branch);
/// any `!` the lift cannot place leaves the statement as it was.
fn loop_uw_lift_bound_branch(s: &IrStmt, err_ty: &Ty, nv: &mut u32) -> Option<[IrStmt; 2]> {
    let IrStmtKind::Bind { var, ty, value, mutability } = &s.kind else { return None };
    if !matches!(value.kind, IrExprKind::If { .. } | IrExprKind::Match { .. }) || !expr_has_unwrap(value) {
        return None;
    }
    let res_ty = Ty::result(ty.clone(), err_ty.clone());
    let lifted = loop_uw_lift_arm(value, &res_ty, err_ty, nv)?;
    let r = VarId(*nv);
    *nv += 1;
    let r_bind = IrStmtKind::Bind {
        var: r,
        mutability: almide_ir::Mutability::Let,
        ty: res_ty.clone(),
        value: lifted,
    };
    let unwrap = loop_uw_node(
        IrExprKind::Unwrap { expr: Box::new(loop_uw_node(IrExprKind::Var { id: r }, res_ty)) },
        ty.clone(),
    );
    let v_bind = IrStmtKind::Bind { var: *var, mutability: *mutability, ty: ty.clone(), value: unwrap };
    Some([r_bind, v_bind].map(|kind| IrStmt { kind, span: s.span }))
}

/// One arm of a lifted bound branch as a `res_ty` value (see [`loop_uw_lift_bound_branch`]).
fn loop_uw_lift_arm(e: &IrExpr, res_ty: &Ty, err_ty: &Ty, nv: &mut u32) -> Option<IrExpr> {
    if !expr_has_unwrap(e) {
        return Some(loop_uw_node(IrExprKind::ResultOk { expr: Box::new(e.clone()) }, res_ty.clone()));
    }
    match &e.kind {
        IrExprKind::Unwrap { expr } if expr.ty == *res_ty => Some((**expr).clone()),
        IrExprKind::If { cond, then, else_ } if !expr_has_unwrap(cond) => {
            let then = loop_uw_lift_arm(then, res_ty, err_ty, nv)?;
            let else_ = loop_uw_lift_arm(else_, res_ty, err_ty, nv)?;
            Some(loop_uw_node(
                IrExprKind::If { cond: cond.clone(), then: Box::new(then), else_: Box::new(else_) },
                res_ty.clone(),
            ))
        }
        IrExprKind::Match { subject, arms } if !expr_has_unwrap(subject) => {
            let mut new_arms = Vec::with_capacity(arms.len());
            for a in arms {
                if a.guard.as_ref().is_some_and(expr_has_unwrap) {
                    return None;
                }
                let body = loop_uw_lift_arm(&a.body, res_ty, err_ty, nv)?;
                new_arms.push(almide_ir::IrMatchArm { pattern: a.pattern.clone(), guard: a.guard.clone(), body });
            }
            Some(loop_uw_node(IrExprKind::Match { subject: subject.clone(), arms: new_arms }, res_ty.clone()))
        }
        IrExprKind::Block { stmts, expr: Some(tail) } => loop_uw_lift_arm_block(stmts, tail, res_ty, err_ty, nv),
        _ => None,
    }
}

/// An arm block `{ pre; let t = X!; rest; tail }` with a `!`-free `pre`:
/// `{ pre; match X { ok(t) => <{ rest; tail } lifted>, err(e) => err(e) } }`.
/// A block whose `!` is only in its tail lifts the tail.
fn loop_uw_lift_arm_block(
    stmts: &[IrStmt],
    tail: &IrExpr,
    res_ty: &Ty,
    err_ty: &Ty,
    nv: &mut u32,
) -> Option<IrExpr> {
    let block = |stmts: Vec<IrStmt>, tail: IrExpr| {
        loop_uw_node(IrExprKind::Block { stmts, expr: Some(Box::new(tail)) }, res_ty.clone())
    };
    let Some(i) = stmts.iter().position(stmt_has_unwrap) else {
        return Some(block(stmts.to_vec(), loop_uw_lift_arm(tail, res_ty, err_ty, nv)?));
    };
    let (ok_pat, x) = match &stmts[i].kind {
        IrStmtKind::Bind { var, ty, value, mutability: almide_ir::Mutability::Let } => match &value.kind {
            IrExprKind::Unwrap { expr } => (almide_ir::IrPattern::Bind { var: *var, ty: ty.clone() }, expr),
            _ => return None,
        },
        IrStmtKind::Expr { expr: value } => match &value.kind {
            IrExprKind::Unwrap { expr } => (almide_ir::IrPattern::Wildcard, expr),
            _ => return None,
        },
        _ => return None,
    };
    if x.ty.result_err_ty().as_ref() != Some(err_ty) {
        return None;
    }
    let ok_body = loop_uw_lift_arm_block(&stmts[i + 1..], tail, res_ty, err_ty, nv)?;
    let e = VarId(*nv);
    *nv += 1;
    let err_body = loop_uw_node(
        IrExprKind::ResultErr { expr: Box::new(loop_uw_node(IrExprKind::Var { id: e }, err_ty.clone())) },
        res_ty.clone(),
    );
    let arms = vec![
        almide_ir::IrMatchArm { pattern: almide_ir::IrPattern::Ok { inner: Box::new(ok_pat) }, guard: None, body: ok_body },
        almide_ir::IrMatchArm {
            pattern: almide_ir::IrPattern::Err {
                inner: Box::new(almide_ir::IrPattern::Bind { var: e, ty: err_ty.clone() }),
            },
            guard: None,
            body: err_body,
        },
    ];
    let m = loop_uw_node(IrExprKind::Match { subject: x.clone(), arms }, res_ty.clone());
    Some(block(stmts[..i].to_vec(), m))
}
