// The N-ARM tuple-of-variants column specializer (Maranget-style) — the
// deep sibling of desugar_match_b's 2-arm refinement. include!-spliced from
// desugar_match_b.rs, sharing the lower module's imports.

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
            HKey::Some_ => applied_payload(cty, TypeConstructorId::Option, 1, 0),
            HKey::Ok_ => applied_payload(cty, TypeConstructorId::Result, 2, 0),
            HKey::Err_ => applied_payload(cty, TypeConstructorId::Result, 2, 1),
            HKey::None_ => Some(vec![]),
        }
    }
    /// The payload slot `idx` of `cty` when it is `ctor` applied at `arity`
    /// — the Some/Ok/Err rows of [`head_field_tys`].
    fn applied_payload(cty: &Ty, ctor: TypeConstructorId, arity: usize, idx: usize) -> Option<Vec<Ty>> {
        match cty {
            Ty::Applied(c, a) if *c == ctor && a.len() == arity => Some(vec![a[idx].clone()]),
            _ => None,
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

    /// Normalize arms to pattern ROWS: a Tuple pattern of matching width, or
    /// a trailing top-level `_` (an all-wildcard row). `None` = a row is out
    /// of shape, or no row has a conditional column (nothing to specialize).
    fn normalize_pattern_rows(arms: &[almide_ir::IrMatchArm], n: usize) -> Option<Vec<Row>> {
        let mut rows: Vec<Row> = Vec::with_capacity(arms.len());
        let mut any_cond = false;
        for (idx, a) in arms.iter().enumerate() {
            let pats: Vec<IrPattern> = match &a.pattern {
                IrPattern::Tuple { elements: ps } if ps.len() == n => ps.clone(),
                IrPattern::Wildcard => vec![IrPattern::Wildcard; n],
                _ => return None,
            };
            if !pats.iter().all(comp_ok) {
                return None;
            }
            if pats.iter().any(|p| !trivial(p)) {
                any_cond = true;
            }
            rows.push(Row { pats, body: a.body.clone(), idx });
        }
        any_cond.then_some(rows)
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
            return Some(trivial_row_body(&rows[0], refs, emitted));
        }
        let j = (0..refs.len()).find(|&c| rows.iter().any(|r| !trivial(&r.pats[c])))?;
        let rows = substitute_dispatch_column_binds(rows, refs, j);
        let keys = ordered_ctor_heads(&rows, j)?;
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
            let nrefs = refs_with_head_fields(refs, j, &fresh, tmpl);
            let nrows = specialize_rows_for_head(&rows, j, key, *arity, nrefs.len());
            let branch = compile(&nrefs, nrows, tmpl, next, layouts, emitted)?;
            let pat_args: Vec<IrPattern> = fresh
                .iter()
                .map(|(v, t)| IrPattern::Bind { var: *v, ty: t.clone() })
                .collect();
            arms.push(IrMatchArm { pattern: head_pattern(key, pat_args), guard: None, body: branch });
        }
        let head_keys: Vec<HKey> = keys.iter().map(|(k, _)| k.clone()).collect();
        if !heads_cover(&head_keys, layouts) {
            let drows = rows_without_dispatch_column(&rows, j);
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

    /// The LEAF of [`compile`]: an all-trivial row matches unconditionally, so its body IS
    /// the branch — with each `Bind` pattern's var substituted by the column ref it names,
    /// and the row's duplication count bumped (the gate reads it after). Extracted verbatim
    /// (codopsy round-3 sweep, #852).
    fn trivial_row_body(r: &Row, refs: &[IrExpr], emitted: &mut [usize]) -> IrExpr {
        let mut b = r.body.clone();
        for (i, p) in r.pats.iter().enumerate() {
            if let IrPattern::Bind { var, .. } = p {
                b = almide_ir::substitute_var_in_expr(&b, *var, &refs[i]);
            }
        }
        emitted[r.idx] += 1;
        b
    }

    /// A `Bind` in the DISPATCH column names the WHOLE component: substitute the component
    /// ref now (once, before the row joins multiple branches) and dispatch on `_`. Extracted
    /// verbatim from [`compile`] (codopsy round-3 sweep, #852).
    fn substitute_dispatch_column_binds(rows: Vec<Row>, refs: &[IrExpr], j: usize) -> Vec<Row> {
        rows.into_iter()
            .map(|mut r| {
                if let IrPattern::Bind { var, .. } = &r.pats[j] {
                    r.body = almide_ir::substitute_var_in_expr(&r.body, *var, &refs[j]);
                    r.pats[j] = IrPattern::Wildcard;
                }
                r
            })
            .collect()
    }

    /// The dispatch column's ctor heads in FIRST-OCCURRENCE order, with each head's arity.
    /// `None` declines the whole compile: a same-head arity drift cannot be dispatched.
    /// Extracted verbatim from [`compile`] (codopsy round-3 sweep, #852).
    fn ordered_ctor_heads(rows: &[Row], j: usize) -> Option<Vec<(HKey, usize)>> {
        let mut keys: Vec<(HKey, usize)> = Vec::new();
        for r in rows {
            if let Some((k, args)) = head_of(&r.pats[j]) {
                match keys.iter().find(|(k2, _)| *k2 == k) {
                    Some((_, a)) if *a != args.len() => return None,
                    Some(_) => {}
                    None => keys.push((k, args.len())),
                }
            }
        }
        Some(keys)
    }

    /// The column refs ONE head's branch sees: the dispatch column replaced in place by the
    /// head's freshly-bound field refs. Extracted verbatim from [`compile`] (codopsy round-3
    /// sweep, #852).
    fn refs_with_head_fields(
        refs: &[IrExpr],
        j: usize,
        fresh: &[(VarId, Ty)],
        tmpl: &IrExpr,
    ) -> Vec<IrExpr> {
        let mut nrefs: Vec<IrExpr> = Vec::with_capacity(refs.len() - 1 + fresh.len());
        nrefs.extend_from_slice(&refs[..j]);
        for (v, t) in fresh {
            nrefs.push(IrExpr {
                kind: IrExprKind::Var { id: *v },
                ty: t.clone(),
                span: tmpl.span.clone(),
                def_id: None,
            });
        }
        nrefs.extend_from_slice(&refs[j + 1..]);
        nrefs
    }

    /// The rows ONE head's branch sees: a row whose head IS this key contributes its head
    /// args in the dispatch column's place; a HEADLESS (trivial) row contributes wildcards
    /// there (it still matches); a row with a DIFFERENT head is dropped. Extracted verbatim
    /// from [`compile`] (codopsy round-3 sweep, #852).
    fn specialize_rows_for_head(
        rows: &[Row],
        j: usize,
        key: &HKey,
        arity: usize,
        width: usize,
    ) -> Vec<Row> {
        let mut nrows: Vec<Row> = Vec::new();
        for r in rows {
            match head_of(&r.pats[j]) {
                Some((k, args)) if k == *key => {
                    let mut np = Vec::with_capacity(width);
                    np.extend_from_slice(&r.pats[..j]);
                    np.extend(args);
                    np.extend_from_slice(&r.pats[j + 1..]);
                    nrows.push(Row { pats: np, body: r.body.clone(), idx: r.idx });
                }
                Some(_) => {}
                None => {
                    let mut np = Vec::with_capacity(width);
                    np.extend_from_slice(&r.pats[..j]);
                    np.extend(std::iter::repeat(IrPattern::Wildcard).take(arity));
                    np.extend_from_slice(&r.pats[j + 1..]);
                    nrows.push(Row { pats: np, body: r.body.clone(), idx: r.idx });
                }
            }
        }
        nrows
    }

    /// One head key as the arm PATTERN binding its fresh field vars. Extracted verbatim
    /// from [`compile`] (codopsy round-3 sweep, #852).
    fn head_pattern(key: &HKey, mut pat_args: Vec<IrPattern>) -> IrPattern {
        match key {
            HKey::User(name) => IrPattern::Constructor { name: name.clone(), args: pat_args },
            HKey::Some_ => IrPattern::Some { inner: Box::new(pat_args.remove(0)) },
            HKey::None_ => IrPattern::None,
            HKey::Ok_ => IrPattern::Ok { inner: Box::new(pat_args.remove(0)) },
            HKey::Err_ => IrPattern::Err { inner: Box::new(pat_args.remove(0)) },
        }
    }

    /// The DEFAULT arm's rows: the headless rows only, with the dispatch column removed
    /// (nothing tested it). Extracted verbatim from [`compile`] (codopsy round-3 sweep,
    /// #852).
    fn rows_without_dispatch_column(rows: &[Row], j: usize) -> Vec<Row> {
        let mut drows: Vec<Row> = Vec::new();
        for r in rows {
            if head_of(&r.pats[j]).is_none() {
                let mut np = r.pats.clone();
                np.remove(j);
                drows.push(Row { pats: np, body: r.body.clone(), idx: r.idx });
            }
        }
        drows
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
            let Some(rows) = normalize_pattern_rows(arms, n) else { return };
            // Hoist each non-Var component ONCE into a temp (a Var component reads direct).
            let span = e.span.clone();
            let (stmts, refs) = hoist_tuple_components(elements, &mut self.next, &span);
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
    let mut v = V { next: crate::lower::desugar_var_seed(), layouts, changed: false };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    v.changed.then_some(out)
}
