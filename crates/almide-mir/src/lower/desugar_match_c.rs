
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
            let (last, init) = arms.split_last().expect("arms.len() >= 3, guarded above, so split_last() is Some");
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
            let (last, init) = arms.split_last().expect("arms.len() >= 2, guarded above, so split_last() is Some");
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
