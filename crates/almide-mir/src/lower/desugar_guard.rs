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

/// TAIL ERR-RAISE IF → BIND-POSITION UNWRAP (a pre-lowering program pass, shared
/// chain like [`desugar_fn_body_guards`], which feeds it: a fn-body guard whose
/// else is `err(x)!` restructures into exactly this shape). A SCALAR-tail `if`
/// whose one arm RAISES (`if c then a / b else err("…")!` — the `Unwrap` of an
/// always-Err Result) cannot lower on the scalar tail path (no early return).
/// But the SAME semantics in bind position is the machinery the `!` desugars
/// already prove end-to-end (the lifted-Result materialization):
///
///   { …; if c then T else err(e)! }         (T scalar, fn can-err)
///     ≡  { …; let $g = (if c then ok(T) else err(e))!; $g }
///
/// — the Result-typed `if` materializes via the heap-result-if arms, and the
/// bind-position `!` propagates the Err exactly as the caller observes on v0.
/// Both orientations (raise in then / raise in else) normalize. Only the
/// FN-BODY tail chain is rewritten (same scope as the guard pass); no calls
/// are added or removed, so the caps `mir == ir` invariant holds.
pub fn normalize_tail_err_raise_ifs(program: &mut almide_ir::IrProgram) {
    use almide_ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind, Mutability, VarTable};
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;

    fn is_scalar_value_ty(ty: &Ty) -> bool {
        // Scalar payloads AND String: both have a proven bind-position `!` unwrap
        // (`let $g: String = $r!` is the fs.read_text class), so both normalize.
        matches!(ty, Ty::Int | Ty::Float | Ty::Bool | Ty::String)
    }
    /// The raising arm's inner Result expr (`err(e)` out of `err(e)!`), if this
    /// arm IS an err-raise: an `Unwrap` whose inner is a `ResultErr` ctor.
    fn err_raise_inner(e: &IrExpr) -> Option<&IrExpr> {
        let IrExprKind::Unwrap { expr } = &e.kind else { return None };
        matches!(expr.kind, IrExprKind::ResultErr { .. }).then_some(expr.as_ref())
    }

    fn rewrite_tail(e: &mut IrExpr, vt: &mut VarTable) {
        match &mut e.kind {
            IrExprKind::Block { stmts, expr: Some(t) } => {
                rewrite_tail(t, vt);
                // FLATTEN a Block-valued tail into THIS block (its statements run
                // unconditionally before the tail value; VarIds are unique, and the
                // lowering already rides nested-block locals to the enclosing scope —
                // the same conservative lifetime extension). This puts the
                // normalization's `let $r = …; let $g = $r!` pair on the fn-body TOP
                // block, the only statement list `desugar_let_unwrap` scans.
                if let IrExprKind::Block { stmts: inner, expr: Some(iv) } = &mut t.kind {
                    if !inner.is_empty() {
                        stmts.extend(inner.drain(..));
                        let v = (**iv).clone();
                        **t = v;
                    }
                }
            }
            IrExprKind::If { .. } => {
                // Fold the WHOLE guard if-CHAIN (any nesting of raise arms and one
                // value-ty leaf class) into ONE Result-typed if tree: every VALUE
                // leaf wraps in ok(…), every RAISE leaf sheds its `!` — so a chained
                // guard (`validate_age`'s two guards) normalizes to a single bind +
                // unwrap instead of nesting ok() around inner binds (which no
                // lowering path executes). `classify_chain` returns the uniform
                // value-leaf ty and the raise arms' Err ty, or None outside the
                // subset (a leaf that is neither).
                // Look through the empty-Block wrappers the guard restructure leaves
                // around each continuation (`if c then { <inner if> } else E`).
                fn peel(e: &IrExpr) -> &IrExpr {
                    match &e.kind {
                        IrExprKind::Block { stmts, expr: Some(t) } if stmts.is_empty() => peel(t),
                        _ => e,
                    }
                }
                fn classify_chain(e: &IrExpr) -> Option<(Ty, Option<Ty>)> {
                    let e = peel(e);
                    if let Some(inner) = err_raise_inner(e) {
                        let Ty::Applied(TypeConstructorId::Result, a) = &inner.ty else {
                            return None;
                        };
                        if a.len() != 2 {
                            return None;
                        }
                        // A raise leaf: no value ty contributed; err ty named.
                        return Some((Ty::Unknown, Some(a[1].clone())));
                    }
                    if let IrExprKind::If { then, else_, .. } = &e.kind {
                        let (t_val, t_err) = classify_chain(then)?;
                        let (e_val, e_err) = classify_chain(else_)?;
                        let val = match (&t_val, &e_val) {
                            (Ty::Unknown, v) | (v, Ty::Unknown) => (*v).clone(),
                            (a, b) if a == b => (*a).clone(),
                            _ => return None,
                        };
                        return Some((val, t_err.or(e_err)));
                    }
                    Some((e.ty.clone(), None))
                }
                let Some((value_arm_ty, Some(err_ty))) = classify_chain(e) else { return };
                if !is_scalar_value_ty(&value_arm_ty) {
                    return;
                }
                let result_ty = Ty::result(value_arm_ty.clone(), err_ty);
                // Transform the tree: value leaves → ok(leaf); raise leaves → inner
                // err(…) retyped; nested ifs keep structure at the Result ty.
                fn to_result_tree(e: &IrExpr, result_ty: &Ty) -> IrExpr {
                    let e = peel(e);
                    if let Some(inner) = err_raise_inner(e) {
                        return IrExpr { ty: result_ty.clone(), ..inner.clone() };
                    }
                    if let IrExprKind::If { cond, then, else_ } = &e.kind {
                        return IrExpr {
                            kind: IrExprKind::If {
                                cond: cond.clone(),
                                then: Box::new(to_result_tree(then, result_ty)),
                                else_: Box::new(to_result_tree(else_, result_ty)),
                            },
                            ty: result_ty.clone(),
                            span: e.span.clone(),
                            def_id: None,
                        };
                    }
                    IrExpr {
                        kind: IrExprKind::ResultOk { expr: Box::new(e.clone()) },
                        ty: result_ty.clone(),
                        span: e.span.clone(),
                        def_id: None,
                    }
                }
                let (new_then, new_else) = {
                    let IrExprKind::If { then, else_, .. } = &e.kind else { unreachable!() };
                    (to_result_tree(then, &result_ty), to_result_tree(else_, &result_ty))
                };
                // Two-step bind: the Result-if materializes into a TRACKED var first
                // (the heap-result-if BIND machinery seeds its match shape), then the
                // bind-position `!` unwraps THAT var — the exact subject class the
                // effect-unwrap desugar already proves (`let x = int.parse(s)!`).
                let r = vt.alloc(
                    almide_lang::intern::sym("__guard_res"),
                    result_ty.clone(),
                    Mutability::Let,
                    None,
                );
                let g = vt.alloc(
                    almide_lang::intern::sym("__guard_ok"),
                    value_arm_ty.clone(),
                    Mutability::Let,
                    None,
                );
                let result_if = IrExpr {
                    kind: IrExprKind::If {
                        cond: match &e.kind {
                            IrExprKind::If { cond, .. } => cond.clone(),
                            _ => unreachable!(),
                        },
                        then: Box::new(new_then),
                        else_: Box::new(new_else),
                    },
                    ty: result_ty.clone(),
                    span: e.span.clone(),
                    def_id: None,
                };
                let bind_r = IrStmt {
                    kind: IrStmtKind::Bind {
                        var: r,
                        ty: result_ty.clone(),
                        value: result_if,
                        mutability: Mutability::Let,
                    },
                    span: None,
                };
                let r_ref = IrExpr {
                    kind: IrExprKind::Var { id: r },
                    ty: result_ty,
                    span: None,
                    def_id: None,
                };
                let bind_g = IrStmt {
                    kind: IrStmtKind::Bind {
                        var: g,
                        ty: value_arm_ty.clone(),
                        value: IrExpr {
                            kind: IrExprKind::Unwrap { expr: Box::new(r_ref) },
                            ty: value_arm_ty.clone(),
                            span: e.span.clone(),
                            def_id: None,
                        },
                        mutability: Mutability::Let,
                    },
                    span: None,
                };
                let g_ref = IrExpr {
                    kind: IrExprKind::Var { id: g },
                    ty: value_arm_ty,
                    span: None,
                    def_id: None,
                };
                *e = IrExpr {
                    kind: IrExprKind::Block {
                        stmts: vec![bind_r, bind_g],
                        expr: Some(Box::new(g_ref)),
                    },
                    ty: e.ty.clone(),
                    span: e.span.clone(),
                    def_id: e.def_id,
                };
            }
            _ => {}
        }
    }

    let almide_ir::IrProgram { functions, modules, var_table, .. } = program;
    for func in functions
        .iter_mut()
        .chain(modules.iter_mut().flat_map(|m| m.functions.iter_mut()))
    {
        rewrite_tail(&mut func.body, var_table);
    }
}
