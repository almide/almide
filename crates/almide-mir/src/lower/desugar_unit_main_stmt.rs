// ─────────── effect-`!` under a branch STATEMENT of a void-`main` loop body ───────────
//
// `desugar_unit_main_loop` gives a loop body's `!` the die continuation, but
// `desugar_effect_unwrap_inner` only reaches a block's own statements, its
// nested loops and its TAIL. A `!` in an arm of an `if`/`match` STATEMENT that
// more statements follow —
//
//   while … { if first then { let m = e!; xs = m } else (); i = i + 1 }
//
// — was left in place and walled the loop. The die arm never returns, so the
// arm's own block is the whole continuation of its `!`: rewrite it where it
// stands (`{ match e { err($e) => <die>, ok(m) => { xs = m } } }`), nothing
// lifted and nothing duplicated; the statements after the branch run exactly
// when the arm completes.

/// Rewrite every `!` of a void-`main` loop body `e` that sits under a Unit
/// branch statement, innermost first, then the block's own `!` statements.
/// `None` = nothing was rewritten.
fn unit_main_branch_stmt_bangs(e: &IrExpr, next_var: &mut u32, ret_is_result: bool) -> Option<IrExpr> {
    if !unit_main_has_bang(e) {
        return None;
    }
    match &e.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            let mut changed = false;
            let new_stmts: Vec<IrStmt> = stmts
                .iter()
                .map(|s| match unit_main_branch_stmt(s, next_var, ret_is_result) {
                    Some(n) => {
                        changed = true;
                        n
                    }
                    None => s.clone(),
                })
                .collect();
            let block = IrExpr {
                kind: IrExprKind::Block { stmts: new_stmts, expr: tail.clone() },
                ty: e.ty.clone(),
                span: e.span,
                def_id: e.def_id,
            };
            desugar_effect_unwrap_inner(&block, next_var, true, ret_is_result).or(changed.then_some(block))
        }
        IrExprKind::If { cond, then, else_ } => {
            let nt = unit_main_branch_stmt_bangs(then, next_var, ret_is_result);
            let ne = unit_main_branch_stmt_bangs(else_, next_var, ret_is_result);
            if nt.is_none() && ne.is_none() {
                return None;
            }
            Some(mk_at(
                e,
                IrExprKind::If {
                    cond: cond.clone(),
                    then: Box::new(nt.unwrap_or_else(|| (**then).clone())),
                    else_: Box::new(ne.unwrap_or_else(|| (**else_).clone())),
                },
                e.ty.clone(),
            ))
        }
        IrExprKind::Match { subject, arms } => {
            let mut changed = false;
            let new_arms: Vec<almide_ir::IrMatchArm> = arms
                .iter()
                .map(|a| {
                    let body = unit_main_branch_stmt_bangs(&a.body, next_var, ret_is_result);
                    changed |= body.is_some();
                    almide_ir::IrMatchArm {
                        pattern: a.pattern.clone(),
                        guard: a.guard.clone(),
                        body: body.unwrap_or_else(|| a.body.clone()),
                    }
                })
                .collect();
            changed.then(|| mk_at(e, IrExprKind::Match { subject: subject.clone(), arms: new_arms }, e.ty.clone()))
        }
        _ => desugar_tail_effect_unwrap(e, next_var, true, ret_is_result),
    }
}

/// One statement of such a block: a Unit `if`/`match`/block statement holding a
/// `!` is rewritten through its arms; any other statement is left alone.
fn unit_main_branch_stmt(s: &IrStmt, next_var: &mut u32, ret_is_result: bool) -> Option<IrStmt> {
    let IrStmtKind::Expr { expr } = &s.kind else { return None };
    let branch = matches!(&expr.kind, IrExprKind::If { .. } | IrExprKind::Match { .. } | IrExprKind::Block { .. });
    if !branch || !matches!(expr.ty, Ty::Unit) {
        return None;
    }
    let expr = unit_main_branch_stmt_bangs(expr, next_var, ret_is_result)?;
    Some(IrStmt { kind: IrStmtKind::Expr { expr }, span: s.span })
}
