
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
    struct V {
        next: u32,
        changed: bool,
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Match { subject, arms } = &e.kind else { return };
            let IrExprKind::Tuple { elements } = &subject.kind else { return };
            if elements.len() < 2 || arms.len() != 2 || arms.iter().any(|a| a.guard.is_some()) {
                return;
            }
            let IrPattern::Tuple { elements: pats } = &arms[0].pattern else { return };
            if pats.len() != elements.len() {
                return;
            }
            let mut cond_n = 0usize;
            for p in pats {
                if conditional(p) {
                    cond_n += 1;
                } else if !matches!(p, IrPattern::Wildcard | IrPattern::Bind { .. }) {
                    return;
                }
            }
            if cond_n == 0 {
                return;
            }
            match &arms[1].pattern {
                IrPattern::Wildcard => {}
                IrPattern::Tuple { elements: p2 }
                    if p2.len() == pats.len()
                        && p2.iter().all(|p| {
                            matches!(p, IrPattern::Wildcard | IrPattern::None)
                                || matches!(p, IrPattern::Constructor { args, .. } if args.is_empty())
                        }) => {}
                _ => return,
            }
            let els = &arms[1].body;
            if cond_n > 1 && introduces_binder(els) {
                return;
            }
            let span = e.span.clone();
            // Hoist each non-Var component ONCE into a temp (a Var component is used direct).
            let mut stmts: Vec<IrStmt> = Vec::new();
            let mut refs: Vec<IrExpr> = Vec::new();
            for c in elements {
                if matches!(c.kind, IrExprKind::Var { .. }) {
                    refs.push(c.clone());
                } else {
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
                    refs.push(IrExpr {
                        kind: IrExprKind::Var { id: t },
                        ty: c.ty.clone(),
                        span: span.clone(),
                        def_id: None,
                    });
                }
            }
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
            // Nest the conditional components right-to-left (leftmost test outermost).
            for (i, p) in pats.iter().enumerate().rev() {
                if !conditional(p) {
                    continue;
                }
                inner = IrExpr {
                    kind: IrExprKind::Match {
                        subject: Box::new(refs[i].clone()),
                        arms: vec![
                            almide_ir::IrMatchArm {
                                pattern: p.clone(),
                                guard: Option::None,
                                body: inner,
                            },
                            almide_ir::IrMatchArm {
                                pattern: IrPattern::Wildcard,
                                guard: Option::None,
                                body: els.clone(),
                            },
                        ],
                    },
                    ty: e.ty.clone(),
                    span: span.clone(),
                    def_id: e.def_id,
                };
            }
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
            self.changed = true;
        }
    }
    let mut v = V { next: max_var_id(body) + 1, changed: false };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    v.changed.then_some(out)
}

