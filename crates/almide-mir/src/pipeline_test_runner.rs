// The v0 `__test_runner` protocol synthesis: promote a no-`main` test file's
// `test` fns to ordinary effect fns and build the runner `main` that calls each
// in declaration order. Split out of pipeline.rs (max-lines, #852); moved verbatim.

/// Render a `.almd` **source** program to a COMPLETE wasm module (WAT text) via the v1 MIR renderer.
///
/// `self_modules` are the caller-resolved `import self.<submodule>` siblings (empty ⇒ single file).
/// Promote a NO-`main` test file's `test` fns to ordinary effect fns and synthesize the
/// runner `main` (v0 `__test_runner` protocol). See [`try_render_wasm_source_tests`].
fn synthesize_test_runner_main(ir: &mut almide_ir::IrProgram) -> Result<(), LowerError> {
    use almide_ir::{CallTarget, IrExpr, IrExprKind, IrStmt, IrStmtKind};
    use almide_lang::intern::sym;
    use almide_lang::types::Ty;
    let has_tests = ir.functions.iter().any(|f| f.is_test);
    if let Some(main_idx) =
        ir.functions.iter().position(|f| !f.is_test && f.name.as_str() == "main")
    {
        if !has_tests {
            // main-mode: both legs run main only (v0's `__main_runner` protocol).
            return Ok(());
        }
        // main + test blocks. NATIVE test mode compiles `main` but never calls it —
        // cargo's harness runs the `#[test]` fns alone. Mirror that: drop the user
        // `main` so the synthesized runner is the entry and the TESTS run. The old
        // behaviour kept `main` as the entry and left the tests unlowered, so the
        // harness skipped the file to native ("wasm test-mode runs main only") —
        // 17 of the 32 fallbacks in `almide test` were this one harness gap, not a
        // v1 subset wall (#813). These fixtures' `main` is separately exercised in
        // MAIN mode by the cross-target parity gate, so nothing loses coverage.
        ir.functions.remove(main_idx);
    }
    if !has_tests {
        return Err(LowerError::Unsupported(
            "test mode: no `main` and no test blocks — nothing to run".into(),
        ));
    }
    // v0's `__test_runner` re-initializes module globals before EVERY test (native
    // thread-isolation parity). The v1 `_start` runs `__global_init`/`__mg_init` ONCE —
    // so the runner main re-ASSIGNS every MUTABLE main-region top-let to its
    // initializer before each test (the ordinary `lower_mutable_global_assign` path:
    // take + drop-old + store — no leak, no new runtime). An IMMUTABLE top-let cannot
    // change between tests and needs no re-init. MODULE top-lets stay walled: their
    // VarIds live in a different numbering region than the main-region runner, so a
    // synthesized Assign could collide with an unrelated main-side id (the per-region
    // globals discipline) — that bridge is the remaining piece of this brick.
    if ir.modules.iter().any(|m| m.top_lets.iter().any(|tl| tl.mutable)) {
        // MUTABLE module top-lets stay walled in test mode: the per-test re-init
        // Assign would have to cross the VarId numbering-region bridge (a
        // synthesized main-region Assign could collide with an unrelated module
        // id — the per-region globals discipline). IMMUTABLE literal-init
        // top-lets cannot change between tests and need no re-init — they lower
        // through the const-bridge.
        return Err(LowerError::Unsupported(
            "test mode: MUTABLE module top-lets need the per-test region bridge, \
             not in this brick"
                .into(),
        ));
    }
    // A referenced IMPURE-call-initialized module top-let walls the file: the
    // const-bridge drops a call init (mod.rs's expr_has_call), a PURE one is
    // substituted into the reader bodies later (the ceangal/#785 substitution +
    // the record-field hoist), but an IMPURE one has no faithful route — and an
    // unbound reference TRAPS at runtime (index OOB) instead of walling.
    // Referenced = a frontend-synthesized cross-module ref entry names it.
    {
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
    }
    let reinit_stmts: Vec<IrStmt> = ir
        .top_lets
        .iter()
        .filter(|tl| tl.mutable)
        .map(|tl| IrStmt {
            kind: IrStmtKind::Assign { var: tl.var, value: tl.value.clone() },
            span: None,
        })
        .collect();
    let unit_expr =
        || IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None };
    let println_stmt = |text: String| IrStmt {
        kind: IrStmtKind::Expr {
            expr: IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Named { name: sym("println") },
                    args: vec![IrExpr {
                        kind: IrExprKind::LitStr { value: text },
                        ty: Ty::String,
                        span: None,
                        def_id: None,
                    }],
                    type_args: Vec::new(),
                },
                ty: Ty::Unit,
                span: None,
                def_id: None,
            },
        },
        span: None,
    };
    let mut stmts: Vec<IrStmt> = Vec::new();
    let mut idx = 0usize;
    for f in ir.functions.iter_mut() {
        if !f.is_test {
            continue;
        }
        let display = f
            .name
            .as_str()
            .strip_prefix(almide_ir::TEST_NAME_PREFIX)
            .unwrap_or(f.name.as_str())
            .to_string();
        // Raw test names carry spaces/parens/unicode no WAT identifier admits — rename
        // to a mechanical id and drop `is_test` so the render loop lowers it like any
        // other effect fn (nothing else references a test fn by name).
        let mangled = format!("__almd_test_{idx}");
        idx += 1;
        f.name = sym(&mangled);
        f.is_test = false;
        // v0 isolation parity: reset every mutable top-let to its initializer
        // before the test body runs (see the reinit_stmts derivation above).
        stmts.extend(reinit_stmts.iter().cloned());
        stmts.push(println_stmt(format!("test: {display} ... ")));
        // The stmt-position effect call, in the SAME shape the frontend gives user
        // code: `Try { call }` with the LIFTED `Result[Unit, String]` call type — the
        // never-err strips / can-err propagation then classify it exactly like any
        // other caller (the C-135 def/callsite agreement).
        let call = IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Named { name: sym(&mangled) },
                args: Vec::new(),
                type_args: Vec::new(),
            },
            ty: Ty::result(Ty::Unit, Ty::String),
            span: None,
            def_id: None,
        };
        stmts.push(IrStmt {
            kind: IrStmtKind::Expr {
                expr: IrExpr {
                    kind: IrExprKind::Try { expr: Box::new(call) },
                    ty: Ty::Unit,
                    span: None,
                    def_id: None,
                },
            },
            span: None,
        });
        stmts.push(println_stmt("ok".to_string()));
    }
    let body = IrExpr {
        kind: IrExprKind::Block { stmts, expr: Some(Box::new(unit_expr())) },
        ty: Ty::Unit,
        span: None,
        def_id: None,
    };
    ir.functions.push(almide_ir::IrFunction {
        name: sym("main"),
        params: vec![],
        ret_ty: Ty::Unit,
        body,
        is_effect: true,
        is_test: false,
        generics: None,
        extern_attrs: vec![],
        export_attrs: vec![],
        attrs: vec![],
        visibility: almide_ir::IrVisibility::Public,
        doc: None,
        blank_lines_before: 0,
        def_id: None,
        mutated_params: vec![],
        module_origin: None,
    });
    Ok(())
}
