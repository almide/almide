/// Is `e` a PURE, freely-duplicable match subject — a `Var` or a literal? `build_match_chain`
/// inlines such a subject into EACH literal arm's `==` test (no re-eval cost / effect). A
/// non-pure subject (a CALL) inlined per arm would be EVALUATED once per arm — wrong if it has
/// effects, wasteful always, and it makes the MIR carry N subject-calls where the source had one
/// (a `mir > ir` caps-gate breach). [`desugar_match_subject_hoist`] lifts those to a single eval.
fn is_pure_match_subject(e: &IrExpr) -> bool {
    matches!(
        &e.kind,
        IrExprKind::Var { .. }
            | IrExprKind::LitInt { .. }
            | IrExprKind::LitBool { .. }
            | IrExprKind::LitFloat { .. }
            | IrExprKind::LitStr { .. }
    )
}

/// HOIST a non-pure (call) subject of a NON-VARIANT `match` (the `build_match_chain` literal-arm
/// shape) into a single `let __m = subject` and rewrite the match to dispatch on `Var(__m)` — so
/// the subject call is EVALUATED ONCE, not duplicated into each arm's `==` test. This is both a
/// correctness fix (a side-effecting subject must run once) and the alignment that keeps the caps
/// gate exact: `count_ir_calls` then sees ONE subject call (matching the MIR's one), so `mir <= ir`
/// holds for a resolved cross-module/self-pkg call subject (`match q.kind(x) { "a" => .., .. }`).
/// Applied in the SHARED [`desugar_heap_branches`] so the lowering and the count gate agree.
/// FIRES ONLY for a non-variant match (Int/String/Bool literal arms) whose subject is non-pure —
/// a variant (Option/Result/ADT) match already evaluates its subject once (`bind_subject` /
/// `try_lower_variant_value_match`), so it is left untouched (no v0-corpus shape changes). Recurses
/// into block stmts / tails / if & match arms so a nested such match is hoisted too.
fn desugar_match_subject_hoist(body: &IrExpr, next_var: &mut u32) -> Option<IrExpr> {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    use almide_lang::types::Ty;
    // A variant subject (Option/Result/user ADT) goes through the variant path (single-eval),
    // NOT build_match_chain — leave it alone.
    let is_variant_subject = |ty: &Ty| {
        matches!(ty, Ty::Applied(TC::Option | TC::Result, _))
            || matches!(ty, Ty::Named(..) | Ty::Variant { .. })
    };
    if let IrExprKind::Match { subject, arms } = &body.kind {
        let has_literal_arm = arms
            .iter()
            .any(|a| matches!(a.pattern, almide_ir::IrPattern::Literal { .. }));
        // A COMPUTED-call (funcref / `Op::CallIndirect`) subject cannot lower INLINE through the
        // variant path (which materializes only a Var / Named / Module subject), so hoist it to a
        // `let $t = f(x); match $t` even for an Option/Result subject — the bind's heap-result
        // CallIndirect + seeded read-shape then makes the match lower (the `fan.map` traverse `match
        // f(x) { ok/err }` shape).
        let is_computed_call = matches!(
            &subject.kind,
            IrExprKind::Call { target: almide_ir::CallTarget::Computed { .. }, .. }
        ) || matches!(
            &subject.kind,
            // `fan.map` (a compiler intrinsic lowered to a self-host Result call) as a match subject —
            // hoist it so the bind seeds its cap-as-tag read-shape, then `match $t { ok/err }` lowers
            // (its auto-`!` desugars to exactly this match).
            IrExprKind::Call { target: almide_ir::CallTarget::Module { module, func, .. }, .. }
                if module.as_str() == "fan" && func.as_str() == "map"
        ) || matches!(
            &subject.kind,
            // `regex.find(...)` (a self-host Option[String] call) as a match subject —
            // hoist so the bind seeds its materialized-Option read-shape, then
            // `match $t { some/none }` lowers (the regex-corpus match shape).
            IrExprKind::Call { target: almide_ir::CallTarget::Module { module, func, .. }, .. }
                if module.as_str() == "regex" && (func.as_str() == "find" || func.as_str() == "captures")
        );
        if (has_literal_arm
            && !is_pure_match_subject(subject)
            && !is_variant_subject(&subject.ty))
            || is_computed_call
        {
            let tmp = VarId(*next_var);
            *next_var += 1;
            let tmp_var = IrExpr {
                kind: IrExprKind::Var { id: tmp },
                ty: subject.ty.clone(),
                span: subject.span.clone(),
                def_id: None,
            };
            // The match dispatching on the hoisted `Var(tmp)` (arms unchanged — they reference the
            // subject only through the desugar's `subject.clone()`, now the cheap Var).
            let new_match = IrExpr {
                kind: IrExprKind::Match { subject: Box::new(tmp_var), arms: arms.clone() },
                ty: body.ty.clone(),
                span: body.span.clone(),
                def_id: body.def_id,
            };
            let bind = IrStmt {
                kind: IrStmtKind::Bind {
                    var: tmp,
                    mutability: almide_ir::Mutability::Let,
                    ty: subject.ty.clone(),
                    value: (**subject).clone(),
                },
                span: body.span.clone(),
            };
            return Some(IrExpr {
                kind: IrExprKind::Block { stmts: vec![bind], expr: Some(Box::new(new_match)) },
                ty: body.ty.clone(),
                span: body.span.clone(),
                def_id: body.def_id,
            });
        }
    }
    // Recurse into the structural positions a match can hide in.
    match &body.kind {
        IrExprKind::Block { stmts, expr } => {
            // Recurse into each stmt's value (Bind / Expr / Assign — the value-bearing stmts a
            // match can sit in) by cloning the stmt and replacing its value via `map_children`.
            for (i, s) in stmts.iter().enumerate() {
                let v = match &s.kind {
                    IrStmtKind::Expr { expr } => Some(expr),
                    IrStmtKind::Bind { value, .. } => Some(value),
                    IrStmtKind::Assign { value, .. } => Some(value),
                    _ => None,
                };
                if let Some(v) = v {
                    if let Some(nv) = desugar_match_subject_hoist(v, next_var) {
                        let mut ns = stmts.clone();
                        ns[i].kind = match s.kind.clone() {
                            IrStmtKind::Expr { .. } => IrStmtKind::Expr { expr: nv },
                            IrStmtKind::Bind { var, mutability, ty, .. } => {
                                IrStmtKind::Bind { var, mutability, ty, value: nv }
                            }
                            IrStmtKind::Assign { var, .. } => IrStmtKind::Assign { var, value: nv },
                            other => other,
                        };
                        return Some(IrExpr {
                            kind: IrExprKind::Block { stmts: ns, expr: expr.clone() },
                            ty: body.ty.clone(),
                            span: body.span.clone(),
                            def_id: body.def_id,
                        });
                    }
                }
            }
            if let Some(t) = expr {
                if let Some(nt) = desugar_match_subject_hoist(t, next_var) {
                    return Some(IrExpr {
                        kind: IrExprKind::Block { stmts: stmts.clone(), expr: Some(Box::new(nt)) },
                        ty: body.ty.clone(),
                        span: body.span.clone(),
                        def_id: body.def_id,
                    });
                }
            }
            None
        }
        IrExprKind::If { cond, then, else_ } => {
            if let Some(nt) = desugar_match_subject_hoist(then, next_var) {
                return Some(IrExpr {
                    kind: IrExprKind::If {
                        cond: cond.clone(),
                        then: Box::new(nt),
                        else_: else_.clone(),
                    },
                    ty: body.ty.clone(),
                    span: body.span.clone(),
                    def_id: body.def_id,
                });
            }
            if let Some(ne) = desugar_match_subject_hoist(else_, next_var) {
                return Some(IrExpr {
                    kind: IrExprKind::If {
                        cond: cond.clone(),
                        then: then.clone(),
                        else_: Box::new(ne),
                    },
                    ty: body.ty.clone(),
                    span: body.span.clone(),
                    def_id: body.def_id,
                });
            }
            None
        }
        IrExprKind::Match { subject, arms } => {
            for (i, a) in arms.iter().enumerate() {
                if let Some(nb) = desugar_match_subject_hoist(&a.body, next_var) {
                    let mut na = arms.clone();
                    na[i].body = nb;
                    return Some(IrExpr {
                        kind: IrExprKind::Match { subject: subject.clone(), arms: na },
                        ty: body.ty.clone(),
                        span: body.span.clone(),
                        def_id: body.def_id,
                    });
                }
            }
            None
        }
        _ => None,
    }
}

