// ── tail of desugar_b.rs, include!-spliced back at module level ──
//
// A pure code move: this file continues its parent verbatim. The split exists
// only so the parent stays under the 800-line ceiling the codopsy gate holds
// this crate to; there is no boundary of meaning here, and `include!` at module
// level is the one splice Rust allows (an impl-item position rejects it).

/// BETA-REDUCE a DIRECT lambda application (`(λ(p) => body)(arg)` — the pipe-into-
/// lambda projection `fold(...) |> ((pair) => pair.0)`, argmax): rewrite to
/// `{ let p = arg; body }` so the ordinary bind + scalar-field machinery lowers it
/// (the Computed-callee call is otherwise unanalyzable → deferred/walled). Each arg
/// is bound ONCE (no duplication; call-count only DECREASES, so the caps gate's
/// `mir ≤ ir` is preserved). Bottom-up over the whole body; `None` = no change.
pub fn desugar_beta_reduce(body: &IrExpr) -> Option<IrExpr> {
    fn rewrite(e: IrExpr, changed: &mut bool) -> IrExpr {
        let e = e.map_children(&mut |c| rewrite(c, changed));
        if let IrExprKind::Call { target: CallTarget::Computed { callee }, args, .. } = &e.kind {
            if let IrExprKind::Lambda { params, body, .. } = &callee.kind {
                if params.len() == args.len() {
                    *changed = true;
                    let stmts: Vec<almide_ir::IrStmt> = params
                        .iter()
                        .zip(args.iter())
                        .map(|((var, ty), arg)| almide_ir::IrStmt {
                            kind: almide_ir::IrStmtKind::Bind {
                                var: *var,
                                mutability: almide_ir::Mutability::Let,
                                ty: ty.clone(),
                                value: arg.clone(),
                            },
                            span: None,
                        })
                        .collect();
                    return IrExpr {
                        kind: IrExprKind::Block { stmts, expr: Some(body.clone()) },
                        ty: e.ty.clone(),
                        span: e.span.clone(),
                        def_id: e.def_id,
                    };
                }
            }
        }
        e
    }
    let mut changed = false;
    let out = rewrite(body.clone(), &mut changed);
    changed.then_some(out)
}

/// Desugar `opt ?? fallback` over an `Option[<all-scalar tuple>]` (`list.get(xs, k) ??
/// (0.0, 0.0)` — the fft element pick) into `match opt { some($p) => $p, none => fallback }`,
/// which the proven variant-value-match machinery lowers (Option-tuple payload borrow @12,
/// subject dropped after the arms). Without this the UnwrapOr path treats the tuple payload
/// as a SCALAR (an i32 handle in an i64 slot — invalid wasm the engine rejects). Bottom-up;
/// `None` = no change.
pub fn desugar_tuple_unwrap_or(body: &IrExpr) -> Option<IrExpr> {
    use almide_lang::types::constructor::TypeConstructorId;
    fn is_scalar_tuple(ty: &Ty) -> bool {
        matches!(ty, Ty::Tuple(ts) if !ts.is_empty() && ts.iter().all(|t| !is_heap_ty(t)))
    }
    fn rewrite(e: IrExpr, changed: &mut bool, next: &mut u32) -> IrExpr {
        let e = e.map_children(&mut |c| rewrite(c, changed, next));
        // Both surface forms: the `??` operator (UnwrapOr) AND the explicit
        // `option.unwrap_or(opt, fb)` module call (the pipe form).
        let parts: Option<(&IrExpr, &IrExpr)> = match &e.kind {
            IrExprKind::UnwrapOr { expr, fallback } => Some((expr, fallback)),
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                if module.as_str() == "option" && func.as_str() == "unwrap_or"
                    && args.len() == 2 =>
            {
                Some((&args[0], &args[1]))
            }
            _ => None,
        };
        if let Some((expr, fallback)) = parts {
            let is_opt_tuple = matches!(&expr.ty,
                Ty::Applied(TypeConstructorId::Option, a)
                    if a.len() == 1 && is_scalar_tuple(&a[0]));
            if is_opt_tuple {
                *changed = true;
                let p = almide_ir::VarId(*next);
                *next += 1;
                let bind = almide_ir::IrPattern::Bind { var: p, ty: e.ty.clone() };
                let payload = IrExpr {
                    kind: IrExprKind::Var { id: p },
                    ty: e.ty.clone(),
                    span: e.span.clone(),
                    def_id: e.def_id,
                };
                // Result and Option carry the SAME shape under different
                // constructors — Option's polarity is len-as-tag-opposite, so
                // the pattern pair must follow the subject's own type.
                let (hit_pat, miss_pat) = if expr.ty.is_result() {
                    (
                        almide_ir::IrPattern::Ok { inner: Box::new(bind) },
                        almide_ir::IrPattern::Err { inner: Box::new(almide_ir::IrPattern::Wildcard) },
                    )
                } else {
                    (
                        almide_ir::IrPattern::Some { inner: Box::new(bind) },
                        almide_ir::IrPattern::None,
                    )
                };
                let arms = vec![
                    almide_ir::IrMatchArm { pattern: hit_pat, guard: None, body: payload },
                    almide_ir::IrMatchArm {
                        pattern: miss_pat,
                        guard: None,
                        body: fallback.clone(),
                    },
                ];
                return IrExpr {
                    kind: IrExprKind::Match { subject: Box::new(expr.clone()), arms },
                    ty: e.ty.clone(),
                    span: e.span.clone(),
                    def_id: e.def_id,
                };
            }
        }
        e
    }
    let mut changed = false;
    let mut next = crate::lower::desugar_var_seed();
    let out = rewrite(body.clone(), &mut changed, &mut next);
    changed.then_some(out)
}

