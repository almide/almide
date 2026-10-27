// The heap-`??` operand ANF hoist. A HEAP-typed `a ?? d` lowers when it is
// bound by a `let` at statement position (the bind-position `??` machinery),
// but walls as an OPERAND: an `==`/`!=` side (`assert_eq(map.get(m, k) ?? none,
// some("z"))` — the cond declines, the assert's call-bearing arm then walls)
// or a record/tuple FIELD (`Tok { kind: list.get(ps, 0) ?? "", … }` — the
// ctor's field route has no `??` arm). Hoist exactly those operand lists into
// fresh `let`s, IN OPERAND ORDER (every non-atom operand of the carrying node
// is bound, so effects keep their sequence), and leave `Var`s behind.
//
// Only UNCONDITIONALLY-evaluated positions are walked: an `if` cond, a `match`
// subject, the left of `and`/`or`, a `??` operand — never an arm, a fallback,
// a short-circuit right side, a lambda, a loop body or a nested block (each of
// those is its own hoist root via the block/loop walk), so a hoisted bind
// never runs a computation the original would have skipped.

/// `true` for a heap-typed `??` — the shape the operand routes decline.
fn is_heap_qq(e: &IrExpr) -> bool {
    matches!(e.kind, IrExprKind::UnwrapOr { .. }) && crate::lower::is_heap_ty(&e.ty)
}

/// The operand ANF of [`is_heap_qq`] in `==`/`!=` operand and record/tuple
/// field position (see the file header). `None` when nothing qualifies.
pub(crate) fn desugar_heap_qq_operands(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{IrStmt, IrStmtKind, Mutability, VarId};

    fn is_atom(a: &IrExpr) -> bool {
        matches!(
            a.kind,
            IrExprKind::Var { .. }
                | IrExprKind::LitInt { .. }
                | IrExprKind::LitFloat { .. }
                | IrExprKind::LitBool { .. }
                | IrExprKind::LitStr { .. }
                | IrExprKind::Unit
        )
    }

    fn bind_operand(a: &mut IrExpr, binds: &mut Vec<IrStmt>, next: &mut u32) {
        if is_atom(a) {
            return;
        }
        let tmp = VarId(*next);
        *next += 1;
        let placeholder = IrExpr {
            kind: IrExprKind::Var { id: tmp },
            ty: a.ty.clone(),
            span: a.span.clone(),
            def_id: None,
        };
        let bound = std::mem::replace(a, placeholder);
        binds.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: tmp,
                mutability: Mutability::Let,
                ty: bound.ty.clone(),
                value: bound,
            },
            span: a.span.clone(),
        });
    }

    /// Descend the unconditionally-evaluated children of `e` (children first,
    /// so an inner node's binds precede the outer's), then hoist `e`'s own
    /// operands when it carries a heap `??`.
    fn hoist(e: &mut IrExpr, binds: &mut Vec<IrStmt>, next: &mut u32, changed: &mut bool) {
        match &mut e.kind {
            IrExprKind::If { cond, .. } => hoist(cond, binds, next, changed),
            IrExprKind::Match { subject, .. } => hoist(subject, binds, next, changed),
            IrExprKind::UnwrapOr { expr, .. } => hoist(expr, binds, next, changed),
            IrExprKind::BinOp { op: almide_ir::BinOp::And | almide_ir::BinOp::Or, left, .. } => {
                hoist(left, binds, next, changed)
            }
            IrExprKind::BinOp { op, left, right } => {
                hoist(left, binds, next, changed);
                hoist(right, binds, next, changed);
                let eq = matches!(op, almide_ir::BinOp::Eq | almide_ir::BinOp::Neq);
                // A String `??` side already lowers through the string eq route.
                let q = |x: &IrExpr| is_heap_qq(x) && !matches!(x.ty, Ty::String);
                if eq && (q(left) || q(right)) {
                    *changed = true;
                    bind_operand(left, binds, next);
                    bind_operand(right, binds, next);
                }
            }
            IrExprKind::Record { fields, .. } => {
                for (_, f) in fields.iter_mut() {
                    hoist(f, binds, next, changed);
                }
                if fields.iter().any(|(_, f)| is_heap_qq(f)) {
                    *changed = true;
                    for (_, f) in fields.iter_mut() {
                        bind_operand(f, binds, next);
                    }
                }
            }
            IrExprKind::Tuple { elements } => {
                for x in elements.iter_mut() {
                    hoist(x, binds, next, changed);
                }
                if elements.iter().any(is_heap_qq) {
                    *changed = true;
                    for x in elements.iter_mut() {
                        bind_operand(x, binds, next);
                    }
                }
            }
            IrExprKind::Call { args, .. } => {
                for a in args.iter_mut() {
                    hoist(a, binds, next, changed);
                }
            }
            _ => {}
        }
    }

    fn rewrite_stmts(stmts: &mut Vec<IrStmt>, next: &mut u32, changed: &mut bool) {
        let mut out: Vec<IrStmt> = Vec::with_capacity(stmts.len());
        for mut s in stmts.drain(..) {
            let mut binds = Vec::new();
            match &mut s.kind {
                IrStmtKind::Bind { value, .. }
                | IrStmtKind::Assign { value, .. }
                | IrStmtKind::Expr { expr: value }
                | IrStmtKind::BindDestructure { value, .. } => hoist(value, &mut binds, next, changed),
                _ => {}
            }
            out.extend(binds);
            out.push(s);
        }
        *stmts = out;
    }

    struct BlockWalk<'a> {
        next: &'a mut u32,
        changed: &'a mut bool,
    }
    impl almide_ir::IrMutVisitor for BlockWalk<'_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            almide_ir::visit_mut::walk_expr_mut(self, e);
            match &mut e.kind {
                IrExprKind::Block { stmts, expr } => {
                    rewrite_stmts(stmts, self.next, self.changed);
                    if let Some(t) = expr {
                        let mut binds = Vec::new();
                        hoist(t, &mut binds, self.next, self.changed);
                        stmts.extend(binds);
                    }
                }
                IrExprKind::ForIn { body, .. } | IrExprKind::While { body, .. } => {
                    rewrite_stmts(body, self.next, self.changed)
                }
                _ => {}
            }
        }
    }

    let mut changed = false;
    let mut next = crate::lower::desugar_var_seed();
    let mut out = body.clone();
    almide_ir::IrMutVisitor::visit_expr_mut(&mut BlockWalk { next: &mut next, changed: &mut changed }, &mut out);
    // A non-Block body (`fn f() -> T = Tok { … }`) whose tail needs hoists: wrap it.
    if !matches!(out.kind, IrExprKind::Block { .. }) {
        let mut binds = Vec::new();
        hoist(&mut out, &mut binds, &mut next, &mut changed);
        if !binds.is_empty() {
            let (ty, span) = (out.ty.clone(), out.span.clone());
            out = IrExpr {
                kind: IrExprKind::Block { stmts: binds, expr: Some(Box::new(out)) },
                ty,
                span,
                def_id: None,
            };
        }
    }
    changed.then_some(out)
}
