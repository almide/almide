
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
                        pattern: IrPattern::List { elements: Vec::new(), rest: Option::None },
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
    impl V {
        /// The rows of the specialization matrix: one `(column patterns, body)`
        /// per non-wildcard arm, plus the catch-all as an all-`Any` row.
        /// `None` when any arm is outside the admitted shape (a non-tuple
        /// pattern, a wrong arity, a column that is neither `[]` nor `_`, an
        /// all-`_` row, or a duplicated `_`-column body that binds).
        fn collect_rows(
            init: &[almide_ir::IrMatchArm],
            last: &almide_ir::IrMatchArm,
            k: usize,
        ) -> Option<Vec<(Vec<Cp>, IrExpr)>> {
            let mut rows: Vec<(Vec<Cp>, IrExpr)> = Vec::new();
            for a in init {
                let IrPattern::Tuple { elements: pats } = &a.pattern else { return None };
                if pats.len() != k {
                    return None;
                }
                let cps = column_patterns(pats)?;
                rows.push((cps, a.body.clone()));
            }
            rows.push((vec![Cp::Any; k], last.body.clone()));
            // A row with an `_` column can land in both spec branches — its body
            // duplicates, so it must not introduce binders.
            let dup_binds = rows
                .iter()
                .any(|(cps, b)| cps.iter().any(|c| *c == Cp::Any) && introduces_binder(b));
            (!dup_binds).then_some(rows)
        }

        /// Bind every non-Var subject column to a fresh temp so the built tree
        /// can test it more than once. Returns the pre-tree binds and the
        /// per-column reference expressions.
        fn bind_subject_columns(
            &mut self,
            elements: &[IrExpr],
            span: &Option<almide_ir::Span>,
        ) -> (Vec<IrStmt>, Vec<IrExpr>) {
            let mut stmts: Vec<IrStmt> = Vec::new();
            let mut refs: Vec<IrExpr> = Vec::new();
            for c in elements {
                if matches!(c.kind, IrExprKind::Var { .. }) {
                    refs.push(c.clone());
                    continue;
                }
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
            (stmts, refs)
        }
    }

    /// One arm's column patterns: `[]` is a TEST column, `_` matches anything.
    /// `None` when a column is some other pattern, or when the row tests
    /// nothing (an all-`_` row is the catch-all, handled separately).
    fn column_patterns(pats: &[IrPattern]) -> Option<Vec<Cp>> {
        let mut cps = Vec::with_capacity(pats.len());
        let mut cond_n = 0usize;
        for p in pats {
            match p {
                IrPattern::List { elements, rest: Option::None } if elements.is_empty() => {
                    cps.push(Cp::Empty);
                    cond_n += 1;
                }
                IrPattern::Wildcard => cps.push(Cp::Any),
                _ => return None,
            }
        }
        (cond_n > 0).then_some(cps)
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
            let (last, init) = arms.split_last().expect("arms.len() >= 3, guarded above, so split_last() is Some");
            if !matches!(last.pattern, IrPattern::Wildcard) {
                return;
            }
            let Some(rows) = Self::collect_rows(init, last, k) else { return };
            let span = e.span.clone();
            let (stmts, refs) = self.bind_subject_columns(elements, &span);
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
    let mut v = V { next: crate::lower::desugar_var_seed(), changed: false };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    // GROWTH CAP (arc v1-join-completeness, J0): this rewrite duplicates a
    // catch-all body per specialization branch and runs OUTSIDE the
    // desugar_heap_branches fixpoint, so MAX_DESUGARED_NODES never sees its
    // output — it was one of the two UNCAPPED duplicators of the 2026-07-27
    // incident class. Growth-based so a big-but-undupped body is not punished;
    // past the cap the rewrite is DISCARDED and the un-desugared match walls
    // honestly (the desugar_heap_branches discard precedent, one level out).
    if v.changed && count_expr_nodes(&out) > count_expr_nodes(body) + 50_000 {
        return None;
    }
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
    /// Extracted verbatim from `desugar_list_pattern_match`'s `visit_expr_mut`
    /// (codopsy r2, #852): decides SUBJECT admission — a `List[T]` whose single
    /// element type is scalar (non-heap) — and returns that element type.
    fn scalar_list_elem_ty(subject_ty: &Ty) -> Option<Ty> {
        match subject_ty {
            Ty::Applied(TypeConstructorId::List, a)
                if a.len() == 1 && !is_heap_ty(&a[0]) =>
            {
                Some(a[0].clone())
            }
            _ => None,
        }
    }
    /// Extracted verbatim from `desugar_list_pattern_match`'s `visit_expr_mut`
    /// (codopsy r2, #852): decides ARM admission and groups the non-terminal arms
    /// by pattern length. Admit only list patterns of Bind/Wildcard/Literal
    /// elements; at least one arm must need this desugar (a length > 0 or a
    /// guard/literal — the plain 2-arm `[] / bind` forms already lower elsewhere)
    /// — that requirement is the returned "interesting" flag.
    #[allow(clippy::type_complexity)]
    fn group_arms_by_pattern_len<'a>(
        init: &'a [almide_ir::IrMatchArm],
    ) -> Option<(Vec<(usize, Vec<&'a almide_ir::IrMatchArm>)>, bool)> {
        let mut groups: Vec<(usize, Vec<&almide_ir::IrMatchArm>)> = Vec::new();
        let mut interesting = false;
        for a in init {
            let IrPattern::List { elements, rest: Option::None } = &a.pattern else { return None };
            for p in elements {
                match p {
                    IrPattern::Bind { .. } | IrPattern::Wildcard | IrPattern::Literal { .. } => {}
                    _ => return None,
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
        Some((groups, interesting))
    }
    /// Extracted verbatim from `desugar_list_pattern_match`'s `visit_expr_mut`
    /// (codopsy r2, #852): decides whether the catch-all body will be DUPLICATED —
    /// a group without an unconditional terminal (a no-guard arm whose elements are
    /// all Bind/Wildcard) falls through to its own copy of the final else, in
    /// addition to the chain's final else.
    fn catch_all_duplication_needed(groups: &[(usize, Vec<&almide_ir::IrMatchArm>)]) -> bool {
        groups.iter().any(|(_, gas)| {
            !gas.iter().any(|a| {
                a.guard.is_none()
                    && matches!(&a.pattern, IrPattern::List { elements, rest: Option::None }
                        if elements.iter().all(|p| matches!(p,
                            IrPattern::Bind { .. } | IrPattern::Wildcard)))
            })
        })
    }
    /// Extracted verbatim from `desugar_list_pattern_match`'s `visit_expr_mut`
    /// (codopsy r2, #852, was the `mk_int` closure): an Int literal node.
    fn mk_int(v: i64, span: &Option<almide_lang::span::Span>) -> IrExpr {
        IrExpr {
            kind: IrExprKind::LitInt { value: v },
            ty: Ty::Int,
            span: span.clone(),
            def_id: None,
        }
    }
    /// Extracted verbatim from `desugar_list_pattern_match`'s `visit_expr_mut`
    /// (codopsy r2, #852, was the `mk_eq` closure): a Bool `==` node.
    fn mk_eq(l: IrExpr, r: IrExpr, span: &Option<almide_lang::span::Span>) -> IrExpr {
        IrExpr {
            kind: IrExprKind::BinOp { op: BinOp::Eq, left: Box::new(l), right: Box::new(r) },
            ty: Ty::Bool,
            span: span.clone(),
            def_id: None,
        }
    }
    /// Extracted verbatim from `desugar_list_pattern_match`'s `visit_expr_mut`
    /// (codopsy r2, #852): hoists the SUBJECT — a Var subject is referenced
    /// directly; anything else binds to a fresh temp so the list evaluates once.
    fn hoist_subject_var(
        next: &mut u32,
        stmts: &mut Vec<IrStmt>,
        subject: &IrExpr,
        span: &Option<almide_lang::span::Span>,
    ) -> IrExpr {
        if matches!(subject.kind, IrExprKind::Var { .. }) {
            subject.clone()
        } else {
            let t = VarId(*next);
            *next += 1;
            stmts.push(IrStmt {
                kind: IrStmtKind::Bind {
                    var: t,
                    ty: subject.ty.clone(),
                    value: subject.clone(),
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
        }
    }
    /// Extracted verbatim from `desugar_list_pattern_match`'s `visit_expr_mut`
    /// (codopsy r2, #852): binds `$len = list.len($t)` to a fresh temp and returns
    /// the Var reference the length tests compare against.
    fn bind_list_len(
        next: &mut u32,
        stmts: &mut Vec<IrStmt>,
        t_ref: &IrExpr,
        span: &Option<almide_lang::span::Span>,
    ) -> IrExpr {
        let len_var = VarId(*next);
        *next += 1;
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
        IrExpr {
            kind: IrExprKind::Var { id: len_var },
            ty: Ty::Int,
            span: span.clone(),
            def_id: None,
        }
    }
    /// Extracted verbatim from `desugar_list_pattern_match`'s `visit_expr_mut`
    /// (codopsy r2, #852): binds one fresh temp per element (`$ei = $t[i]`) —
    /// element loads sit UNDER their length test (no out-of-range read) — and
    /// returns the group statements plus the Var references to the element temps.
    fn bind_group_element_temps(
        next: &mut u32,
        k: usize,
        elem_ty: &Ty,
        t_ref: &IrExpr,
        span: &Option<almide_lang::span::Span>,
    ) -> (Vec<IrStmt>, Vec<IrExpr>) {
        let mut gstmts: Vec<IrStmt> = Vec::new();
        let mut elem_refs: Vec<IrExpr> = Vec::new();
        for i in 0..k {
            let ev = VarId(*next);
            *next += 1;
            gstmts.push(IrStmt {
                kind: IrStmtKind::Bind {
                    var: ev,
                    ty: elem_ty.clone(),
                    value: IrExpr {
                        kind: IrExprKind::IndexAccess {
                            object: Box::new(t_ref.clone()),
                            index: Box::new(mk_int(i as i64, span)),
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
        (gstmts, elem_refs)
    }
    /// Extracted verbatim from `desugar_list_pattern_match`'s `visit_expr_mut`
    /// (codopsy r2, #852): per-arm — hoist binds (aliases of element temps) at the
    /// group top, then the cond chain (literal eqs AND the guard); a group's first
    /// unconditional arm terminates it, else the catch-all body fills in.
    fn build_group_arm_chain(
        gas: &[&almide_ir::IrMatchArm],
        elem_refs: &[IrExpr],
        gstmts: &mut Vec<IrStmt>,
        last_body: &IrExpr,
        out_ty: &Ty,
        span: &Option<almide_lang::span::Span>,
    ) -> IrExpr {
        let mut inner = last_body.clone();
        let mut terminated = false;
        for a in gas.iter().rev() {
            let IrPattern::List { elements, .. } = &a.pattern else { unreachable!() };
            let mut cond: Option<IrExpr> = Option::None;
            for (i, p) in elements.iter().enumerate() {
                match p {
                    IrPattern::Literal { expr } => {
                        let eqc = mk_eq(elem_refs[i].clone(), expr.clone(), span);
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
        inner
    }
    /// Extracted verbatim from `desugar_list_pattern_match`'s `visit_expr_mut`
    /// (codopsy r2, #852): builds each group's body (element temps, per-arm conds,
    /// terminal) and chains the groups on `$len == k` tests, innermost group first,
    /// with the catch-all body as the final else. Lengths are mutually exclusive,
    /// so grouping preserves first-match.
    fn build_length_dispatch_chain(
        next: &mut u32,
        groups: &[(usize, Vec<&almide_ir::IrMatchArm>)],
        last_body: &IrExpr,
        elem_ty: &Ty,
        t_ref: &IrExpr,
        len_ref: &IrExpr,
        out_ty: &Ty,
        span: &Option<almide_lang::span::Span>,
    ) -> IrExpr {
        let mut chain = last_body.clone();
        for (k, gas) in groups.iter().rev() {
            let (mut gstmts, elem_refs) = bind_group_element_temps(next, *k, elem_ty, t_ref, span);
            let inner = build_group_arm_chain(gas, &elem_refs, &mut gstmts, last_body, out_ty, span);
            let group_body = IrExpr {
                kind: IrExprKind::Block { stmts: gstmts, expr: Some(Box::new(inner)) },
                ty: out_ty.clone(),
                span: span.clone(),
                def_id: None,
            };
            let len_cond = mk_eq(len_ref.clone(), mk_int(*k as i64, span), span);
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
        chain
    }
    struct V {
        next: u32,
        changed: bool,
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Match { subject, arms } = &e.kind else { return };
            let Some(elem_ty) = scalar_list_elem_ty(&subject.ty) else { return };
            if arms.len() < 2 {
                return;
            }
            let (last, init) = arms.split_last().expect("arms.len() >= 2, guarded above, so split_last() is Some");
            if last.guard.is_some() || !matches!(last.pattern, IrPattern::Wildcard) {
                return;
            }
            let Some((groups, interesting)) = group_arms_by_pattern_len(init) else { return };
            if !interesting {
                return;
            }
            // A duplicated catch-all (a group without an unconditional terminal, plus
            // the final else) must not introduce binders.
            if catch_all_duplication_needed(&groups) && introduces_binder(&last.body) {
                return;
            }
            let span = e.span.clone();
            let out_ty = e.ty.clone();
            // Hoist the subject (Var direct) and its length.
            let mut stmts: Vec<IrStmt> = Vec::new();
            let t_ref = hoist_subject_var(&mut self.next, &mut stmts, subject, &span);
            let len_ref = bind_list_len(&mut self.next, &mut stmts, &t_ref, &span);
            // Build each group's body: element temps, per-arm conds, terminal.
            let chain = build_length_dispatch_chain(
                &mut self.next,
                &groups,
                &last.body,
                &elem_ty,
                &t_ref,
                &len_ref,
                &out_ty,
                &span,
            );
            *e = IrExpr {
                kind: IrExprKind::Block { stmts, expr: Some(Box::new(chain)) },
                ty: out_ty,
                span,
                def_id: e.def_id,
            };
            self.changed = true;
        }
    }
    let mut v = V { next: crate::lower::desugar_var_seed(), changed: false };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    // GROWTH CAP (arc v1-join-completeness, J0): this rewrite duplicates a
    // catch-all body per specialization branch and runs OUTSIDE the
    // desugar_heap_branches fixpoint, so MAX_DESUGARED_NODES never sees its
    // output — it was one of the two UNCAPPED duplicators of the 2026-07-27
    // incident class. Growth-based so a big-but-undupped body is not punished;
    // past the cap the rewrite is DISCARDED and the un-desugared match walls
    // honestly (the desugar_heap_branches discard precedent, one level out).
    if v.changed && count_expr_nodes(&out) > count_expr_nodes(body) + 50_000 {
        return None;
    }
    v.changed.then_some(out)
}