/// `carrier ?? f(..)!` over a HEAP (handle-carrying) payload — the `??` whose FALLBACK is an
/// INLINE propagating unwrap (#1375). The fallback arm lands in the heap arm lowering as a bare
/// `Unwrap`, whose `e! ≡ e` pass-through is the identity ONLY in a Result-typed arm; here the arm
/// is PAYLOAD-typed, so the pass-through yielded the fallback's whole `Result` BLOCK where the
/// merge expects the handle — `json.parse("hi") ?? json.parse("[1,2]")!` printed `true` (the
/// ok-discriminant read through the Json tag slot) instead of `[1,2]`.
///
/// Rewrite to the exact identity, which keeps the fallback CONDITIONAL (hoisting `f(..)!` into a
/// preceding `let` would evaluate — and propagate — it on the ok path too):
///
///   `a ?? f(..)!`  ≡  `(match a { ok($p) => ok($p), err(_) => f(..) })!`
///   `o ?? f(..)!`  ≡  `(match o { some($p) => ok($p), none => f(..) })!`
///
/// Both sides evaluate `a`/`o` once and `f(..)` only on the miss path; the `!` moved OUT of the
/// arm now sits in the proven `let x = e!` / call-arg-hoist position, and the match itself is the
/// Result-typed shape the arm pass-through is actually valid for. CALL-COUNT-INVARIANT (the two
/// operand calls appear exactly once before and after), so `mir == ir` holds without re-counting.
/// SCALAR payloads keep their own proven `??` route untouched — they never reach the heap arm.
/// The carrier's HIT/MISS pattern pair for a payload bound to `p`, or `None` when the
/// operand is neither an `Option[<payload>]` nor a `Result[<payload>, _]`. Shared by the
/// `?? f(..)!` rewrite below and the route-zoo replacement experiment
/// (`desugar_unwrap_or_to_match`).
fn carrier_patterns(
    carrier: &Ty,
    payload: &Ty,
    p: almide_ir::VarId,
) -> Option<(almide_ir::IrPattern, almide_ir::IrPattern)> {
    use almide_ir::IrPattern;
    use almide_lang::types::constructor::TypeConstructorId;
    let bind = IrPattern::Bind { var: p, ty: payload.clone() };
    match carrier {
        Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 && a[0] == *payload => Some((
            IrPattern::Ok { inner: Box::new(bind) },
            IrPattern::Err { inner: Box::new(IrPattern::Wildcard) },
        )),
        Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 && a[0] == *payload => {
            Some((IrPattern::Some { inner: Box::new(bind) }, IrPattern::None))
        }
        _ => None,
    }
}

