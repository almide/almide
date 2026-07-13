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
    let mut s = S { next_var: max_var_id(body) + 1 };
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
                kind: IrExprKind::Block { stmts, expr: Some(Box::new(chain.unwrap())) },
                ty: e.ty.clone(),
                span: span.clone(),
                def_id: e.def_id,
            };
            self.changed = true;
        }
    }
    let mut v = V { next: max_var_id(body) + 1, changed: false };
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
            let (last, init) = arms.split_last().unwrap();
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
                        cond: Box::new(arm.guard.clone().unwrap()),
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
    let mut v = V { next: max_var_id(body) + 1, changed: false };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    v.changed.then_some(out)
}


pub fn desugar_grouped_variant_match(body: &IrExpr, next_var: &mut u32) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    struct V<'a> {
        next: &'a mut u32,
        changed: bool,
    }
    impl IrMutVisitor for V<'_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            if let IrExprKind::Match { subject, arms } = &e.kind {
                if let Some(new_arms) = group_option_result_arms(subject, arms, self.next) {
                    e.kind = IrExprKind::Match {
                        subject: subject.clone(),
                        arms: new_arms,
                    };
                    self.changed = true;
                }
            }
        }
    }
    let mut v = V {
        next: next_var,
        changed: false,
    };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    if v.changed {
        Some(out)
    } else {
        None
    }
}

