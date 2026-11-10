// the if-arm unwrap lift of desugar_unwrap_b.rs, include!-spliced back at module level
// (a code move under the 800-line ceiling the codopsy gate holds this crate to).

/// `let v = if c then e! else d` — an unwrap-`!` INSIDE an `if` arm (base64 decode_chunks:
/// `let v1 = if remaining > 1 then char_to_val(..)! else 0`). LIFT the `!` out of the arm: wrap each
/// NON-unwrap arm in `ok(..)` and strip `!` from each unwrap arm, so the `if` becomes a `Result[T,E]`,
/// bind it to `$r`, then `let v = $r!` — a plain let-unwrap the [`desugar_let_unwrap`] pass then
/// handles (and the let-bound heap-result `if` `$r` the pre-TCO pass tail-duplicates). Composes the
/// two existing desugars to reach the unwrap-in-if-arm shape. v0's `!` early-returns identically.
pub fn desugar_if_arm_unwrap(body: &IrExpr) -> Option<IrExpr> {
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        return None;
    };
    let (i, bind_var, bind_ty, cond, then_e, else_e) =
        stmts.iter().enumerate().find_map(|(i, s)| match &s.kind {
            IrStmtKind::Bind { var, ty, value, .. } => match &value.kind {
                IrExprKind::If { cond, then, else_ }
                    if matches!(&then.kind, IrExprKind::Unwrap { .. })
                        || matches!(&else_.kind, IrExprKind::Unwrap { .. }) =>
                {
                    Some((i, *var, ty.clone(), (**cond).clone(), (**then).clone(), (**else_).clone()))
                }
                _ => None,
            },
            _ => None,
        })?;
    // The `Result[T, E]` type the arms unify to — take it from an unwrap arm's operand.
    let res_ty = match (&then_e.kind, &else_e.kind) {
        (IrExprKind::Unwrap { expr }, _) | (_, IrExprKind::Unwrap { expr }) => expr.ty.clone(),
        _ => return None,
    };
    let mk = |kind: IrExprKind, ty: Ty| IrExpr {
        kind,
        ty,
        span: body.span,
        def_id: body.def_id,
    };
    // strip `!` from an unwrap arm (already Result[T,E]); wrap a plain arm in `ok(..)`.
    let conv = |arm: IrExpr| -> IrExpr {
        match arm.kind {
            IrExprKind::Unwrap { expr } => *expr,
            _ => mk(IrExprKind::ResultOk { expr: Box::new(arm) }, res_ty.clone()),
        }
    };
    let new_if = mk(
        IrExprKind::If {
            cond: Box::new(cond),
            then: Box::new(conv(then_e)),
            else_: Box::new(conv(else_e)),
        },
        res_ty.clone(),
    );
    let r_var = VarId(crate::lower::desugar_var_seed());
    let r_bind = IrStmt {
        kind: IrStmtKind::Bind {
            var: r_var,
            ty: res_ty.clone(),
            value: new_if,
            mutability: almide_ir::Mutability::Let,
        },
        span: None,
    };
    let v_value = mk(
        IrExprKind::Unwrap { expr: Box::new(mk(IrExprKind::Var { id: r_var }, res_ty)) },
        bind_ty.clone(),
    );
    let v_bind = IrStmt {
        kind: IrStmtKind::Bind {
            var: bind_var,
            ty: bind_ty,
            value: v_value,
            mutability: almide_ir::Mutability::Let,
        },
        span: None,
    };
    let mut new_stmts = stmts[..i].to_vec();
    new_stmts.push(r_bind);
    new_stmts.push(v_bind);
    new_stmts.extend_from_slice(&stmts[i + 1..]);
    Some(mk(IrExprKind::Block { stmts: new_stmts, expr: tail.clone() }, body.ty.clone()))
}

/// `let (a, b) = if c then { let h = f(x)!; (h, q) } else (d, p)` — a
/// destructuring bind of a branch that holds a propagation. Every lift of a
/// `!` out of a bound branch ([`desugar_if_arm_unwrap`], the let-unwrap and the
/// tail duplication of a let-bound branch) reads a plain `let v = <branch>`, so
/// the destructuring spelling reached the lowering with its `!` still inside an
/// arm and the function was refused. Bind the branch whole and destructure it
/// as the next statement: `let $t = <branch>; let (a, b) = $t`.
pub fn desugar_destructure_branch_unwrap(body: &IrExpr, next_var: &mut u32) -> Option<IrExpr> {
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        return None;
    };
    let (i, pattern, value) = stmts.iter().enumerate().find_map(|(i, s)| match &s.kind {
        IrStmtKind::BindDestructure { pattern, value }
            if matches!(value.kind, IrExprKind::If { .. } | IrExprKind::Match { .. }) && expr_has_unwrap(value) =>
        {
            Some((i, pattern, value))
        }
        _ => None,
    })?;
    let tmp = VarId(*next_var);
    *next_var += 1;
    let whole = IrStmtKind::Bind {
        var: tmp,
        ty: value.ty.clone(),
        value: value.clone(),
        mutability: almide_ir::Mutability::Let,
    };
    let payload = IrExpr { kind: IrExprKind::Var { id: tmp }, ty: value.ty.clone(), span: value.span, def_id: None };
    let destructure = IrStmtKind::BindDestructure { pattern: pattern.clone(), value: payload };
    let split = [whole, destructure].map(|kind| IrStmt { kind, span: stmts[i].span });
    let stmts = [&stmts[..i], &split[..], &stmts[i + 1..]].concat();
    Some(IrExpr { kind: IrExprKind::Block { stmts, expr: tail.clone() }, ty: body.ty.clone(), span: body.span, def_id: body.def_id })
}

/// `if c then { …; while g { q = f(q)! }; … } else ()` followed by more of a
/// `Result[T, String]` block — a branch statement whose arm holds a loop that
/// propagates. The loop flag form ([`desugar_loop_unwrap`]) reads a loop that is
/// a statement of a Result-typed block, and its post-loop dispatch is that
/// block's tail; a loop in a unit arm has neither, so its `!` reached the
/// lowering and the function was refused. The continuation lift reads this
/// beside the direct `!`s of the arms: once the rest of the block is in each
/// arm, the arm is the Result-typed block the loop form needs. Only a `String`
/// error qualifies — the one error type the loop form carries.
fn branch_arm_holds_propagating_loop(branch: &IrExpr, result_ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    let string_err = matches!(
        result_ty,
        Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 && matches!(a[1], Ty::String)
    );
    string_err && arm_spine_holds_unwrap_loop(branch)
}

/// A loop with a propagating body on the statement spine of a branch's arms:
/// through arm blocks and the branch statements nested in them, never into a
/// bound value or a loop body (the lift moves statements, not values).
fn arm_spine_holds_unwrap_loop(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::ForIn { body, .. } | IrExprKind::While { body, .. } => body.iter().any(stmt_has_unwrap),
        IrExprKind::Block { stmts, expr } => {
            stmts.iter().any(|s| matches!(&s.kind, IrStmtKind::Expr { expr } if arm_spine_holds_unwrap_loop(expr)))
                || expr.as_deref().is_some_and(arm_spine_holds_unwrap_loop)
        }
        IrExprKind::If { then, else_, .. } => {
            arm_spine_holds_unwrap_loop(then) || arm_spine_holds_unwrap_loop(else_)
        }
        IrExprKind::Match { arms, .. } => arms.iter().any(|a| arm_spine_holds_unwrap_loop(&a.body)),
        _ => false,
    }
}
