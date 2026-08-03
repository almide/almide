// Phase 5 of the wasm pipeline: mutable module-level `var` storage slots —
// assignment (declaration order = slot order) and the repair + substitution
// walk that routes reads/assigns through the slots. Split out of pipeline_b.rs
// (max-lines, #852); both fns moved verbatim.

/// Phase 5: MUTABLE module-level `var`s (program + modules) — assign each a
/// linear-memory storage slot (declaration order = VarId order, the same ordering
/// `__global_init` uses) and publish the VarId → (slot, Ty) map — reads/assigns then
/// route through the slot (`Load`/`$__mg_get`/`$__mg_take`+`Store`). A VarId collision
/// across regions or an over-cap count WALLS the program (honest, never a mis-routed
/// slot). Returns the sorted mutable top-lets (their declaration order IS the slot
/// order, needed again by `__mg_init` synthesis below).
fn assign_mutable_global_slots(
    ir: &almide_ir::IrProgram,
    mutable_toplet_aliases: &std::collections::HashMap<almide_ir::VarId, almide_ir::VarId>,
) -> Result<Vec<almide_ir::IrTopLet>, LowerError> {
    let mut mutable_tls: Vec<_> = ir
        .top_lets
        .iter()
        .chain(ir.modules.iter().flat_map(|m| m.top_lets.iter()))
        .filter(|tl| tl.mutable)
        .cloned()
        .collect();
    mutable_tls.sort_by_key(|tl| tl.var.0);
    if mutable_tls.len() > 64 {
        return Err(LowerError::Unsupported(format!(
            "{} mutable module-level vars exceed the 64-slot global region",
            mutable_tls.len()
        )));
    }
    {
        let mut seen = std::collections::HashSet::new();
        for tl in &mutable_tls {
            if !seen.insert(tl.var.0) {
                return Err(LowerError::Unsupported(format!(
                    "mutable module-level var id collision across regions ({:?})",
                    tl.var
                )));
            }
        }
    }
    let mut mutable_global_map: std::collections::HashMap<u32, (u32, almide_lang::types::Ty)> = mutable_tls
        .iter()
        .enumerate()
        .map(|(i, tl)| (tl.var.0, (i as u32, tl.ty.clone())))
        .collect();
    // #782: alias each main-side synthesized ref onto its module var's slot —
    // the retired v0 fallback used to absorb these as walls; now `m.count`
    // reads and assigns route through the SAME storage the owning module uses.
    for (main_id, mod_id) in mutable_toplet_aliases {
        if let Some(entry) = mutable_global_map.get(&mod_id.0).cloned() {
            mutable_global_map.insert(main_id.0, entry);
        }
    }
    crate::lower::set_mutable_global_vars(mutable_global_map);
    Ok(mutable_tls)
}

/// Phase 6: repair CROSS-MODULE global refs whose expr type the frontend never
/// inferred (`v.white` — the ceangal theme class) from the bridged globals maps
/// BEFORE lowering (or the AllTypesConcrete precondition walls the whole fn), then
/// SUBSTITUTE every BRIDGED cross-module ref whose module-side init is a pure call
/// or heap ctor (`v.black` → view's `let black = rgb(0,0,0)`) into the referencing fn
/// bodies — a lowering-time CallFn there would break the classify `mir == ir` count,
/// so the call must exist in the IR itself instead (the `inline_pure_call_globals`
/// discipline, extended across the cross-module bridge). Purity gate: every call
/// inside the init is a pure stdlib call or a RESOLVED user fn that is itself
/// call-transitively clean (effect fns were mangled through the resolver and appear
/// in no pure set). MAIN-region readers take the BIND form (a fn-top `let __g_init =
/// …` plus a `Var` reference — a call spliced into an arbitrary position, e.g. a
/// record field, would render as an invalid dst-less bare call); module siblings keep
/// the raw substitution (their separate VarId region cannot carry a main-region bind
/// id).
/// Collect the module-origin globals whose init must SUBSTITUTE into readers:
/// a call-bearing init, or (#782) a HEAP toplet whose init is a CTOR form
/// (tuple/record/variant/some/ok — `let PAIR = ("a", 1)`, `let MOOD = Happy`)
/// — value_or_global's CONST path only materializes flat literals. Only PURE
/// inits qualify (an impure init cannot be re-evaluated at each reader).
fn collect_pure_global_subs(
    ir: &almide_ir::IrProgram,
    layouts: &PipelineLayouts,
    all_fns: &[almide_ir::IrFunction],
) -> Vec<(almide_ir::VarId, almide_ir::IrExpr)> {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct HasImpure<'a> {
        impure: bool,
        effectish: &'a std::collections::HashSet<String>,
    }
    impl IrVisitor for HasImpure<'_> {
        fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
            match &e.kind {
                almide_ir::IrExprKind::RuntimeCall { .. } => self.impure = true,
                almide_ir::IrExprKind::Call { target, .. } => match target {
                    almide_ir::CallTarget::Module { module, func, .. } => {
                        if !crate::purity::is_pure(module.as_str(), func.as_str()) {
                            self.impure = true;
                        }
                    }
                    almide_ir::CallTarget::Named { name } => {
                        if self.effectish.contains(name.as_str()) {
                            self.impure = true;
                        }
                    }
                    _ => self.impure = true,
                },
                _ => {}
            }
            walk_expr(self, e);
        }
    }
    let effectish: std::collections::HashSet<String> = all_fns
        .iter()
        .filter(|f| f.is_effect)
        .map(|f| f.name.as_str().to_string())
        .collect();
    let mut subs: Vec<(almide_ir::VarId, almide_ir::IrExpr)> = Vec::new();
    for (i, info) in ir.var_table.entries.iter().enumerate() {
        if info.module_origin.is_none() {
            continue;
        }
        let id = almide_ir::VarId(i as u32);
        let Some(init) = layouts.main_global_inits.get(&id) else { continue };
        // #782: with the v0 fallback retired, a HEAP toplet whose init is a
        // CTOR form (tuple/record/variant/some/ok — `let PAIR = ("a", 1)`,
        // `let MOOD = Happy`) must ALSO substitute: value_or_global's CONST
        // path only materializes flat literals (LitStr / all-literal List),
        // and the old "computed init" wall used to fall back to v0. The
        // bind-form substitution evaluates the pure ctor at fn-top — same
        // discipline as the pure-call inits below.
        let heap_ctor_init = crate::lower::is_heap_ty(&init.ty)
            && !matches!(
                &init.kind,
                almide_ir::IrExprKind::LitStr { .. } | almide_ir::IrExprKind::List { .. }
            );
        if !crate::lower::expr_contains_call(init) && !heap_ctor_init {
            continue;
        }
        let mut h = HasImpure { impure: false, effectish: &effectish };
        h.visit_expr(init);
        if !h.impure {
            subs.push((id, init.clone()));
        }
    }
    subs
}

