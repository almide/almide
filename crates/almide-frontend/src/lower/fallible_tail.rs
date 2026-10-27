/// ADR-0002 Phase 1b (#1103): lift a `-> T!` fn's VALUE exits into `ok(...)`.
/// Type-driven, mirroring codegen's `wrap_tail_in_ok` discipline: an exit
/// whose IR type is already Result (Phase 1a's pass-through / `ok` / `err`
/// bodies, a call to a Result-returning fn) passes through untouched; a
/// T-typed exit wraps in `ResultOk` at `Result[T, String]`. Runs in the
/// FRONTEND lowering so all three IR consumers (codegen, mir, interp) see
/// the same lifted body.
/// The `E` of the enclosing fallible signature rides in `err_ty` — ADR-0012
/// D2 (#1193): a `-> T!E` fn's lifted `ok(...)` tail must carry
/// `Result[T, E]`, not the historical `Result[T, String]` (the codegen's
/// turbofish prints this node's type verbatim, so a String here was a rustc
/// E0308 on every typed-E fn). Bare `T!` callers pass `Ty::String`.
pub(crate) fn wrap_fallible_value_tail(body: IrExpr, err_ty: &Ty) -> IrExpr {
    let ty = body.ty.clone();
    let span = body.span.clone();
    match body.kind {
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            let stmts = stmts
                .into_iter()
                .map(|stmt| {
                    let sspan = stmt.span.clone();
                    match stmt.kind {
                        IrStmtKind::Guard { cond, else_ } if !else_.ty.is_result() => IrStmt {
                            kind: IrStmtKind::Guard { cond, else_: wrap_fallible_value_tail(else_, err_ty) },
                            span: sspan,
                        },
                        other => IrStmt { kind: other, span: sspan },
                    }
                })
                .collect();
            let wrapped = wrap_fallible_value_tail(*tail, err_ty);
            let wty = wrapped.ty.clone();
            IrExpr { kind: IrExprKind::Block { stmts, expr: Some(Box::new(wrapped)) }, ty: wty, span, def_id: None }
        }
        IrExprKind::If { cond, then, else_ } => {
            let then = Box::new(wrap_fallible_value_tail(*then, err_ty));
            let else_ = Box::new(wrap_fallible_value_tail(*else_, err_ty));
            let diverges = |e: &IrExpr| matches!(e.ty, Ty::Never) || is_panic_call(e);
            let wty = if diverges(&then) { else_.ty.clone() } else { then.ty.clone() };
            IrExpr { kind: IrExprKind::If { cond, then, else_ }, ty: wty, span, def_id: None }
        }
        IrExprKind::Match { subject, arms } => {
            let arms: Vec<IrMatchArm> = arms
                .into_iter()
                .map(|arm| IrMatchArm {
                    pattern: arm.pattern,
                    guard: arm.guard,
                    body: wrap_fallible_value_tail(arm.body, err_ty),
                })
                .collect();
            let wty = arms
                .iter()
                .find(|a| !matches!(a.body.ty, Ty::Never) && !is_panic_call(&a.body))
                .or(arms.first())
                .map(|a| a.body.ty.clone())
                .unwrap_or_else(|| Ty::result(ty, err_ty.clone()));
            IrExpr { kind: IrExprKind::Match { subject, arms }, ty: wty, span, def_id: None }
        }
        // Already Result — Phase 1a's forms stay untouched.
        IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. } => body,
        _ if ty.is_result() => body,
        // A diverging leaf (`panic(..)`) yields no value to lift; wrapping it
        // stamped `ok(panic)` at the wrong payload (the native tail wrap
        // leaves it bare for the same reason, #3118).
        _ if matches!(ty, Ty::Never) || is_panic_call(&body) => body,
        _ => {
            let result_ty = Ty::result(ty.clone(), err_ty.clone());
            IrExpr {
                kind: IrExprKind::ResultOk { expr: Box::new(IrExpr { kind: body.kind, ty, span: span.clone(), def_id: None }) },
                ty: result_ty,
                span,
                def_id: None,
            }
        }
    }
}

/// `panic(..)` as a value leaf (possibly a block's tail): it diverges, so the
/// lift leaves it bare.
fn is_panic_call(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::Call { target: almide_ir::CallTarget::Named { name }, .. } => name.as_str() == "panic",
        IrExprKind::Block { expr: Some(tail), .. } => is_panic_call(tail),
        _ => false,
    }
}
