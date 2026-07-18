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

/// ARG-POSITION BLOCK HOIST (a pre-lowering program pass, shared chain like the
/// guard passes above): a BLOCK expression as a call argument
/// (`int.abs({ let a = -5; let b = -3; a + b })` — scope_test's "block expression
/// as argument") has no faithful lowering as an operand. But the block can ABSORB
/// the call:
///
///   f(p…, { stmts; e }, q…)  ≡  { stmts; f(p…, e, q…) }
///
/// — exact when every argument BEFORE the block is effect-free (a Var/literal;
/// their evaluation now happens after `stmts`, which is unobservable for pure
/// operands; arguments AFTER the block already evaluated after `stmts`). One
/// block argument per call (two block args would interleave their stmts). No
/// calls are added or removed — the caps `mir == ir` invariant holds.
pub fn hoist_block_call_args(program: &mut almide_ir::IrProgram) {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    use almide_ir::{IrExpr, IrExprKind};

    fn is_pure_operand(e: &IrExpr) -> bool {
        matches!(
            e.kind,
            IrExprKind::Var { .. }
                | IrExprKind::LitInt { .. }
                | IrExprKind::LitFloat { .. }
                | IrExprKind::LitBool { .. }
                | IrExprKind::LitStr { .. }
                | IrExprKind::Unit
        )
    }

    struct H;
    impl IrMutVisitor for H {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Call { args, .. } = &mut e.kind else { return };
            // Exactly ONE non-empty Block argument, every earlier arg pure.
            let blocks: Vec<usize> = args
                .iter()
                .enumerate()
                .filter(|(_, a)| {
                    matches!(&a.kind,
                        IrExprKind::Block { stmts, expr: Some(_) } if !stmts.is_empty())
                })
                .map(|(i, _)| i)
                .collect();
            let [bi] = blocks.as_slice() else { return };
            let bi = *bi;
            if !args[..bi].iter().all(is_pure_operand) {
                return;
            }
            let IrExprKind::Block { stmts, expr: Some(tail) } = &mut args[bi].kind else {
                return;
            };
            let hoisted = std::mem::take(stmts);
            let tail = (**tail).clone();
            args[bi] = tail;
            let call = std::mem::replace(
                e,
                IrExpr {
                    kind: IrExprKind::Unit,
                    ty: almide_lang::types::Ty::Unit,
                    span: None,
                    def_id: None,
                },
            );
            *e = IrExpr {
                ty: call.ty.clone(),
                span: call.span.clone(),
                def_id: call.def_id,
                kind: IrExprKind::Block { stmts: hoisted, expr: Some(Box::new(call)) },
            };
        }
    }
    // Pass 2 — STATEMENT-level interp-part hoist: a Block part inside a bind's
    // StringInterp (`let s = "r: ${int.to_string({ let x = 10; x * 2 })}"` — the
    // expr-level hoist above already absorbed the call into the block) splices its
    // statements BEFORE the bind, leaving the tail as the part — the concat-tree
    // interp lowering then sees only plain operands. Sound when every EARLIER Expr
    // part is pure (a literal/Var); parts after the block already evaluate after it.
    fn hoist_in_stmts(stmts: &mut Vec<almide_ir::IrStmt>) {
        use almide_ir::{IrExprKind, IrStmtKind, IrStringPart};
        let mut i = 0;
        while i < stmts.len() {
            let mut hoisted: Vec<almide_ir::IrStmt> = Vec::new();
            if let IrStmtKind::Bind { value, .. } = &mut stmts[i].kind {
                if let IrExprKind::StringInterp { parts } = &mut value.kind {
                    let mut earlier_pure = true;
                    for part in parts.iter_mut() {
                        let IrStringPart::Expr { expr } = part else { continue };
                        if let IrExprKind::Block { stmts: inner, expr: Some(tail) } = &mut expr.kind
                        {
                            if earlier_pure && !inner.is_empty() {
                                hoisted = std::mem::take(inner);
                                let t = (**tail).clone();
                                *expr = t;
                            }
                            break;
                        }
                        if !is_pure_operand(expr) {
                            earlier_pure = false;
                        }
                    }
                }
            }
            if hoisted.is_empty() {
                i += 1;
            } else {
                for (k, s) in hoisted.into_iter().enumerate() {
                    stmts.insert(i + k, s);
                }
                // Re-examine the same bind: another block part may remain.
            }
        }
    }
    struct S2;
    impl IrMutVisitor for S2 {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            if let IrExprKind::Block { stmts, .. } = &mut e.kind {
                hoist_in_stmts(stmts);
            }
        }
    }
    for func in program
        .functions
        .iter_mut()
        .chain(program.modules.iter_mut().flat_map(|m| m.functions.iter_mut()))
    {
        H.visit_expr_mut(&mut func.body);
        S2.visit_expr_mut(&mut func.body);
    }
}

