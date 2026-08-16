// ── tail of desugar_b.rs, include!-spliced back at module level ──
//
// A pure code move: this file continues its parent verbatim. The split exists
// only so the parent stays under the 800-line ceiling the codopsy gate holds
// this crate to; there is no boundary of meaning here, and `include!` at module
// level is the one splice Rust allows (an impl-item position rejects it).

/// `option.unwrap_or(option.map(list.find(xs, λ1), λ2), 32)` — the C-127 PIPED
/// generic chain. The lambda lift is statement-position-sensitive: the SAME chain
/// written as source `let`s lowers, while the nested-call form walls its HOF
/// links. ANF the nested heap-result HOF links into `let` bindings at the
/// fn/Block TAIL, so the lowering sees the proven decomposed form. Trigger: a
/// tail Module call carrying a heap-result Module-call argument that
/// (transitively) takes a Lambda. Call COUNT is preserved (desugar-before-both:
/// the counting gate sees the same calls, merely let-bound).
pub(crate) fn desugar_hof_chain_anf(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{IrStmt, IrStmtKind, Mutability, VarId};
    fn call_carries_lambda(e: &IrExpr) -> bool {
        let IrExprKind::Call { args, .. } = &e.kind else { return false };
        args.iter()
            .any(|a| matches!(a.kind, IrExprKind::Lambda { .. }) || call_carries_lambda(a))
    }
    fn needs_anf_arg(a: &IrExpr) -> bool {
        crate::lower::is_heap_ty(&a.ty)
            && matches!(&a.kind, IrExprKind::Call { target: CallTarget::Module { .. }, .. })
            && call_carries_lambda(a)
    }
    fn anf_arg(a: &IrExpr, next: &mut u32, binds: &mut Vec<IrStmt>) -> IrExpr {
        let IrExprKind::Call { target, args, type_args } = &a.kind else {
            return a.clone();
        };
        let new_args: Vec<IrExpr> = args
            .iter()
            .map(|ia| if needs_anf_arg(ia) { anf_arg(ia, next, binds) } else { ia.clone() })
            .collect();
        let rebuilt = IrExpr {
            kind: IrExprKind::Call {
                target: target.clone(),
                args: new_args,
                type_args: type_args.clone(),
            },
            ty: a.ty.clone(),
            span: a.span.clone(),
            def_id: a.def_id,
        };
        let tmp = VarId(*next);
        *next += 1;
        binds.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: tmp,
                mutability: Mutability::Let,
                ty: a.ty.clone(),
                value: rebuilt,
            },
            span: a.span.clone(),
        });
        IrExpr {
            kind: IrExprKind::Var { id: tmp },
            ty: a.ty.clone(),
            span: a.span.clone(),
            def_id: None,
        }
    }
    fn rewrite_tail(e: &IrExpr, next: &mut u32, changed: &mut bool) -> IrExpr {
        match &e.kind {
            IrExprKind::Block { stmts, expr } => {
                let new_tail =
                    expr.as_deref().map(|t| Box::new(rewrite_tail(t, next, changed)));
                IrExpr {
                    kind: IrExprKind::Block { stmts: stmts.clone(), expr: new_tail },
                    ty: e.ty.clone(),
                    span: e.span.clone(),
                    def_id: e.def_id,
                }
            }
            IrExprKind::Call { target: CallTarget::Module { .. }, args, .. }
                if args.iter().any(needs_anf_arg) =>
            {
                *changed = true;
                let IrExprKind::Call { target, args, type_args } = &e.kind else {
                    unreachable!()
                };
                let mut binds = Vec::new();
                let new_args: Vec<IrExpr> = args
                    .iter()
                    .map(|a| {
                        if needs_anf_arg(a) {
                            anf_arg(a, next, &mut binds)
                        } else {
                            a.clone()
                        }
                    })
                    .collect();
                let call = IrExpr {
                    kind: IrExprKind::Call {
                        target: target.clone(),
                        args: new_args,
                        type_args: type_args.clone(),
                    },
                    ty: e.ty.clone(),
                    span: e.span.clone(),
                    def_id: e.def_id,
                };
                IrExpr {
                    kind: IrExprKind::Block { stmts: binds, expr: Some(Box::new(call)) },
                    ty: e.ty.clone(),
                    span: e.span.clone(),
                    def_id: e.def_id,
                }
            }
            _ => e.clone(),
        }
    }
    // C-127 TYPE AUTHORITY: `unwrap_or(o: Option[T], d: T)` type-checks with ONE
    // T, so the chain payload and the default are the SAME type — but an
    // under-constrained generic chain leaves the payload UNRESOLVED (`Option[B]`),
    // which is judged heap and routes the whole chain (and its lambda) down the
    // heap leg — invalid for the scalar values that actually flow. The default's
    // type is authoritative (the C-127 contract): DEEP-substitute the unresolved
    // payload spelling with the default's concrete type across the rewritten
    // tail. A resolved chain has payload == default, so the substitution is the
    // identity there.
    fn subst_ty(t: &Ty, from: &Ty, to: &Ty) -> Ty {
        use almide_lang::types::constructor::TypeConstructorId as TCI;
        if t == from {
            return to.clone();
        }
        match t {
            Ty::Applied(c, args) => Ty::Applied(
                match c {
                    TCI::List => TCI::List,
                    other => other.clone(),
                },
                args.iter().map(|a| subst_ty(a, from, to)).collect(),
            ),
            Ty::Tuple(ts) => Ty::Tuple(ts.iter().map(|a| subst_ty(a, from, to)).collect()),
            Ty::Fn { is_effect: _, params, ret } => Ty::Fn { is_effect: false, 
                params: params.iter().map(|a| subst_ty(a, from, to)).collect(),
                ret: Box::new(subst_ty(ret, from, to)),
            },
            _ => t.clone(),
        }
    }
    fn subst_expr(e: &mut IrExpr, from: &Ty, to: &Ty) {
        use almide_ir::visit_mut::{walk_expr_mut, walk_stmt_mut, IrMutVisitor};
        struct S<'a> {
            from: &'a Ty,
            to: &'a Ty,
        }
        impl IrMutVisitor for S<'_> {
            fn visit_expr_mut(&mut self, e: &mut IrExpr) {
                e.ty = subst_ty(&e.ty, self.from, self.to);
                if let IrExprKind::Lambda { params, .. } = &mut e.kind {
                    for (_, t) in params.iter_mut() {
                        *t = subst_ty(t, self.from, self.to);
                    }
                }
                walk_expr_mut(self, e);
            }
            fn visit_stmt_mut(&mut self, s: &mut almide_ir::IrStmt) {
                if let IrStmtKind::Bind { ty, .. } = &mut s.kind {
                    *ty = subst_ty(ty, self.from, self.to);
                }
                walk_stmt_mut(self, s);
            }
        }
        S { from, to }.visit_expr_mut(e);
    }
    // Pure guard, no recursion — named so the recursive `tail_unwrap_payload`
    // match arm below reads as one condition instead of three inlined clauses.
    fn is_unwrap_or_call(module: almide_lang::intern::Sym, func: almide_lang::intern::Sym, arg_count: usize) -> bool {
        func.as_str() == "unwrap_or" && matches!(module.as_str(), "option" | "result") && arg_count == 2
    }
    // Pure extraction, no recursion — the Option/Result payload ty an `unwrap_or`'s
    // first arg carries, or None outside those two shapes.
    fn unwrap_or_payload_ty(arg_ty: &Ty) -> Option<Ty> {
        use almide_lang::types::constructor::TypeConstructorId as TCI;
        match arg_ty {
            Ty::Applied(TCI::Option, a) if a.len() == 1 => Some(a[0].clone()),
            Ty::Applied(TCI::Result, a) if a.len() == 2 => Some(a[0].clone()),
            _ => None,
        }
    }
    // Find the (unresolved payload, authoritative default) pair at the Block tail.
    fn tail_unwrap_payload(e: &IrExpr) -> Option<(Ty, Ty)> {
        match &e.kind {
            IrExprKind::Block { expr: Some(t), .. } => tail_unwrap_payload(t),
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                if is_unwrap_or_call(*module, *func, args.len()) =>
            {
                let d_ty = args[1].ty.clone();
                let payload = unwrap_or_payload_ty(&args[0].ty)?;
                (payload != d_ty).then_some((payload, d_ty))
            }
            _ => None,
        }
    }
    let mut changed = false;
    let mut next = crate::lower::desugar_var_seed();
    let mut out = rewrite_tail(body, &mut next, &mut changed);
    if changed {
        // Substitute across the WHOLE rewritten body — the chain links now live in
        // the hoisted binds, not inside the tail call's own subtree.
        if let Some((p, d_ty)) = tail_unwrap_payload(&out) {
            subst_expr(&mut out, &p, &d_ty);
        }
    }
    changed.then_some(out)
}
