/// Tail-call conversion makes immutable parameters into mutable loop slots.
/// A closure must capture this invocation's parameter values, not a cell that
/// later iterations overwrite (a closure accumulator would capture itself).
/// Snapshot at closure construction, preserving intervening effects and the
/// separate semantics of explicitly mutable parameters.
fn snapshot_tco_captures(body: &mut IrExpr, params: &[almide_ir::IrParam], carried: &[bool], next: &mut u32) {
    use almide_ir::{IrMutVisitor, walk_expr_mut, Mutability};
    struct Snapshot<'a> { params: &'a [almide_ir::IrParam], carried: &'a [bool], next: &'a mut u32 }
    impl IrMutVisitor for Snapshot<'_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            if !matches!(e.kind, IrExprKind::Lambda { .. }) { walk_expr_mut(self, e); return; }
            let free = almide_ir::free_vars::free_vars(e, &std::collections::HashSet::new());
            let mut stmts = Vec::new();
            let mut lambda = e.clone();
            for (i, p) in self.params.iter().enumerate() {
                if !self.carried[i] || p.is_mut || !free.contains(&p.var) { continue; }
                let var = VarId(*self.next);
                *self.next += 1;
                stmts.push(IrStmt { kind: IrStmtKind::Bind {
                    var, mutability: Mutability::Let, ty: p.ty.clone(),
                    value: tco_ir(IrExprKind::Var { id: p.var }, p.ty.clone()),
                }, span: e.span });
                lambda = almide_ir::substitute_var_in_expr(&lambda, p.var,
                    &tco_ir(IrExprKind::Var { id: var }, p.ty.clone()));
            }
            if !stmts.is_empty() {
                e.kind = IrExprKind::Block { stmts, expr: Some(Box::new(lambda)) };
            }
        }
    }
    Snapshot { params, carried, next }.visit_expr_mut(body);
}
