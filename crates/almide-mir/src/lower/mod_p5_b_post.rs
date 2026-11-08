// the TCO rewrite's post-loop dispatch and loop-state inits, include!-spliced back
// from mod_p5_b.rs at module level (a code move under the 800-line ceiling the
// codopsy gate holds this crate to).

/// The post-loop dispatch `if rk == 1 then base_1 else if … else base_N` —
/// recomputes the hit base from the carried params.
fn tco_post_dispatch(base_exprs: &[IrExpr], rk: VarId, ret_ty: &Ty) -> Option<IrExpr> {
    let mut post = base_exprs.last()?.clone();
    for (idx, base) in base_exprs.iter().enumerate().rev().skip(1) {
        post = tco_ir(
            IrExprKind::If {
                cond: Box::new(tco_eq_rk(rk, (idx + 1) as i64)),
                then: Box::new(base.clone()),
                else_: Box::new(post),
            },
            ret_ty.clone(),
        );
    }
    Some(post)
}

/// The loop-state init binds, in order: `[var idx = 0;] [var result = <empty>;]
/// var rk = 0;`. The result accumulator (when used) starts at an empty value of
/// the result type — a placeholder the first base case overwrites IN the loop;
/// declared mutable so the in-loop base assigns it.
fn tco_state_inits(
    idx_var: Option<VarId>,
    result_var: Option<VarId>,
    rk: VarId,
    ret_ty: &Ty,
) -> Vec<IrStmt> {
    let mut inits: Vec<IrStmt> = Vec::new();
    if let Some(iv) = idx_var {
        inits.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: iv,
                mutability: almide_ir::Mutability::Var,
                ty: Ty::Int,
                value: tco_ir(IrExprKind::LitInt { value: 0 }, Ty::Int),
            },
            span: None,
        });
    }
    if let Some(rv) = result_var {
        inits.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: rv,
                mutability: almide_ir::Mutability::Var,
                ty: ret_ty.clone(),
                value: tco_empty_for(ret_ty).expect("checked Some above"),
            },
            span: None,
        });
    }
    inits.push(IrStmt {
        kind: IrStmtKind::Bind {
            var: rk,
            mutability: almide_ir::Mutability::Var,
            ty: Ty::Int,
            value: tco_ir(IrExprKind::LitInt { value: 0 }, Ty::Int),
        },
        span: None,
    });
    inits
}
