// Option/Result arm specialization and unreachable-arm pruning (#2473). include!-spliced from desugar_match_b.rs, sharing
// the lower module's imports.

/// Drop the match arms no value can reach: every arm AFTER an unguarded catch-all
/// (`_` / a binder), and every arm after an Option / Result match already covers BOTH
/// constructors with unguarded, irrefutable payload patterns (`some(v)` + `none`,
/// `ok(v)` + `err(e)`).
///
/// The or-pattern / nested-literal desugars leave exactly this residue: `match x {
/// some(3) | some(4) => a, some(5) | none => b, _ => c }` regroups into `some(v) =>
/// <inner literal match>`, `none => b`, and the source's trailing `_ => c` survives
/// OUTSIDE as a third arm that can never run. Every executed variant-match route
/// admits exactly two unguarded arms, so that dead arm alone walled the function (the
/// #2435 reference suites' `alias5` / `or_at`). Removing it is semantics-preserving by
/// construction — first-match order means no input ever selects it — and it runs in the
/// shared `desugar_all` chain, so the lowering and the caps count read the same tree.
pub fn desugar_prune_unreachable_arms(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    use almide_ir::IrPattern;

    fn irrefutable(p: &IrPattern) -> bool {
        match p {
            IrPattern::Wildcard | IrPattern::Bind { .. } => true,
            IrPattern::As { inner, .. } => irrefutable(inner),
            IrPattern::Tuple { elements } => elements.iter().all(irrefutable),
            _ => false,
        }
    }

    /// The number of arms that can run: everything up to (and including) the arm after
    /// which the match is already exhaustive.
    fn reachable_len(arms: &[almide_ir::IrMatchArm]) -> usize {
        let (mut some, mut none, mut ok, mut err) = (false, false, false, false);
        for (i, arm) in arms.iter().enumerate() {
            if arm.guard.is_some() {
                continue;
            }
            match &arm.pattern {
                p if irrefutable(p) => return i + 1,
                IrPattern::Some { inner } if irrefutable(inner) => some = true,
                IrPattern::None => none = true,
                IrPattern::Ok { inner } if irrefutable(inner) => ok = true,
                IrPattern::Err { inner } if irrefutable(inner) => err = true,
                _ => {}
            }
            if (some && none) || (ok && err) {
                return i + 1;
            }
        }
        arms.len()
    }

    struct V {
        changed: bool,
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Match { arms, .. } = &mut e.kind else { return };
            let keep = reachable_len(arms);
            if keep < arms.len() {
                arms.truncate(keep);
                self.changed = true;
            }
        }
    }
    let mut v = V { changed: false };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    v.changed.then_some(out)
}

