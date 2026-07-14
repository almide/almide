/// Desugar `fan.race` / `fan.any` over a LITERAL thunk list by INLINING each thunk's body — avoiding a
/// `List[funcref]` (unrepresentable in v1) entirely. On wasm the fan combinators are deterministic:
///   `fan.race([() => t0, () => t1, …])`  ≡  `t0`           (the FIRST thunk settles first)
///   `fan.any([() => t0, () => t1, …])`   ≡  `match t0 { ok(v) => ok(v), err(_) => <any of the rest> }`
///                                             (the FIRST Ok in list order; the last thunk's result is
///                                              the fallback if every earlier one errs)
/// Each `t_i` is a no-param lambda whose body is a `Result[T, String]` (an effect fn call). The inlined
/// form is a plain match-over-a-call chain, all in v1's subset. A NON-literal thunk list (`let ts =
/// […]; fan.race(ts)`) has no inlinable bodies → left for the call-site purity wall.
pub fn desugar_fan_race_any(body: &IrExpr, _next_var: &mut u32) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    use almide_ir::{CallTarget, IrMatchArm, IrPattern};
    struct V {
        changed: bool,
        next_var: u32,
    }
    // Extract the no-param thunk bodies of a `fan.race`/`fan.any` LITERAL-list call, or `None` if the
    // expr is not such a call (a non-literal thunk list has no inlinable bodies → declines).
    fn fan_bodies(e: &IrExpr, want: &str) -> Option<Vec<IrExpr>> {
        let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &e.kind
        else {
            return None;
        };
        if module.as_str() != "fan" || func.as_str() != want {
            return None;
        }
        let [arg] = &args[..] else { return None };
        let IrExprKind::List { elements } = &arg.kind else {
            return None;
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
    // `fan.timeout(ms, () => body)` → `body`: v0's WASM leg has NO timeout — it calls
    // the thunk INLINE (calls_p4.rs "just call fn (no timeout in WASM)"), so the literal-
    // thunk form desugars to the body itself (the fan.race head-settle precedent). The ms
    // arg is DISCARDED — gated CALL-FREE so no effect (or count) is dropped with it; a
    // non-literal thunk declines (no inlinable body → the honest wall).
    fn fan_timeout_body(e: &IrExpr) -> Option<IrExpr> {
        let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &e.kind
        else {
            return None;
        };
        if module.as_str() != "fan" || func.as_str() != "timeout" {
            return None;
        }
        let [ms, thunk] = &args[..] else { return None };
        if crate::lower::expr_contains_call(ms) {
            return None;
        }
        let IrExprKind::Lambda { params, body, .. } = &thunk.kind else {
            return None;
        };
        if !params.is_empty() {
            return None;
        }
        Some((**body).clone())
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            // `fan.timeout(ms, () => body)` — inline the thunk body (v0-wasm semantics).
            if let Some(b) = fan_timeout_body(e) {
                *e = b;
                self.changed = true;
                // fall through — the substituted body is walked below.
            }
            // PRE-order: `match fan.any([() => t0, …]) { ok(pat) => okbody, err(epat) => errbody }` —
            // INLINE the outer arms into each thunk level (avoiding the intermediate Result + a
            // match-over-match). Each thunk `t_i` runs, an Ok takes `okbody`, an Err falls to the NEXT
            // thunk; the LAST thunk's Err takes the original `errbody` (the all-errored fallback). Only
            // one arm ever executes, so duplicating `okbody` per level is dead-code-safe.
            if let IrExprKind::Match { subject, arms } = &e.kind {
                if arms.len() == 2 {
                    if let Some(bodies) = fan_bodies(subject, "any") {
                        let ty = e.ty.clone();
                        let ok_arm = arms.iter().find(|a| matches!(a.pattern, IrPattern::Ok { .. }));
                        let err_arm = arms.iter().find(|a| matches!(a.pattern, IrPattern::Err { .. }));
                        if let (Some(ok_arm), Some(err_arm)) = (ok_arm, err_arm) {
                            // The ALL-FAILED fallback is v0's fixed `fan.any: all candidates failed`
                            // Err (NOT the last thunk's own error): run the outer `err` arm's body with
                            // its bound var substituted by that literal (a `Wildcard` err pattern just
                            // runs the body). Then wrap each thunk: `match t_i { ok(pat) => okbody,
                            // err(_) => rest }` — an Ok short-circuits, an Err falls through in order.
                            let msg = IrExpr {
                                kind: IrExprKind::LitStr {
                                    value: "fan.any: all candidates failed".to_string(),
                                },
                                ty: almide_lang::types::Ty::String,
                                span: None,
                                def_id: None,
                            };
                            let mut acc = match &err_arm.pattern {
                                IrPattern::Err { inner } => match &**inner {
                                    IrPattern::Bind { var, .. } => {
                                        almide_ir::substitute_var_in_expr(&err_arm.body, *var, &msg)
                                    }
                                    _ => err_arm.body.clone(),
                                },
                                _ => err_arm.body.clone(),
                            };
                            for tb in bodies.into_iter().rev() {
                                acc = IrExpr {
                                    kind: IrExprKind::Match {
                                        subject: Box::new(tb),
                                        arms: vec![
                                            ok_arm.clone(),
                                            IrMatchArm {
                                                pattern: IrPattern::Err {
                                                    inner: Box::new(IrPattern::Wildcard),
                                                },
                                                guard: None,
                                                body: acc,
                                            },
                                        ],
                                    },
                                    ty: ty.clone(),
                                    span: None,
                                    def_id: None,
                                };
                            }
                            *e = acc;
                            self.changed = true;
                            walk_expr_mut(self, e);
                            return;
                        }
                    }
                }
            }
            // BIND-VALUE / BLOCK-TAIL positions for the settle/any VALUE rewrites: an
            // `!`-wrapped `fan.any(…)!` must stay for the effect-unwrap desugar (which builds
            // the match shape the PRE-order inliner above handles) — rewriting under the
            // Unwrap left a match-over-match the subject tracking cannot follow (the
            // fan_any_allfail regression, by-name diff).
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
            walk_expr_mut(self, e);
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
            // real Err path.
            if let Some(bodies) = fan_bodies(e, "race") {
                let orig_ty = e.ty.clone();
                let t0 = bodies.into_iter().next().unwrap();
                *e = if crate::lower::is_result_ty(&orig_ty) && !crate::lower::is_result_ty(&t0.ty) {
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
        fn rewrite_settle_any(&mut self, e: &mut IrExpr) {
            use almide_ir::{IrMatchArm, IrPattern};
            // `fan.settle([…])` as a bind value / tail — the results list literal.
            // A PURE thunk's body is bare `T` while settle's checked type is
            // `List[Result[T, E]]` (FanLowering's phantom-Result convention) — wrap each
            // non-Result body in a genuine `ok(...)` so the literal's elements match its
            // element type (the B115 `fan.race` contract-preservation fix, settle's turn:
            // without it the raw `List[Int]` bodies hit the List[heap]-literal wall).
            if let Some(bodies) = fan_bodies(e, "settle") {
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
            // `fan.any([…])` as a bind value / tail — the first-Ok chain VALUE:
            // `match t0 { ok($x) => ok($x), err(_) => <next … err("fan.any: all candidates
            // failed")> }`. The match-subject shape (pre-order) already inlined outer arms.
            if let Some(bodies) = fan_bodies(e, "any") {
                use almide_lang::types::constructor::TypeConstructorId;
                use almide_lang::types::Ty;
                let ok_ty = match &e.ty {
                    Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => a[0].clone(),
                    _ => return,
                };
                let ty = e.ty.clone();
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
                    ty: ty.clone(),
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
                                        ty: ty.clone(),
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
                        ty: ty.clone(),
                        span: None,
                        def_id: None,
                    };
                }
                *e = acc;
                self.changed = true;
            }
        }
    }
    let mut v = V { changed: false, next_var: max_var_id(body) + 1 };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    if v.changed {
        Some(out)
    } else {
        None
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
                        Some(a[0].clone())
                    }
                    _ if !crate::lower::is_result_ty(&x.ty) => Some(x.ty.clone()),
                    _ => None,
                }
            };
            if exprs.len() < 2 || exprs.iter().any(|x| phantom_ok_ty(x).is_none()) {
                return;
            }
            let elements: Vec<IrExpr> = exprs
                .iter()
                .map(|x| {
                    let mut nx = x.clone();
                    nx.ty = phantom_ok_ty(x).expect("gated above");
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
        }
    }
    let mut v = V { changed: false };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    v.changed.then_some(out)
}
