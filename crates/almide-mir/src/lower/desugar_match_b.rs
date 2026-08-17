
/// Desugar a `match` over a TUPLE subject into element accesses + a linear guard/`if` chain — `match t
/// { ("a", 1) => A, ("a", _) => B, (_, _) => C }` becomes `if t.0 == "a" && t.1 == 1 then A else if
/// t.0 == "a" then B else C`. Each column's LITERAL becomes an `== `-test on `t.<c>`, each BIND is
/// substituted by `t.<c>` in the guard + body, and a trailing all-wildcard/binder arm is the `else`.
/// The trust-spine already lowers tuple index (`t.0`) + the heap-result `if` chain; the TUPLE-pattern
/// match itself was the gap. Requires a pure (`Var`) subject (element re-reads are effect-free) + a
/// trailing catch-all (exhaustiveness); a nested column pattern bails (a later brick).
pub fn desugar_tuple_match(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    struct V {
        changed: bool,
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            if let IrExprKind::Match { subject, arms } = &e.kind {
                if let Some(chain) = rewrite_tuple_match(subject, arms) {
                    *e = chain;
                    self.changed = true;
                }
            }
        }
    }
    let mut v = V { changed: false };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    if v.changed {
        Some(out)
    } else {
        None
    }
}

fn rewrite_tuple_match(subject: &IrExpr, arms: &[almide_ir::IrMatchArm]) -> Option<IrExpr> {
    use almide_ir::{substitute_var_in_expr, BinOp, IrPattern};
    use almide_lang::types::Ty;
    let Ty::Tuple(elem_tys) = &subject.ty else {
        return None;
    };
    let n = elem_tys.len();
    if n == 0 || arms.is_empty() {
        return None;
    }
    // The column source. A `Var` subject is re-read per column via a side-effect-free `t.<c>` index; a
    // TUPLE LITERAL of pure elements (`match ($a, $b) { .. }` — what a multi-field variant regroup
    // produces) uses each element directly. Any other subject (a call) is left to
    // `desugar_match_subject_hoist` to bind first.
    let pure_elems: Option<Vec<IrExpr>> = match &subject.kind {
        IrExprKind::Tuple { elements }
            if elements.len() == n
                && elements.iter().all(|e| {
                    matches!(
                        &e.kind,
                        IrExprKind::Var { .. }
                            | IrExprKind::LitInt { .. }
                            | IrExprKind::LitBool { .. }
                            | IrExprKind::LitFloat { .. }
                    )
                }) =>
        {
            Some(elements.clone())
        }
        _ => None,
    };
    if pure_elems.is_none() && !matches!(&subject.kind, IrExprKind::Var { .. }) {
        return None;
    }
    let result_ty = arms[0].body.ty.clone();
    // `t.<c>` (Var subject) or the c-th tuple-literal element.
    let elem = |c: usize| match &pure_elems {
        Some(elems) => elems[c].clone(),
        None => IrExpr {
            kind: IrExprKind::TupleIndex {
                object: Box::new(subject.clone()),
                index: c,
            },
            ty: elem_tys[c].clone(),
            span: subject.span.clone(),
            def_id: None,
        },
    };
    // Recursively fold the arms into a right-nested `if`/`else` chain.
    fn build(
        arms: &[almide_ir::IrMatchArm],
        n: usize,
        subject: &IrExpr,
        elem: &dyn Fn(usize) -> IrExpr,
        result_ty: &Ty,
    ) -> Option<IrExpr> {
        let (first, rest) = arms.split_first()?;
        // Build the literal `==` tests and the bind substitution for this arm.
        let mut conds: Vec<IrExpr> = Vec::new();
        let mut subst: Vec<(VarId, IrExpr)> = Vec::new();
        match &first.pattern {
            // A whole-tuple catch-all: `_` binds nothing, a binder maps to the whole subject.
            IrPattern::Wildcard => {}
            IrPattern::Bind { var, .. } => subst.push((*var, subject.clone())),
            // A `(c0, c1, ..)` tuple pattern: each scalar column contributes a test or a bind.
            IrPattern::Tuple { elements } if elements.len() == n => {
                for (c, col) in elements.iter().enumerate() {
                    match col {
                        IrPattern::Literal { expr } => conds.push(IrExpr {
                            kind: IrExprKind::BinOp {
                                op: BinOp::Eq,
                                left: Box::new(elem(c)),
                                right: Box::new(expr.clone()),
                            },
                            ty: Ty::Bool,
                            span: None,
                            def_id: None,
                        }),
                        IrPattern::Bind { var, .. } => subst.push((*var, elem(c))),
                        IrPattern::Wildcard => {}
                        _ => return None, // a nested column — a later brick
                    }
                }
            }
            _ => return None,
        }
        // Apply the bind substitution to the guard + body.
        let apply = |e: &IrExpr| -> IrExpr {
            let mut out = e.clone();
            for (v, rep) in &subst {
                out = substitute_var_in_expr(&out, *v, rep);
            }
            out
        };
        let body = apply(&first.body);
        if let Some(g) = &first.guard {
            conds.push(apply(g));
        }
        if conds.is_empty() {
            // A trivially-true arm (all binds/wildcards, no guard) — the catch-all terminator.
            return Some(body);
        }
        // cond = conds[0] && conds[1] && ...
        let cond = conds
            .into_iter()
            .reduce(|a, b| IrExpr {
                kind: IrExprKind::BinOp {
                    op: BinOp::And,
                    left: Box::new(a),
                    right: Box::new(b),
                },
                ty: Ty::Bool,
                span: None,
                def_id: None,
            })
            .expect("conds is non-empty: the is_empty() early-return above already handled that case");
        let else_ = build(rest, n, subject, elem, result_ty)?;
        Some(IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(cond),
                then: Box::new(body),
                else_: Box::new(else_),
            },
            ty: result_ty.clone(),
            span: None,
            def_id: None,
        })
    }
    build(arms, n, subject, &elem, &result_ty)
}


