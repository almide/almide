/// Detect + rewrite the LIST-ITERATOR heap-loop-carried pattern (oct_rec/bin_rec): a heap carried
/// param `cs` consumed in EVERY self-call ONLY as `list.drop(Var(cs), 1)`, with the body an outer
/// `match list.first(Var(cs)) { none => BASE, some(ch) => BODY }`. Returns the rewritten body (the
/// match → `if idx < list.len(cs) then { let ch = cs[idx]; BODY } else BASE`) + the fresh `idx`
/// VarId, and FLIPS `carried[ci]` to false (cs is now invariant — the iterator is `idx`, bumped per
/// self-call in `tco_rewrite`). `None` if the pattern does not hold. Cert-clean: the result is the
/// scalar-TCO loop over `idx` + the borrowed-stable `cs`; no heap back-edge merge.
fn try_list_iter_rewrite(
    fn_name: &str,
    body: &IrExpr,
    params: &[almide_ir::IrParam],
    fresh: u32,
) -> Option<(IrExpr, VarId, usize)> {
    // The body must be `match SUBJ { none => .., some(ch) => .. }` with SUBJ = `list.first(Var(cs))`.
    let IrExprKind::Match { subject, arms } = &body.kind else { return None };
    if arms.len() != 2 {
        return None;
    }
    let (cs_var, first_ty) = match &subject.kind {
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
            if module.as_str() == "list" && func.as_str() == "first" && args.len() == 1 =>
        {
            match &args[0].kind {
                IrExprKind::Var { id } => (*id, subject.ty.clone()),
                _ => return None,
            }
        }
        _ => return None,
    };
    // `cs` must be a param, and EVERY self-call must pass `list.drop(Var(cs), 1)` in its slot.
    let ci = params.iter().position(|p| p.var == cs_var)?;
    if !is_heap_ty(&params[ci].ty) {
        return None;
    }
    let is_drop1 = |e: &IrExpr| -> bool {
        matches!(&e.kind, IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
            if module.as_str() == "list" && func.as_str() == "drop" && args.len() == 2
                && matches!(&args[0].kind, IrExprKind::Var { id } if *id == cs_var)
                && matches!(&args[1].kind, IrExprKind::LitInt { value: 1 }))
    };
    // Collect EVERY self-call anywhere in the body (not just tail position) and require each to pass
    // `list.drop(cs,1)` in slot `ci` — so `cs` is a pure forward iterator with no other use.
    let mut ok = true;
    let mut any_self = false;
    {
        use almide_ir::visit::IrVisitor;
        struct W<'a> {
            fn_name: &'a str,
            ci: usize,
            is_drop1: &'a dyn Fn(&IrExpr) -> bool,
            ok: &'a mut bool,
            any: &'a mut bool,
        }
        impl IrVisitor for W<'_> {
            fn visit_expr(&mut self, e: &IrExpr) {
                if let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &e.kind {
                    if name.as_str() == self.fn_name {
                        *self.any = true;
                        if self.ci >= args.len() || !(self.is_drop1)(&args[self.ci]) {
                            *self.ok = false;
                        }
                    }
                }
                almide_ir::visit::walk_expr(self, e);
            }
        }
        let mut w = W { fn_name, ci, is_drop1: &is_drop1, ok: &mut ok, any: &mut any_self };
        w.visit_expr(body);
    }
    if !ok || !any_self {
        return None;
    }
    // Parse the two arms: a `None` arm (the BASE) and a `Some(ch | _)` arm (the BODY). `ch` is a
    // scalar element bind (String element) — bound to `cs[idx]` (a borrow) in the rewrite.
    use almide_ir::IrPattern;
    let mut none_body: Option<&IrExpr> = None;
    let mut some_body: Option<(&IrExpr, Option<(VarId, Ty)>)> = None;
    for arm in arms {
        if arm.guard.is_some() {
            return None;
        }
        match &arm.pattern {
            IrPattern::None | IrPattern::Wildcard if none_body.is_none() => none_body = Some(&arm.body),
            IrPattern::Some { inner } if some_body.is_none() => {
                let bind = match inner.as_ref() {
                    IrPattern::Bind { var, ty } => Some((*var, ty.clone())),
                    IrPattern::Wildcard => None,
                    _ => return None,
                };
                some_body = Some((&arm.body, bind));
            }
            _ => return None,
        }
    }
    let none_body = none_body?;
    let (some_body, ch_bind) = some_body?;
    let idx = VarId(fresh);
    let elem_ty = match &first_ty {
        Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, a) if a.len() == 1 => {
            a[0].clone()
        }
        _ => return None,
    };
    // list.len(cs): clone the `list.first` subject node + retarget to `len`, typed Int.
    let len_call = match &subject.kind {
        IrExprKind::Call { target: CallTarget::Module { module, def_id, .. }, args, type_args } => {
            tco_ir(
                IrExprKind::Call {
                    target: CallTarget::Module {
                        module: *module,
                        func: almide_lang::intern::sym("len"),
                        def_id: *def_id,
                    },
                    args: args.clone(),
                    type_args: type_args.clone(),
                },
                Ty::Int,
            )
        }
        _ => return None,
    };
    // cond: `idx < list.len(cs)`
    let cond = tco_ir(
        IrExprKind::BinOp {
            op: almide_ir::BinOp::Lt,
            left: Box::new(tco_ir(IrExprKind::Var { id: idx }, Ty::Int)),
            right: Box::new(len_call),
        },
        Ty::Bool,
    );
    // then: `{ [let ch = cs[idx]]; SOME_BODY }` — the element BORROW.
    let mut then_stmts: Vec<IrStmt> = Vec::new();
    if let Some((ch_var, ch_ty)) = ch_bind {
        let elem = tco_ir(
            IrExprKind::IndexAccess {
                object: Box::new(tco_ir(IrExprKind::Var { id: cs_var }, params[ci].ty.clone())),
                index: Box::new(tco_ir(IrExprKind::Var { id: idx }, Ty::Int)),
            },
            elem_ty,
        );
        then_stmts.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: ch_var,
                mutability: almide_ir::Mutability::Let,
                ty: ch_ty,
                value: elem,
            },
            span: None,
        });
    }
    let then_expr = tco_ir(
        IrExprKind::Block { stmts: then_stmts, expr: Some(Box::new(some_body.clone())) },
        body.ty.clone(),
    );
    let new_body = tco_ir(
        IrExprKind::If {
            cond: Box::new(cond),
            then: Box::new(then_expr),
            else_: Box::new(none_body.clone()),
        },
        body.ty.clone(),
    );
    Some((new_body, idx, ci))
}