/// LOOP EARLY-RETURN desugar (a pre-lowering program pass, shared chain like the
/// guard passes above): a `guard c else E` INSIDE a `while` body where `E` has the
/// FUNCTION's Result type is a function-level early return from the loop — v1 has
/// no mid-function Return, so it walls (the walled-real baseline 6). Rewrite it to
/// the FLAG+RESULT form the proven machinery already runs:
///
///   var __lr_set = false
///   var __lr_val: Ret = err("")               // seed, only read when set
///   while (__lr_set == false) and cond {
///     …; if c then { <rest of body> }
///        else { __lr_val = E; __lr_set = true }
///   }
///   if __lr_set then __lr_val else { <post>; <tail> }
///
/// The guard's containing while AND every ANCESTOR while get the flag conjunct, so
/// a nested-loop early return (first_duplicate) unwinds both levels. SOUNDNESS
/// GUARDS (decline = keep the honest wall): the fn returns `Result[_, _]`; every
/// statement that can still run after the flag is set (the stmts AFTER a nested
/// while inside an outer body) is call-free (pure Assign/arith — running them once
/// more is unobservable); the guard's `E` type equals the fn's Result type.
pub fn desugar_loop_early_returns(program: &mut almide_ir::IrProgram) {
    use almide_ir::{BinOp, IrExpr, IrExprKind, IrStmt, IrStmtKind, Mutability, VarTable};
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;

    fn contains_call(e: &IrExpr) -> bool {
        let mut found = false;
        struct C<'a> {
            found: &'a mut bool,
        }
        impl<'a> almide_ir::visit::IrVisitor for C<'a> {
            fn visit_expr(&mut self, e: &IrExpr) {
                if matches!(e.kind, IrExprKind::Call { .. }) {
                    *self.found = true;
                }
                almide_ir::visit::walk_expr(self, e);
            }
        }
        almide_ir::visit::IrVisitor::visit_expr(&mut C { found: &mut found }, e);
        found
    }
    fn stmt_has_call(s: &IrStmt) -> bool {
        match &s.kind {
            IrStmtKind::Bind { value, .. }
            | IrStmtKind::Assign { value, .. }
            | IrStmtKind::Expr { expr: value } => contains_call(value),
            _ => true, // anything unusual — decline conservatively
        }
    }

    /// Rewrite the guard inside `body` (recursing into nested whiles). Returns
    /// true iff a guard was rewritten somewhere below; conjoins the flag onto
    /// every while on the path. `set`/`val` are the flag/result VarIds.
    fn rewrite_body(
        body: &mut Vec<IrStmt>,
        ret_ty: &Ty,
        set: almide_ir::VarId,
        val: almide_ir::VarId,
    ) -> Option<bool> {
        // find a top-level guard with E : ret_ty
        let gpos = body.iter().position(|s| matches!(&s.kind,
            IrStmtKind::Guard { else_, .. } if else_.ty == *ret_ty && !contains_call(else_)));
        if let Some(gi) = gpos {
            let rest: Vec<IrStmt> = body.split_off(gi + 1);
            let Some(IrStmt { kind: IrStmtKind::Guard { cond, else_ }, span }) = body.pop().map(|s| s)
            else {
                unreachable!()
            };
            let set_stmts = vec![
                IrStmt {
                    kind: IrStmtKind::Assign { var: val, value: else_ },
                    span: None,
                },
                IrStmt {
                    kind: IrStmtKind::Assign {
                        var: set,
                        value: IrExpr {
                            kind: IrExprKind::LitBool { value: true },
                            ty: Ty::Bool,
                            span: None,
                            def_id: None,
                        },
                    },
                    span: None,
                },
            ];
            let mk_block = |stmts: Vec<IrStmt>| IrExpr {
                kind: IrExprKind::Block {
                    stmts,
                    expr: Some(Box::new(IrExpr {
                        kind: IrExprKind::Unit,
                        ty: Ty::Unit,
                        span: None,
                        def_id: None,
                    })),
                },
                ty: Ty::Unit,
                span: None,
                def_id: None,
            };
            body.push(IrStmt {
                kind: IrStmtKind::Expr {
                    expr: IrExpr {
                        kind: IrExprKind::If {
                            cond: Box::new(cond),
                            then: Box::new(mk_block(rest)),
                            else_: Box::new(mk_block(set_stmts)),
                        },
                        ty: Ty::Unit,
                        span: None,
                        def_id: None,
                    },
                },
                span,
            });
            return Some(true);
        }
        // recurse into ONE nested while (the first that rewrites)
        for wi in 0..body.len() {
            let IrStmtKind::Expr { expr } = &mut body[wi].kind else { continue };
            let IrExprKind::While { cond, body: inner } = &mut expr.kind else { continue };
            if let Some(true) = rewrite_body(inner, ret_ty, set, val) {
                conjoin_flag(cond, set);
                // Every stmt after the nested while may run ONCE MORE with the
                // flag set — decline unless all are call-free (pure updates).
                if body[wi + 1..].iter().any(stmt_has_call) {
                    return None;
                }
                return Some(true);
            }
        }
        Some(false)
    }

    fn conjoin_flag(cond: &mut Box<IrExpr>, set: almide_ir::VarId) {
        let not_set = IrExpr {
            kind: IrExprKind::BinOp {
                op: BinOp::Eq,
                left: Box::new(IrExpr {
                    kind: IrExprKind::Var { id: set },
                    ty: Ty::Bool,
                    span: None,
                    def_id: None,
                }),
                right: Box::new(IrExpr {
                    kind: IrExprKind::LitBool { value: false },
                    ty: Ty::Bool,
                    span: None,
                    def_id: None,
                }),
            },
            ty: Ty::Bool,
            span: None,
            def_id: None,
        };
        let old = std::mem::replace(
            cond,
            Box::new(IrExpr {
                kind: IrExprKind::Unit,
                ty: Ty::Unit,
                span: None,
                def_id: None,
            }),
        );
        *cond = Box::new(IrExpr {
            kind: IrExprKind::BinOp { op: BinOp::And, left: Box::new(not_set), right: old },
            ty: Ty::Bool,
            span: None,
            def_id: None,
        });
    }

    fn rewrite_fn(body: &mut IrExpr, ret_ty: &Ty, vt: &mut VarTable) {
        // fn Result type with a String err — the err("") seed is exact.
        let is_res_str = matches!(ret_ty,
            Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 && matches!(a[1], Ty::String));
        if !is_res_str {
            return;
        }
        let IrExprKind::Block { stmts, expr: tail } = &mut body.kind else { return };
        let Some(tail_e) = tail.as_deref_mut() else { return };
        for wi in 0..stmts.len() {
            let IrStmtKind::Expr { expr } = &stmts[wi].kind else { continue };
            let IrExprKind::While { .. } = &expr.kind else { continue };
            let set = vt.alloc(
                almide_lang::intern::sym("__lr_set"),
                Ty::Bool,
                Mutability::Var,
                None,
            );
            let val = vt.alloc(
                almide_lang::intern::sym("__lr_val"),
                ret_ty.clone(),
                Mutability::Var,
                None,
            );
            // try the rewrite on a CLONE — commit only on success.
            let IrStmtKind::Expr { expr } = &mut stmts[wi].kind else { unreachable!() };
            let IrExprKind::While { cond, body: wbody } = &mut expr.kind else { unreachable!() };
            let mut wb = wbody.clone();
            match rewrite_body(&mut wb, ret_ty, set, val) {
                Some(true) => {
                    *wbody = wb;
                    conjoin_flag(cond, set);
                }
                _ => continue,
            }
            // binds BEFORE the while
            let seed = IrExpr {
                kind: IrExprKind::ResultErr {
                    expr: Box::new(IrExpr {
                        kind: IrExprKind::LitStr { value: String::new() },
                        ty: Ty::String,
                        span: None,
                        def_id: None,
                    }),
                },
                ty: ret_ty.clone(),
                span: None,
                def_id: None,
            };
            stmts.insert(
                wi,
                IrStmt {
                    kind: IrStmtKind::Bind {
                        var: set,
                        mutability: Mutability::Var,
                        ty: Ty::Bool,
                        value: IrExpr {
                            kind: IrExprKind::LitBool { value: false },
                            ty: Ty::Bool,
                            span: None,
                            def_id: None,
                        },
                    },
                    span: None,
                },
            );
            stmts.insert(
                wi + 1,
                IrStmt {
                    kind: IrStmtKind::Bind {
                        var: val,
                        mutability: Mutability::Var,
                        ty: ret_ty.clone(),
                        value: seed,
                    },
                    span: None,
                },
            );
            // post-loop continuation → the else of the flag dispatch
            let post: Vec<IrStmt> = stmts.split_off(wi + 3);
            let old_tail = tail_e.clone();
            let else_block = if post.is_empty() {
                old_tail
            } else {
                IrExpr {
                    kind: IrExprKind::Block { stmts: post, expr: Some(Box::new(old_tail)) },
                    ty: ret_ty.clone(),
                    span: None,
                    def_id: None,
                }
            };
            *tail_e = IrExpr {
                kind: IrExprKind::If {
                    cond: Box::new(IrExpr {
                        kind: IrExprKind::Var { id: set },
                        ty: Ty::Bool,
                        span: None,
                        def_id: None,
                    }),
                    then: Box::new(IrExpr {
                        kind: IrExprKind::Var { id: val },
                        ty: ret_ty.clone(),
                        span: None,
                        def_id: None,
                    }),
                    else_: Box::new(else_block),
                },
                ty: ret_ty.clone(),
                span: None,
                def_id: None,
            };
            return; // one loop per fn (the corpus shape); later loops keep the wall
        }
    }

    let almide_ir::IrProgram { functions, modules, var_table, .. } = program;
    for func in functions
        .iter_mut()
        .chain(modules.iter_mut().flat_map(|m| m.functions.iter_mut()))
    {
        let ret_ty = func.ret_ty.clone();
        rewrite_fn(&mut func.body, &ret_ty, var_table);
    }
}