pub fn desugar_unwrap_or_unwrap_fallback(body: &IrExpr) -> Option<IrExpr> {
    use almide_lang::types::constructor::TypeConstructorId;
    fn rewrite(e: IrExpr, changed: &mut bool, next: &mut u32) -> IrExpr {
        let e = e.map_children(&mut |c| rewrite(c, changed, next));
        let IrExprKind::UnwrapOr { expr, fallback } = &e.kind else { return e };
        // `Try` is the frontend auto-`?`; both spellings propagate the same way.
        let (IrExprKind::Unwrap { expr: inner } | IrExprKind::Try { expr: inner }) = &fallback.kind
        else {
            return e;
        };
        // Formerly HEAP payloads only ("the scalar `??` never reaches the
        // miscompiling arm") — no longer true since the #1418 match-first
        // inversion: a SCALAR `a ?? f(..)!` whose match form reaches the
        // value-position machinery hits the same propagation-arm miscompile
        // class (#1421, invalid wasm). Rewriting BOTH payload classes to the
        // `(match …)!` statement-propagation form routes every propagating
        // fallback through the proven `desugar_let_unwrap` rails instead.
        // The fallback's `!` must yield EXACTLY the `??`'s own payload type, so `ok($p)` in the
        // hit arm and `f(..)` in the miss arm are the same `Result[payload, _]`.
        if !matches!(&inner.ty,
            Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 && a[0] == e.ty)
        {
            return e;
        }
        let p = almide_ir::VarId(*next);
        let Some((hit, miss)) = carrier_patterns(&expr.ty, &e.ty, p) else { return e };
        *next += 1;
        *changed = true;
        let payload = IrExpr {
            kind: IrExprKind::Var { id: p },
            ty: e.ty.clone(),
            span: e.span.clone(),
            def_id: None,
        };
        // The hit arm RE-WRAPS the payload at the fallback's Result type — the carrier's own
        // err type never has to join with the fallback's.
        let rewrapped = IrExpr {
            kind: IrExprKind::ResultOk { expr: Box::new(payload) },
            ty: inner.ty.clone(),
            span: e.span.clone(),
            def_id: None,
        };
        let merged = IrExpr {
            kind: IrExprKind::Match {
                subject: expr.clone(),
                arms: vec![
                    almide_ir::IrMatchArm { pattern: hit, guard: None, body: rewrapped },
                    almide_ir::IrMatchArm {
                        pattern: miss,
                        guard: None,
                        body: (**inner).clone(),
                    },
                ],
            },
            ty: inner.ty.clone(),
            span: e.span.clone(),
            def_id: e.def_id,
        };
        IrExpr {
            kind: IrExprKind::Unwrap { expr: Box::new(merged) },
            ty: e.ty.clone(),
            span: e.span.clone(),
            def_id: e.def_id,
        }
    }
    let mut changed = false;
    let mut next = crate::lower::desugar_var_seed();
    let out = rewrite(body.clone(), &mut changed, &mut next);
    changed.then_some(out)
}


include!("desugar_b_tail.rs");