/// N-ARM tuple-of-variants match — the Maranget-style column specialization the 2-arm
/// [`desugar_tuple_variant_match`] (which runs FIRST in both chains) declines: 3+ arms, a
/// binder-carrying fall-through arm (`(Leaf(a), Leaf(b)) => …, (l, r) => …` — the #610
/// in-group refinement the deep variant regroup emits), and arbitrary ctor DEPTH in any
/// component (`(Leaf(a), Node(Leaf(b), Leaf(c)))`). Recursively specialize the LEFTMOST
/// conditional column into one single-subject match per ctor head: the head's payload
/// fields bind to FRESH vars (new columns), a row whose column is Bind/Wildcard joins
/// EVERY head's branch (its Bind substituted by the component ref — no duplicate binder),
/// and the trivial-column rows form the `_` default — OMITTED when the heads cover the
/// component's type exhaustively (a reachable-only-through-covered-heads default would
/// embed a NON-exhaustive inner match and wall the whole fn). First-match order is
/// preserved inside every branch; rows after the first all-trivial row prune. A body
/// cloned into >1 branch must not introduce binders ([`introduces_binder`] — VarId
/// uniqueness under duplication); Literal / record / list components decline (the literal
/// tuple chain and the `[]`-column specializer own those). Runs in BOTH chains
/// (desugar-before-both), so duplicated bodies count 1:1 in the caps `mir == ir` gate.
pub fn desugar_tuple_variant_match_deep(
    body: &IrExpr,
    layouts: &crate::lower::VariantLayouts,
) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    use almide_ir::{IrMatchArm, IrPattern};
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;

    /// A dispatchable constructor head in a tuple column. (Guards/Literals/records never
    /// reach here — `comp_ok` gates them out before compilation.)
    #[derive(Clone, PartialEq, Eq)]
    enum HKey {
        User(String),
        Some_,
        None_,
        Ok_,
        Err_,
    }
    fn head_of(p: &IrPattern) -> Option<(HKey, Vec<IrPattern>)> {
        match p {
            IrPattern::Constructor { name, args } => {
                Some((HKey::User(name.clone()), args.clone()))
            }
            IrPattern::Some { inner } => Some((HKey::Some_, vec![(**inner).clone()])),
            IrPattern::None => Some((HKey::None_, vec![])),
            IrPattern::Ok { inner } => Some((HKey::Ok_, vec![(**inner).clone()])),
            IrPattern::Err { inner } => Some((HKey::Err_, vec![(**inner).clone()])),
            _ => None,
        }
    }
    fn trivial(p: &IrPattern) -> bool {
        matches!(p, IrPattern::Wildcard | IrPattern::Bind { .. })
    }
    fn comp_ok(p: &IrPattern) -> bool {
        trivial(p)
            || head_of(p).is_some_and(|(_, args)| args.iter().all(comp_ok))
    }
    /// The declared payload types of `key` when the component has type `cty` — `None`
    /// declines (unknown ctor, arity drift, a still-generic layout).
    fn head_field_tys(
        key: &HKey,
        arity: usize,
        cty: &Ty,
        layouts: &crate::lower::VariantLayouts,
    ) -> Option<Vec<Ty>> {
        match key {
            HKey::User(name) => {
                let (tyname, layout, case) = layouts.lookup_ctor(name)?;
                let _ = tyname;
                if !layout.generics.is_empty() || case.fields.len() != arity {
                    return None;
                }
                Some(case.fields.iter().map(|(_, t)| t.clone()).collect())
            }
            HKey::Some_ => match cty {
                Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 => {
                    Some(vec![a[0].clone()])
                }
                _ => None,
            },
            HKey::Ok_ => match cty {
                Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => {
                    Some(vec![a[0].clone()])
                }
                _ => None,
            },
            HKey::Err_ => match cty {
                Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => {
                    Some(vec![a[1].clone()])
                }
                _ => None,
            },
            HKey::None_ => Some(vec![]),
        }
    }
    /// Do `keys` cover the component's type EXHAUSTIVELY (so the emitted match needs no
    /// `_` arm)? Conservative: an unresolvable/generic layout answers `false` (the caller
    /// then requires a real default or declines).
    fn heads_cover(keys: &[HKey], layouts: &crate::lower::VariantLayouts) -> bool {
        if keys.iter().all(|k| matches!(k, HKey::Some_ | HKey::None_)) {
            return keys.contains(&HKey::Some_) && keys.contains(&HKey::None_);
        }
        if keys.iter().all(|k| matches!(k, HKey::Ok_ | HKey::Err_)) {
            return keys.contains(&HKey::Ok_) && keys.contains(&HKey::Err_);
        }
        if !keys.iter().all(|k| matches!(k, HKey::User(_))) {
            return false;
        }
        let HKey::User(first) = &keys[0] else { return false };
        let Some(tyname) = layouts.ctor_to_type.get(first) else { return false };
        let Some(layout) = layouts.by_type.get(tyname) else { return false };
        !layout.cases.is_empty()
            && layout.cases.iter().all(|c| {
                keys.iter().any(|k| matches!(k, HKey::User(n) if n == c.ctor.as_str()))
            })
    }

    struct Row {
        pats: Vec<IrPattern>,
        body: IrExpr,
        idx: usize,
    }
    /// The recursive column compiler. `refs[i]` is the (Var) expression re-reading column
    /// `i`; `tmpl` supplies the result ty/span/def_id; `emitted[idx]` counts how many
    /// branches cloned original arm `idx`'s body (the duplication gate reads it after).
    fn compile(
        refs: &[IrExpr],
        mut rows: Vec<Row>,
        tmpl: &IrExpr,
        next: &mut u32,
        layouts: &crate::lower::VariantLayouts,
        emitted: &mut Vec<usize>,
    ) -> Option<IrExpr> {
        // First-match pruning: rows after the first all-trivial (always-matching) row are dead.
        if let Some(k) = rows.iter().position(|r| r.pats.iter().all(trivial)) {
            rows.truncate(k + 1);
        }
        let first_all_trivial = rows.first()?.pats.iter().all(trivial);
        if first_all_trivial {
            let r = &rows[0];
            let mut b = r.body.clone();
            for (i, p) in r.pats.iter().enumerate() {
                if let IrPattern::Bind { var, .. } = p {
                    b = almide_ir::substitute_var_in_expr(&b, *var, &refs[i]);
                }
            }
            emitted[r.idx] += 1;
            return Some(b);
        }
        let j = (0..refs.len()).find(|&c| rows.iter().any(|r| !trivial(&r.pats[c])))?;
        // A Bind in the dispatch column names the WHOLE component: substitute the component
        // ref now (once, before the row joins multiple branches) and dispatch on `_`.
        let rows: Vec<Row> = rows
            .into_iter()
            .map(|mut r| {
                if let IrPattern::Bind { var, .. } = &r.pats[j] {
                    r.body = almide_ir::substitute_var_in_expr(&r.body, *var, &refs[j]);
                    r.pats[j] = IrPattern::Wildcard;
                }
                r
            })
            .collect();
        // Ordered ctor heads (first occurrence); a same-head arity drift declines.
        let mut keys: Vec<(HKey, usize)> = Vec::new();
        for r in &rows {
            if let Some((k, args)) = head_of(&r.pats[j]) {
                match keys.iter().find(|(k2, _)| *k2 == k) {
                    Some((_, a)) if *a != args.len() => return None,
                    Some(_) => {}
                    None => keys.push((k, args.len())),
                }
            }
        }
        let mut arms: Vec<IrMatchArm> = Vec::new();
        for (key, arity) in &keys {
            let ftys = head_field_tys(key, *arity, &refs[j].ty, layouts)?;
            let fresh: Vec<(VarId, Ty)> = ftys
                .iter()
                .map(|t| {
                    let v = VarId(*next);
                    *next += 1;
                    (v, t.clone())
                })
                .collect();
            let mut nrefs: Vec<IrExpr> = Vec::with_capacity(refs.len() - 1 + arity);
            nrefs.extend_from_slice(&refs[..j]);
            for (v, t) in &fresh {
                nrefs.push(IrExpr {
                    kind: IrExprKind::Var { id: *v },
                    ty: t.clone(),
                    span: tmpl.span.clone(),
                    def_id: None,
                });
            }
            nrefs.extend_from_slice(&refs[j + 1..]);
            let mut nrows: Vec<Row> = Vec::new();
            for r in &rows {
                match head_of(&r.pats[j]) {
                    Some((k, args)) if k == *key => {
                        let mut np = Vec::with_capacity(nrefs.len());
                        np.extend_from_slice(&r.pats[..j]);
                        np.extend(args);
                        np.extend_from_slice(&r.pats[j + 1..]);
                        nrows.push(Row { pats: np, body: r.body.clone(), idx: r.idx });
                    }
                    Some(_) => {}
                    None => {
                        let mut np = Vec::with_capacity(nrefs.len());
                        np.extend_from_slice(&r.pats[..j]);
                        np.extend(std::iter::repeat(IrPattern::Wildcard).take(*arity));
                        np.extend_from_slice(&r.pats[j + 1..]);
                        nrows.push(Row { pats: np, body: r.body.clone(), idx: r.idx });
                    }
                }
            }
            let branch = compile(&nrefs, nrows, tmpl, next, layouts, emitted)?;
            let mut pat_args: Vec<IrPattern> = fresh
                .iter()
                .map(|(v, t)| IrPattern::Bind { var: *v, ty: t.clone() })
                .collect();
            let pattern = match key {
                HKey::User(name) => {
                    IrPattern::Constructor { name: name.clone(), args: pat_args }
                }
                HKey::Some_ => IrPattern::Some { inner: Box::new(pat_args.remove(0)) },
                HKey::None_ => IrPattern::None,
                HKey::Ok_ => IrPattern::Ok { inner: Box::new(pat_args.remove(0)) },
                HKey::Err_ => IrPattern::Err { inner: Box::new(pat_args.remove(0)) },
            };
            arms.push(IrMatchArm { pattern, guard: None, body: branch });
        }
        let head_keys: Vec<HKey> = keys.iter().map(|(k, _)| k.clone()).collect();
        if !heads_cover(&head_keys, layouts) {
            let mut drows: Vec<Row> = Vec::new();
            for r in &rows {
                if head_of(&r.pats[j]).is_none() {
                    let mut np = r.pats.clone();
                    np.remove(j);
                    drows.push(Row { pats: np, body: r.body.clone(), idx: r.idx });
                }
            }
            if drows.is_empty() {
                // Frontend exhaustiveness says this path is unreachable, but emitting a
                // non-exhaustive inner match would wall — decline instead.
                return None;
            }
            let mut nrefs = refs.to_vec();
            nrefs.remove(j);
            let dbody = compile(&nrefs, drows, tmpl, next, layouts, emitted)?;
            arms.push(IrMatchArm { pattern: IrPattern::Wildcard, guard: None, body: dbody });
        }
        Some(IrExpr {
            kind: IrExprKind::Match { subject: Box::new(refs[j].clone()), arms },
            ty: tmpl.ty.clone(),
            span: tmpl.span.clone(),
            def_id: tmpl.def_id,
        })
    }

    struct V<'a> {
        next: u32,
        layouts: &'a crate::lower::VariantLayouts,
        changed: bool,
    }
    impl IrMutVisitor for V<'_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Match { subject, arms } = &e.kind else { return };
            let IrExprKind::Tuple { elements } = &subject.kind else { return };
            let n = elements.len();
            if n < 2 || arms.is_empty() || arms.iter().any(|a| a.guard.is_some()) {
                return;
            }
            // Normalize arms to pattern ROWS: a Tuple pattern of matching width, or a
            // trailing top-level `_` (an all-wildcard row). Anything else declines.
            let mut rows: Vec<Row> = Vec::with_capacity(arms.len());
            let mut any_cond = false;
            for (idx, a) in arms.iter().enumerate() {
                let pats: Vec<IrPattern> = match &a.pattern {
                    IrPattern::Tuple { elements: ps } if ps.len() == n => ps.clone(),
                    IrPattern::Wildcard => vec![IrPattern::Wildcard; n],
                    _ => return,
                };
                if !pats.iter().all(comp_ok) {
                    return;
                }
                if pats.iter().any(|p| !trivial(p)) {
                    any_cond = true;
                }
                rows.push(Row { pats, body: a.body.clone(), idx });
            }
            if !any_cond {
                return;
            }
            // Hoist each non-Var component ONCE into a temp (a Var component reads direct).
            let span = e.span.clone();
            let mut stmts: Vec<IrStmt> = Vec::new();
            let mut refs: Vec<IrExpr> = Vec::new();
            for c in elements {
                if matches!(c.kind, IrExprKind::Var { .. }) {
                    refs.push(c.clone());
                } else {
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
                    refs.push(IrExpr {
                        kind: IrExprKind::Var { id: t },
                        ty: c.ty.clone(),
                        span: span.clone(),
                        def_id: None,
                    });
                }
            }
            let mut emitted = vec![0usize; arms.len()];
            let mut next = self.next;
            let Some(compiled) =
                compile(&refs, rows, e, &mut next, self.layouts, &mut emitted)
            else {
                return;
            };
            // Duplication gates: a body cloned into >1 branch must be binder-free, and the
            // whole tree must stay small (the same blow-up discipline as heap-branches).
            for (idx, count) in emitted.iter().enumerate() {
                if *count > 1 && introduces_binder(&arms[idx].body) {
                    return;
                }
            }
            if count_expr_nodes(&compiled) > 50_000 {
                return;
            }
            self.next = next;
            *e = if stmts.is_empty() {
                compiled
            } else {
                IrExpr {
                    kind: IrExprKind::Block { stmts, expr: Some(Box::new(compiled)) },
                    ty: e.ty.clone(),
                    span,
                    def_id: e.def_id,
                }
            };
            self.changed = true;
        }
    }
    let mut v = V { next: max_var_id(body) + 1, layouts, changed: false };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    v.changed.then_some(out)
}