/// Does `e` introduce any BINDER (a `let` bind, lambda, `for..in`, or a binding
/// match pattern)? Used by [`desugar_tuple_variant_match`] to keep VarIds unique:
/// the catch-all body is DUPLICATED per conditional component, and a duplicated
/// binder would give two textual binds the same VarId (the lowering's `value_of`
/// map assumes one bind site per VarId).
fn introduces_binder(e: &IrExpr) -> bool {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    fn pattern_binds(p: &almide_ir::IrPattern) -> bool {
        use almide_ir::IrPattern as P;
        match p {
            P::Bind { .. } => true,
            P::Wildcard | P::None | P::Literal { .. } => false,
            P::Some { inner } | P::Ok { inner } | P::Err { inner } => pattern_binds(inner),
            P::Constructor { args, .. } => args.iter().any(pattern_binds),
            P::Tuple { elements } | P::List { elements } => elements.iter().any(pattern_binds),
            P::RecordPattern { fields, .. } => {
                fields.iter().any(|f| f.pattern.as_ref().map(pattern_binds).unwrap_or(true))
            }
        }
    }
    struct V {
        found: bool,
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            if self.found {
                return;
            }
            match &e.kind {
                IrExprKind::Lambda { .. } | IrExprKind::ForIn { .. } => {
                    self.found = true;
                    return;
                }
                IrExprKind::Block { stmts, .. }
                    if stmts.iter().any(|s| matches!(s.kind, IrStmtKind::Bind { .. })) =>
                {
                    self.found = true;
                    return;
                }
                IrExprKind::Match { arms, .. }
                    if arms.iter().any(|a| pattern_binds(&a.pattern)) =>
                {
                    self.found = true;
                    return;
                }
                _ => {}
            }
            walk_expr_mut(self, e);
        }
    }
    let mut v = V { found: false };
    let mut c = e.clone();
    v.visit_expr_mut(&mut c);
    v.found
}

