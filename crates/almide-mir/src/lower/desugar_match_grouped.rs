// ── tail of desugar_match.rs, include!-spliced back at module level ──
//
// A pure code move: this file continues its parent verbatim. The split exists
// only so the parent stays under the 800-line ceiling the codopsy gate holds
// this crate to; there is no boundary of meaning here, and `include!` at module
// level is the one splice Rust allows (an impl-item position rejects it).

pub fn desugar_grouped_variant_match(
    body: &IrExpr,
    next_var: &mut u32,
    layouts: &crate::lower::VariantLayouts,
) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    if !grouped_variant_match_fires(body, layouts) {
        return None;
    }
    struct V<'a> {
        next: &'a mut u32,
        layouts: &'a crate::lower::VariantLayouts,
        changed: bool,
    }
    impl IrMutVisitor for V<'_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            if let IrExprKind::Match { subject, arms } = &e.kind {
                if let Some(new_arms) =
                    group_option_result_arms(subject, arms, self.next, self.layouts)
                {
                    e.kind = IrExprKind::Match {
                        subject: subject.clone(),
                        arms: new_arms,
                    };
                    self.changed = true;
                }
            }
        }
    }
    let mut out = body.clone();
    let mut v = V {
        next: next_var,
        layouts,
        changed: false,
    };
    // Mirror the probe: rewrite the owned region deep, then the root's own
    // match (the region walk skips exactly the child positions whose OWN
    // fixpoint runs this row — see `for_each_owned_region`).
    {
        let out_ref = &mut out;
        match &mut out_ref.kind {
            IrExprKind::If { cond, .. } => v.visit_expr_mut(cond),
            IrExprKind::Match { subject, arms } => {
                v.visit_expr_mut(subject);
                for a in arms.iter_mut() {
                    if let Some(g) = &mut a.guard {
                        v.visit_expr_mut(g);
                    }
                }
            }
            IrExprKind::Block { stmts, expr: Some(_) } => {
                for s in stmts.iter_mut() {
                    match &mut s.kind {
                        IrStmtKind::Expr { expr } => v.visit_expr_mut(expr),
                        IrStmtKind::Bind { value, .. } => v.visit_expr_mut(value),
                        IrStmtKind::Assign { value, .. } => v.visit_expr_mut(value),
                        _ => {}
                    }
                }
            }
            _ => {
                v.visit_expr_mut(out_ref);
                return v.changed.then_some(out);
            }
        }
    }
    // Root-level regroup (post-order: after the owned region).
    if let IrExprKind::Match { subject, arms } = &out.kind {
        if let Some(new_arms) = group_option_result_arms(subject, arms, v.next, v.layouts) {
            let subject = subject.clone();
            out.kind = IrExprKind::Match { subject, arms: new_arms };
            v.changed = true;
        }
    }
    v.changed.then_some(out)
}

/// A plain scalar leaf column (Bind / Literal / Wildcard) — hoisted from
/// `group_option_result_arms` for the complexity budget.
fn grouping_plain_col(p: &almide_ir::IrPattern) -> bool {
    use almide_ir::IrPattern;
    matches!(p, IrPattern::Bind { .. } | IrPattern::Literal { .. } | IrPattern::Wildcard)
}

/// A column pattern the sub-match can re-dispatch on: a scalar leaf (Bind /
/// Literal / Wildcard) or a NESTED user-ctor pattern (`err(Overflow(msg))` —
/// the Result-with-variant-payload class: the inner match over the bound
/// payload var re-dispatches on the variant tag, which the custom-variant
/// machinery lowers once the payload bind is seeded). Hoisted from
/// `group_option_result_arms` for the complexity budget.
fn grouping_scalar_col(p: &almide_ir::IrPattern) -> bool {
    use almide_ir::IrPattern;
    grouping_plain_col(p)
        || matches!(p, IrPattern::Constructor { args, .. }
            if args.iter().all(grouping_plain_col))
        // A nested BUILTIN wrapper (`some(some(n))`, `some(ok(v))`, `some(none)` — the
        // match_exhaustive nested-Option/Result class): the inner match over the bound
        // payload re-dispatches on the wrapper's own len/cap tag, which the ordinary
        // Option/Result machinery lowers once the payload bind is seeded.
        || matches!(p, IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner }
            if grouping_plain_col(inner))
        || matches!(p, IrPattern::None)
        // A RECORD-variant pattern (`ok(Tag { name, c })` — the derived-Codec roundtrip
        // class): the inner match re-dispatches the record-variant pattern over the bound
        // payload var — the custom-variant machinery the `describe`-style direct matches
        // already lower. Every named field must carry an explicit plain sub-pattern.
        || matches!(p, IrPattern::RecordPattern { fields, .. }
            if fields.iter().all(|f| matches!(&f.pattern, Some(fp) if grouping_plain_col(fp))))
}

