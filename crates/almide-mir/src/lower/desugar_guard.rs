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
                // #1117: fold a CONSTANT condition at construction. The
                // optimize-stage const fold has already run by this point, so
                // an If born here reaches the renderers unfolded — and the
                // render path LOST the else's effect call (`guard false else
                // process.exit(3)` exited 0 on run/build/wasm while the
                // --target rust emission was correct). Folding here means the
                // renderers only ever see the shapes the source-level If
                // already proves out.
                if let IrExprKind::LitBool { value } = cond.kind {
                    let taken = if value { rewrite_tail_block(cont) } else { else_ };
                    return IrExpr {
                        kind: IrExprKind::Block { stmts: before, expr: Some(Box::new(taken)) },
                        ty,
                        span,
                        def_id,
                    };
                }
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

/// TAIL ERR-RAISE BRANCH → BIND-POSITION UNWRAP (a pre-lowering program pass,
/// shared chain like [`desugar_fn_body_guards`], which feeds it: a fn-body guard
/// whose else is `err(x)` / `err(x)!` restructures into exactly this shape). A
/// SCALAR-tail `if` whose one arm RAISES (`if c then a / b else err("…")[!]` —
/// an always-Err Result, with or without the explicit `!`; see
/// `err_raise_inner`) cannot lower on the scalar tail path (no early return).
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
///
/// The same normalization covers the tail-position variant `match` with a raise
/// leaf — the shape `guard let v = int.parse(s) else err("…")` desugars to. That
/// rewrite happens in the FRONTEND (`lower_block_body`), so it never reaches the
/// MIR `Guard` statement or the `if`-restructure above; it arrives here already
/// shaped as
///
///   { match int.parse(s) { ok(v) => v * 2, _ => err("…") } }
///
/// nested one level under the fn-body Block. That one level is the whole
/// difference: the auto-wrap ABI retype (`auto_wrap_abi_body`) rewrites the ty of
/// the body ROOT only, so a BARE `= match … { ok(v) => v*2, err(_) => err("…") }`
/// tail carries the `Result[Int, String]` carrier and lowers through the proven
/// heap-tail match, while the identical match one level down keeps its `Int` ty,
/// falls to `lower_tail_scalar_match`, and hits the variant-match wall. Folding
/// the non-root branch into the Result-typed bind shape puts it back on the same
/// proven path. Root branches are left ALONE (they lower today — normalizing them
/// would take a working shape off a proven path, the over-reach that regressed the
/// first heap-`??` attempt).
pub fn normalize_tail_err_raise_ifs(program: &mut almide_ir::IrProgram) {
    use almide_ir::{IrExpr, IrExprKind, IrMatchArm, IrStmt, IrStmtKind, Mutability, VarTable};
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;

    fn is_scalar_value_ty(ty: &Ty) -> bool {
        // Scalar payloads AND String: both have a proven bind-position `!` unwrap
        // (`let $g: String = $r!` is the fs.read_text class), so both normalize.
        matches!(ty, Ty::Int | Ty::Float | Ty::Bool | Ty::String)
    }
    /// The raising arm's inner Result expr, if this arm IS an err-raise. Two
    /// spellings, ONE meaning at a fn tail — "this function returns Err(e)":
    ///
    /// - `err(e)!` — an `Unwrap` over a `ResultErr` ctor (an explicit raise);
    /// - `err(e)` — the BARE ctor. In the tail of a scalar-returning `effect
    ///   fn` (whose compiled carrier is the synthetic `Result[T, String]`) the
    ///   frontend accepts an `err(…)` arm against a scalar sibling arm — that
    ///   heterogeneous `if` IS the auto-wrap ABI's carrier. This is the arm
    ///   `guard c else err(e)` (no `!`) leaves after the guard restructure.
    ///
    /// The bare spelling only typechecks in that auto-wrap position (a declared
    /// `-> Result[…]` fn rejects `if c then 1 else err("x")` with E001), so
    /// admitting it here cannot capture a declared-Result tail.
    fn err_raise_inner(e: &IrExpr) -> Option<&IrExpr> {
        match &e.kind {
            IrExprKind::Unwrap { expr } => {
                matches!(expr.kind, IrExprKind::ResultErr { .. }).then_some(expr.as_ref())
            }
            IrExprKind::ResultErr { .. } => Some(e),
            _ => None,
        }
    }

    /// `root` = this expr IS the function body (not nested under a Block tail).
    /// A root `match` already carries the auto-wrap ABI's Result carrier and
    /// lowers as-is, so only NON-root matches normalize (see the pass doc).
    fn rewrite_tail(e: &mut IrExpr, vt: &mut VarTable, root: bool) {
        match &mut e.kind {
            IrExprKind::Block { stmts, expr: Some(t) } => {
                rewrite_tail(t, vt, false);
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
            IrExprKind::If { .. } => rewrite_tail_raise_branch(e, vt),
            // The `guard let` shape: a variant (Option/Result) `match` in a
            // NON-root tail with a raise leaf. Gated on a variant subject —
            // that is the class the tail wall rejects, and the class the
            // bind-position value-match machinery is proven over.
            IrExprKind::Match { subject, .. }
                if !root && crate::lower::is_variant_ty(&subject.ty) =>
            {
                rewrite_tail_raise_branch(e, vt)
            }
            _ => {}
        }
    }

    /// The branch arm's whole body, moved out of [`rewrite_tail`]'s match — same
    /// "outer name router unchanged, arm body to a named helper" split used
    /// throughout this pass family (e.g. [`rewrite_tail_block`] above). A
    /// sibling nested `fn` in the same block as `rewrite_tail`, so it shares
    /// that block's other nested-fn items (`is_scalar_value_ty`,
    /// `err_raise_inner`) exactly as the inline arm body did.
    ///
    /// Fold the WHOLE guard BRANCH-CHAIN (any nesting of `if`s and variant
    /// `match`es mixing raise arms and one value-ty leaf class) into ONE
    /// Result-typed branch tree: every VALUE leaf wraps in ok(…), every RAISE
    /// leaf sheds its `!` — so a chained guard (`validate_age`'s two guards, or
    /// two stacked `guard let`s) normalizes to a single bind + unwrap instead of
    /// nesting ok() around inner binds (which no lowering path executes).
    /// `classify_chain` returns the uniform value-leaf ty and the raise arms' Err
    /// ty, or None outside the subset (a leaf that is neither).
    fn rewrite_tail_raise_branch(e: &mut IrExpr, vt: &mut VarTable) {
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
            /// Unify two leaf classifications: `Unknown` (a raise leaf, which
            /// contributes no value ty) yields to the other side; two concrete
            /// value tys must agree, else the chain is outside the subset.
            fn unify(a: &Ty, b: &Ty) -> Option<Ty> {
                match (a, b) {
                    (Ty::Unknown, v) | (v, Ty::Unknown) => Some(v.clone()),
                    (x, y) if x == y => Some(x.clone()),
                    _ => None,
                }
            }
            if let IrExprKind::If { then, else_, .. } = &e.kind {
                let (t_val, t_err) = classify_chain(then)?;
                let (e_val, e_err) = classify_chain(else_)?;
                return Some((unify(&t_val, &e_val)?, t_err.or(e_err)));
            }
            // A variant `match` is the same chain node with N arms. A GUARDED
            // arm declines: the rewrite keeps the arms verbatim, and the
            // bind-position value-match subset the fold targets is proven over
            // un-guarded constructor arms only.
            if let IrExprKind::Match { arms, .. } = &e.kind {
                let mut val = Ty::Unknown;
                let mut err = None;
                for arm in arms {
                    if arm.guard.is_some() {
                        return None;
                    }
                    let (a_val, a_err) = classify_chain(&arm.body)?;
                    val = unify(&val, &a_val)?;
                    err = err.or(a_err);
                }
                return Some((val, err));
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
            // The variant-`match` chain node: subject and patterns verbatim
            // (the arm binders keep binding exactly what they bound), only the
            // arm BODIES are lifted to the Result carrier.
            if let IrExprKind::Match { subject, arms } = &e.kind {
                return IrExpr {
                    kind: IrExprKind::Match {
                        subject: subject.clone(),
                        arms: arms
                            .iter()
                            .map(|arm| IrMatchArm {
                                pattern: arm.pattern.clone(),
                                guard: arm.guard.clone(),
                                body: to_result_tree(&arm.body, result_ty),
                            })
                            .collect(),
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
        // The WHOLE branch tree lifted to the Result carrier — for an `if` this is
        // literally the old per-arm `to_result_tree` pair reassembled, for a
        // `match` it is the arms with their patterns intact.
        let result_branch = to_result_tree(e, &result_ty);
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
        let bind_r = IrStmt {
            kind: IrStmtKind::Bind {
                var: r,
                ty: result_ty.clone(),
                value: result_branch,
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

    let almide_ir::IrProgram { functions, modules, var_table, .. } = program;
    for func in functions
        .iter_mut()
        .chain(modules.iter_mut().flat_map(|m| m.functions.iter_mut()))
    {
        rewrite_tail(&mut func.body, var_table, true);
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
        
        let mut i = 0;
        while i < stmts.len() {
            let hoisted = take_first_block_part(&mut stmts[i]);
            if hoisted.is_empty() {
                i += 1;
                continue;
            }
            for (k, s) in hoisted.into_iter().enumerate() {
                stmts.insert(i + k, s);
            }
            // Re-examine the same bind: another block part may remain.
        }
    }

    /// Take the statements of the FIRST block-shaped interpolation part of a
    /// `let` bind, leaving that part as its own tail expression. Empty when the
    /// statement is not that shape, or when an EARLIER part is impure (moving
    /// the block's statements before it would reorder the evaluation).
    ///
    /// A non-Block part only updates the purity flag; a Block part ends the scan
    /// either way, so at most one hoist happens per call — the caller re-runs on
    /// the same statement to find the next.
    fn take_first_block_part(stmt: &mut almide_ir::IrStmt) -> Vec<almide_ir::IrStmt> {
        use almide_ir::{IrExprKind, IrStmtKind, IrStringPart};
        let IrStmtKind::Bind { value, .. } = &mut stmt.kind else { return Vec::new() };
        let IrExprKind::StringInterp { parts } = &mut value.kind else { return Vec::new() };
        let mut earlier_pure = true;
        for part in parts.iter_mut() {
            let IrStringPart::Expr { expr } = part else { continue };
            let IrExprKind::Block { stmts: inner, expr: Some(tail) } = &mut expr.kind else {
                if !is_pure_operand(expr) {
                    earlier_pure = false;
                }
                continue;
            };
            if !earlier_pure || inner.is_empty() {
                return Vec::new();
            }
            let hoisted = std::mem::take(inner);
            let t = (**tail).clone();
            *expr = t;
            return hoisted;
        }
        Vec::new()
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

/// ASSERT-SUBJECT HOIST (a pre-lowering program pass, shared chain like the
/// block-arg hoist above): a call-bearing operand of a statement- or
/// tail-position `assert` / `assert_eq` / `assert_ne` hoists into a `let`
/// bind spliced before the assert —
///
///   assert_eq(pick(5), ok(5))  ≡  let s = pick(5); assert_eq(s, ok(5))
///
/// This must run HERE, before the never-err/auto-wrap ABI rewrites: a bind is
/// a shape that family rewraps (the raw never-err callee result re-wrapped at
/// the site), while an inline call argument skips it and reaches the metered
/// wasm test leg as an unresolvable `if` condition — the call-bearing
/// linearization wall (the inline_assert probe). Evaluation order is
/// unchanged (left before right, both before the compare); the 2-arg assert
/// MESSAGE is untouched (it must stay lazy, failure-path only). No calls are
/// added or removed — the caps `mir == ir` invariant holds.
pub fn hoist_assert_call_subjects(program: &mut almide_ir::IrProgram) {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    use almide_ir::{CallTarget, IrExpr, IrExprKind, IrStmt, IrStmtKind, Mutability, VarId};

    fn operand_has_call(e: &IrExpr) -> bool {
        let mut found = false;
        struct C<'a> {
            found: &'a mut bool,
        }
        impl<'a> almide_ir::visit::IrVisitor for C<'a> {
            fn visit_expr(&mut self, e: &IrExpr) {
                if matches!(
                    e.kind,
                    IrExprKind::Call { .. }
                        | IrExprKind::RuntimeCall { .. }
                        | IrExprKind::Try { .. }
                        | IrExprKind::Unwrap { .. }
                ) {
                    *self.found = true;
                }
                almide_ir::visit::walk_expr(self, e);
            }
        }
        almide_ir::visit::IrVisitor::visit_expr(&mut C { found: &mut found }, e);
        found
    }

    /// When `e` is an assert whose comparable operands carry calls, hoist
    /// those operands into fresh binds (returned) and rewrite the args to
    /// read the bound Vars. The Bool condition of 1/2-arg `assert` hoists as
    /// a whole; the message arg (index 1 of 2-arg assert) never hoists.
    /// SCOPE: only RESULT-typed operands — the lifted-effect-call subject that
    /// needs the rewrap (`assert_eq(pick(5), ok(5))`). Other heap operand
    /// classes stay INLINE deliberately: the inline eq path materializes a
    /// fresh call result directly, while a hoisted Var must be in the tracked
    /// read-shape sets — hoisting an `Option[String]` operand REGRESSED
    /// map_higher_order_test onto the linearization wall.
    fn hoist_assert(e: &mut IrExpr, next: &mut u32) -> Vec<IrStmt> {
        use almide_lang::types::constructor::TypeConstructorId;
        let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &mut e.kind
        else {
            return Vec::new();
        };
        let hoistable: usize = match (name.as_str(), args.len()) {
            ("assert", 1 | 2) => 1, // the Bool cond only; a 2-arg msg stays lazy
            ("assert_eq" | "assert_ne", 2) => 2,
            _ => return Vec::new(),
        };
        let mut binds = Vec::new();
        for arg in args.iter_mut().take(hoistable) {
            if !matches!(
                arg.ty,
                almide_lang::types::Ty::Applied(TypeConstructorId::Result, _)
            ) || !operand_has_call(arg)
            {
                continue;
            }
            let var = VarId(*next);
            *next += 1;
            let ty = arg.ty.clone();
            let span = arg.span.clone();
            let value = std::mem::replace(
                arg,
                IrExpr { kind: IrExprKind::Var { id: var }, ty: ty.clone(), span: span.clone(), def_id: None },
            );
            binds.push(IrStmt {
                kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value },
                span,
            });
        }
        binds
    }

    struct H<'a> {
        next: &'a mut u32,
    }
    impl<'a> IrMutVisitor for H<'a> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Block { stmts, expr } = &mut e.kind else { return };
            let mut out: Vec<IrStmt> = Vec::with_capacity(stmts.len());
            for mut stmt in stmts.drain(..) {
                if let IrStmtKind::Expr { expr } = &mut stmt.kind {
                    out.extend(hoist_assert(expr, self.next));
                }
                out.push(stmt);
            }
            if let Some(tail) = expr.as_deref_mut() {
                out.extend(hoist_assert(tail, self.next));
            }
            *stmts = out;
        }
    }
    for func in program
        .functions
        .iter_mut()
        .chain(program.modules.iter_mut().flat_map(|m| m.functions.iter_mut()))
    {
        let mut next = crate::lower::max_var_id(&func.body) + 1;
        H { next: &mut next }.visit_expr_mut(&mut func.body);
    }
}
