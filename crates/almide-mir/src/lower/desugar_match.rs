/// Desugar `match opt { some("lit1") => A1, …, none/_ => D }` — an `Option[String]`
/// subject whose Some patterns carry LITERAL payloads (the almide-grammar CLI
/// dispatch `match list.get(args, 1) { some("tree-sitter") => …, _ => usage }`) —
/// into the EXECUTABLE 2-arm form the variant match already lowers:
///   `match opt { some($p) => { if $p == "lit1" then A1 else … else D }, none => D }`.
/// String equality is a `BinOp` (not a call) and the duplicated default sits in a
/// BRANCH (only one side runs), and the count gate counts the SAME desugared tree
/// (desugar-before-both) — so `mir == ir` stays exact. Unit-typed matches only (the
/// grammar dispatch shape); a value match keeps its existing walls.
pub fn desugar_option_str_literal_match(body: &mut IrExpr) {
    use almide_ir::{walk_expr_mut, IrMatchArm, IrMutVisitor, IrPattern};
    use almide_lang::types::constructor::TypeConstructorId;
    struct S {
        next_var: u32,
    }
    impl IrMutVisitor for S {
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            walk_expr_mut(self, expr);
            if !matches!(expr.ty, Ty::Unit) {
                return;
            }
            let IrExprKind::Match { subject, arms } = &expr.kind else { return };
            let is_opt_str = matches!(&subject.ty,
                Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 && matches!(a[0], Ty::String));
            if !is_opt_str || arms.len() < 2 {
                return;
            }
            let (default, lits) = match arms.split_last() {
                Some((last, rest))
                    if matches!(last.pattern, IrPattern::Wildcard | IrPattern::None)
                        && last.guard.is_none() =>
                {
                    (last, rest)
                }
                _ => return,
            };
            let mut cases: Vec<(String, IrExpr)> = Vec::new();
            for a in lits {
                if a.guard.is_some() {
                    return;
                }
                let IrPattern::Some { inner } = &a.pattern else { return };
                let IrPattern::Literal { expr: lit_e } = &**inner else { return };
                let IrExprKind::LitStr { value } = &lit_e.kind else { return };
                cases.push((value.clone(), a.body.clone()));
            }
            let p = VarId(self.next_var);
            self.next_var += 1;
            let pvar = |ty: Ty| IrExpr {
                kind: IrExprKind::Var { id: p },
                ty,
                span: None,
                def_id: None,
            };
            // Build the innermost-first if-chain: … else D.
            let mut chain = default.body.clone();
            for (lit, arm_body) in cases.into_iter().rev() {
                let cond = IrExpr {
                    kind: IrExprKind::BinOp {
                        op: almide_ir::BinOp::Eq,
                        left: Box::new(pvar(Ty::String)),
                        right: Box::new(IrExpr {
                            kind: IrExprKind::LitStr { value: lit },
                            ty: Ty::String,
                            span: None,
                            def_id: None,
                        }),
                    },
                    ty: Ty::Bool,
                    span: None,
                    def_id: None,
                };
                chain = IrExpr {
                    kind: IrExprKind::If {
                        cond: Box::new(cond),
                        then: Box::new(arm_body),
                        else_: Box::new(chain),
                    },
                    ty: Ty::Unit,
                    span: None,
                    def_id: None,
                };
            }
            let new_arms = vec![
                IrMatchArm {
                    pattern: IrPattern::Some {
                        inner: Box::new(IrPattern::Bind { var: p, ty: Ty::String }),
                    },
                    guard: None,
                    body: chain,
                },
                IrMatchArm { pattern: IrPattern::None, guard: None, body: default.body.clone() },
            ];
            let subject = subject.clone();
            *expr = IrExpr {
                kind: IrExprKind::Match { subject, arms: new_arms },
                ty: Ty::Unit,
                span: expr.span.clone(),
                def_id: expr.def_id,
            };
        }
    }
    let mut s = S { next_var: crate::lower::desugar_var_seed() };
    s.visit_expr_mut(body);
}