fn loop_uw_node(kind: IrExprKind, ty: Ty) -> IrExpr {
    IrExpr { kind, ty, span: None, def_id: None }
}

/// The `err($x) => { __ef = true; __ev = $x }` arm (a fresh `$x` allocated from `nv`).
fn loop_uw_err_arm(ef: VarId, ev: VarId, err_ty: &Ty, nv: &mut u32) -> almide_ir::IrMatchArm {
    let x = VarId(*nv);
    *nv += 1;
    let set_flag = IrStmt {
        kind: IrStmtKind::Assign {
            var: ef,
            value: loop_uw_node(IrExprKind::LitBool { value: true }, Ty::Bool),
        },
        span: None,
    };
    // Store an OWNED copy (`$x ++ ""`, a fresh String) — NOT the borrowed match payload. The
    // loop-carried slot must OWN its value so the post-loop move-out is not a double-free of the
    // subject's reference; the concat allocates a fresh String, severing the borrow. This is what
    // turns the slot's ownership certificate into the PROVEN `i(id)m` loop-slot shape (storing the
    // bare borrow certifies as the unsound `idm` = init/drop/move-a-dead-ref). `err_ty` is gated
    // to `String` by the caller, so `ConcatStr` typechecks and yields the same bytes as `$x`.
    let owned = loop_uw_node(
        IrExprKind::BinOp {
            op: almide_ir::BinOp::ConcatStr,
            left: Box::new(loop_uw_node(IrExprKind::Var { id: x }, err_ty.clone())),
            right: Box::new(loop_uw_node(
                IrExprKind::LitStr { value: String::new() },
                Ty::String,
            )),
        },
        err_ty.clone(),
    );
    let set_val = IrStmt {
        kind: IrStmtKind::Assign { var: ev, value: owned },
        span: None,
    };
    almide_ir::IrMatchArm {
        pattern: almide_ir::IrPattern::Err {
            inner: Box::new(almide_ir::IrPattern::Bind { var: x, ty: err_ty.clone() }),
        },
        guard: None,
        body: loop_uw_node(
            IrExprKind::Block { stmts: vec![set_flag, set_val], expr: None },
            Ty::Unit,
        ),
    }
}