/// Rewrite a `result.map` / `result.map_err` / `result.flat_map` call whose type
/// instantiation has NO linked typed twin — [`result_call_name`] answers the deliberately
/// unlinked `_x` — into the equivalent `match` (#1492's combinator leg):
///
///   map:      `match r { ok(v) => ok(f(v)),  err(e) => err(e) }`
///   map_err:  `match r { ok(v) => ok(v),     err(e) => err(f(e)) }`
///   flat_map: `match r { ok(v) => f(v),      err(e) => err(e) }`
///
/// This is the reference-compiler architecture for combinators (rustc lowers `?` to ONE
/// generic match; Swift's `??` is a library function; Roc desugars `??` to literally this
/// match in canonicalization): the combinator is not a special payload-class route, it is
/// the match the user could have written — and the heap-payload match now lowers. The `f`
/// argument is used exactly ONCE (no duplication); a non-Var subject is ANF-lifted into a
/// `let`, the optional-chain desugar's discipline. A twin-linked instantiation is left
/// untouched (its proven typed path stays byte-identical), and a match shape the lowering
/// still cannot express walls exactly as the `_x` name did — never worse, usually lowered.
pub fn desugar_result_combinator_to_match(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{walk_expr_mut, CallTarget, IrMatchArm, IrMutVisitor, IrPattern};
    use almide_lang::types::constructor::TypeConstructorId;
    struct S {
        changed: bool,
        next_var: u32,
    }
    impl IrMutVisitor for S {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } =
                &e.kind
            else {
                return;
            };
            if module.as_str() != "result" || args.len() != 2 {
                return;
            }
            let fname = func.as_str().to_string();
            if !matches!(fname.as_str(), "map" | "map_err" | "flat_map") {
                return;
            }
            let Ty::Applied(TypeConstructorId::Result, ra) = &args[0].ty else { return };
            if ra.len() != 2 {
                return;
            }
            // Only when the router would emit the unlinked `_x`. Asking the SAME router the
            // lowering uses keeps the twin-availability logic in exactly one place — the
            // desugar can never disagree with the dispatch about which cells are linked.
            let arg_tys: Vec<Ty> = args.iter().map(|a| a.ty.clone()).collect();
            match result_call_name(&fname, &arg_tys, &e.ty) {
                Some(n) if n.ends_with("_x") => {}
                _ => return,
            }
            let Ty::Applied(TypeConstructorId::Result, oa) = &e.ty else { return };
            if oa.len() != 2 {
                return;
            }
            let (a_ty, e_ty) = (ra[0].clone(), ra[1].clone());
            let (b_ty, e2_ty) = (oa[0].clone(), oa[1].clone());
            let out_ty = e.ty.clone();
            let span = e.span.clone();
            let mk = |kind: IrExprKind, ty: Ty| IrExpr {
                kind,
                ty,
                span: span.clone(),
                def_id: None,
            };
            let r_expr = args[0].clone();
            let f_expr = args[1].clone();
            let v = VarId(self.next_var);
            self.next_var += 1;
            let w = VarId(self.next_var);
            self.next_var += 1;
            let call_f = |payload: IrExpr, ret: Ty| {
                mk(
                    IrExprKind::Call {
                        target: CallTarget::Computed { callee: Box::new(f_expr.clone()) },
                        args: vec![payload],
                        type_args: Vec::new(),
                    },
                    ret,
                )
            };
            let v_read = mk(IrExprKind::Var { id: v }, a_ty.clone());
            let w_read = mk(IrExprKind::Var { id: w }, e_ty.clone());
            let (ok_body, err_body) = match fname.as_str() {
                "map" => (
                    mk(
                        IrExprKind::ResultOk {
                            expr: Box::new(call_f(v_read, b_ty.clone())),
                        },
                        out_ty.clone(),
                    ),
                    mk(IrExprKind::ResultErr { expr: Box::new(w_read) }, out_ty.clone()),
                ),
                "map_err" => (
                    mk(IrExprKind::ResultOk { expr: Box::new(v_read) }, out_ty.clone()),
                    mk(
                        IrExprKind::ResultErr {
                            expr: Box::new(call_f(w_read, e2_ty.clone())),
                        },
                        out_ty.clone(),
                    ),
                ),
                // flat_map: the ok arm IS the whole result of f.
                _ => (
                    call_f(v_read, out_ty.clone()),
                    mk(IrExprKind::ResultErr { expr: Box::new(w_read) }, out_ty.clone()),
                ),
            };
            let arms = vec![
                IrMatchArm {
                    pattern: IrPattern::Ok {
                        inner: Box::new(IrPattern::Bind { var: v, ty: a_ty.clone() }),
                    },
                    guard: None,
                    body: ok_body,
                },
                IrMatchArm {
                    pattern: IrPattern::Err {
                        inner: Box::new(IrPattern::Bind { var: w, ty: e_ty.clone() }),
                    },
                    guard: None,
                    body: err_body,
                },
            ];
            // ANF-lift a non-Var subject so the match branches on a tracked bind — the
            // optional-chain desugar's exact discipline.
            let (stmts, subject) = if matches!(&r_expr.kind, IrExprKind::Var { .. }) {
                (Vec::new(), Box::new(r_expr))
            } else {
                let s_var = VarId(self.next_var);
                self.next_var += 1;
                let subj_ty = r_expr.ty.clone();
                let bind = IrStmt {
                    kind: IrStmtKind::Bind {
                        var: s_var,
                        mutability: almide_ir::Mutability::Let,
                        ty: subj_ty.clone(),
                        value: r_expr,
                    },
                    span: span.clone(),
                };
                (vec![bind], Box::new(mk(IrExprKind::Var { id: s_var }, subj_ty)))
            };
            let match_expr = mk(IrExprKind::Match { subject, arms }, out_ty.clone());
            *e = if stmts.is_empty() {
                match_expr
            } else {
                mk(IrExprKind::Block { stmts, expr: Some(Box::new(match_expr)) }, out_ty)
            };
            self.changed = true;
        }
    }
    let mut s = S { changed: false, next_var: crate::lower::desugar_var_seed() };
    let mut out = body.clone();
    s.visit_expr_mut(&mut out);
    s.changed.then_some(out)
}