/// A `match` over a TUPLE LITERAL of SCALAR components whose every arm is a tuple pattern of
/// scalar literals / binds / wildcards (`match (a, b) { (true, true) => "tt", … }` —
/// bool_pair, the truth-table class) — rewrite to the PROVEN hoist + if-chain form:
///   `{ let $t0 = a; let $t1 = b; if $t0 == true and $t1 == true then <arm0> else if … else
///   <last arm> }`
/// First-match semantics IS the if-chain order; the LAST arm becomes the unconditional else
/// (sound: the frontend enforces exhaustiveness, so a value reaching the last test matches
/// it — v0's own codegen compiles `_` the same way). Components hoist ONCE (evaluation
/// order/count preserved); a Bind component prefixes the arm body (`(x, true) => f(x)` →
/// `{ let x = $t0; f(x) }`). No calls duplicated (mir == ir holds).
pub fn desugar_scalar_tuple_literal_match(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    use almide_ir::{BinOp, IrPattern};
    use almide_lang::types::Ty;
    struct V {
        next: u32,
        changed: bool,
    }
    fn admits_arm(p: &IrPattern, n: usize) -> bool {
        matches!(p, IrPattern::Tuple { elements }
            if elements.len() == n
                && elements.iter().all(|c| matches!(c,
                    IrPattern::Wildcard
                        | IrPattern::Bind { .. }
                        | IrPattern::Literal { .. })))
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Match { subject, arms } = &e.kind else { return };
            let IrExprKind::Tuple { elements } = &subject.kind else { return };
            if elements.is_empty()
                || elements.iter().any(|c| is_heap_ty(&c.ty))
                || arms.len() < 2
                || arms.iter().any(|a| a.guard.is_some())
                || arms.iter().any(|a| !admits_arm(&a.pattern, elements.len()))
            {
                return;
            }
            let span = e.span.clone();
            // Hoist each component ONCE into a fresh scalar temp.
            let mut stmts = Vec::with_capacity(elements.len());
            let mut temp_refs = Vec::with_capacity(elements.len());
            for c in elements {
                let t = VarId(self.next);
                self.next += 1;
                stmts.push(IrStmt {
                    kind: IrStmtKind::Bind {
                        var: t,
                        ty: c.ty.clone(),
                        value: c.clone(),
                        mutability: almide_ir::Mutability::Let,
                    },
                    span: span.clone(),
                });
                temp_refs.push(IrExpr {
                    kind: IrExprKind::Var { id: t },
                    ty: c.ty.clone(),
                    span: span.clone(),
                    def_id: None,
                });
            }
            // One arm → (condition over the temps, body with bind prefixes).
            let arm_parts: Vec<(Option<IrExpr>, IrExpr)> = arms
                .iter()
                .map(|a| {
                    let IrPattern::Tuple { elements: pats } = &a.pattern else { unreachable!() };
                    let mut cond: Option<IrExpr> = Option::None;
                    let mut binds: Vec<IrStmt> = Vec::new();
                    for (i, pat) in pats.iter().enumerate() {
                        match pat {
                            IrPattern::Literal { expr } => {
                                let eq = IrExpr {
                                    kind: IrExprKind::BinOp {
                                        op: BinOp::Eq,
                                        left: Box::new(temp_refs[i].clone()),
                                        right: Box::new(expr.clone()),
                                    },
                                    ty: Ty::Bool,
                                    span: span.clone(),
                                    def_id: None,
                                };
                                cond = Some(match cond.take() {
                                    Some(c) => IrExpr {
                                        kind: IrExprKind::BinOp {
                                            op: BinOp::And,
                                            left: Box::new(c),
                                            right: Box::new(eq),
                                        },
                                        ty: Ty::Bool,
                                        span: span.clone(),
                                        def_id: None,
                                    },
                                    Option::None => eq,
                                });
                            }
                            IrPattern::Bind { var, ty } => binds.push(IrStmt {
                                kind: IrStmtKind::Bind {
                                    var: *var,
                                    ty: ty.clone(),
                                    value: temp_refs[i].clone(),
                                    mutability: almide_ir::Mutability::Let,
                                },
                                span: span.clone(),
                            }),
                            IrPattern::Wildcard => {}
                            _ => unreachable!(),
                        }
                    }
                    let body_e = if binds.is_empty() {
                        a.body.clone()
                    } else {
                        IrExpr {
                            kind: IrExprKind::Block { stmts: binds, expr: Some(Box::new(a.body.clone())) },
                            ty: a.body.ty.clone(),
                            span: span.clone(),
                            def_id: a.body.def_id,
                        }
                    };
                    (cond, body_e)
                })
                .collect();
            // Right-fold into the if-chain; the FIRST unconditional arm (or the last arm)
            // terminates the chain as the else (later arms are unreachable by first-match).
            let mut chain: Option<IrExpr> = Option::None;
            for (cond, body_e) in arm_parts.into_iter().rev() {
                chain = Some(match (cond, chain.take()) {
                    (_, Option::None) | (Option::None, _) => body_e,
                    (Some(c), Some(rest)) => IrExpr {
                        kind: IrExprKind::If {
                            cond: Box::new(c),
                            then: Box::new(body_e),
                            else_: Box::new(rest),
                        },
                        ty: e.ty.clone(),
                        span: span.clone(),
                        def_id: e.def_id,
                    },
                });
            }
            *e = IrExpr {
                kind: IrExprKind::Block { stmts, expr: Some(Box::new(chain.expect("chain is Some: the loop above ran at least once (arm_parts.len() == arms.len() >= 2, guarded above) and always assigns Some"))) },
                ty: e.ty.clone(),
                span: span.clone(),
                def_id: e.def_id,
            };
            self.changed = true;
        }
    }
    let mut v = V { next: crate::lower::desugar_var_seed(), changed: false };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    v.changed.then_some(out)
}


