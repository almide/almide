// Late propagation desugaring can create an interpolation after the display
// pass. Prepare its ordinary typed helper before MIR lowering, and scope the
// lookup to this native compilation so wasm/direct lowering cannot inherit it.
thread_local! {
    static ERROR_DISPLAYS: std::cell::RefCell<Option<Vec<(Ty, Sym)>>> = const { std::cell::RefCell::new(None) };
}
pub(crate) struct ErrorDisplayScope(Option<Vec<(Ty, Sym)>>);
impl ErrorDisplayScope {
    pub(crate) fn enter() -> Self { Self(ERROR_DISPLAYS.with(|s| s.replace(Some(Vec::new())))) }
}
impl Drop for ErrorDisplayScope {
    fn drop(&mut self) { ERROR_DISPLAYS.with(|s| { s.replace(self.0.take()); }); }
}
pub(crate) fn erased_error(value: &IrExpr) -> Option<IrExpr> {
    ERROR_DISPLAYS.with(|s| s.borrow().as_ref()?.iter()
        .find(|(ty, _)| *ty == value.ty).map(|(_, name)| named(*name, value.clone())))
}
fn error_types(ir: &IrProgram) -> Vec<Ty> {
    struct Find { string_channel: bool, types: Vec<Ty> }
    impl almide_ir::visit::IrVisitor for Find {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Lambda { body, .. } = &e.kind {
                let old = self.string_channel;
                self.string_channel = matches!(&e.ty, Ty::Fn { ret, .. } if ret.result_err_ty() == Some(Ty::String));
                self.visit_expr(body);
                self.string_channel = old;
                return;
            }
            if self.string_channel {
                if let IrExprKind::Unwrap { expr } | IrExprKind::Try { expr } = &e.kind {
                    if let Some(error) = expr.ty.result_err_ty() {
                        if structural(&error) && !self.types.contains(&error) { self.types.push(error); }
                    }
                }
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    let mut find = Find { string_channel: false, types: Vec::new() };
    for f in &ir.functions {
        find.string_channel = f.ret_ty.result_err_ty() == Some(Ty::String)
            || (f.is_effect && f.ret_ty.result_err_ty().is_none());
        almide_ir::visit::IrVisitor::visit_expr(&mut find, &f.body);
    }
    find.types
}
impl Display<'_> {
    fn prepare_errors(&mut self, types: Vec<Ty>) {
        for ty in types {
            let saved = (self.vars.entries.len(), self.keys.len(), self.helpers.len());
            // Only the helper's own parameter enters the emitted body. This
            // temporary IR argument asks format to materialize the type's helper.
            let v = self.vars.alloc_fresh("error_display_type", ty.clone(), Mutability::Let, None);
            if self.format(var_expr(v, &ty)).is_none() {
                self.vars.entries.truncate(saved.0);
                self.keys.truncate(saved.1);
                self.helpers.truncate(saved.2);
            }
        }
    }
}

#[cfg(test)]
mod error_display_scope_tests {
    use super::*;
    #[test]
    fn error_helpers_cannot_leak_across_compilations() {
        let ty = Ty::Tuple(vec![Ty::String].into());
        let value = var_expr(VarId(0), &ty);
        assert!(erased_error(&value).is_none());
        {
            let _scope = ErrorDisplayScope::enter();
            ERROR_DISPLAYS.with(|s| *s.borrow_mut() = Some(vec![(ty, sym("formatter"))]));
            assert!(erased_error(&value).is_some());
            { let _nested = ErrorDisplayScope::enter(); assert!(erased_error(&value).is_none()); }
            assert!(erased_error(&value).is_some());
        }
        assert!(erased_error(&value).is_none());
    }
}
