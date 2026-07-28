/// Desugar `fan.race` / `fan.any` over a LITERAL thunk list by INLINING each thunk's body — avoiding a
/// `List[funcref]` (unrepresentable in v1) entirely. On wasm the fan combinators are deterministic:
///   `fan.race([() => t0, () => t1, …])`  ≡  `t0`           (the FIRST thunk settles first)
///   `fan.any([() => t0, () => t1, …])`   ≡  `match t0 { ok(v) => ok(v), err(_) => <any of the rest> }`
///                                             (the FIRST Ok in list order; the last thunk's result is
///                                              the fallback if every earlier one errs)
/// Each `t_i` is a no-param lambda whose body is a `Result[T, String]` (an effect fn call). The inlined
/// form is a plain match-over-a-call chain, all in v1's subset. A NON-literal thunk list (`let ts =
/// […]; fan.race(ts)`) has no inlinable bodies → left for the call-site purity wall.
pub fn desugar_fan_race_any(body: &IrExpr, next_var: &mut u32) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    use almide_ir::{CallTarget, IrMatchArm, IrPattern};
    // A LET-BOUND, never-reassigned thunk-list literal (`let lam: List[() -> R] = [() => …];
    // fan.race(lam)` — the #599 var-bound form): resolve the Var back to its literal elements
    // so the SAME inliners run as for the inline form. Sound: VarIds are shadowing-free
    // (frontend guarantee), the binding is single-assignment (reassigned vars are dropped from
    // the map), and the list's construction stays in place (its lambdas are never evaluated at
    // construction, so inlining the fan semantics duplicates no effect). The same desugar runs
    // on the count-gate side (desugar-before-both), so `mir == ir` accounting is preserved.
    fn collect_thunk_list_lets(body: &IrExpr) -> std::collections::HashMap<u32, Vec<IrExpr>> {
        use almide_ir::visit::{walk_stmt, IrVisitor};
        use almide_lang::types::Ty;
        #[derive(Default)]
        struct C {
            lets: std::collections::HashMap<u32, Vec<IrExpr>>,
            reassigned: std::collections::HashSet<u32>,
        }
        impl IrVisitor for C {
            fn visit_stmt(&mut self, s: &IrStmt) {
                match &s.kind {
                    IrStmtKind::Bind { var, ty, value, .. } => {
                        if let (
                            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a),
                            IrExprKind::List { elements },
                        ) = (ty, &value.kind)
                        {
                            if a.len() == 1
                                && matches!(a[0], Ty::Fn { .. })
                                && !elements.is_empty()
                                && elements.iter().all(|el| matches!(&el.kind,
                                    IrExprKind::Lambda { params, .. } if params.is_empty()))
                            {
                                self.lets.insert(var.0, elements.clone());
                            }
                        }
                    }
                    IrStmtKind::Assign { var, .. } => {
                        self.reassigned.insert(var.0);
                    }
                    _ => {}
                }
                walk_stmt(self, s);
            }
        }
        let mut c = C::default();
        c.visit_expr(body);
        for v in &c.reassigned {
            c.lets.remove(v);
        }
        c.lets
    }
    let thunk_lets = collect_thunk_list_lets(body);
    struct V {
        changed: bool,
        next_var: u32,
        thunk_lets: std::collections::HashMap<u32, Vec<IrExpr>>,
    }
    // Extract the no-param thunk bodies of a `fan.race`/`fan.any` call over a LITERAL list OR a
    // let-bound literal Var (resolved via `thunk_lets`), or `None` otherwise (a genuinely
    // dynamic thunk list has no inlinable bodies → declines to the honest wall).
    fn fan_bodies(
        e: &IrExpr,
        want: &str,
        thunk_lets: &std::collections::HashMap<u32, Vec<IrExpr>>,
    ) -> Option<Vec<IrExpr>> {
        let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &e.kind
        else {
            return None;
        };
        if module.as_str() != "fan" || func.as_str() != want {
            return None;
        }
        let [arg] = &args[..] else { return None };
        let resolved;
        let elements = match &arg.kind {
            IrExprKind::List { elements } => elements,
            IrExprKind::Var { id } => {
                resolved = thunk_lets.get(&id.0)?.clone();
                &resolved
            }
            _ => return None,
        };
        if elements.is_empty() {
            return None;
        }
        let mut bodies = Vec::with_capacity(elements.len());
        for el in elements {
            let IrExprKind::Lambda { params, body, .. } = &el.kind else {
                return None;
            };
            if !params.is_empty() {
                return None;
            }
            bodies.push((**body).clone());
        }
        Some(bodies)
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            // PRE-order: the match-over-`fan.any` subject rewrite — on a hit, walk the
            // rewritten node and stop (the original control flow); see
            // `bind_match_over_any`.
            if self.bind_match_over_any(e) {
                walk_expr_mut(self, e);
                return;
            }
            // BIND-VALUE / BLOCK-TAIL settle/any VALUE rewrites — see
            // `rewrite_settle_any_in_block`.
            self.rewrite_settle_any_in_block(e);
            walk_expr_mut(self, e);
            // POST-order: the `fan.race` head substitution — see `rewrite_race_head`.
            self.rewrite_race_head(e);
            // POST-order: `fan.settle([() => t0, …])` in ANY position — deterministic sequential
            // semantics on wasm: the results list IS the list of each thunk's Result, in order.
            // Rewrite to the LITERAL `[t0, t1, …]` — the List[Result] literal machinery (the
            // lenlist stage) materializes it; a declared-Result thunk body keeps its Result type
            // (a never-err LIFTED body's raw type is declined by the literal's e.ty == elem_ty
            // gate → the whole call walls honestly, as before).
            let _ = e; // settle/any handled position-limited via rewrite_settle_any above
        }
    }
    impl V {
        // PRE-order: `match fan.any([() => t0, …]) { ok(pat) => okbody, err(epat) => errbody }` —
        // fold the thunks into ONE Result value and apply the ORIGINAL match to it once:
        //   `{ let $r = <first-Ok chain>; match $r { ok(pat) => okbody, err(epat) => errbody } }`
        // Every piece appears exactly once: each thunk body once inside the chain, each outer
        // arm once in the single match. The all-failed path reaches `errbody` through the
        // chain's innermost `err("fan.any: all candidates failed")` — v0's fixed message, so
        // a bound err var reads the same string the old body-substitution produced.
        //
        // SHAPE HISTORY (both rejected alternatives are load-bearing):
        //  - Inlining the outer arms into each thunk level (the pre-J3 form) duplicated
        //    `okbody` — the function's whole remaining continuation — once per level, which
        //    COMPOUNDS across sequential fan.any calls: fan_any_early_winner.almd's eight
        //    chained calls multiplied to ~1,700 copies of the tail, the 231KB names witness
        //    of the 2026-07-27 trust-spine incident. It also needed a fresh Ok binder per
        //    level (#900); binding once, the aliasing class is gone structurally.
        //  - Substituting the chain DIRECTLY as the match subject leaves a match-over-match
        //    the branch lowering's subject tracking cannot follow (the fan_any_allfail
        //    regression — see `rewrite_settle_any_in_block`'s comment). The `$r` bind gives
        //    the match the var subject it can track.
        // The bind form is LINEAR only because `desugar_let_bound_heap_branch` DECLINES the
        // first-Ok-chain shape (`is_first_ok_chain`, arc v1-join-completeness J2a) — without
        // that decline it tail-duplicates the following match into every chain arm
        // (measured ~3.5×/chained call). The two pieces land together and the fixture's
        // 8-chain main + corpus-wall's witness-size gate ratchet the linearity.
        // A guarded ok arm now evaluates its guard ONCE on the folded Result — equal to v0
        // native, which picks the first Ok and only then runs the match guard.
        // Returns true when the rewrite fired (the caller then walks the rewritten node and
        // stops, exactly the original control flow).
        fn bind_match_over_any(&mut self, e: &mut IrExpr) -> bool {
            use almide_lang::types::constructor::TypeConstructorId;
            let IrExprKind::Match { subject, arms } = &e.kind else {
                return false;
            };
            if arms.len() != 2 {
                return false;
            }
            let has_ok = arms.iter().any(|a| matches!(a.pattern, IrPattern::Ok { .. }));
            let has_err = arms.iter().any(|a| matches!(a.pattern, IrPattern::Err { .. }));
            if !has_ok || !has_err {
                return false;
            }
            let Some(bodies) = fan_bodies(subject, "any", &self.thunk_lets) else {
                return false;
            };
            let (result_ty, ok_ty) = match &subject.ty {
                Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => {
                    (subject.ty.clone(), a[0].clone())
                }
                _ => return false,
            };
            let arms = arms.clone();
            let chain = self.first_ok_chain(bodies, &result_ty, &ok_ty);
            let r = VarId(self.next_var);
            self.next_var += 1;
            let match_once = IrExpr {
                kind: IrExprKind::Match {
                    subject: Box::new(IrExpr {
                        kind: IrExprKind::Var { id: r },
                        ty: result_ty.clone(),
                        span: None,
                        def_id: None,
                    }),
                    arms,
                },
                ty: e.ty.clone(),
                span: e.span.clone(),
                def_id: e.def_id,
            };
            *e = IrExpr {
                kind: IrExprKind::Block {
                    stmts: vec![almide_ir::IrStmt {
                        kind: almide_ir::IrStmtKind::Bind {
                            var: r,
                            mutability: almide_ir::Mutability::Let,
                            ty: result_ty,
                            value: chain,
                        },
                        span: None,
                    }],
                    expr: Some(Box::new(match_once)),
                },
                ty: e.ty.clone(),
                span: e.span.clone(),
                def_id: e.def_id,
            };
            self.changed = true;
            true
        }
        // The first-Ok chain VALUE over the inlined thunk bodies:
        //   `match t0 { ok($x) => ok($x), err(_) => <next … err("fan.any: all candidates
        //   failed")> }`
        // — the innermost fallback is v0's fixed all-failed message. LINEAR by construction:
        // each thunk body appears exactly once and nothing else is duplicated. Both fan.any
        // positions (bind-value / block-tail AND match-subject) fold through here, and
        // `is_first_ok_chain` (the J2a decline predicate) recognizes exactly this shape.
        fn first_ok_chain(&mut self, bodies: Vec<IrExpr>, result_ty: &Ty, ok_ty: &Ty) -> IrExpr {
            use almide_ir::{IrMatchArm, IrPattern};
            let mut acc = IrExpr {
                kind: IrExprKind::ResultErr {
                    expr: Box::new(IrExpr {
                        kind: IrExprKind::LitStr {
                            value: "fan.any: all candidates failed".to_string(),
                        },
                        ty: Ty::String,
                        span: None,
                        def_id: None,
                    }),
                },
                ty: result_ty.clone(),
                span: None,
                def_id: None,
            };
            for tb in bodies.into_iter().rev() {
                let x = VarId(self.next_var);
                self.next_var += 1;
                let x_ref = IrExpr {
                    kind: IrExprKind::Var { id: x },
                    ty: ok_ty.clone(),
                    span: None,
                    def_id: None,
                };
                acc = IrExpr {
                    kind: IrExprKind::Match {
                        subject: Box::new(tb),
                        arms: vec![
                            IrMatchArm {
                                pattern: IrPattern::Ok {
                                    inner: Box::new(IrPattern::Bind { var: x, ty: ok_ty.clone() }),
                                },
                                guard: None,
                                body: IrExpr {
                                    kind: IrExprKind::ResultOk { expr: Box::new(x_ref) },
                                    ty: result_ty.clone(),
                                    span: None,
                                    def_id: None,
                                },
                            },
                            IrMatchArm {
                                pattern: IrPattern::Err { inner: Box::new(IrPattern::Wildcard) },
                                guard: None,
                                body: acc,
                            },
                        ],
                    },
                    ty: result_ty.clone(),
                    span: None,
                    def_id: None,
                };
            }
            acc
        }
        // BIND-VALUE / BLOCK-TAIL positions for the settle/any VALUE rewrites: an
        // `!`-wrapped `fan.any(…)!` must stay for the effect-unwrap desugar (which builds
        // the match shape the PRE-order inliner above handles) — rewriting under the
        // Unwrap left a match-over-match the subject tracking cannot follow (the
        // fan_any_allfail regression, by-name diff). Extracted verbatim from
        // `visit_expr_mut` (codopsy r2, #852, group 2 of 3).
        fn rewrite_settle_any_in_block(&mut self, e: &mut IrExpr) {
            if let IrExprKind::Block { stmts, expr } = &mut e.kind {
                for st in stmts.iter_mut() {
                    if let IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } =
                        &mut st.kind
                    {
                        self.rewrite_settle_any(value);
                    }
                }
                if let Some(t) = expr {
                    self.rewrite_settle_any(t);
                }
            }
        }
        // POST-order: `fan.race([() => t0, …])` — the FIRST thunk's body (deterministic head).
        // The CHECKED type of `fan.race(…)` is uniformly `Result[T, String]` (the fan thunk
        // convention — see `desugar_fan_block`'s twin comment), even when every thunk is a
        // PLAIN (non-Result) fn (`fan.race([thunk_a, thunk_b])`, `thunk_a -> Int` — v0's
        // FanLowering wraps a non-Result thunk in an Ok adapter). A caller reaching `fan.race`
        // through an un-annotated bind (`let r = fan.race([...])`) gets the frontend's auto-`?`
        // `Try` node over this Result-checked type — which `desugar_effect_unwrap` (a LATER
        // pass) turns into a real `match … { err(e)=>.., ok(r)=>.. }`. If this rule substitutes
        // the RAW thunk body (`t0`, Int-typed) in place of the ORIGINAL Result-typed call, the
        // surrounding Try/match sees a type it no longer matches — producing a structurally
        // invalid `Ok/Err`-pattern match over a scalar Int subject (confirmed via debug tracing
        // on `fan_pure_thunks.almd`: exactly this shape reaches `lower_branch`'s untracked-
        // subject-with-call-bearing-arm wall). PRESERVE the Result contract instead: when the
        // ORIGINAL call was Result-typed but `t0` is not, wrap `t0` in a genuine `ok(t0)`
        // (`ResultOk`) at the original type — sound for EVERY position (Try, match subject, a
        // scalar use), not just the one that happened to break, and unconditionally in step
        // with the "FanLowering always Oks a non-Result thunk" contract this file's header
        // documents. A thunk that is ALREADY Result-typed (a real fallible race — not used in
        // this corpus but structurally possible) is untouched — its own `!`/match handles the
        // real Err path. Extracted verbatim from `visit_expr_mut` (codopsy r2, #852,
        // group 3 of 3).
        fn rewrite_race_head(&mut self, e: &mut IrExpr) {
            if let Some(bodies) = fan_bodies(e, "race", &self.thunk_lets) {
                let orig_ty = e.ty.clone();
                let t0 = bodies.into_iter().next().expect("fan_bodies() never returns Some(bodies) with an empty bodies (elements.is_empty() is rejected internally)");
                *e = if crate::lower::is_result_ty(&orig_ty) && !crate::lower::is_result_ty(&t0.ty)
                {
                    IrExpr {
                        kind: IrExprKind::ResultOk { expr: Box::new(t0) },
                        ty: orig_ty,
                        span: e.span.clone(),
                        def_id: e.def_id,
                    }
                } else {
                    t0
                };
                self.changed = true;
            }
        }
        fn rewrite_settle_any(&mut self, e: &mut IrExpr) {
            use almide_ir::{IrMatchArm, IrPattern};
            // `fan.settle([…])` as a bind value / tail — the results list literal.
            // A PURE thunk's body is bare `T` while settle's checked type is
            // `List[Result[T, E]]` (FanLowering's phantom-Result convention) — wrap each
            // non-Result body in a genuine `ok(...)` so the literal's elements match its
            // element type (the B115 `fan.race` contract-preservation fix, settle's turn:
            // without it the raw `List[Int]` bodies hit the List[heap]-literal wall).
            if let Some(bodies) = fan_bodies(e, "settle", &self.thunk_lets) {
                use almide_lang::types::constructor::TypeConstructorId;
                use almide_lang::types::Ty;
                let elem_ty = match &e.ty {
                    Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => a[0].clone(),
                    _ => Ty::Unknown,
                };
                let elem_is_result =
                    matches!(&elem_ty, Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2);
                let elements = bodies
                    .into_iter()
                    .map(|b| {
                        if elem_is_result
                            && !matches!(&b.ty, Ty::Applied(TypeConstructorId::Result, _))
                        {
                            IrExpr {
                                span: b.span.clone(),
                                def_id: None,
                                kind: IrExprKind::ResultOk { expr: Box::new(b) },
                                ty: elem_ty.clone(),
                            }
                        } else {
                            b
                        }
                    })
                    .collect();
                e.kind = IrExprKind::List { elements };
                self.changed = true;
                return;
            }
            // `fan.any([…])` as a bind value / tail — the first-Ok chain VALUE (the shared
            // `first_ok_chain` builder). The match-subject shape (pre-order) folds through
            // the same builder via `bind_match_over_any`.
            if let Some(bodies) = fan_bodies(e, "any", &self.thunk_lets) {
                use almide_lang::types::constructor::TypeConstructorId;
                use almide_lang::types::Ty;
                let ok_ty = match &e.ty {
                    Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => a[0].clone(),
                    _ => return,
                };
                let ty = e.ty.clone();
                *e = self.first_ok_chain(bodies, &ty, &ok_ty);
                self.changed = true;
            }
        }
    }
    // Seed above BOTH the body's own vars and the shared counter, and write the
    // counter back. `desugar_heap_branches_inner` threads ONE `next_var` through
    // every rewrite in its loop; this one used to ignore the parameter in both
    // directions, seeding only from `max_var_id(body) + 1` and never publishing
    // what it consumed — so a later rewrite in the same loop could hand out an
    // id the per-level Ok binders here already own.
    let seed = (max_var_id(body) + 1).max(*next_var);
    let mut v = V { changed: false, next_var: seed, thunk_lets };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    *next_var = v.next_var;
    if v.changed {
        Some(out)
    } else {
        None
    }
}