/// Count the occurrences of `var` (as an `IrExprKind::Var`) inside `e` — a local use-count for the
/// inline desugar below (the global var_table.use_count is post-lowering, unavailable here).
fn count_var_uses(e: &IrExpr, var: VarId) -> usize {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct C {
        var: VarId,
        n: usize,
    }
    impl IrVisitor for C {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Var { id } = &e.kind {
                if *id == self.var {
                    self.n += 1;
                }
            }
            walk_expr(self, e);
        }
    }
    let mut c = C { var, n: 0 };
    c.visit_expr(e);
    c.n
}

/// Is `var`'s ONLY occurrence in `e` the SUBJECT of a VARIANT (Option/Result/ADT) `match` with NO
/// literal-pattern arm? Walks `e` and, for the one `Var(var)`, requires it sits directly under such a
/// `Match { subject }`. A use anywhere else (a match ARM, an arg, an operand) returns false — so the
/// inline below never moves the bound value into a position that would re-evaluate it or change
/// ownership. CRUCIAL: the VARIANT-subject + NO-literal-arm gate keeps this DISJOINT from
/// `desugar_match_subject_hoist`, which deliberately HOISTS a LITERAL-arm match's call subject into a
/// `let` — inlining THAT back would ping-pong the fixpoint forever (a stack overflow). The two desugars
/// own non-overlapping match shapes.
fn sole_use_is_match_subject(e: &IrExpr, var: VarId) -> bool {
    use almide_ir::visit::{walk_expr, IrVisitor};
    use almide_lang::types::constructor::TypeConstructorId as TC;
    let is_inlinable_variant_match = |arms: &[almide_ir::IrMatchArm], subj_ty: &Ty| -> bool {
        let variant_subject = matches!(subj_ty, Ty::Applied(TC::Option | TC::Result, _))
            || matches!(subj_ty, Ty::Named(..) | Ty::Variant { .. });
        let has_literal_arm = arms
            .iter()
            .any(|a| matches!(a.pattern, almide_ir::IrPattern::Literal { .. }));
        variant_subject && !has_literal_arm
    };
    struct C<'a> {
        var: VarId,
        ok: bool,        // the one occurrence is an inlinable-variant-match subject
        bad: bool,       // a non-(inlinable-match-subject) occurrence was seen
        subjects: Vec<usize>, // ptr-identity of subjects of inlinable variant matches
        pred: &'a dyn Fn(&[almide_ir::IrMatchArm], &Ty) -> bool,
    }
    impl IrVisitor for C<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Match { subject, arms } = &e.kind {
                if (self.pred)(arms, &subject.ty) {
                    self.subjects.push(subject.as_ref() as *const IrExpr as usize);
                }
            }
            if let IrExprKind::Var { id } = &e.kind {
                if *id == self.var {
                    if self.subjects.contains(&(e as *const IrExpr as usize)) {
                        self.ok = true;
                    } else {
                        self.bad = true;
                    }
                }
            }
            walk_expr(self, e);
        }
    }
    let mut c = C { var, ok: false, bad: false, subjects: Vec::new(), pred: &is_inlinable_variant_match };
    c.visit_expr(e);
    c.ok && !c.bad
}