/// Rewrite a TWO-ARM match over a TUPLE subject whose first arm tests variant/list
/// components (`match (list.get(xs,0), list.get(ys,0)) { (some(a), some(b)) =>
/// some((a, b)), _ => none }`) into per-component temps + NESTED single-subject
/// matches — each component match then rides the proven Option/Result/custom-variant
/// machinery. The catch-all body is DUPLICATED into each conditional component's
/// wildcard arm (branch-exclusive, so it RUNS at most once; desugar-before-both
/// keeps the caps `mir == ir` count exact). To keep VarIds unique under that
/// duplication, the rewrite declines when >1 conditional component and the
/// catch-all body introduces binders ([`introduces_binder`]). The last arm must be
/// `_` or a tuple of Wildcard / `none` / fieldless-ctor components (no binds —
/// exhaustiveness is the frontend's guarantee, the same last-arm-else discipline
/// as every match lowering).
/// Hoist each non-Var tuple component ONCE into a temp; a Var component
/// is used direct. Returns the hoist binds and the per-component refs.
fn hoist_tuple_components(
    elements: &[IrExpr],
    next: &mut u32,
    span: &Option<almide_ir::Span>,
) -> (Vec<IrStmt>, Vec<IrExpr>) {
    // Hoist each non-Var component ONCE into a temp (a Var component is used direct).
    let mut stmts: Vec<IrStmt> = Vec::new();
    let mut refs: Vec<IrExpr> = Vec::new();
    for c in elements {
        if matches!(c.kind, IrExprKind::Var { .. }) {
            refs.push(c.clone());
        } else {
            let t = VarId(*next);
            *next += 1;
            stmts.push(IrStmt {
                kind: IrStmtKind::Bind {
                    var: t,
                    ty: c.ty.clone(),
                    value: c.clone(),
                    mutability: almide_ir::Mutability::Let,
                },
                span: span.clone(),
            });
            refs.push(IrExpr {
                kind: IrExprKind::Var { id: t },
                ty: c.ty.clone(),
                span: span.clone(),
                def_id: None,
            });
        }
    }
    (stmts, refs)
}