/// `let v = e!` / `Expr(e!)` whose `!` propagates `Result[_, err_ty]` → `(ok_pattern, inner)`.
fn loop_uw_unwrap_stmt(s: &IrStmt, err_ty: &Ty) -> Option<(almide_ir::IrPattern, IrExpr)> {
    use almide_lang::types::constructor::TypeConstructorId;
    let (ok_pat, inner): (almide_ir::IrPattern, IrExpr) = match &s.kind {
        IrStmtKind::Bind { var, ty, value, .. } => match &value.kind {
            IrExprKind::Unwrap { expr } => {
                (almide_ir::IrPattern::Bind { var: *var, ty: ty.clone() }, (**expr).clone())
            }
            _ => return None,
        },
        IrStmtKind::Expr { expr } => match &expr.kind {
            IrExprKind::Unwrap { expr: inner } => {
                (almide_ir::IrPattern::Wildcard, (**inner).clone())
            }
            _ => return None,
        },
        _ => return None,
    };
    match &inner.ty {
        Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 && &a[1] == err_ty => {
            Some((ok_pat, inner))
        }
        _ => None,
    }
}

/// Rewrite a UNIT-typed loop-body remainder `e`, replacing each effect-`!` with a flag-setting
/// `match`. Returns `None` (the whole desugar declines, leaving the `!` to WALL) if any `!` sits
/// in a position where a clean continuation cannot be captured.
fn loop_uw_rewrite(e: &IrExpr, ef: VarId, ev: VarId, err_ty: &Ty, nv: &mut u32) -> Option<IrExpr> {
    if !expr_has_unwrap(e) {
        return Some(e.clone());
    }
    match &e.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            // First DIRECT `let v=e!` / `Expr(e!)`: push the rest of the block into its ok-arm.
            for (i, s) in stmts.iter().enumerate() {
                if let Some((ok_pat, inner)) = loop_uw_unwrap_stmt(s, err_ty) {
                    // Everything BEFORE the `!` must be `!`-free (else its continuation is wrong).
                    if stmts[..i].iter().any(stmt_has_unwrap) {
                        return None;
                    }
                    let rest = loop_uw_node(
                        IrExprKind::Block { stmts: stmts[i + 1..].to_vec(), expr: tail.clone() },
                        Ty::Unit,
                    );
                    let rest2 = loop_uw_rewrite(&rest, ef, ev, err_ty, nv)?;
                    let ok_arm = almide_ir::IrMatchArm {
                        pattern: almide_ir::IrPattern::Ok { inner: Box::new(ok_pat) },
                        guard: None,
                        body: rest2,
                    };
                    let err_arm = loop_uw_err_arm(ef, ev, err_ty, nv);
                    let m = loop_uw_node(
                        IrExprKind::Match { subject: Box::new(inner), arms: vec![ok_arm, err_arm] },
                        Ty::Unit,
                    );
                    return Some(loop_uw_node(
                        IrExprKind::Block { stmts: stmts[..i].to_vec(), expr: Some(Box::new(m)) },
                        Ty::Unit,
                    ));
                }
            }
            // No direct `!` stmt: the `!` is nested in a TERMINAL `if`/`match` (the tail, or the
            // last stmt) — recurse into it. Everything else must be `!`-free.
            if let Some(t) = tail {
                if stmts.iter().all(|s| !stmt_has_unwrap(s)) {
                    let nt = loop_uw_rewrite(t, ef, ev, err_ty, nv)?;
                    return Some(loop_uw_node(
                        IrExprKind::Block { stmts: stmts.clone(), expr: Some(Box::new(nt)) },
                        Ty::Unit,
                    ));
                }
                return None;
            }
            // No tail: the unwrap must be in the LAST stmt (an `Expr(if/match)`), rest `!`-free.
            let last = stmts.len().checked_sub(1)?;
            if stmts[..last].iter().any(stmt_has_unwrap) {
                return None;
            }
            if let IrStmtKind::Expr { expr } = &stmts[last].kind {
                let ne = loop_uw_rewrite(expr, ef, ev, err_ty, nv)?;
                let mut ns = stmts[..last].to_vec();
                ns.push(IrStmt { kind: IrStmtKind::Expr { expr: ne }, span: stmts[last].span.clone() });
                return Some(loop_uw_node(
                    IrExprKind::Block { stmts: ns, expr: None },
                    Ty::Unit,
                ));
            }
            None
        }
        IrExprKind::If { cond, then, else_ } => {
            if expr_has_unwrap(cond) {
                return None;
            }
            let nt = loop_uw_rewrite(then, ef, ev, err_ty, nv)?;
            let ne = loop_uw_rewrite(else_, ef, ev, err_ty, nv)?;
            Some(loop_uw_node(
                IrExprKind::If { cond: cond.clone(), then: Box::new(nt), else_: Box::new(ne) },
                e.ty.clone(),
            ))
        }
        IrExprKind::Match { subject, arms } => {
            if expr_has_unwrap(subject) {
                return None;
            }
            let mut new_arms = Vec::with_capacity(arms.len());
            for a in arms {
                if a.guard.as_ref().is_some_and(expr_has_unwrap) {
                    return None;
                }
                let nb = loop_uw_rewrite(&a.body, ef, ev, err_ty, nv)?;
                new_arms.push(almide_ir::IrMatchArm {
                    pattern: a.pattern.clone(),
                    guard: a.guard.clone(),
                    body: nb,
                });
            }
            Some(loop_uw_node(
                IrExprKind::Match { subject: subject.clone(), arms: new_arms },
                e.ty.clone(),
            ))
        }
        // A bare trailing `e!` (Unit-typed): `match e { ok(_) => (), err($x) => { flag } }`.
        IrExprKind::Unwrap { expr } => {
            use almide_lang::types::constructor::TypeConstructorId;
            if expr_has_unwrap(expr) {
                return None;
            }
            match &expr.ty {
                Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 && &a[1] == err_ty => {}
                _ => return None,
            }
            let ok_arm = almide_ir::IrMatchArm {
                pattern: almide_ir::IrPattern::Ok {
                    inner: Box::new(almide_ir::IrPattern::Wildcard),
                },
                guard: None,
                body: loop_uw_node(IrExprKind::Unit, Ty::Unit),
            };
            let err_arm = loop_uw_err_arm(ef, ev, err_ty, nv);
            Some(loop_uw_node(
                IrExprKind::Match { subject: expr.clone(), arms: vec![ok_arm, err_arm] },
                Ty::Unit,
            ))
        }
        // An `!` in a kind we do not rewrite — decline (fail-safe wall).
        _ => None,
    }
}

