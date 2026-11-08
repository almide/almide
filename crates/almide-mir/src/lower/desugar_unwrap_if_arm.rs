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
        span: body.span.clone(),
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