/// INLINE a SINGLE-USE let-bound MATCH SUBJECT: `{ …; let p = <expr>; …; match p { … } }` →
/// `{ …; …; match <expr> { … } }` WHEN `p`'s ONLY occurrence (across the remaining block stmts + tail)
/// is that match's subject. This turns a let-bound-Var variant-match subject (`let p = json.get(case,
/// "payload"); match p { some(pl) => …inner flat_map(pf => f(pf, capture))… }`) into the INLINE-subject
/// form the variant-match str-acc handler lowers via C1 (materializes `<expr>` fresh/owned + C1-inlines
/// the inner capturing flat_map) — instead of the let-bound Var routing the inner flat_map to the
/// funcref-dropping C2-lift (which now WALLS, the value-position-HOF guard). The bindgen / wasm-bindgen
/// `gen_pack_variant` / `emit_variant_helpers` value-position-HOF blocker.
///
/// VALUE-PRESERVING + ownership-neutral: `<expr>` is evaluated EXACTLY ONCE either way (the SINGLE-use
/// gate ⇒ no duplicated evaluation, no duplicated allocation), and moving it into the (sole) subject
/// position is exactly what the inline-subject source would have produced — the variant-match handler
/// materializes/owns it identically. STRICT: `p` is a `let` (not `var`), used EXACTLY ONCE, and that one
/// use is the match SUBJECT (`sole_use_is_match_subject`). A multi-use `p`, or a use elsewhere, declines
/// (NO inline — duplicating the value's evaluation would change semantics/ownership). A pure IR→IR
/// rewrite applied desugar-before-both, so the lowering + the `count_ir_calls` caps gate see the same
/// tree (mir == ir by construction — the call moves position, it is not duplicated).
pub fn desugar_inline_single_use_match_subject(body: &IrExpr) -> Option<IrExpr> {
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        return None;
    };
    // Find a `let p = <expr>` whose `p` is used EXACTLY ONCE — as a match subject — in everything that
    // FOLLOWS it (the later stmts + the tail). (A use BEFORE the bind is impossible — `p` is not yet in
    // scope — so counting the suffix is the whole live range.)
    let (i, p, value) = stmts.iter().enumerate().find_map(|(i, s)| match &s.kind {
        IrStmtKind::Bind { var, value, mutability: almide_ir::Mutability::Let, .. } => {
            let rest_stmts = &stmts[i + 1..];
            let uses: usize = rest_stmts.iter().map(|s| count_var_uses_in_stmt(s, *var)).sum::<usize>()
                + tail.as_ref().map(|t| count_var_uses(t, *var)).unwrap_or(0);
            if uses != 1 {
                return None;
            }
            // The sole use must be a match subject in EXACTLY the position it occurs (the rest stmts OR
            // the tail). Check both — exactly one holds (uses == 1).
            let in_rest = rest_stmts.iter().any(|s| stmt_sole_use_is_match_subject(s, *var));
            let in_tail = tail.as_ref().map(|t| sole_use_is_match_subject(t, *var)).unwrap_or(false);
            if in_rest || in_tail {
                Some((i, *var, value.clone()))
            } else {
                None
            }
        }
        _ => None,
    })?;
    // Substitute `p` → `<expr>` in the FOLLOWING stmts + tail, drop the bind. `<expr>` lands exactly in
    // the (sole) match-subject slot.
    let mut new_stmts: Vec<IrStmt> = stmts[..i].to_vec();
    for s in &stmts[i + 1..] {
        new_stmts.push(almide_ir::substitute::substitute_var_in_stmt(s, p, &value));
    }
    let new_tail = tail
        .as_ref()
        .map(|t| Box::new(almide_ir::substitute::substitute_var_in_expr(t, p, &value)));
    Some(IrExpr {
        kind: IrExprKind::Block { stmts: new_stmts, expr: new_tail },
        ty: body.ty.clone(),
        span: body.span.clone(),
        def_id: body.def_id,
    })
}