/// Recognize EXACTLY the shape `first_ok_chain` builds (arc v1-join-completeness,
/// J2a — the decline predicate for `desugar_let_bound_heap_branch`):
///   chain ::= err("fan.any: all candidates failed")
///           | match <thunk> { ok($x) => ok($x), err(_) => chain }
/// A bind of this shape must NOT be tail-duplicated: the chain is variant-typed, so
/// the bind-position join (`lower_bind_heap_if`/`_match`'s `is_variant_ty` path,
/// certified by the released-merge-dst credits) lowers it LINEARLY — duplicating
/// instead re-multiplies the following match into every chain arm (~3.5× per
/// chained fan.any, the 2026-07-27 exponential through a different door). The
/// pattern is strict (matching binder-to-var Ok arm, wildcard Err arm, the exact
/// v0 all-failed literal innermost) so nothing hand-written trips it today; if the
/// builder above changes shape, change THIS in the same commit.
fn is_first_ok_chain(e: &IrExpr) -> bool {
    use almide_ir::IrPattern;
    match &e.kind {
        IrExprKind::ResultErr { expr } => matches!(
            &expr.kind,
            IrExprKind::LitStr { value } if value == "fan.any: all candidates failed"
        ),
        IrExprKind::Match { subject: _, arms } => {
            let [ok, err] = &arms[..] else { return false };
            let ok_shape = ok.guard.is_none()
                && match (&ok.pattern, &ok.body.kind) {
                    (IrPattern::Ok { inner }, IrExprKind::ResultOk { expr }) => matches!(
                        (&**inner, &expr.kind),
                        (IrPattern::Bind { var, .. }, IrExprKind::Var { id }) if var == id
                    ),
                    _ => false,
                };
            let err_shape = err.guard.is_none()
                && matches!(&err.pattern, IrPattern::Err { inner }
                    if matches!(&**inner, IrPattern::Wildcard));
            ok_shape && err_shape && is_first_ok_chain(&err.body)
        }
        _ => false,
    }
}