/// The grouping transform for [`desugar_grouped_variant_match`]. `None` when the subject is not an
/// `Option`/`Result`, an arm is a top-level catch-all (`_`/binder — not a pure constructor dispatch),
/// a payload pattern is nested (a later brick), or NO arm carries a guard/literal (the plain variant
/// match already lowers — leave it untouched so nothing regresses).
fn group_option_result_arms(
    subject: &IrExpr,
    arms: &[almide_ir::IrMatchArm],
    next_var: &mut u32,
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
    // A column pattern the sub-match can re-dispatch on: a scalar leaf (Bind /
    // Literal / Wildcard) or a NESTED user-ctor pattern (`err(Overflow(msg))` —
    // the Result-with-variant-payload class: the inner match over the bound
    // payload var re-dispatches on the variant tag, which the custom-variant
    // machinery lowers once the payload bind is seeded).
    let plain_col =
        |p: &IrPattern| matches!(p, IrPattern::Bind { .. } | IrPattern::Literal { .. } | IrPattern::Wildcard);
    let scalar_col = |p: &IrPattern| {
        plain_col(p)
            || matches!(p, IrPattern::Constructor { args, .. }
                if args.iter().all(plain_col))
            // A nested BUILTIN wrapper (`some(some(n))`, `some(ok(v))`, `some(none)` — the
            // match_exhaustive nested-Option/Result class): the inner match over the bound
            // payload re-dispatches on the wrapper's own len/cap tag, which the ordinary
            // Option/Result machinery lowers once the payload bind is seeded.
            || matches!(p, IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner }
                if plain_col(inner))
            || matches!(p, IrPattern::None)
            // A RECORD-variant pattern (`ok(Tag { name, c })` — the derived-Codec roundtrip
            // class): the inner match re-dispatches the record-variant pattern over the bound
            // payload var — the custom-variant machinery the `describe`-style direct matches
            // already lower. Every named field must carry an explicit plain sub-pattern.
            || matches!(p, IrPattern::RecordPattern { fields, .. }
                if fields.iter().all(|f| matches!(&f.pattern, Some(fp) if plain_col(fp))))
    };
    let is_nested_ctor = |p: &IrPattern| {
        matches!(p,
            IrPattern::Constructor { .. }
                | IrPattern::RecordPattern { .. }
                | IrPattern::Some { .. }
                | IrPattern::None
                | IrPattern::Ok { .. }
                | IrPattern::Err { .. })
    };
    // `(key, field_patterns)` for one arm — `None` (bail) for a top-level catch-all/binder, a
    // record-variant, or a nested column. Field arity: 0 (nullary), 1 (Some/Ok/Err/single-field), or
    // N (a multi-field user ctor `KV(String, Int)` → grouped via a TUPLE payload sub-match).
    let parse = |p: &IrPattern| -> Option<(CKey, Vec<IrPattern>)> {
        match p {
            IrPattern::Some { inner } if scalar_col(inner) => Some((CKey::Some_, vec![(**inner).clone()])),
            IrPattern::None => Some((CKey::None_, vec![])),
            IrPattern::Ok { inner } if scalar_col(inner) => Some((CKey::Ok_, vec![(**inner).clone()])),
            IrPattern::Err { inner } if scalar_col(inner) => Some((CKey::Err_, vec![(**inner).clone()])),
            // A USER-variant subject keeps the STRICT columns: its nested-ctor arms
            // (`Node(Leaf(a), Leaf(b))` then `Node(l, r)` — #610 fall-through
            // refinement) already lower via the custom-variant machinery, and
            // regrouping them here would shadow that working path.
            IrPattern::Constructor { name, args } if args.iter().all(plain_col) => {
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
            CKey::User(_) => bucket.iter().find_map(|(pats, _, _)| match pats.get(c) {
                Some(IrPattern::Bind { ty, .. }) => Some(ty.clone()),
                Some(IrPattern::Literal { expr }) => Some(expr.ty.clone()),
                _ => Option::None,
            }),
        }
    };
    let rebuild = |key: &CKey, args: Vec<IrPattern>| -> IrPattern {
        match key {
            CKey::Some_ => IrPattern::Some { inner: Box::new(args.into_iter().next().unwrap()) },
            CKey::None_ => IrPattern::None,
            CKey::Ok_ => IrPattern::Ok { inner: Box::new(args.into_iter().next().unwrap()) },
            CKey::Err_ => IrPattern::Err { inner: Box::new(args.into_iter().next().unwrap()) },
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
            let (fields, guard, body) = bucket.into_iter().next().unwrap();
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
                    fields.into_iter().next().unwrap()
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
            .unwrap();
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

/// N-ARM tuple-of-lists match whose tests are all BINDLESS `[]` patterns
/// (`match (a, b) { ([], []) => "both", ([], _) => "a", (_, []) => "b", _ => "none" }`
/// — the regression `classify` shape): specialize on the FIRST conditional column
/// recursively (a mini decision tree — trivial here because `[]` binds nothing):
/// THEN keeps every row whose column accepts `[]` (the `[]` rows and the `_` rows),
/// ELSE keeps only the `_` rows; rows after the first all-`_` row prune (first-match).
/// Each level emits a 2-arm `[] / _` match over ONE hoisted component — exactly the
/// `try_lower_list_match_value` subset. A body on a row with any `_` column can
/// appear in BOTH branches (duplication is branch-exclusive at runtime and
/// desugar-before-both keeps the count gate exact); such a body must not introduce
/// binders (VarId uniqueness — [`introduces_binder`]).
pub fn desugar_tuple_empty_list_match(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    use almide_ir::IrPattern;
    #[derive(Clone, Copy, PartialEq)]
    enum Cp {
        Empty,
        Any,
    }
    fn build(
        rows: &[(Vec<Cp>, IrExpr)],
        refs: &[IrExpr],
        cols: &[usize],
        out_ty: &Ty,
        span: &Option<almide_lang::span::Span>,
    ) -> IrExpr {
        // First-match pruning: rows after the first all-`_` row are unreachable.
        let mut live: Vec<&(Vec<Cp>, IrExpr)> = Vec::new();
        for r in rows {
            live.push(r);
            if cols.iter().all(|&j| r.0[j] == Cp::Any) {
                break;
            }
        }
        let first = live[0];
        let Some(j) = cols.iter().copied().find(|&j| first.0[j] == Cp::Empty) else {
            return first.1.clone();
        };
        let rest_cols: Vec<usize> = cols.iter().copied().filter(|&c| c != j).collect();
        let then_rows: Vec<(Vec<Cp>, IrExpr)> = live.iter().map(|r| (*r).clone()).collect();
        let else_rows: Vec<(Vec<Cp>, IrExpr)> = live
            .iter()
            .filter(|r| r.0[j] == Cp::Any)
            .map(|r| (*r).clone())
            .collect();
        let then_e = build(&then_rows, refs, &rest_cols, out_ty, span);
        let else_e = build(&else_rows, refs, &rest_cols, out_ty, span);
        IrExpr {
            kind: IrExprKind::Match {
                subject: Box::new(refs[j].clone()),
                arms: vec![
                    almide_ir::IrMatchArm {
                        pattern: IrPattern::List { elements: Vec::new() },
                        guard: Option::None,
                        body: then_e,
                    },
                    almide_ir::IrMatchArm {
                        pattern: IrPattern::Wildcard,
                        guard: Option::None,
                        body: else_e,
                    },
                ],
            },
            ty: out_ty.clone(),
            span: span.clone(),
            def_id: None,
        }
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
            let k = elements.len();
            if k < 2 || arms.len() < 3 || !is_heap_ty(&e.ty) || arms.iter().any(|a| a.guard.is_some())
            {
                return;
            }
            let (last, init) = arms.split_last().unwrap();
            if !matches!(last.pattern, IrPattern::Wildcard) {
                return;
            }
            let mut rows: Vec<(Vec<Cp>, IrExpr)> = Vec::new();
            for a in init {
                let IrPattern::Tuple { elements: pats } = &a.pattern else { return };
                if pats.len() != k {
                    return;
                }
                let mut cps = Vec::with_capacity(k);
                let mut cond_n = 0usize;
                for p in pats {
                    match p {
                        IrPattern::List { elements } if elements.is_empty() => {
                            cps.push(Cp::Empty);
                            cond_n += 1;
                        }
                        IrPattern::Wildcard => cps.push(Cp::Any),
                        _ => return,
                    }
                }
                if cond_n == 0 {
                    return;
                }
                rows.push((cps, a.body.clone()));
            }
            rows.push((vec![Cp::Any; k], last.body.clone()));
            // A row with an `_` column can land in both spec branches — its body
            // duplicates, so it must not introduce binders.
            for (cps, b) in &rows {
                if cps.iter().any(|c| *c == Cp::Any) && introduces_binder(b) {
                    return;
                }
            }
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
            let cols: Vec<usize> = (0..k).collect();
            let tree = build(&rows, &refs, &cols, &e.ty, &span);
            *e = if stmts.is_empty() {
                tree
            } else {
                IrExpr {
                    kind: IrExprKind::Block { stmts, expr: Some(Box::new(tree)) },
                    ty: e.ty.clone(),
                    span,
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

/// Rewrite a match over a PLAIN RECORD subject whose first arm is that record's
/// OWN RecordPattern (`match f { Flags { ok: o, err: e, .. } => B, _ => C }` —
/// the soft-keyword-field destructure shape) into the unconditional destructure
/// `{ let o = f.ok; let e = f.err; B }`. GATES: the pattern NAME equals the
/// subject's Named TYPE (a variant CASE pattern carries the case name, not the
/// type name), every later arm is a bare Wildcard (a real variant match has
/// sibling ctor arms), fields bind with plain Bind/Wildcard only, no guards.
/// Under those gates the first arm always matches, so `C` is dead — dropped on
/// BOTH sides (desugar-before-both keeps the count exact).
pub fn desugar_record_destructure_match(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    use almide_ir::IrPattern;
    struct V {
        changed: bool,
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Match { subject, arms } = &e.kind else { return };
            let Ty::Named(tname, targs) = &subject.ty else { return };
            if !targs.is_empty() || arms.is_empty() || arms.iter().any(|a| a.guard.is_some()) {
                return;
            }
            let IrPattern::RecordPattern { name, fields, .. } = &arms[0].pattern else {
                return;
            };
            if name != tname.as_str() {
                return;
            }
            if !arms[1..].iter().all(|a| matches!(a.pattern, IrPattern::Wildcard)) {
                return;
            }
            let mut binds: Vec<IrStmt> = Vec::new();
            for f in fields {
                match &f.pattern {
                    Some(IrPattern::Bind { var, ty }) => binds.push(IrStmt {
                        kind: IrStmtKind::Bind {
                            var: *var,
                            ty: ty.clone(),
                            value: IrExpr {
                                kind: IrExprKind::Member {
                                    object: Box::new((**subject).clone()),
                                    field: almide_lang::intern::sym(&f.name),
                                },
                                ty: ty.clone(),
                                span: e.span.clone(),
                                def_id: None,
                            },
                            mutability: almide_ir::Mutability::Let,
                        },
                        span: e.span.clone(),
                    }),
                    Some(IrPattern::Wildcard) => {}
                    // A shorthand/nested field pattern — outside this brick.
                    _ => return,
                }
            }
            let body_e = arms[0].body.clone();
            *e = IrExpr {
                kind: IrExprKind::Block { stmts: binds, expr: Some(Box::new(body_e)) },
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

/// Rewrite a match over a SCALAR-element LIST subject whose arms are FIXED-LENGTH
/// list patterns (`match xs { [] => A, [0] => B, [n] if n > 0 => C, [_] => D,
/// [a, b] => E, _ => F }` — the `describe` shape) into a LENGTH-GROUPED if chain:
///
///   { let $t = xs; let $len = list.len($t);
///     if $len == 0 then A
///     else if $len == 1 then { let $e0 = $t[0];
///        if $e0 == 0 then B else { let n = $e0; if n > 0 then C else D } }
///     else if $len == 2 then { let a = $t[0]; let b = $t[1]; E }
///     else F }
///
/// Element loads sit UNDER their length test (no out-of-range read); per-group
/// binds alias the element temps at the group top (scalar copies — guards need
/// them in scope, the scalar_guard_match discipline); literal elements become
/// `==` conds; a group's first unconditional arm terminates it, else the
/// catch-all fills in (duplication gated by [`introduces_binder`]). Lengths are
/// mutually exclusive, so grouping preserves first-match. Count-exact by
/// desugar-before-both (the one `list.len` call + any duplicated catch-all
/// appear identically on both sides).
pub fn desugar_list_pattern_match(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    use almide_ir::{BinOp, IrPattern};
    use almide_lang::types::constructor::TypeConstructorId;
    struct V {
        next: u32,
        changed: bool,
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Match { subject, arms } = &e.kind else { return };
            let elem_ty = match &subject.ty {
                Ty::Applied(TypeConstructorId::List, a)
                    if a.len() == 1 && !is_heap_ty(&a[0]) =>
                {
                    a[0].clone()
                }
                _ => return,
            };
            if arms.len() < 2 {
                return;
            }
            let (last, init) = arms.split_last().unwrap();
            if last.guard.is_some() || !matches!(last.pattern, IrPattern::Wildcard) {
                return;
            }
            // Admit only list patterns of Bind/Wildcard/Literal elements; at least
            // one arm must need this desugar (a length > 0 or a guard/literal —
            // the plain 2-arm `[] / bind` forms already lower elsewhere).
            #[allow(clippy::type_complexity)]
            let mut groups: Vec<(usize, Vec<&almide_ir::IrMatchArm>)> = Vec::new();
            let mut interesting = false;
            for a in init {
                let IrPattern::List { elements } = &a.pattern else { return };
                for p in elements {
                    match p {
                        IrPattern::Bind { .. } | IrPattern::Wildcard | IrPattern::Literal { .. } => {}
                        _ => return,
                    }
                }
                if !elements.is_empty() || a.guard.is_some() {
                    interesting = true;
                }
                let k = elements.len();
                match groups.iter_mut().find(|(gk, _)| *gk == k) {
                    Some((_, v)) => v.push(a),
                    None => groups.push((k, vec![a])),
                }
            }
            if !interesting {
                return;
            }
            // A duplicated catch-all (a group without an unconditional terminal, plus
            // the final else) must not introduce binders.
            let dup_needed = groups.iter().any(|(_, gas)| {
                !gas.iter().any(|a| {
                    a.guard.is_none()
                        && matches!(&a.pattern, IrPattern::List { elements }
                            if elements.iter().all(|p| matches!(p,
                                IrPattern::Bind { .. } | IrPattern::Wildcard)))
                })
            });
            if dup_needed && introduces_binder(&last.body) {
                return;
            }
            let span = e.span.clone();
            let out_ty = e.ty.clone();
            // Hoist the subject (Var direct) and its length.
            let mut stmts: Vec<IrStmt> = Vec::new();
            let t_ref = if matches!(subject.kind, IrExprKind::Var { .. }) {
                (**subject).clone()
            } else {
                let t = VarId(self.next);
                self.next += 1;
                stmts.push(IrStmt {
                    kind: IrStmtKind::Bind {
                        var: t,
                        ty: subject.ty.clone(),
                        value: (**subject).clone(),
                        mutability: almide_ir::Mutability::Let,
                    },
                    span: span.clone(),
                });
                IrExpr {
                    kind: IrExprKind::Var { id: t },
                    ty: subject.ty.clone(),
                    span: span.clone(),
                    def_id: None,
                }
            };
            let len_var = VarId(self.next);
            self.next += 1;
            stmts.push(IrStmt {
                kind: IrStmtKind::Bind {
                    var: len_var,
                    ty: Ty::Int,
                    value: IrExpr {
                        kind: IrExprKind::Call {
                            target: almide_ir::CallTarget::Module {
                                module: almide_lang::intern::sym("list"),
                                func: almide_lang::intern::sym("len"),
                                def_id: None,
                            },
                            args: vec![t_ref.clone()],
                            type_args: Vec::new(),
                        },
                        ty: Ty::Int,
                        span: span.clone(),
                        def_id: None,
                    },
                    mutability: almide_ir::Mutability::Let,
                },
                span: span.clone(),
            });
            let len_ref = IrExpr {
                kind: IrExprKind::Var { id: len_var },
                ty: Ty::Int,
                span: span.clone(),
                def_id: None,
            };
            let mk_int = |v: i64| IrExpr {
                kind: IrExprKind::LitInt { value: v },
                ty: Ty::Int,
                span: span.clone(),
                def_id: None,
            };
            let mk_eq = |l: IrExpr, r: IrExpr| IrExpr {
                kind: IrExprKind::BinOp { op: BinOp::Eq, left: Box::new(l), right: Box::new(r) },
                ty: Ty::Bool,
                span: span.clone(),
                def_id: None,
            };
            // Build each group's body: element temps, per-arm conds, terminal.
            let mut chain = last.body.clone();
            for (k, gas) in groups.iter().rev() {
                let mut gstmts: Vec<IrStmt> = Vec::new();
                let mut elem_refs: Vec<IrExpr> = Vec::new();
                for i in 0..*k {
                    let ev = VarId(self.next);
                    self.next += 1;
                    gstmts.push(IrStmt {
                        kind: IrStmtKind::Bind {
                            var: ev,
                            ty: elem_ty.clone(),
                            value: IrExpr {
                                kind: IrExprKind::IndexAccess {
                                    object: Box::new(t_ref.clone()),
                                    index: Box::new(mk_int(i as i64)),
                                },
                                ty: elem_ty.clone(),
                                span: span.clone(),
                                def_id: None,
                            },
                            mutability: almide_ir::Mutability::Let,
                        },
                        span: span.clone(),
                    });
                    elem_refs.push(IrExpr {
                        kind: IrExprKind::Var { id: ev },
                        ty: elem_ty.clone(),
                        span: span.clone(),
                        def_id: None,
                    });
                }
                // Per-arm: hoist binds (aliases of element temps) at the group top,
                // then the cond chain (literal eqs AND the guard).
                let mut inner = last.body.clone();
                let mut terminated = false;
                for a in gas.iter().rev() {
                    let IrPattern::List { elements } = &a.pattern else { unreachable!() };
                    let mut cond: Option<IrExpr> = Option::None;
                    for (i, p) in elements.iter().enumerate() {
                        match p {
                            IrPattern::Literal { expr } => {
                                let eqc = mk_eq(elem_refs[i].clone(), expr.clone());
                                cond = Some(match cond.take() {
                                    Some(c) => IrExpr {
                                        kind: IrExprKind::BinOp {
                                            op: BinOp::And,
                                            left: Box::new(c),
                                            right: Box::new(eqc),
                                        },
                                        ty: Ty::Bool,
                                        span: span.clone(),
                                        def_id: None,
                                    },
                                    Option::None => eqc,
                                });
                            }
                            IrPattern::Bind { var, ty } => gstmts.push(IrStmt {
                                kind: IrStmtKind::Bind {
                                    var: *var,
                                    ty: ty.clone(),
                                    value: elem_refs[i].clone(),
                                    mutability: almide_ir::Mutability::Let,
                                },
                                span: span.clone(),
                            }),
                            IrPattern::Wildcard => {}
                            _ => unreachable!(),
                        }
                    }
                    if let Some(g) = &a.guard {
                        cond = Some(match cond.take() {
                            Some(c) => IrExpr {
                                kind: IrExprKind::BinOp {
                                    op: BinOp::And,
                                    left: Box::new(c),
                                    right: Box::new(g.clone()),
                                },
                                ty: Ty::Bool,
                                span: span.clone(),
                                def_id: None,
                            },
                            Option::None => g.clone(),
                        });
                    }
                    inner = match cond {
                        Some(c) => IrExpr {
                            kind: IrExprKind::If {
                                cond: Box::new(c),
                                then: Box::new(a.body.clone()),
                                else_: Box::new(inner),
                            },
                            ty: out_ty.clone(),
                            span: span.clone(),
                            def_id: None,
                        },
                        Option::None => {
                            terminated = true;
                            a.body.clone()
                        }
                    };
                }
                let _ = terminated;
                let group_body = IrExpr {
                    kind: IrExprKind::Block { stmts: gstmts, expr: Some(Box::new(inner)) },
                    ty: out_ty.clone(),
                    span: span.clone(),
                    def_id: None,
                };
                let len_cond = mk_eq(len_ref.clone(), mk_int(*k as i64));
                chain = IrExpr {
                    kind: IrExprKind::If {
                        cond: Box::new(len_cond),
                        then: Box::new(group_body),
                        else_: Box::new(chain),
                    },
                    ty: out_ty.clone(),
                    span: span.clone(),
                    def_id: None,
                };
            }
            *e = IrExpr {
                kind: IrExprKind::Block { stmts, expr: Some(Box::new(chain)) },
                ty: out_ty,
                span,
                def_id: e.def_id,
            };
            self.changed = true;
        }
    }
    let mut v = V { next: max_var_id(body) + 1, changed: false };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    v.changed.then_some(out)
}
