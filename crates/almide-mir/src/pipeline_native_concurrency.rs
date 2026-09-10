// The MIR's WASM lowering intentionally serializes fan work. Until the native
// MIR renderer represents scoped threads, decline these programs BEFORE that
// lowering erases the concurrency boundary. The CLI then uses native codegen.
fn require_native_concurrency_support(ir: &almide_ir::IrProgram) -> Result<(), LowerError> {
    use almide_ir::{CallTarget, IrExpr, IrExprKind};
    use almide_ir::visit::{IrVisitor, walk_expr};
    struct Concurrency(Option<LowerError>);
    impl IrVisitor for Concurrency {
        fn visit_expr(&mut self, expr: &IrExpr) {
            let concurrent = matches!(&expr.kind, IrExprKind::Fan { .. })
                || matches!(&expr.kind,
                    IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. }
                    if module.as_str() == "fan" && matches!(func.as_str(), "map" | "any_map"));
            if concurrent && self.0.is_none() {
                self.0 = Some(LowerError::at(expr.span, "native: fan concurrency requires the scoped-thread codegen"));
            }
            walk_expr(self, expr);
        }
    }
    let mut concurrency = Concurrency(None);
    for func in &ir.functions {
        if !func.is_test { concurrency.visit_expr(&func.body); }
    }
    concurrency.0.map_or(Ok(()), Err)
}