/// Rewrite a `fan { e1; e2; … }` BLOCK whose expressions are all NON-Result into the
/// plain tuple `(e1, e2, …)` — v0's wasm emission for the fan block IS the sequential
/// fallback (expressions_g2 "Fan block — no parallelism in WASM"): each expr evaluated
/// in list order, results stored into a fresh tuple. A Tuple literal evaluates its
/// elements in exactly that order, so the rewrite is byte-identical on the wasm
/// target (contract C-004's determinism family). A Result-typed expr (an effect-fn
/// thunk) needs v0's auto-unwrap + Err early-return — DECLINED here (a later brick),
/// so the function stays honestly walled. Count-invariant: every expr appears once.
pub fn desugar_fan_block(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    struct V {
        changed: bool,
        next_var: u32,
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            use almide_lang::types::constructor::TypeConstructorId;
            walk_expr_mut(self, e);
            let IrExprKind::Fan { exprs } = &e.kind else { return };
            // The checker types EVERY fan expr as `Result[T, String]` (the fan thunk
            // convention) even when the callee is a PLAIN fn whose runtime value is the
            // raw T (v0 native builds the raw tuple; a plain call never errs, so the
            // wasm auto-unwrap is a no-op on it). Admit a direct NAMED call with that
            // PHANTOM Result type and strip it to the Ok type — the v1 call of a plain
            // fn yields the raw T, so the element ty must say T for the tuple build.
            // A Module/Method/Computed expr (a REAL fallible thunk, `fs.read` etc.)
            // stays declined — its unwrap + Err early-return is a later brick.
            let phantom_ok_ty = |x: &IrExpr| -> Option<Ty> {
                match &x.ty {
                    Ty::Applied(TypeConstructorId::Result, a)
                        if a.len() == 2
                            && matches!(
                                &x.kind,
                                IrExprKind::Call {
                                    target: almide_ir::CallTarget::Named { .. },
                                    ..
                                }
                            ) =>
                    {
                        // The Result type is PHANTOM (v1 value = raw T) ONLY for a
                        // never-err LIFTED callee. A DECLARED-Result thunk (`effect fn
                        // add(..) -> Result[Int, String] = ok(a + b)` — fan_test) builds
                        // a REAL Result block; stripping its type made the callsite read
                        // the i32 handle as a raw i64 — INVALID WASM (latent while the
                        // file walled elsewhere; exposed by the single-expr arm,
                        // 2026-07-17). Such a thunk needs the real fan unwrap + Err
                        // propagation — a later brick — so it DECLINES (honest wall).
                        let IrExprKind::Call {
                            target: almide_ir::CallTarget::Named { name }, ..
                        } = &x.kind
                        else {
                            return None;
                        };
                        let raw_abi = crate::lower::NEVER_ERR_LIFTED_FNS
                            .with(|s| s.borrow().contains(name.as_str()))
                            && !crate::lower::AUTO_WRAP_ABI_FNS
                                .with(|s| s.borrow().contains(name.as_str()));
                        if raw_abi {
                            Some(a[0].clone())
                        } else {
                            None
                        }
                    }
                    _ if !crate::lower::is_result_ty(&x.ty) => Some(x.ty.clone()),
                    _ => None,
                }
            };
            // A REAL-Result thunk (a declared-Result / lifted-can-err / auto-wrapped
            // NAMED call — its v1 value IS a Result block) takes the SEQUENTIAL
            // `e!`-equivalent: `fan { e1; e2 } ≡ { let $f1 = e1!; let $f2 = e2!;
            // ($f1, $f2) }` — v0 joins in list order and `?`-propagates the first Err,
            // which is exactly the bind-`!` chain's semantics; the whole existing
            // unwrap machinery (desugar_let_unwrap's ok/err match, the err-type join)
            // then lowers it. Count-invariant: each thunk call appears exactly once.
            let real_result_ok_ty = |x: &IrExpr| -> Option<Ty> {
                match (&x.ty, &x.kind) {
                    (
                        Ty::Applied(TypeConstructorId::Result, a),
                        IrExprKind::Call { target: almide_ir::CallTarget::Named { .. }, .. },
                    ) if a.len() == 2 && phantom_ok_ty(x).is_none() => Some(a[0].clone()),
                    _ => None,
                }
            };
            // A SINGLE-expression fan (`let r = fan { add(10, 20) }`) IS its expression:
            // there is nothing to run concurrently with, v0's sequential value is the bare
            // result (no tuple; a real-Result thunk keeps its `!`). fan thunks cannot
            // capture `var`s — the rewrite is observation-equal and count-invariant. It
            // previously fell through to the scalar-bind deferred-Const wall (fan_test).
            if exprs.len() == 1 {
                if let Some(ok_ty) = phantom_ok_ty(&exprs[0]) {
                    let mut nx = exprs[0].clone();
                    nx.ty = ok_ty;
                    *e = nx;
                    self.changed = true;
                } else if let Some(ok_ty) = real_result_ok_ty(&exprs[0]) {
                    *e = IrExpr {
                        kind: IrExprKind::Unwrap { expr: Box::new(exprs[0].clone()) },
                        ty: ok_ty,
                        span: e.span.clone(),
                        def_id: e.def_id,
                    };
                    self.changed = true;
                }
                return;
            }
            if exprs.len() < 2 {
                return;
            }
            enum Elem {
                Plain(Ty),
                Unwrap(Ty),
            }
            let classes: Option<Vec<Elem>> = exprs
                .iter()
                .map(|x| {
                    phantom_ok_ty(x)
                        .map(Elem::Plain)
                        .or_else(|| real_result_ok_ty(x).map(Elem::Unwrap))
                })
                .collect();
            let Some(classes) = classes else { return };
            if classes.iter().all(|c| matches!(c, Elem::Plain(_))) {
                // Every element raw — the original direct-tuple rewrite (no binds).
                let elements: Vec<IrExpr> = exprs
                    .iter()
                    .zip(&classes)
                    .map(|(x, c)| {
                        let Elem::Plain(t) = c else { unreachable!() };
                        let mut nx = x.clone();
                        nx.ty = t.clone();
                        nx
                    })
                    .collect();
                *e = IrExpr {
                    kind: IrExprKind::Tuple { elements },
                    ty: e.ty.clone(),
                    span: e.span.clone(),
                    def_id: e.def_id,
                };
                self.changed = true;
                return;
            }
            // Mixed / real-Result elements: the sequential bind-`!` block.
            let mut stmts: Vec<almide_ir::IrStmt> = Vec::with_capacity(exprs.len());
            let mut elements: Vec<IrExpr> = Vec::with_capacity(exprs.len());
            for (x, c) in exprs.iter().zip(&classes) {
                let (val, vty) = match c {
                    Elem::Plain(t) => {
                        let mut nx = x.clone();
                        nx.ty = t.clone();
                        (nx, t.clone())
                    }
                    Elem::Unwrap(t) => (
                        IrExpr {
                            kind: IrExprKind::Unwrap { expr: Box::new(x.clone()) },
                            ty: t.clone(),
                            span: x.span.clone(),
                            def_id: None,
                        },
                        t.clone(),
                    ),
                };
                let var = almide_ir::VarId(self.next_var);
                self.next_var += 1;
                stmts.push(almide_ir::IrStmt {
                    kind: almide_ir::IrStmtKind::Bind {
                        var,
                        mutability: almide_ir::Mutability::Let,
                        ty: vty.clone(),
                        value: val,
                    },
                    span: None,
                });
                elements.push(IrExpr {
                    kind: IrExprKind::Var { id: var },
                    ty: vty,
                    span: None,
                    def_id: None,
                });
            }
            let tuple = IrExpr {
                kind: IrExprKind::Tuple { elements },
                ty: e.ty.clone(),
                span: e.span.clone(),
                def_id: e.def_id,
            };
            *e = IrExpr {
                kind: IrExprKind::Block { stmts, expr: Some(Box::new(tuple)) },
                ty: e.ty.clone(),
                span: e.span.clone(),
                def_id: e.def_id,
            };
            self.changed = true;
        }
    }
    let mut v = V { changed: false, next_var: crate::lower::max_var_id(body) + 1 };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    v.changed.then_some(out)
}