/// Specialize an Option / Result match whose arms carry GUARDS or refutable payload
/// patterns into the two-arm constructor dispatch every executed variant-match route
/// admits, with the per-constructor residue moved INSIDE each arm:
///
/// ```text
/// match o { some(3) if g => a, none if h => b, some(_) | none => c }
///   → match o { some(p) => match p { 3 if g => a, _ => c },
///               none    => if h then b else c }
/// ```
///
/// Each constructor branch keeps the source rows that can match it, in source order: its
/// own `K(p)` rows (the payload pattern `p` moves to the inner match over a fresh payload
/// var) and every catch-all row (`_`, or a binder — substituted by the subject). A nullary
/// branch (`none`) becomes the guard chain of its rows. First-match order is therefore
/// preserved inside every branch, and only one branch runs. The inner matches are scalar /
/// String literal-and-guard chains, which `build_match_chain` already lowers.
///
/// Declines (leaving the match to the existing routes and their honest walls) when a
/// pattern is outside `some/none/ok/err/_/binder`, a branch ends without an unguarded
/// catch, or a row cloned into both branches would duplicate a binder
/// ([`introduces_binder`] — VarId uniqueness under duplication). Runs in both chains
/// (desugar-before-both), so duplicated bodies count 1:1 in the caps `mir == ir` gate.
///
/// A match over a CUSTOM variant or a plain record takes the column compiler in
/// `desugar_variant_arms_b.rs` ([`specialize_user_variant_match`]): multi-field
/// positional payloads (`Circle(0, _)`), record-form field patterns (`Circle { r: 0,
/// tag: _ }`, `P { x: 0, y }`) and nested constructor patterns (`Tagged { kind: Big(n),
/// .. }`) all reduce to the same per-constructor-arm + inner-match shape (#2559).
pub fn desugar_variant_guard_match(
    body: &IrExpr,
    layouts: &crate::lower::VariantLayouts,
) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    use almide_ir::{IrMatchArm, IrPattern};
    use almide_lang::types::constructor::TypeConstructorId as TC;

    #[derive(Clone, Copy, PartialEq)]
    enum K {
        Some_,
        None_,
        Ok_,
        Err_,
    }
    fn simple(p: &IrPattern) -> bool {
        matches!(p, IrPattern::Wildcard | IrPattern::Bind { .. })
    }
    /// The constructor and payload pattern of an arm, `None` for a catch-all.
    fn head(p: &IrPattern) -> Option<(K, Option<&IrPattern>)> {
        match p {
            IrPattern::Some { inner } => Some((K::Some_, Some(inner))),
            IrPattern::None => Some((K::None_, None)),
            IrPattern::Ok { inner } => Some((K::Ok_, Some(inner))),
            IrPattern::Err { inner } => Some((K::Err_, Some(inner))),
            _ => None,
        }
    }
    fn mk(kind: IrExprKind, ty: &Ty, span: &Option<almide_ir::Span>) -> IrExpr {
        IrExpr { kind, ty: ty.clone(), span: *span, def_id: None }
    }

    struct Branch {
        rows: Vec<IrMatchArm>,
        closed: bool,
    }

    /// The rewritten match, or `None` to leave it alone.
    fn specialize(e: &IrExpr, next: &mut u32) -> Option<IrExpr> {
        let IrExprKind::Match { subject, arms } = &e.kind else { return None };
        let (ctors, payload_tys): ([K; 2], [Option<Ty>; 2]) = match &subject.ty {
            Ty::Applied(TC::Option, a) if a.len() == 1 => {
                ([K::Some_, K::None_], [Some(a[0].clone()), None])
            }
            Ty::Applied(TC::Result, a) if a.len() == 2 => {
                ([K::Ok_, K::Err_], [Some(a[0].clone()), Some(a[1].clone())])
            }
            _ => return None,
        };
        let needed = arms.iter().any(|a| {
            a.guard.is_some() || head(&a.pattern).and_then(|(_, p)| p).is_some_and(|p| !simple(p))
        });
        if !needed {
            return None;
        }
        // Every pattern must be one of this type's two constructors or a catch-all.
        if !arms.iter().all(|a| match head(&a.pattern) {
            Some((k, _)) => ctors.contains(&k),
            None => simple(&a.pattern),
        }) {
            return None;
        }
        let span = e.span;
        // A catch-all BINDER names the whole subject: substitute a Var subject; hoist any
        // other subject once so the substitution reads a value, never a re-evaluation.
        let binds_subject = arms.iter().any(|a| matches!(a.pattern, IrPattern::Bind { .. }));
        let mut hoist: Vec<IrStmt> = Vec::new();
        let subj: IrExpr = if binds_subject && !matches!(subject.kind, IrExprKind::Var { .. }) {
            let t = VarId(*next);
            *next += 1;
            hoist.push(IrStmt {
                kind: IrStmtKind::Bind {
                    var: t,
                    ty: subject.ty.clone(),
                    value: (**subject).clone(),
                    mutability: almide_ir::Mutability::Let,
                },
                span,
            });
            mk(IrExprKind::Var { id: t }, &subject.ty, &span)
        } else {
            (**subject).clone()
        };
        let mut branches = [Branch { rows: Vec::new(), closed: false }, Branch { rows: Vec::new(), closed: false }];
        let mut copies = vec![0usize; arms.len()];
        for (i, arm) in arms.iter().enumerate() {
            let (pat, targets): (IrPattern, Vec<usize>) = match head(&arm.pattern) {
                Some((k, inner)) => {
                    let bi = ctors.iter().position(|c| *c == k)?;
                    (inner.cloned().unwrap_or(IrPattern::Wildcard), vec![bi])
                }
                None => (IrPattern::Wildcard, vec![0, 1]),
            };
            let (guard, body) = match &arm.pattern {
                IrPattern::Bind { var, .. } => (
                    arm.guard.as_ref().map(|g| almide_ir::substitute_var_in_expr(g, *var, &subj)),
                    almide_ir::substitute_var_in_expr(&arm.body, *var, &subj),
                ),
                _ => (arm.guard.clone(), arm.body.clone()),
            };
            for bi in targets {
                if branches[bi].closed {
                    continue;
                }
                if arm.guard.is_none() && simple(&pat) {
                    branches[bi].closed = true;
                }
                copies[i] += 1;
                branches[bi].rows.push(IrMatchArm { pattern: pat.clone(), guard: guard.clone(), body: body.clone() });
            }
        }
        for (i, n) in copies.iter().enumerate() {
            if *n > 1
                && (introduces_binder(&arms[i].body)
                    || arms[i].guard.as_ref().is_some_and(introduces_binder))
            {
                return None;
            }
        }
        let mut out_arms: Vec<IrMatchArm> = Vec::with_capacity(2);
        for (bi, branch) in branches.into_iter().enumerate() {
            if !branch.closed {
                return None;
            }
            let k = ctors[bi];
            let mut rows = branch.rows;
            let (payload_pat, arm_body) = match &payload_tys[bi] {
                // A nullary constructor: the guard chain of its rows, ending in the
                // first unguarded one (the branch is closed, so one exists).
                None => {
                    let last = rows.pop()?;
                    let mut acc = last.body;
                    while let Some(r) = rows.pop() {
                        acc = match r.guard {
                            Some(g) => mk(
                                IrExprKind::If { cond: Box::new(g), then: Box::new(r.body), else_: Box::new(acc) },
                                &e.ty,
                                &span,
                            ),
                            None => r.body,
                        };
                    }
                    (None, acc)
                }
                // A single unguarded row with a plain payload pattern needs no inner match.
                Some(_) if rows.len() == 1 && rows[0].guard.is_none() && simple(&rows[0].pattern) => {
                    let r = rows.pop()?;
                    (Some(r.pattern), r.body)
                }
                Some(pty) => {
                    let pv = VarId(*next);
                    *next += 1;
                    let inner = mk(
                        IrExprKind::Match {
                            subject: Box::new(mk(IrExprKind::Var { id: pv }, pty, &span)),
                            arms: rows,
                        },
                        &e.ty,
                        &span,
                    );
                    (Some(IrPattern::Bind { var: pv, ty: pty.clone() }), inner)
                }
            };
            let pattern = match (k, payload_pat) {
                (K::Some_, Some(p)) => IrPattern::Some { inner: Box::new(p) },
                (K::Ok_, Some(p)) => IrPattern::Ok { inner: Box::new(p) },
                (K::Err_, Some(p)) => IrPattern::Err { inner: Box::new(p) },
                (K::None_, _) => IrPattern::None,
                _ => return None,
            };
            out_arms.push(IrMatchArm { pattern, guard: None, body: arm_body });
        }
        let m = IrExpr {
            kind: IrExprKind::Match { subject: Box::new(subj), arms: out_arms },
            ty: e.ty.clone(),
            span,
            def_id: e.def_id,
        };
        Some(if hoist.is_empty() {
            m
        } else {
            IrExpr {
                kind: IrExprKind::Block { stmts: hoist, expr: Some(Box::new(m)) },
                ty: e.ty.clone(),
                span,
                def_id: e.def_id,
            }
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
            let mut next = self.next;
            if let Some(r) = specialize(e, &mut next)
                .or_else(|| specialize_user_variant_match(e, &mut next, self.layouts))
            {
                self.next = next;
                *e = r;
                self.changed = true;
            }
        }
    }
    let mut v = V { next: crate::lower::desugar_var_seed(), layouts, changed: false };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    v.changed.then_some(out)
}
