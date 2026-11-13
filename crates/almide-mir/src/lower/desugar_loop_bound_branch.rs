// the bound-branch continuation lift of the loop flag rewrite (desugar_loop.rs),
// include!-spliced at module level beside it (the 800-line ceiling the codopsy gate holds
// this crate to).

/// `let v = if c then d else { pre; let t = X!; tail }; rest` in a loop body (#4715) —
/// a `!` inside an arm of a bound branch. The flag rewrite reads a `!` only in bind
/// position, so this shape declined the whole loop and the `!` reached the bound return
/// rule, whose exit leaves the loop's own state undropped. Move the continuation into
/// the arms instead: the statement becomes the branch
/// `if c then { let v = d; rest } else { pre; let t = X!; let v = tail; rest }`,
/// whose arm blocks hold their `!` in bind position with everything after it behind
/// it — the shape the flag rewrite places. No new value is built (the err arm of the
/// flag rewrite needs no payload for `v`), so any bound type takes it. `None` when
/// statement `i` is not a bound branch holding a `!`.
fn loop_uw_bound_branch_continuation(
    stmts: &[IrStmt],
    i: usize,
    tail: &Option<Box<IrExpr>>,
) -> Option<Vec<IrStmt>> {
    let IrStmtKind::Bind { var, ty, value, mutability } = &stmts[i].kind else { return None };
    if !matches!(value.kind, IrExprKind::If { .. } | IrExprKind::Match { .. }) || !expr_has_unwrap(value) {
        return None;
    }
    let rest = &stmts[i + 1..];
    let arm = |a: &IrExpr| {
        let (mut body, val) = match &a.kind {
            IrExprKind::Block { stmts, expr: Some(t) } => (stmts.clone(), (**t).clone()),
            _ => (Vec::new(), a.clone()),
        };
        body.push(IrStmt {
            kind: IrStmtKind::Bind { var: *var, ty: ty.clone(), value: val, mutability: *mutability },
            span: stmts[i].span,
        });
        body.extend_from_slice(rest);
        loop_uw_node(IrExprKind::Block { stmts: body, expr: tail.clone() }, Ty::Unit)
    };
    let branch = match &value.kind {
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: cond.clone(),
            then: Box::new(arm(then)),
            else_: Box::new(arm(else_)),
        },
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: subject.clone(),
            arms: arms
                .iter()
                .map(|a| almide_ir::IrMatchArm { pattern: a.pattern.clone(), guard: a.guard.clone(), body: arm(&a.body) })
                .collect(),
        },
        _ => return None,
    };
    let mut out = stmts[..i].to_vec();
    out.push(IrStmt {
        kind: IrStmtKind::Expr { expr: loop_uw_node(branch, Ty::Unit) },
        span: stmts[i].span,
    });
    Some(out)
}