/// See the module comment above: rewrite the FIRST `for` loop (in a `Result[T, E]`-returning block)
/// whose body contains an effect-`!` into the loop-carried error-flag form.
pub fn desugar_loop_unwrap(body: &IrExpr, next_var: &mut u32) -> Option<IrExpr> {
    use almide_lang::types::constructor::TypeConstructorId;
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        return None;
    };
    // The enclosing result must be `Result[T, E]`. `E` is gated to `String`: the accumulator's
    // owned-copy (`$x ++ ""`, see `loop_uw_err_arm`) and `""` seed are String-specific, and a
    // String error is the effect-fn norm (it covers every porta wall). A non-String `E` declines
    // (the `!` is left to WALL — never a silent miscompile).
    let err_ty = match &body.ty {
        Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 && matches!(a[1], Ty::String) => {
            a[1].clone()
        }
        _ => return None,
    };
    let empty_err = tco_empty_for(&err_ty)?;
    // The FIRST `for` loop whose body holds an `!`. (`while` is excluded — see the module comment.)
    let loop_idx = stmts.iter().position(|s| match &s.kind {
        IrStmtKind::Expr { expr } => matches!(
            &expr.kind,
            IrExprKind::ForIn { body: lbody, .. } if lbody.iter().any(stmt_has_unwrap)
        ),
        _ => false,
    })?;
    let IrStmtKind::Expr { expr: loop_expr } = &stmts[loop_idx].kind else {
        return None;
    };
    let IrExprKind::ForIn { var, var_tuple, iterable, body: lbody } = &loop_expr.kind else {
        return None;
    };
    let ef = VarId(*next_var);
    let ev = VarId(*next_var + 1);
    *next_var += 2;
    // Rewrite the loop body's `!`s (declining the whole pass if any cannot be cleanly placed).
    let body_block =
        loop_uw_node(IrExprKind::Block { stmts: lbody.clone(), expr: None }, Ty::Unit);
    let rewritten = loop_uw_rewrite(&body_block, ef, ev, &err_ty, next_var)?;
    // Guard the iteration: `if not __ef then { <rewritten> } else ()`.
    let not_ef = loop_uw_node(
        IrExprKind::UnOp {
            op: almide_ir::UnOp::Not,
            operand: Box::new(loop_uw_node(IrExprKind::Var { id: ef }, Ty::Bool)),
        },
        Ty::Bool,
    );
    let guard_if = loop_uw_node(
        IrExprKind::If {
            cond: Box::new(not_ef),
            then: Box::new(rewritten),
            else_: Box::new(loop_uw_node(IrExprKind::Unit, Ty::Unit)),
        },
        Ty::Unit,
    );
    let new_loop = loop_uw_node(
        IrExprKind::ForIn {
            var: *var,
            var_tuple: var_tuple.clone(),
            iterable: iterable.clone(),
            body: vec![IrStmt { kind: IrStmtKind::Expr { expr: guard_if }, span: None }],
        },
        Ty::Unit,
    );
    // `<stmts before loop>; var __ef=false; var __ev=<empty>; <new_loop>`.
    let mut new_stmts: Vec<IrStmt> = stmts[..loop_idx].to_vec();
    new_stmts.push(IrStmt {
        kind: IrStmtKind::Bind {
            var: ef,
            mutability: almide_ir::Mutability::Var,
            ty: Ty::Bool,
            value: loop_uw_node(IrExprKind::LitBool { value: false }, Ty::Bool),
        },
        span: None,
    });
    new_stmts.push(IrStmt {
        kind: IrStmtKind::Bind {
            var: ev,
            mutability: almide_ir::Mutability::Var,
            ty: err_ty.clone(),
            value: empty_err,
        },
        span: None,
    });
    new_stmts.push(IrStmt { kind: IrStmtKind::Expr { expr: new_loop }, span: None });
    // Post-loop dispatch: `if __ef then err(__ev) else { <post-stmts>; <orig tail> }`.
    let post = loop_uw_node(
        IrExprKind::Block { stmts: stmts[loop_idx + 1..].to_vec(), expr: tail.clone() },
        body.ty.clone(),
    );
    let err_result = loop_uw_node(
        IrExprKind::ResultErr {
            expr: Box::new(loop_uw_node(IrExprKind::Var { id: ev }, err_ty.clone())),
        },
        body.ty.clone(),
    );
    let new_tail = loop_uw_node(
        IrExprKind::If {
            cond: Box::new(loop_uw_node(IrExprKind::Var { id: ef }, Ty::Bool)),
            then: Box::new(err_result),
            else_: Box::new(post),
        },
        body.ty.clone(),
    );
    Some(loop_uw_node(
        IrExprKind::Block { stmts: new_stmts, expr: Some(Box::new(new_tail)) },
        body.ty.clone(),
    ))
}

