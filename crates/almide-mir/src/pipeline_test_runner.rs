// The INCUMBENT brick's own refusal for test mode. The `__test_runner`
// synthesis itself is leg-independent and lives in
// `almide_driver::test_runner` — this file holds only what is true of THIS
// renderer, so a limit of one leg can never be read as a limit of the other
// (#2121: the test runner used to render through this leg alone, and reported
// its walls as "no verified wasm rendering" for programs the default leg
// emitted and ran correctly).

/// The incumbent's test-mode wall, run before the shared synthesis for a
/// program that declares tests at all — exactly where it ran when the two were
/// one function.
///
/// A referenced IMPURE-call-initialized module top-let walls the file: the
/// const-bridge drops a call init (mod.rs's expr_has_call), a PURE one is
/// substituted into the reader bodies later (the ceangal/#785 substitution +
/// the record-field hoist), but an IMPURE one has no faithful route — and an
/// unbound reference TRAPS at runtime (index OOB) instead of walling.
/// Referenced = a frontend-synthesized cross-module ref entry names it.
fn incumbent_test_mode_wall(ir: &almide_ir::IrProgram) -> Result<(), LowerError> {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct C {
        has_call: bool,
        impure: bool,
    }
    impl IrVisitor for C {
        fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
            match &e.kind {
                almide_ir::IrExprKind::RuntimeCall { .. } => {
                    self.has_call = true;
                    self.impure = true;
                }
                almide_ir::IrExprKind::Call { target, .. } => {
                    self.has_call = true;
                    match target {
                        almide_ir::CallTarget::Module { module, func, .. } => {
                            if !crate::purity::is_pure(module.as_str(), func.as_str()) {
                                self.impure = true;
                            }
                        }
                        almide_ir::CallTarget::Named { .. } => {}
                        _ => self.impure = true,
                    }
                }
                _ => {}
            }
            walk_expr(self, e);
        }
    }
    let effectish: std::collections::HashSet<&str> = ir
        .functions
        .iter()
        .chain(ir.modules.iter().flat_map(|m| m.functions.iter()))
        .filter(|f| f.is_effect)
        .map(|f| f.name.as_str())
        .collect();
    let impure_call_inits: std::collections::HashSet<(String, String)> = ir
        .modules
        .iter()
        .flat_map(|m| {
            let effectish = &effectish;
            m.top_lets.iter().filter_map(move |tl| {
                let mut c = C { has_call: false, impure: false };
                c.visit_expr(&tl.value);
                // A Named callee that is an EFFECT fn is impure too.
                let named_effect = {
                    use almide_ir::visit::{walk_expr, IrVisitor};
                    struct N<'a> {
                        hit: bool,
                        effectish: &'a std::collections::HashSet<&'a str>,
                    }
                    impl IrVisitor for N<'_> {
                        fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
                            if let almide_ir::IrExprKind::Call {
                                target: almide_ir::CallTarget::Named { name },
                                ..
                            } = &e.kind
                            {
                                if self.effectish.contains(name.as_str()) {
                                    self.hit = true;
                                }
                            }
                            walk_expr(self, e);
                        }
                    }
                    let mut n = N { hit: false, effectish };
                    n.visit_expr(&tl.value);
                    n.hit
                };
                // PURE call inits PASS: the bind-form substitution places
                // the init at the fn top, and repair_record_literal_field_tys
                // heals the Unknown declared-field type the linked literal
                // carried (#785) — the full single-file-proven form. IMPURE
                // inits have no faithful route and stay walled.
                if !(c.has_call && (c.impure || named_effect)) {
                    return None;
                }
                // Keyed by the `module_origin` SPELLING, which is what the
                // reference entries below carry — the dotted module name never
                // matched, so this wall silently under-fired (#904).
                m.var_table.entries.get(tl.var.0 as usize).map(|e| {
                    (crate::lower::module_origin_key(m), e.name.as_str().to_uppercase())
                })
            })
        })
        .collect();
    if !impure_call_inits.is_empty()
        && ir.var_table.entries.iter().any(|e| {
            e.module_origin.as_ref().is_some_and(|mo| {
                impure_call_inits.contains(&(mo.clone(), e.name.as_str().to_uppercase()))
            })
        })
    {
        return Err(LowerError::Unsupported(
            "test mode: a referenced impure-call-initialized module top-let \
             needs the slot-routed bridge, not in this brick"
                .into(),
        ));
    }
    Ok(())
}