/// Splice one pure global init into every MAIN-region reader as the BIND form:
/// `let __g_init = <init>; …Var(__g_init)…` — a call spliced into an arbitrary
/// position (a record FIELD — the #785 shape) rendered as a dst-less bare call
/// (invalid wasm), while a call in BIND position plus a Var reference is the
/// proven single-file form. The bind goes at the fn-body top (the init is
/// pure, so hoisting its evaluation is unobservable); fns that never reference
/// the global are untouched.
fn substitute_global_bind_form(
    ir: &mut almide_ir::IrProgram,
    inlined_fns: &mut [almide_ir::IrFunction],
    id: almide_ir::VarId,
    init: &almide_ir::IrExpr,
) {
    for f in inlined_fns.iter_mut() {
        fn references(e: &almide_ir::IrExpr, id: almide_ir::VarId) -> bool {
            use almide_ir::visit::{walk_expr, IrVisitor};
            struct V(almide_ir::VarId, bool);
            impl IrVisitor for V {
                fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
                    if matches!(&e.kind, almide_ir::IrExprKind::Var { id } if *id == self.0) {
                        self.1 = true;
                    }
                    walk_expr(self, e);
                }
            }
            let mut v = V(id, false);
            v.visit_expr(e);
            v.1
        }
        if !references(&f.body, id) {
            continue;
        }
        let nv = ir.var_table.alloc(
            almide_lang::intern::sym("__g_init"),
            init.ty.clone(),
            almide_ir::Mutability::Let,
            None,
        );
        let nv_ref = almide_ir::IrExpr {
            kind: almide_ir::IrExprKind::Var { id: nv },
            ty: init.ty.clone(),
            span: None,
            def_id: None,
        };
        f.body = almide_ir::substitute::substitute_var_in_expr(&f.body, id, &nv_ref);
        let bind_stmt = almide_ir::IrStmt {
            kind: almide_ir::IrStmtKind::Bind {
                var: nv,
                mutability: almide_ir::Mutability::Let,
                ty: init.ty.clone(),
                value: init.clone(),
            },
            span: None,
        };
        if let almide_ir::IrExprKind::Block { stmts, .. } = &mut f.body.kind {
            stmts.insert(0, bind_stmt);
        } else {
            // An EXPRESSION-form body (`effect fn main() -> Unit =
            // println(m.CFG.name)`) has no statement list — wrap it in a
            // Block so the fn-top bind exists. Without this the
            // substitution left `Var(__g_init)` UNBOUND (#782, the
            // record/variant toplet matrix cells).
            let old_ty = f.body.ty.clone();
            let old_span = f.body.span.clone();
            let old = std::mem::replace(
                &mut f.body,
                almide_ir::IrExpr {
                    kind: almide_ir::IrExprKind::Unit,
                    ty: almide_lang::types::Ty::Unit,
                    span: None,
                    def_id: None,
                },
            );
            f.body = almide_ir::IrExpr {
                kind: almide_ir::IrExprKind::Block {
                    stmts: vec![bind_stmt],
                    expr: Some(Box::new(old)),
                },
                ty: old_ty,
                span: old_span,
                def_id: None,
            };
        }
    }
}

fn repair_and_substitute_globals(
    ir: &mut almide_ir::IrProgram,
    inlined_fns: &mut [almide_ir::IrFunction],
    module_fn_sibs: &mut [almide_ir::IrFunction],
    layouts: &PipelineLayouts,
    all_fns: &[almide_ir::IrFunction],
) {
    for f in inlined_fns.iter_mut() {
        crate::lower::repair_unknown_global_ref_tys(f, &layouts.main_globals);
        crate::lower::repair_member_field_tys(f, &layouts.record_layouts);
    }
    for f in module_fn_sibs.iter_mut() {
        crate::lower::repair_unknown_global_ref_tys(f, &layouts.globals);
        crate::lower::repair_member_field_tys(f, &layouts.record_layouts);
    }

    let subs = collect_pure_global_subs(ir, layouts, all_fns);
    for (id, init) in &subs {
        substitute_global_bind_form(ir, inlined_fns, *id, init);
        // Module siblings keep the raw substitution (their separate VarId
        // numbering region cannot carry a main-region bind id) — the ceangal
        // in-module reader class this path has always served.
        for f in module_fn_sibs.iter_mut() {
            f.body = almide_ir::substitute::substitute_var_in_expr(&f.body, *id, init);
        }
    }
    if !subs.is_empty() {
        for f in inlined_fns.iter_mut() {
            crate::lower::repair_record_literal_field_tys(f);
        }
    }
}