/// Rewrite a SCALAR-subject match whose arms are guarded BINDS (`match Package.weight(p) {
/// w if w <= 1 => "envelope", w if w <= 10 => "box", _ => "freight" }`) into a hoisted
/// scalar temp + an `if` chain — the guard-match twin of `desugar_scalar_tuple_literal_match`.
/// The subject evaluates ONCE into a fresh temp; every arm's bind var aliases that temp at
/// the block TOP (a scalar copy, no ownership — guards must see their var before the chain),
/// each guard becomes an `if` condition in arm order, and the single UNGUARDED catch-all
/// (`_` or a bare bind) terminates the chain as the else. Heap-result bodies then lower
/// through the proven heap-result-`if` machinery (previously: an honest wall).
/// Call-count-invariant: the subject and every guard/body appear EXACTLY ONCE
/// (desugar-before-both keeps `mir == ir`).
pub fn desugar_scalar_guard_match(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    use almide_ir::IrPattern;
    struct V {
        next: u32,
        changed: bool,
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Match { subject, arms } = &e.kind else { return };
            if is_heap_ty(&subject.ty) || arms.len() < 2 {
                return;
            }
            // Every arm but the last must be a GUARDED Bind/Wildcard; the last an UNGUARDED
            // Bind/Wildcard catch-all. Literal/ctor patterns stay for the other paths.
            let (last, init) = arms.split_last().expect("arms.len() >= 2, guarded above, so split_last() is Some");
            if last.guard.is_some()
                || !matches!(last.pattern, IrPattern::Wildcard | IrPattern::Bind { .. })
                || init.iter().any(|a| {
                    a.guard.is_none()
                        || !matches!(a.pattern, IrPattern::Wildcard | IrPattern::Bind { .. })
                })
            {
                return;
            }
            let span = e.span.clone();
            let t = VarId(self.next);
            self.next += 1;
            let mut stmts = vec![IrStmt {
                kind: IrStmtKind::Bind {
                    var: t,
                    ty: subject.ty.clone(),
                    value: (**subject).clone(),
                    mutability: almide_ir::Mutability::Let,
                },
                span: span.clone(),
            }];
            let temp_ref = IrExpr {
                kind: IrExprKind::Var { id: t },
                ty: subject.ty.clone(),
                span: span.clone(),
                def_id: None,
            };
            for arm in arms {
                if let IrPattern::Bind { var, ty } = &arm.pattern {
                    stmts.push(IrStmt {
                        kind: IrStmtKind::Bind {
                            var: *var,
                            ty: ty.clone(),
                            value: temp_ref.clone(),
                            mutability: almide_ir::Mutability::Let,
                        },
                        span: span.clone(),
                    });
                }
            }
            // Right-fold the guarded arms over the catch-all body.
            let mut chain = last.body.clone();
            for arm in init.iter().rev() {
                chain = IrExpr {
                    kind: IrExprKind::If {
                        cond: Box::new(arm.guard.clone().expect("every `init` arm has a guard: the early-return above already rejected any init arm with guard.is_none()")),
                        then: Box::new(arm.body.clone()),
                        else_: Box::new(chain),
                    },
                    ty: e.ty.clone(),
                    span: span.clone(),
                    def_id: e.def_id,
                };
            }
            *e = IrExpr {
                kind: IrExprKind::Block { stmts, expr: Some(Box::new(chain)) },
                ty: e.ty.clone(),
                span: span.clone(),
                def_id: e.def_id,
            };
            self.changed = true;
        }
    }
    let mut v = V { next: crate::lower::desugar_var_seed(), changed: false };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    v.changed.then_some(out)
}


