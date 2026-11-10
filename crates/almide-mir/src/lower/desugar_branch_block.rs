// Included alongside the shared branch-desugar helpers.
/// Recurse into BOTH the block's `let`-bind STMT values AND its tail. A HOF call binding
/// `let case_lines = cases |> list.flat_map((entry) => { … let cond = if … ; … })` hides a
/// let-bound heap `if` inside the lambda arg — only reachable by descending the bind value.
/// The stmt-value recursion uses the FOCUSED `desugar_lambda_let_branches` (let-bound-branch
/// duplication ONLY, into nested if/match/block/lambda) — NOT the full `desugar_heap_branches
/// _inner` fixpoint, whose function-body-tuned passes regress an already-lowerable bind value
/// (julia `gen_variant_types`'s `let case_lines = <flat_map of match {…}+[""]>` walled when run
/// through the full fixpoint). The tail KEEPS the full fixpoint (the existing nested-arm path).
fn desugar_nested_branch_block_arm(
    body: &IrExpr,
    next_var: &mut u32,
    layouts: &crate::lower::VariantLayouts,
) -> Option<IrExpr> {
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else { return None };
    let mut changed = false;
    let new_stmts: Vec<IrStmt> = stmts
        .iter()
        .map(|s| match &s.kind {
            IrStmtKind::Bind { var, mutability, ty, value } => {
                match desugar_lambda_let_branches(value) {
                    Some(nv) => {
                        changed = true;
                        IrStmt {
                            kind: IrStmtKind::Bind {
                                var: *var,
                                mutability: *mutability,
                                ty: ty.clone(),
                                value: nv,
                            },
                            span: s.span,
                        }
                    }
                    None => s.clone(),
                }
            }
            // A conditional statement owns its branches just as a tail does.
            // Normalize inside each arm, keeping a lazy assertion message's
            // unwrap in the failing arm rather than hoisting it before the if.
            IrStmtKind::Expr { expr } if branch_stmt_arms_normalize_in_place(expr, layouts) => {
                match desugar_nested_branch_arms(expr, next_var, layouts) {
                    Some(expr) => {
                        changed = true;
                        IrStmt { kind: IrStmtKind::Expr { expr }, span: s.span }
                    }
                    None => s.clone(),
                }
            }
            _ => s.clone(),
        })
        .collect();
    let nt = tail.as_deref().and_then(|t| desugar_heap_branches_inner(t, next_var, layouts));
    if nt.is_some() {
        changed = true;
    }
    if !changed {
        return None;
    }
    Some(IrExpr {
        kind: IrExprKind::Block {
            stmts: new_stmts,
            expr: nt.map(Box::new).or_else(|| tail.clone()),
        },
        ty: body.ty.clone(),
        span: body.span,
        def_id: body.def_id,
    })
}

/// May the arms of a branch STATEMENT be normalized where they stand?
///
/// Not when an arm propagates on its statement spine (`let v = f()!`, `q = f()!`,
/// a bare `f()!`). The propagation rewrites turn such a `!` into a match whose
/// err arm is the enclosing block's VALUE — the function's result when that
/// block is the function's tail, a discarded value in a statement arm, where the
/// error would be dropped and the statements after the branch would run. Such a
/// branch is left whole: the continuation lift moves it to the tail when every
/// payload is admitted, and otherwise its `!` reaches the lowering and is refused.
fn branch_stmt_arms_normalize_in_place(expr: &IrExpr, layouts: &crate::lower::VariantLayouts) -> bool {
    if !matches!(expr.kind, IrExprKind::If { .. } | IrExprKind::Match { .. } | IrExprKind::Block { .. }) {
        return false;
    }
    let (mut propagates, mut admitted) = (false, true);
    collect_arm_unwrap_admit(expr, &mut propagates, &mut admitted, layouts);
    !propagates
}

