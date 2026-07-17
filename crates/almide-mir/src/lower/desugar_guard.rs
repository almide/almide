/// GUARD → IF RESTRUCTURE (a pre-lowering program pass, desugar-before-both): a
/// `guard cond else E` in a function-body BLOCK is a conditional early return —
/// when `!cond`, `E` is the function's result; otherwise execution continues.
/// v1 has no early-return control flow, so the Guard statement WALLED both legs
/// (deferring it silently miscompiles the `!cond` path). But at the FUNCTION-BODY
/// tail chain the early return IS expressible without any new control flow:
///
///   { s1; …; guard c else E; rest…; tail }
///     ≡  { s1; …; if c then { rest…; tail } else E }
///
/// — exact because the continuation of a fn-body-level guard is precisely the
/// function tail. The rewrite recurses into the `then` continuation (a later
/// guard is again at the fn tail) but NOT into nested blocks/ifs/loops: a guard
/// whose continuation is not the fn tail keeps the honest wall. Running ONCE on
/// the linked program in the SAME post-link fixup chain the pipeline and the
/// classify counter share keeps the caps `mir == ir` invariant (no calls are
/// added or removed — the exprs only move).
pub fn desugar_fn_body_guards(program: &mut almide_ir::IrProgram) {
    use almide_ir::{IrExpr, IrExprKind, IrStmtKind};

    /// Restructure the guards of ONE fn-tail block expr, recursively (the `then`
    /// continuation stays fn-tail). Non-Block bodies (a bare tail expr) have no
    /// statements, hence no guards — returned unchanged.
    fn rewrite_tail_block(e: IrExpr) -> IrExpr {
        let IrExpr { kind, ty, span, def_id } = e;
        let IrExprKind::Block { stmts, expr } = kind else {
            return IrExpr { kind, ty, span, def_id };
        };
        let mut before = Vec::with_capacity(stmts.len());
        let mut iter = stmts.into_iter();
        while let Some(stmt) = iter.next() {
            if let IrStmtKind::Guard { cond, else_ } = stmt.kind {
                let rest: Vec<_> = iter.collect();
                let cont = IrExpr {
                    kind: IrExprKind::Block { stmts: rest, expr },
                    ty: ty.clone(),
                    span: span.clone(),
                    def_id,
                };
                let if_expr = IrExpr {
                    kind: IrExprKind::If {
                        cond: Box::new(cond),
                        then: Box::new(rewrite_tail_block(cont)),
                        else_: Box::new(else_),
                    },
                    ty: ty.clone(),
                    span: span.clone(),
                    def_id,
                };
                return IrExpr {
                    kind: IrExprKind::Block { stmts: before, expr: Some(Box::new(if_expr)) },
                    ty,
                    span,
                    def_id,
                };
            }
            before.push(stmt);
        }
        IrExpr { kind: IrExprKind::Block { stmts: before, expr }, ty, span, def_id }
    }

    let unit = IrExpr {
        kind: IrExprKind::Unit,
        ty: almide_lang::types::Ty::Unit,
        span: None,
        def_id: None,
    };
    for func in program
        .functions
        .iter_mut()
        .chain(program.modules.iter_mut().flat_map(|m| m.functions.iter_mut()))
    {
        let body = std::mem::replace(&mut func.body, unit.clone());
        func.body = rewrite_tail_block(body);
    }
}