/// Read-only pre-scan: would [`desugar_grouped_variant_match`] fire anywhere
/// in the region this fixpoint level OWNS? The branch fixpoint re-runs that
/// row at every level of the nested-arms recursion, and a whole-subtree
/// clone+walk per level made a deep continuation chain quadratic (#1220).
/// The probe calls the SAME decision fn as the rewrite, so the two cannot
/// drift; ids minted into the scratch counter are discarded (the firing
/// decision never depends on the counter's value).
///
/// Exactness: the mutating pass rewrites post-order over the same region, so
/// its FIRST rewrite happens at a node whose subtree is still the original —
/// a node this probe also fires on. Conversely a probe hit means SOME owned
/// node fires on the original tree, so the mutating pass reports `changed`.
fn grouped_variant_match_fires(body: &IrExpr, layouts: &crate::lower::VariantLayouts) -> bool {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct P<'a> {
        layouts: &'a crate::lower::VariantLayouts,
        fires: bool,
    }
    impl IrVisitor for P<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if self.fires {
                return;
            }
            if let IrExprKind::Match { subject, arms } = &e.kind {
                let mut scratch = 0u32;
                if group_option_result_arms(subject, arms, &mut scratch, self.layouts).is_some() {
                    self.fires = true;
                    return;
                }
            }
            walk_expr(self, e);
        }
    }
    let mut p = P { layouts, fires: false };
    // Root-level check, then the owned region only.
    if let IrExprKind::Match { subject, arms } = &body.kind {
        let mut scratch = 0u32;
        if group_option_result_arms(subject, arms, &mut scratch, layouts).is_some() {
            return true;
        }
    }
    for_each_owned_region(body, &mut |e| {
        if !p.fires {
            p.visit_expr(e);
        }
    });
    p.fires
}

/// Visit the sub-regions of a branch-fixpoint subtree ROOT that the deep rows
/// own, SKIPPING the child positions `desugar_nested_branch_arms` (the last
/// `BRANCH_PASSES` row) hands to the FULL inner fixpoint: `If` arms, `Match`
/// arm bodies, and the `Block` tail. Every row runs inside those regions when
/// the recursion reaches them, so a deep row re-walking them from an ancestor
/// level only re-verified an already-normalized subtree — once per ancestor,
/// the O(n²) residual of #1220. The skip applies at the ROOT node only:
/// deeper occurrences of these shapes (e.g. a `Match` inside a `Bind` value,
/// or under a `BinOp` operand, whose recursion applies a restricted rewrite
/// rather than the full row pipeline) are NOT fixpoint-covered and their
/// whole subtree is handed to `f`.
fn for_each_owned_region<'e>(root: &'e IrExpr, f: &mut impl FnMut(&'e IrExpr)) {
    match &root.kind {
        IrExprKind::If { cond, .. } => f(cond),
        IrExprKind::Match { subject, arms } => {
            f(subject);
            for a in arms {
                if let Some(g) = &a.guard {
                    f(g);
                }
            }
        }
        IrExprKind::Block { stmts, expr: Some(_) } => {
            for s in stmts {
                match &s.kind {
                    IrStmtKind::Expr { expr } => f(expr),
                    IrStmtKind::Bind { value, .. } => f(value),
                    IrStmtKind::Assign { value, .. } => f(value),
                    _ => {}
                }
            }
        }
        // Any other root shape has no fully-fixpointed child region — hand
        // the whole subtree over.
        _ => f(root),
    }
}