/// SPREAD-BASE HOIST (a pre-lowering program pass, shared chain like the passes
/// above): a record spread whose BASE is a fn CALL (`let c = { ...toplib.mk(),
/// name: "w" }` — #502) had no faithful inline lowering: the strict path emitted
/// the callee as a dst-less bare call (its i32 result REMAINED ON THE WASM STACK
/// — invalid wasm) and deferred `c` to an Opaque. Hoist the base to its own bind
/// — `let __sb = toplib.mk(); let c = { ...__sb, … }` — so the call result is a
/// MATERIALIZED record (the binds_p2 aggregate seeding) and the spread takes the
/// proven spread-of-var path. Bind/Assign statement positions, every block depth;
/// call-count-invariant (the call node MOVES, never duplicates).
pub fn hoist_spread_call_bases(program: &mut almide_ir::IrProgram) {
    use almide_ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind, Mutability, VarTable};

    fn rewrite_block(stmts: &mut Vec<IrStmt>, vt: &mut VarTable) {
        let mut i = 0;
        while i < stmts.len() {
            let hoist = match &mut stmts[i].kind {
                IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => {
                    // recurse into nested blocks first
                    rewrite_expr(value, vt);
                    if let IrExprKind::SpreadRecord { base, .. } = &mut value.kind {
                        if matches!(base.kind, IrExprKind::Call { .. }) {
                            let bty = base.ty.clone();
                            let sb = vt.alloc(
                                almide_lang::intern::sym("__spread_base"),
                                bty.clone(),
                                Mutability::Let,
                                None,
                            );
                            let call = std::mem::replace(
                                &mut **base,
                                IrExpr {
                                    kind: IrExprKind::Var { id: sb },
                                    ty: bty.clone(),
                                    span: None,
                                    def_id: None,
                                },
                            );
                            Some((sb, bty, call))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                IrStmtKind::Expr { expr } => {
                    rewrite_expr(expr, vt);
                    None
                }
                _ => None,
            };
            if let Some((sb, bty, call)) = hoist {
                stmts.insert(
                    i,
                    IrStmt {
                        kind: IrStmtKind::Bind {
                            var: sb,
                            mutability: Mutability::Let,
                            ty: bty,
                            value: call,
                        },
                        span: None,
                    },
                );
                i += 1; // skip the inserted bind; the rewritten stmt is next
            }
            i += 1;
        }
    }

    fn rewrite_expr(e: &mut IrExpr, vt: &mut VarTable) {
        match &mut e.kind {
            IrExprKind::Block { stmts, expr } => {
                rewrite_block(stmts, vt);
                if let Some(t) = expr.as_deref_mut() {
                    rewrite_expr(t, vt);
                }
            }
            IrExprKind::If { cond, then, else_ } => {
                rewrite_expr(cond, vt);
                rewrite_expr(then, vt);
                rewrite_expr(else_, vt);
            }
            IrExprKind::While { cond, body } => {
                rewrite_expr(cond, vt);
                rewrite_block(body, vt);
            }
            _ => {}
        }
    }

    let almide_ir::IrProgram { functions, modules, var_table, .. } = program;
    for func in functions
        .iter_mut()
        .chain(modules.iter_mut().flat_map(|m| m.functions.iter_mut()))
    {
        rewrite_expr(&mut func.body, var_table);
    }
}

/// RECORD-LITERAL ARG HOIST (a pre-lowering program pass, shared chain): a
/// SCALAR-result call carrying a RECORD-LITERAL argument (`10.0 |>
/// letlib.box_left({ top: 0.0, left: letlib.GAP })` — #785's shape) walls in the
/// scalar-bind route (the literal needs aggregate materialization the scalar
/// path cannot do). Hoist the literal to its own bind — `let __arg = { … };
/// letlib.box_left(__arg, 10.0)` — so it builds through the PROVEN record-bind
/// machinery and the call sees a materialized Var. Scoped EXACTLY to the walled
/// set (Bind/Assign value = a Named call with a scalar type and ≥1 record-literal
/// arg) so no already-lowering call path changes. Call-count-invariant.
///
/// `hoist_record_literal_args_in_fn` is the SINGLE-FUNCTION entry: the pipeline
/// re-runs it AFTER the pure-call global substitution (the ceangal/`#785` bridge
/// inlines `letlib.GAP` → `default_gap()` INTO record fields at that later stage
/// — the program-pass run cannot see those calls yet).
pub fn hoist_record_literal_args_in_fn(
    body: &mut almide_ir::IrExpr,
    vt: &mut almide_ir::VarTable,
) {
    hoist_rewrite_expr(body, vt);
}

pub fn hoist_record_literal_args(program: &mut almide_ir::IrProgram) {
    let almide_ir::IrProgram { functions, modules, var_table, .. } = program;
    for func in functions
        .iter_mut()
        .chain(modules.iter_mut().flat_map(|m| m.functions.iter_mut()))
    {
        hoist_rewrite_expr(&mut func.body, var_table);
    }
}

mod hoist_impl {
    use almide_ir::{CallTarget, IrExpr, IrExprKind, IrStmt, IrStmtKind, Mutability, VarTable};
    use almide_lang::types::Ty;

    fn is_scalar_ty(ty: &Ty) -> bool {
        matches!(ty, Ty::Int | Ty::Float | Ty::Bool | Ty::Unit)
            || crate::lower::calls_p4_is_small_int(ty)
    }

    fn rewrite_block(stmts: &mut Vec<IrStmt>, vt: &mut VarTable) {
        let mut i = 0;
        while i < stmts.len() {
            let mut hoists: Vec<IrStmt> = Vec::new();
            match &mut stmts[i].kind {
                IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => {
                    rewrite_expr(value, vt);
                    if is_scalar_ty(&value.ty) {
                        if let IrExprKind::Call {
                            target: CallTarget::Named { .. } | CallTarget::Module { .. },
                            args,
                            ..
                        } = &mut value.kind
                        {
                            for a in args.iter_mut() {
                                if matches!(
                                    a.kind,
                                    IrExprKind::Record { .. } | IrExprKind::SpreadRecord { .. }
                                ) {
                                    let aty = a.ty.clone();
                                    let av = vt.alloc(
                                        almide_lang::intern::sym("__rec_arg"),
                                        aty.clone(),
                                        Mutability::Let,
                                        None,
                                    );
                                    let lit = std::mem::replace(
                                        a,
                                        IrExpr {
                                            kind: IrExprKind::Var { id: av },
                                            ty: aty.clone(),
                                            span: None,
                                            def_id: None,
                                        },
                                    );
                                    hoists.push(IrStmt {
                                        kind: IrStmtKind::Bind {
                                            var: av,
                                            mutability: Mutability::Let,
                                            ty: aty,
                                            value: lit,
                                        },
                                        span: None,
                                    });
                                }
                            }
                        }
                    }
                    // A record-literal BIND whose FIELD is a scalar CALL (`left:
                    // letlib.GAP` — a call-initialized module top-let read reaches
                    // the IR as its init call): the field-position call emitted a
                    // dst-less bare call (result on the stack — invalid wasm).
                    // Hoist each scalar call field to its own bind, declaration
                    // order preserved (= v0's field evaluation order).
                    if let IrExprKind::Record { fields, .. } = &mut value.kind {
                        for (_, fe) in fields.iter_mut() {
                            if is_scalar_ty(&fe.ty)
                                && matches!(fe.kind, IrExprKind::Call { .. })
                            {
                                let fty = fe.ty.clone();
                                let fv = vt.alloc(
                                    almide_lang::intern::sym("__rec_fld"),
                                    fty.clone(),
                                    Mutability::Let,
                                    None,
                                );
                                let call = std::mem::replace(
                                    fe,
                                    IrExpr {
                                        kind: IrExprKind::Var { id: fv },
                                        ty: fty.clone(),
                                        span: None,
                                        def_id: None,
                                    },
                                );
                                hoists.push(IrStmt {
                                    kind: IrStmtKind::Bind {
                                        var: fv,
                                        mutability: Mutability::Let,
                                        ty: fty,
                                        value: call,
                                    },
                                    span: None,
                                });
                            }
                        }
                    }
                }
                IrStmtKind::Expr { expr } => rewrite_expr(expr, vt),
                _ => {}
            }
            let has_hoists = !hoists.is_empty();
            for (k, h) in hoists.into_iter().enumerate() {
                stmts.insert(i + k, h);
            }
            // Re-visit from the first inserted bind: a hoisted record-literal ARG
            // bind may itself carry call FIELDS (`let __rec_arg = { left:
            // default_gap() }` — the substituted #785 shape) that the field pass
            // must hoist in turn. Already-rewritten stmts are no-ops on re-visit
            // (their literals are Vars now), so this terminates.
            if !has_hoists {
                i += 1;
            }
        }
    }

    fn rewrite_expr(e: &mut IrExpr, vt: &mut VarTable) {
        match &mut e.kind {
            IrExprKind::Block { stmts, expr } => {
                rewrite_block(stmts, vt);
                if let Some(t) = expr.as_deref_mut() {
                    rewrite_expr(t, vt);
                }
            }
            IrExprKind::If { cond, then, else_ } => {
                rewrite_expr(cond, vt);
                rewrite_expr(then, vt);
                rewrite_expr(else_, vt);
            }
            IrExprKind::While { cond, body } => {
                rewrite_expr(cond, vt);
                rewrite_block(body, vt);
            }
            _ => {}
        }
    }

    pub(crate) fn rewrite_expr_entry(e: &mut IrExpr, vt: &mut VarTable) {
        rewrite_expr(e, vt)
    }
}

pub(crate) use hoist_impl::rewrite_expr_entry as hoist_rewrite_expr;

/// The small-int scalar classes, shared with the hoist above (calls_p4's
/// int_eq_operand_ty is method-scoped; this free twin serves the desugar).
pub(crate) fn calls_p4_is_small_int(ty: &almide_lang::types::Ty) -> bool {
    use almide_lang::types::Ty;
    matches!(
        ty,
        Ty::Int8
            | Ty::Int16
            | Ty::Int32
            | Ty::Int64
            | Ty::UInt8
            | Ty::UInt16
            | Ty::UInt32
            | Ty::UInt64
            | Ty::Float32
    )
}

/// MEMBER-CHAIN TYPE REPAIR (a pre-lowering per-fn pass): a monomorphized
/// open-record reader (`fn get_port(app: { config: { port: Int, .. }, .. })` →
/// `get_port__App`) leaves an INTERMEDIATE Member node mistyped — `app.config`
/// carries `Named(App)` (the OUTER type) instead of the FIELD's declared type, so
/// the next member (`__.port`) resolves against the wrong record and the scalar
/// tail walls. The DECLARED field type is authoritative: repair every Member
/// node whose object type resolves and whose field type disagrees. Children
/// first (the object repairs before its member); a non-resolvable object type
/// (a genuinely open record at a non-mono site) is left untouched.
pub fn repair_member_field_tys(
    func: &mut almide_ir::IrFunction,
    layouts: &crate::lower::RecordLayouts,
) {
    use almide_ir::{walk_expr_mut, IrExpr, IrExprKind, IrMutVisitor};
    use almide_lang::types::Ty;

    fn field_ty_of(
        layouts: &crate::lower::RecordLayouts,
        ty: &Ty,
        field: almide_lang::intern::Sym,
    ) -> Option<Ty> {
        match ty {
            Ty::Record { fields } | Ty::OpenRecord { fields } => {
                fields.iter().find(|(n, _)| *n == field).map(|(_, t)| t.clone())
            }
            Ty::Named(name, args) => {
                let key = crate::lower::canonical_record_key(layouts, name.as_str())?;
                let (generics, decl_fields) = layouts.get(key)?;
                let mut subst: std::collections::HashMap<almide_lang::intern::Sym, Ty> =
                    std::collections::HashMap::new();
                for (g, a) in generics.iter().zip(args.iter()) {
                    subst.insert(*g, a.clone());
                }
                decl_fields
                    .iter()
                    .find(|(n, _)| *n == field)
                    .map(|(_, t)| calls::subst_type_var(t, &subst))
            }
            _ => None,
        }
    }

    struct R<'a> {
        layouts: &'a crate::lower::RecordLayouts,
    }
    impl IrMutVisitor for R<'_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Member { object, field } = &e.kind else { return };
            let Some(fty) = field_ty_of(self.layouts, &object.ty, *field) else { return };
            if e.ty != fty {
                e.ty = fty;
            }
        }
    }
    let mut r = R { layouts };
    r.visit_expr_mut(&mut func.body);
}