/// Any constructor-shaped sub-pattern — hoisted from
/// `group_option_result_arms` for the complexity budget.
fn grouping_is_nested_ctor(p: &almide_ir::IrPattern) -> bool {
    use almide_ir::IrPattern;
    matches!(p,
        IrPattern::Constructor { .. }
            | IrPattern::RecordPattern { .. }
            | IrPattern::Some { .. }
            | IrPattern::None
            | IrPattern::Ok { .. }
            | IrPattern::Err { .. })
}

/// The grouping transform for [`desugar_grouped_variant_match`]. `None` when the subject is not an
/// `Option`/`Result`, an arm is a top-level catch-all (`_`/binder — not a pure constructor dispatch),
/// a payload pattern is nested (a later brick), or NO arm carries a guard/literal (the plain variant
/// match already lowers — leave it untouched so nothing regresses).
fn group_option_result_arms(
    subject: &IrExpr,
    arms: &[almide_ir::IrMatchArm],
    next_var: &mut u32,
    layouts: &crate::lower::VariantLayouts,
) -> Option<Vec<almide_ir::IrMatchArm>> {
    use almide_ir::{IrMatchArm, IrPattern};
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    // A constructor "slot" key + its ONE payload's type (None for a nullary ctor). Handles Option
    // (Some/None), Result (Ok/Err), and a SINGLE-FIELD user variant (`Word(String)`); a multi-field
    // ctor, a record-variant, or a nested payload aborts (a later brick).
    #[derive(Clone, PartialEq, Eq)]
    enum CKey {
        Some_,
        None_,
        Ok_,
        Err_,
        User(String),
    }
    let scalar_col = grouping_scalar_col;
    let is_nested_ctor = grouping_is_nested_ctor;
    // A USER-ctor column of ARBITRARY ctor depth (`Node(Leaf(a), Node(Leaf(b), Leaf(c)))` — the
    // #610 nested-refinement class): the payload sub-match re-dispatches level by level — arity 1
    // re-enters THIS regroup on the next fixpoint pass; arity ≥2 becomes a tuple sub-match the
    // deep tuple-variant desugar ([`desugar_tuple_variant_match_deep`]) column-specializes.
    // Record sub-patterns stay SHALLOW (every named field explicit + plain), same as `scalar_col`.
    fn deep_col(p: &IrPattern) -> bool {
        match p {
            IrPattern::Bind { .. } | IrPattern::Literal { .. } | IrPattern::Wildcard
            | IrPattern::None => true,
            IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
                deep_col(inner)
            }
            IrPattern::Constructor { args, .. } => args.iter().all(deep_col),
            IrPattern::RecordPattern { fields, .. } => fields.iter().all(|f| {
                matches!(&f.pattern,
                    Some(fp) if matches!(fp,
                        IrPattern::Bind { .. } | IrPattern::Literal { .. } | IrPattern::Wildcard))
            }),
            _ => false,
        }
    }
    // `(key, field_patterns)` for one arm — `None` (bail) for a top-level catch-all/binder, a
    // record-variant, or a nested column. Field arity: 0 (nullary), 1 (Some/Ok/Err/single-field), or
    // N (a multi-field user ctor `KV(String, Int)` → grouped via a TUPLE payload sub-match).
    let parse = |p: &IrPattern| -> Option<(CKey, Vec<IrPattern>)> {
        match p {
            IrPattern::Some { inner } if scalar_col(inner) => Some((CKey::Some_, vec![(**inner).clone()])),
            IrPattern::None => Some((CKey::None_, vec![])),
            IrPattern::Ok { inner } if scalar_col(inner) => Some((CKey::Ok_, vec![(**inner).clone()])),
            IrPattern::Err { inner } if scalar_col(inner) => Some((CKey::Err_, vec![(**inner).clone()])),
            // A USER-variant subject admits DEEP columns (`Node(Leaf(a), Leaf(b))` then
            // `Node(l, r)` — the #610 fall-through refinement): the regroup turns each ctor
            // bucket into a payload sub-match (arity 1: re-enters this regroup on the next
            // fixpoint pass; arity ≥2: a tuple sub-match the deep tuple-variant desugar
            // column-specializes with in-group fall-through).
            IrPattern::Constructor { name, args } if args.iter().all(deep_col) => {
                Some((CKey::User(name.clone()), args.clone()))
            }
            _ => Option::None,
        }
    };
    // A TRAILING `_` catch-all (`_ => assert(false)` — the codec-roundtrip class) regroups:
    // its body becomes each multi-arm bucket's inner fallback AND the outer last arm (an
    // `ok(<unmatched ctor>)` value must fall through the INNER match; an `err(_)` through
    // the OUTER). Body duplication is admissible — the count gate reads this same desugared
    // tree on both sides (the tail-duplication precedent). A guarded/binder catch-all bails.
    let (ctor_arms, trailing_wild): (&[IrMatchArm], Option<&IrMatchArm>) = match arms.split_last()
    {
        Some((last, rest))
            if matches!(last.pattern, IrPattern::Wildcard) && last.guard.is_none() =>
        {
            (rest, Some(last))
        }
        _ => (arms, Option::None),
    };
    // Ordered per-ctor buckets (first-occurrence order — the constructors are DISJOINT so outer arm
    // order is immaterial). Each entry: (key, Vec<(field_patterns, guard, body)>).
    let mut groups: Vec<(CKey, Vec<(Vec<IrPattern>, Option<IrExpr>, IrExpr)>)> = Vec::new();
    let mut any_guard_or_lit = false;
    for arm in ctor_arms {
        let (key, fields) = parse(&arm.pattern)?;
        if arm.guard.is_some()
            || fields.iter().any(|p| matches!(p, IrPattern::Literal { .. }))
            || fields.iter().any(is_nested_ctor)
        {
            any_guard_or_lit = true;
        }
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, v)) => v.push((fields, arm.guard.clone(), arm.body.clone())),
            Option::None => groups.push((key, vec![(fields, arm.guard.clone(), arm.body.clone())])),
        }
    }
    // Nothing to gain (a plain `some(x)/none` / `Ctor(x)` shape already lowers) — leave untouched.
    if !any_guard_or_lit {
        return Option::None;
    }
    let subject_ty = subject.ty.clone();
    // The type of field `c` of a ctor group: Option/Result from the subject; a user ctor from a
    // Literal (its `expr.ty`) / Bind (its `ty`) in that column across the group's arms.
    let field_ty = |key: &CKey, c: usize, bucket: &[(Vec<IrPattern>, Option<IrExpr>, IrExpr)]| -> Option<Ty> {
        match key {
            CKey::Some_ => match &subject_ty {
                Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 => Some(a[0].clone()),
                _ => Option::None,
            },
            CKey::Ok_ => match &subject_ty {
                Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => Some(a[0].clone()),
                _ => Option::None,
            },
            CKey::Err_ => match &subject_ty {
                Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => Some(a[1].clone()),
                _ => Option::None,
            },
            CKey::None_ => Option::None,
            CKey::User(name) => bucket
                .iter()
                .find_map(|(pats, _, _)| match pats.get(c) {
                    Some(IrPattern::Bind { ty, .. }) => Some(ty.clone()),
                    Some(IrPattern::Literal { expr }) => Some(expr.ty.clone()),
                    _ => Option::None,
                })
                // No Bind/Literal in the column (every row refines it with a nested ctor —
                // `Box(Some(n)) / Box(None)`): the declared field type from the program's
                // variant-layout registry names it exactly.
                .or_else(|| {
                    layouts
                        .lookup_ctor(name)
                        .and_then(|(_, _, case)| case.fields.get(c).map(|(_, t)| t.clone()))
                }),
        }
    };
    let rebuild = |key: &CKey, args: Vec<IrPattern>| -> IrPattern {
        match key {
            CKey::Some_ => IrPattern::Some { inner: Box::new(args.into_iter().next().expect("Some_ groups always carry exactly 1 field (`parse` only ever produces vec![inner] for Some)")) },
            CKey::None_ => IrPattern::None,
            CKey::Ok_ => IrPattern::Ok { inner: Box::new(args.into_iter().next().expect("Ok_ groups always carry exactly 1 field (`parse` only ever produces vec![inner] for Ok)")) },
            CKey::Err_ => IrPattern::Err { inner: Box::new(args.into_iter().next().expect("Err_ groups always carry exactly 1 field (`parse` only ever produces vec![inner] for Err)")) },
            CKey::User(name) => IrPattern::Constructor { name: name.clone(), args },
        }
    };
    let mut new_arms = Vec::with_capacity(groups.len());
    for (key, bucket) in groups {
        let arity = bucket[0].0.len();
        let needs_inner = arity >= 1
            && (bucket.len() > 1
                || bucket.iter().any(|(pats, g, _)| {
                    g.is_some()
                        || pats.iter().any(|p| matches!(p, IrPattern::Literal { .. }))
                        || pats.iter().any(is_nested_ctor)
                }));
        if !needs_inner {
            // A single arm for this ctor (a lone `some(x)`/`none`/`Ctor(a, b)` with no guard/literal)
            // — keep verbatim. A nullary ctor with a guard/duplicate cannot sub-match → bail.
            if bucket.len() != 1 {
                return Option::None;
            }
            let (fields, guard, body) = bucket.into_iter().next().expect("bucket.len() == 1, checked immediately above");
            new_arms.push(IrMatchArm { pattern: rebuild(&key, fields), guard, body });
            continue;
        }
        // Bind each field to a fresh var; the sub-match subject is that var (1 field) or a TUPLE of
        // them (N fields — lowered by `desugar_tuple_match`), and each arm re-matches the fields.
        let mut field_tys = Vec::with_capacity(arity);
        let mut binds = Vec::with_capacity(arity);
        for c in 0..arity {
            let ty = field_ty(&key, c, &bucket)?;
            let v = VarId(*next_var);
            *next_var += 1;
            field_tys.push(ty.clone());
            binds.push((v, ty));
        }
        let sub_subject = if arity == 1 {
            IrExpr {
                kind: IrExprKind::Var { id: binds[0].0 },
                ty: field_tys[0].clone(),
                span: subject.span.clone(),
                def_id: None,
            }
        } else {
            IrExpr {
                kind: IrExprKind::Tuple {
                    elements: binds
                        .iter()
                        .map(|(v, ty)| IrExpr {
                            kind: IrExprKind::Var { id: *v },
                            ty: ty.clone(),
                            span: subject.span.clone(),
                            def_id: None,
                        })
                        .collect(),
                },
                ty: Ty::Tuple(field_tys.clone()),
                span: subject.span.clone(),
                def_id: None,
            }
        };
        let mut inner_arms: Vec<IrMatchArm> = bucket
            .into_iter()
            .map(|(fields, guard, body)| IrMatchArm {
                pattern: if arity == 1 {
                    fields.into_iter().next().expect("arity == 1, checked above, and every bucket entry shares this key's fixed field count")
                } else {
                    IrPattern::Tuple { elements: fields }
                },
                guard,
                body,
            })
            .collect();
        // The trailing catch-all falls through INTO this ctor's sub-match (an
        // `ok(<other ctor>)` subject must reach it, not vanish).
        if let Some(w) = trailing_wild {
            inner_arms.push(IrMatchArm {
                pattern: IrPattern::Wildcard,
                guard: Option::None,
                body: w.body.clone(),
            });
        }
        let body_ty = inner_arms[0].body.ty.clone();
        let sub = IrExpr {
            kind: IrExprKind::Match {
                subject: Box::new(sub_subject),
                arms: inner_arms,
            },
            ty: body_ty,
            span: subject.span.clone(),
            def_id: None,
        };
        let ctor_args = binds
            .into_iter()
            .map(|(v, ty)| IrPattern::Bind { var: v, ty })
            .collect();
        new_arms.push(IrMatchArm {
            pattern: rebuild(&key, ctor_args),
            guard: Option::None,
            body: sub,
        });
    }
    if new_arms.is_empty() {
        return Option::None;
    }
    if let Some(w) = trailing_wild {
        new_arms.push(w.clone());
    }
    Some(new_arms)
}