pub fn desugar_tuple_variant_match(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    use almide_ir::IrPattern;
    fn conditional(p: &IrPattern) -> bool {
        matches!(
            p,
            IrPattern::Some { .. }
                | IrPattern::None
                | IrPattern::Ok { .. }
                | IrPattern::Err { .. }
                | IrPattern::Constructor { .. }
        ) || matches!(p, IrPattern::List { elements } if elements.is_empty())
    }

    /// The nesting context of [`nest_conditional_columns`] — the match being
    /// rewritten and the per-column material, which only travel together.
    struct NestCtx<'a> {
        e: &'a IrExpr,
        span: Option<almide_ir::Span>,
        pats: &'a [IrPattern],
        refs: &'a [IrExpr],
        els: &'a IrExpr,
        conditional: fn(&IrPattern) -> bool,
    }

    /// Nest the conditional components right-to-left (leftmost test
    /// outermost) around `inner`.
    fn nest_conditional_columns(cx: NestCtx<'_>, mut inner: IrExpr) -> IrExpr {
        // Nest the conditional components right-to-left (leftmost test outermost).
        for (i, p) in cx.pats.iter().enumerate().rev() {
            if !(cx.conditional)(p) {
                continue;
            }
            inner = IrExpr {
                kind: IrExprKind::Match {
                    subject: Box::new(cx.refs[i].clone()),
                    arms: vec![
                        almide_ir::IrMatchArm {
                            pattern: p.clone(),
                            guard: Option::None,
                            body: inner,
                        },
                        almide_ir::IrMatchArm {
                            pattern: IrPattern::Wildcard,
                            guard: Option::None,
                            body: cx.els.clone(),
                        },
                    ],
                },
                ty: cx.e.ty.clone(),
                span: cx.span.clone(),
                def_id: cx.e.def_id,
            };
        }
        inner
    }

    /// Count the conditional columns of arm 1 and gate arm 2 as a valid
    /// default (bare `_`, or an all-trivial tuple of the same width). `None`
    /// = the shape declines.
    fn refinement_column_census(
        pats: &[IrPattern],
        default_pat: &IrPattern,
        conditional: fn(&IrPattern) -> bool,
    ) -> Option<usize> {
        let mut cond_n = 0usize;
        for p in pats {
            if conditional(p) {
                cond_n += 1;
            } else if !matches!(p, IrPattern::Wildcard | IrPattern::Bind { .. }) {
                return None;
            }
        }
        if cond_n == 0 {
            return None;
        }
        match default_pat {
            IrPattern::Wildcard => {}
            IrPattern::Tuple { elements: p2 }
                if p2.len() == pats.len()
                    && p2.iter().all(|p| {
                        matches!(p, IrPattern::Wildcard | IrPattern::None)
                            || matches!(p, IrPattern::Constructor { args, .. } if args.is_empty())
                    }) => {}
            _ => return None,
        }
        Some(cond_n)
    }

    /// One tuple-refinement rewrite at `e` (the 2-arm conditional-column
    /// shape): hoist each non-Var component once, bind the unconditional
    /// components, and nest the conditional columns right-to-left. True =
    /// rewritten in place.
    fn rewrite_tuple_refinement(
        e: &mut IrExpr,
        next: &mut u32,
        conditional: fn(&IrPattern) -> bool,
    ) -> bool {
            let IrExprKind::Match { subject, arms } = &e.kind else { return false };
            let IrExprKind::Tuple { elements } = &subject.kind else { return false };
            if elements.len() < 2 || arms.len() != 2 || arms.iter().any(|a| a.guard.is_some()) {
                return false;
            }
            let IrPattern::Tuple { elements: pats } = &arms[0].pattern else { return false };
            if pats.len() != elements.len() {
                return false;
            }
            let Some(cond_n) = refinement_column_census(pats, &arms[1].pattern, conditional) else {
                return false;
            };
            let els = &arms[1].body;
            if cond_n > 1 && introduces_binder(els) {
                return false;
            }
            let span = e.span.clone();
            let (stmts, refs) = hoist_tuple_components(elements, next, &span);
            // Innermost THEN: arm-1's body prefixed by its unconditional component binds.
            let mut binds: Vec<IrStmt> = Vec::new();
            for (i, p) in pats.iter().enumerate() {
                if let IrPattern::Bind { var, ty } = p {
                    binds.push(IrStmt {
                        kind: IrStmtKind::Bind {
                            var: *var,
                            ty: ty.clone(),
                            value: refs[i].clone(),
                            mutability: almide_ir::Mutability::Let,
                        },
                        span: span.clone(),
                    });
                }
            }
            let mut inner = if binds.is_empty() {
                arms[0].body.clone()
            } else {
                IrExpr {
                    kind: IrExprKind::Block {
                        stmts: binds,
                        expr: Some(Box::new(arms[0].body.clone())),
                    },
                    ty: arms[0].body.ty.clone(),
                    span: span.clone(),
                    def_id: arms[0].body.def_id,
                }
            };
            inner = nest_conditional_columns(NestCtx { e, span: span.clone(), pats, refs: &refs, els, conditional }, inner);
            *e = if stmts.is_empty() {
                inner
            } else {
                IrExpr {
                    kind: IrExprKind::Block { stmts, expr: Some(Box::new(inner)) },
                    ty: e.ty.clone(),
                    span: span.clone(),
                    def_id: e.def_id,
                }
            };
        true
    }

    struct V {
        next: u32,
        changed: bool,
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            if rewrite_tuple_refinement(e, &mut self.next, conditional) {
                self.changed = true;
            }
        }
    }
    let mut v = V { next: crate::lower::desugar_var_seed(), changed: false };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    v.changed.then_some(out)
}

include!("desugar_match_deep.rs");