/// `count_var_uses` over a STATEMENT's value-bearing children (Bind/Assign/Expr/…).
fn count_var_uses_in_stmt(s: &IrStmt, var: VarId) -> usize {
    use almide_ir::visit::{walk_stmt, IrVisitor};
    struct C {
        var: VarId,
        n: usize,
    }
    impl IrVisitor for C {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Var { id } = &e.kind {
                if *id == self.var {
                    self.n += 1;
                }
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    let mut c = C { var, n: 0 };
    walk_stmt(&mut c, s);
    c.n
}

/// `sole_use_is_match_subject` over a STATEMENT's value-bearing children (same variant-only gate).
fn stmt_sole_use_is_match_subject(s: &IrStmt, var: VarId) -> bool {
    use almide_ir::visit::{walk_stmt, IrVisitor};
    use almide_lang::types::constructor::TypeConstructorId as TC;
    let is_inlinable_variant_match = |arms: &[almide_ir::IrMatchArm], subj_ty: &Ty| -> bool {
        let variant_subject = matches!(subj_ty, Ty::Applied(TC::Option | TC::Result, _))
            || matches!(subj_ty, Ty::Named(..) | Ty::Variant { .. });
        let has_literal_arm = arms
            .iter()
            .any(|a| matches!(a.pattern, almide_ir::IrPattern::Literal { .. }));
        variant_subject && !has_literal_arm
    };
    struct C<'a> {
        var: VarId,
        ok: bool,
        bad: bool,
        subjects: Vec<usize>,
        pred: &'a dyn Fn(&[almide_ir::IrMatchArm], &Ty) -> bool,
    }
    impl IrVisitor for C<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Match { subject, arms } = &e.kind {
                if (self.pred)(arms, &subject.ty) {
                    self.subjects.push(subject.as_ref() as *const IrExpr as usize);
                }
            }
            if let IrExprKind::Var { id } = &e.kind {
                if *id == self.var {
                    if self.subjects.contains(&(e as *const IrExpr as usize)) {
                        self.ok = true;
                    } else {
                        self.bad = true;
                    }
                }
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    let mut c = C { var, ok: false, bad: false, subjects: Vec::new(), pred: &is_inlinable_variant_match };
    walk_stmt(&mut c, s);
    c.ok && !c.bad
}

/// `{ …; let v = <heap expr>; recurse(.., v, ..) }` — a let-bound heap accumulator passed to the block
/// TAIL — is INLINED: substitute `v` with its value in the tail and drop the let, yielding
/// `recurse(.., <heap expr>, ..)`. REQUIRED for the TCO over a tail-duplicated accumulator: the
/// let-bound-if pre-desugar turns `let new_acc = if c then acc+[..] else acc+[..]; recurse(.., new_acc)`
/// into branched `{ let new_acc = acc+[..]; recurse(.., new_acc) }` arms, but the recursion then passes
/// `new_acc` (a Var) — which `is_self_append` (`Var(acc) + …`) does NOT recognize, so the TCO declines.
/// Inlining restores the DIRECT `recurse(.., acc+[..])` the TCO admits. GATED: the let is the LAST stmt
/// (no intervening reassignment of the value's vars), its value is heap, and `v` is used EXACTLY ONCE
/// in the tail (so no allocation is duplicated). Base64 decode_chunks / toml accumulators.
pub fn desugar_inline_tail_accumulator(body: &IrExpr) -> Option<IrExpr> {
    let IrExprKind::Block { stmts, expr: Some(tail) } = &body.kind else {
        return None;
    };
    let i = stmts.len().checked_sub(1)?;
    let IrStmtKind::Bind { var, value, mutability: almide_ir::Mutability::Let, .. } = &stmts[i].kind
    else {
        return None;
    };
    if !is_heap_ty(&value.ty) {
        return None;
    }
    // Only a self-append-shaped value (`acc + …`) — the accumulator case the TCO needs; avoids
    // inlining an arbitrary call (which could move a side effect into the tail).
    if !matches!(
        &value.kind,
        IrExprKind::BinOp {
            op: almide_ir::BinOp::ConcatList | almide_ir::BinOp::ConcatStr,
            ..
        }
    ) {
        return None;
    }
    if count_var_uses(tail, *var) != 1 {
        return None;
    }
    let new_tail = almide_ir::substitute::substitute_var_in_expr(tail, *var, value);
    Some(IrExpr {
        kind: IrExprKind::Block {
            stmts: stmts[..i].to_vec(),
            expr: Some(Box::new(new_tail)),
        },
        ty: body.ty.clone(),
        span: body.span.clone(),
        def_id: body.def_id,
    })
}

