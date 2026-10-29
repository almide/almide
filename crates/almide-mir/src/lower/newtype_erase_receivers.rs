// The in-place mutator receiver fence and its var-use helper for
// `newtype_erase.rs`'s global inlining, split out by size (included there).

/// Every var used as the RECEIVER (args[0]) of an in-place `&mut` mutator anywhere in
/// the program — the [`inline_pure_call_globals`] fence (#906). A receiver position is
/// a WRITE target, so substituting a global's initializer there produces a temporary
/// nobody owns; keeping the `Var` lets the receiver check name the global and the fix.
/// Over-approximating by ignoring VarId regions is harmless: an id that is a local in
/// one region and a top-let in another only loses an inlining opportunity.
fn collect_inplace_mutator_receivers(
    program: &almide_ir::IrProgram,
) -> std::collections::HashSet<almide_ir::VarId> {
    use almide_ir::visit::{walk_expr, IrVisitor};
    use almide_ir::{CallTarget, IrExprKind};
    struct V(std::collections::HashSet<almide_ir::VarId>);
    impl IrVisitor for V {
        fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
            if let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
            | IrExprKind::TailCall {
                target: CallTarget::Module { module, func, .. }, args, ..
            } = &e.kind
            {
                if crate::lower::is_inplace_mutator(module.as_str(), func.as_str()) {
                    if let Some(IrExprKind::Var { id }) = args.first().map(|a| &a.kind) {
                        self.0.insert(*id);
                    }
                }
            }
            walk_expr(self, e);
        }
    }
    let mut v = V(std::collections::HashSet::new());
    for f in &program.functions {
        v.visit_expr(&f.body);
    }
    for tl in &program.top_lets {
        v.visit_expr(&tl.value);
    }
    for m in &program.modules {
        for f in &m.functions {
            v.visit_expr(&f.body);
        }
        for tl in &m.top_lets {
            v.visit_expr(&tl.value);
        }
    }
    v.0
}

/// True when the expression tree contains ANY `Var` node — the cross-region
/// substitution fence: a region-local id spliced into another region would
/// resolve to an unrelated variable.
fn expr_contains_var(e: &almide_ir::IrExpr) -> bool {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct V(bool);
    impl IrVisitor for V {
        fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
            if self.0 {
                return;
            }
            if matches!(e.kind, almide_ir::IrExprKind::Var { .. }) {
                self.0 = true;
                return;
            }
            walk_expr(self, e);
        }
    }
    let mut v = V(false);
    v.visit_expr(e);
    v.0
}
