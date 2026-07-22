
/// Phase 8: synthesize `__mg_init` (assigns each mutable top-let its declared
/// initializer, declaration order, through the SAME slot-routed Assign path user code
/// uses — `_start` calls it before `__global_init`/`main`; a non-lowerable initializer
/// WALLS the whole program, since shipping zeroed globals would be a silent
/// miscompile), then auto-link the self-hosted stdlib runtime (int.to_string,
/// string.concat, `print_str`, …): when a registry entry is called but not defined,
/// its impl fn is lowered and renamed to the call name — iterated to a FIXPOINT since
/// a linked impl may itself call ANOTHER registry entry. A self-hosted fn calling
/// another registered impl by its IMPL name (pre-rename) is rewritten to the call name
/// afterward, and `print_str` is force-linked last (`println` → `PrintStr` → `(call
/// $print_str)`).
#[allow(clippy::too_many_arguments)]
fn synthesize_and_link_runtime_fns(
    functions: &mut Vec<crate::MirFunction>,
    mutable_tls: &[almide_ir::IrTopLet],
    layouts: &PipelineLayouts,
    verbose: bool,
) -> Result<(), LowerError> {
    if !mutable_tls.is_empty() {
        let stmts: Vec<almide_ir::IrStmt> = mutable_tls
            .iter()
            .map(|tl| almide_ir::IrStmt {
                kind: almide_ir::IrStmtKind::Assign { var: tl.var, value: tl.value.clone() },
                span: None,
            })
            .collect();
        let body = almide_ir::IrExpr {
            kind: almide_ir::IrExprKind::Block { stmts, expr: None },
            ty: almide_lang::types::Ty::Unit,
            span: Default::default(),
            def_id: None,
        };
        let init_fn = almide_ir::IrFunction {
            name: almide_lang::intern::sym("__mg_init"),
            params: vec![],
            ret_ty: almide_lang::types::Ty::Unit,
            body,
            is_effect: false,
            is_async: false,
            is_test: false,
            generics: None,
            extern_attrs: vec![],
            export_attrs: vec![],
            attrs: vec![],
            visibility: almide_ir::IrVisibility::Public,
            doc: None,
            blank_lines_before: 0,
            def_id: None,
            module_origin: None,
            mutated_params: vec![],
        };
        match crate::lower::lower_function_all_with_globals(
            &init_fn,
            &layouts.main_globals,
            &layouts.main_global_inits,
            &layouts.record_layouts,
            &layouts.variant_layouts,
        ) {
            Ok(mirs) => functions.extend(mirs),
            Err(e) => {
                return Err(LowerError::Unsupported(format!(
                    "mutable module-level var initializer outside the executable subset: {e:?}"
                )))
            }
        }
    }

    // Auto-link the self-hosted stdlib runtime (int.to_string, string.concat, …) when an entry is
    // called but not defined, renaming its impl fn to the call name. A linked impl may call ANOTHER
    // registry entry, so iterate to a FIXPOINT.
    loop {
        let before = functions.len();
        for (rt_source, entries) in crate::render_wasm::self_host_runtime() {
            let mut any_called = entries.iter().any(|(_, call)| {
                functions.iter().any(|f| {
                    f.ops.iter().any(|op| matches!(op, crate::Op::CallFn { name, .. } if name == call))
                })
            });
            // A Value drop renders `(call $__drop_value …)` — a value_core helper that is NOT a
            // registered call_name, so force value_core when ANY Value-drop op is present.
            if entries.iter().any(|(_, c)| *c == "value.null") {
                any_called = any_called
                    || functions.iter().any(|f| {
                        f.ops.iter().any(|op| {
                            matches!(
                                op,
                                crate::Op::DropValue { .. }
                                    | crate::Op::DropListValue { .. }
                                    | crate::Op::DropListStrValue { .. }
                                    | crate::Op::DropListStrStr { .. }
                                    | crate::Op::DropResultValue { .. }
                                    | crate::Op::DropResultListValue { .. }
                            )
                        })
                    });
            }
            // A `Map[String, <flat heap>]` drop renders `(call $__drop_map_hval …)` — a
            // map_hval helper that is NOT a registered call name, so force map_hval when
            // the DropVariant is present (the C-039 typechange twins build hval-flavor
            // maps without calling any registered map_hval fn — the value_core pattern).
            if entries.iter().any(|(impl_fn, _)| *impl_fn == "map_new_hval") {
                any_called = any_called
                    || functions.iter().any(|f| {
                        f.ops.iter().any(|op| matches!(op,
                            crate::Op::DropVariant { ty, .. } if ty == "map_hval"))
                    });
            }
            let any_defined =
                entries.iter().any(|(_, call)| functions.iter().any(|f| &f.name == call));
            if any_called && !any_defined {
                let rt = source_to_ir(rt_source)?;
                for f in &rt.functions {
                    let lowered = crate::lower::lower_function(f, &layouts.globals);
                    if let Err(e) = &lowered {
                        if verbose
                            && (entries.iter().any(|(impl_fn, _)| f.name.as_str() == *impl_fn)
                                || f.name.as_str().starts_with("__"))
                        {
                            eprintln!("[self-host] {} failed to lower: {:?}", f.name.as_str(), e);
                        }
                    }
                    if let Ok(mut mir) = lowered {
                        if let Some((_, call)) =
                            entries.iter().find(|(impl_fn, _)| &mir.name == impl_fn)
                        {
                            mir.name = call.to_string();
                        }
                        functions.push(mir);
                    }
                }
            }
        }
        // Dedup by name (identical source ⇒ no-op merge).
        let mut seen = std::collections::HashSet::new();
        functions.retain(|f| seen.insert(f.name.clone()));
        if functions.len() == before {
            break;
        }
    }

    // A self-hosted runtime fn may call ANOTHER registered impl by its IMPL name, but the auto-link
    // RENAMED that def to its call_name. Rewrite those call sites to the call_name.
    let impl_to_call: std::collections::HashMap<&str, &str> = crate::render_wasm::self_host_runtime()
        .iter()
        .flat_map(|(_, es)| es.iter().map(|(i, c)| (*i, *c)))
        .collect();
    for f in functions.iter_mut() {
        for op in &mut f.ops {
            if let crate::Op::CallFn { name, .. } = op {
                if let Some(&c) = impl_to_call.get(name.as_str()) {
                    *name = c.to_string();
                }
            }
        }
    }

    // Auto-link the self-hosted runtime `print_str` (`println` → `PrintStr` → `(call $print_str)`).
    if !functions.iter().any(|f| f.name == "print_str") {
        let rt = source_to_ir(include_str!("../../../stdlib/print_str.almd"))?;
        for f in &rt.functions {
            if let Ok(mir) = crate::lower::lower_function(f, &layouts.globals) {
                functions.push(mir);
            }
        }
    }
    Ok(())
}

fn try_render_wasm_source_impl_rest(
    ir: &mut almide_ir::IrProgram,
    verbose: bool,
) -> Result<String, LowerError> {
    let layouts = collect_pipeline_layouts(ir);

    let CrossModuleFns { mut module_fn_sibs, mut inlined_fns, all_fns } =
        inline_and_classify_cross_module_fns(ir, &layouts.main_globals, &layouts.record_layouts);

    bridge_cross_module_derived_methods(ir, &mut inlined_fns, &mut module_fn_sibs);

    let mutable_tls = assign_mutable_global_slots(ir, &layouts.mutable_toplet_aliases)?;
    let mutable_global_count = mutable_tls.len() as u32;

    repair_and_substitute_globals(ir, &mut inlined_fns, &mut module_fn_sibs, &layouts, &all_fns);

    let mut functions = lower_main_and_sibling_fns(
        &inlined_fns,
        &module_fn_sibs,
        &layouts,
        ir.functions.len(),
        verbose,
    );

    synthesize_and_link_runtime_fns(&mut functions, &mutable_tls, &layouts, verbose)?;

    // EAGER GLOBAL-INIT semantics (C-007): v0 evaluates every ABORTABLE top-let initializer at
    // startup. Synthesize `__global_init` binding each CALL-FREE SCALAR initializer and have
    // `_start` call it before `$main`. Call-bearing/heap inits keep per-use/wall handling.
    {
        fn has_call(e: &almide_ir::IrExpr) -> bool {
            use almide_ir::visit::{walk_expr, IrVisitor};
            struct C(bool);
            impl IrVisitor for C {
                fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
                    if matches!(
                        e.kind,
                        almide_ir::IrExprKind::Call { .. } | almide_ir::IrExprKind::RuntimeCall { .. }
                    ) {
                        self.0 = true;
                    }
                    walk_expr(self, e);
                }
            }
            let mut c = C(false);
            c.visit_expr(e);
            c.0
        }
        let mut max_var = 0u32;
        for (v, _) in &layouts.globals {
            max_var = max_var.max(v.0);
        }
        {
            use almide_ir::visit::{walk_expr, IrVisitor};
            struct M(u32);
            impl IrVisitor for M {
                fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
                    if let almide_ir::IrExprKind::Var { id } = &e.kind {
                        self.0 = self.0.max(id.0);
                    }
                    walk_expr(self, e);
                }
            }
            let mut m = M(max_var);
            for f in &ir.functions {
                m.visit_expr(&f.body);
            }
            max_var = m.0;
        }
        let mut stmts: Vec<almide_ir::IrStmt> = Vec::new();
        let mut ordered: Vec<_> = ir
            .top_lets
            .iter()
            .chain(ir.modules.iter().flat_map(|m| m.top_lets.iter()))
            .collect();
        ordered.sort_by_key(|tl| tl.var.0);
        // A later initializer may READ an earlier global — INLINE each processed init into its
        // dependents (declaration order; all call-free ⇒ pure, finite substitution).
        let mut subst: std::collections::HashMap<almide_ir::VarId, almide_ir::IrExpr> =
            std::collections::HashMap::new();
        for tl in ordered {
            let scalar = !crate::lower::is_heap_ty(&tl.ty);
            if scalar && !has_call(&tl.value) {
                let mut value = tl.value.clone();
                for (gv, ge) in &subst {
                    value = almide_ir::substitute::substitute_var_in_expr(&value, *gv, ge);
                }
                subst.insert(tl.var, value.clone());
                max_var += 1;
                stmts.push(almide_ir::IrStmt {
                    kind: almide_ir::IrStmtKind::Bind {
                        var: almide_ir::VarId(max_var),
                        mutability: almide_ir::Mutability::Let,
                        ty: tl.ty.clone(),
                        value,
                    },
                    span: None,
                });
            }
        }
        if !stmts.is_empty() {
            let body = almide_ir::IrExpr {
                kind: almide_ir::IrExprKind::Block { stmts, expr: None },
                ty: almide_lang::types::Ty::Unit,
                span: Default::default(),
                def_id: None,
            };
            let init_fn = almide_ir::IrFunction {
                name: almide_lang::intern::sym("__global_init"),
                params: vec![],
                ret_ty: almide_lang::types::Ty::Unit,
                body,
                is_effect: false,
                is_async: false,
                is_test: false,
                generics: None,
                extern_attrs: vec![],
                export_attrs: vec![],
                attrs: vec![],
                visibility: almide_ir::IrVisibility::Public,
                doc: None,
                blank_lines_before: 0,
                def_id: None,
                module_origin: None,
                mutated_params: vec![],
            };
            if let Ok(mir) = crate::lower::lower_function(&init_fn, &layouts.globals) {
                functions.push(mir);
            }
        }
    }

    // If `main` itself was WALLED, there is no `$main` — yet the renderer emits
    // `(func (export "_start") (call $main))`. Wall the WHOLE program cleanly instead of a
    // main-less (invalid) module.
    if !functions.iter().any(|f| f.name == "main") {
        return Err(LowerError::Unsupported(
            "main is outside the MIR-lowering subset".into(),
        ));
    }

    // `pub fn` EXPORT roots (#457): a Public non-test MAIN-program fn must be a named wasm
    // export (host-invocable, the v0 emitter's export contract). One that LOWERED gets an
    // `(export …)` directive; one that WALLED cannot be exported — decline the WHOLE module
    // so the `--verified` pipeline falls back to v0 (which exports it) rather than shipping
    // an artifact silently missing a public entry point.
    let mut exports: Vec<(String, String, Vec<bool>, Option<bool>)> = Vec::new();
    let is_float_ty = |t: &almide_lang::types::Ty| {
        matches!(
            t,
            almide_lang::types::Ty::Float
                | almide_lang::types::Ty::Float32
                | almide_lang::types::Ty::Float64
        )
    };
    for func in &ir.functions {
        if !func.is_test
            && func.name.as_str() != "main"
            && !func.generics.as_ref().map_or(false, |g| !g.is_empty())
            && matches!(func.visibility, almide_ir::IrVisibility::Public)
        {
            let n = func.name.as_str();
            if functions.iter().any(|f| f.name == n) {
                // `@export(wasm, "sym")` overrides the export name (v0's criterion,
                // mod_p3.rs). v1's internal value model carries a Float as raw i64 BITS;
                // the renderer emits a reinterpret wrapper for Float-bearing signatures
                // so the public ABI presents real f64s (v0 parity).
                let export_name = func
                    .export_attrs
                    .iter()
                    .find(|a| a.target.as_str() == "wasm")
                    .map(|a| a.symbol.to_string())
                    .unwrap_or_else(|| n.to_string());
                let param_floats: Vec<bool> =
                    func.params.iter().map(|p| is_float_ty(&p.ty)).collect();
                let ret_float = match &func.ret_ty {
                    almide_lang::types::Ty::Unit => None,
                    t => Some(is_float_ty(t)),
                };
                exports.push((export_name, n.to_string(), param_floats, ret_float));
            } else {
                return Err(LowerError::Unsupported(format!(
                    "exported `pub fn {n}` is outside the MIR-lowering subset (the wasm module \
                     must carry its export)"
                )));
            }
        }
    }

    // #824: drop MakeUnique guards that are provably dead (the value they'd guard
    // is never aliased anywhere in its own function) — see alias_safety.rs's doc
    // comment for the soundness argument. Target-agnostic (applies before either
    // renderer runs), so the native leg gets the identical benefit below too.
    crate::alias_safety::elide_unaliased_make_unique(&mut functions);

    // Any UNLINKED stdlib/runtime call would render a dangling `(call $name)` (invalid wasm) — the
    // renderer rejects it cleanly. Returns the WAT on success.
    try_render_wasm_program(&MirProgram { functions, exports, mutable_global_count })
}

/// NATIVE leg of the trust spine (#764, rung 1): lower `.almd` source through the
/// SAME Perceus MIR the wasm leg uses and render it to native Rust — `Dup` as
/// `.clone()`, `Drop` erased to Rust's scope-end drop, the runtime boundary mapped
/// to a closed native shim floor. `verify_ownership` certifies the Perceus balance
/// on the same ops before the erasure. WALLS (`Err`) on anything outside the
/// rung-1 subset — the CLI falls back to v0, so a rendered program is never wrong.
/// Debug probe: dump the lowered MIR ops of every non-test fn (walls listed
/// per fn). Used by examples/probe_native.rs during rung development.
pub fn debug_dump_mir(source: &str) -> Result<String, LowerError> {
    crate::lower::STRICT_VALUES.store(true, std::sync::atomic::Ordering::Relaxed);
    let ir = source_to_ir_with(source, &[])?;
    let globals = std::collections::HashMap::new();
    let global_inits = std::collections::HashMap::new();
    // The REAL pipeline's layout registries — without them a record literal
    // lowers as an Opaque skeleton and a field read walls (the very first
    // rung-5 records probe misread that as a lowering gap).
    let record_layouts = crate::lower::build_record_layouts(&ir.type_decls);
    let variant_layouts = crate::lower::build_variant_layouts(&ir.type_decls);
    let mut out = String::new();
    for func in &ir.functions {
        if func.is_test {
            continue;
        }
        match crate::lower::lower_function_all_with_globals(func, &globals, &global_inits, &record_layouts, &variant_layouts) {
            Ok(all) => {
                for f in all {
                    out.push_str(&format!("== fn {} ==\n", f.name));
                    for op in &f.ops {
                        out.push_str(&format!("  {op:?}\n"));
                    }
                }
            }
            Err(e) => out.push_str(&format!("== fn {} LOWER-WALL: {e:?}\n", func.name)),
        }
    }
    Ok(out)
}

pub fn try_render_rust_source(source: &str) -> Result<String, LowerError> {
    crate::lower::STRICT_VALUES.store(true, std::sync::atomic::Ordering::Relaxed);
    let ir = source_to_ir_with(source, &[])?;
    // Rung-5 records slab: the layout registries the wasm leg threads — without
    // them a record literal lowers as an Opaque skeleton and every field read
    // strict-walls (the probe_native trap recorded in the trust-spine ledger).
    let record_layouts = crate::lower::build_record_layouts(&ir.type_decls);
    let variant_layouts = crate::lower::build_variant_layouts(&ir.type_decls);
    if !ir.modules.is_empty() {
        return Err(LowerError::Unsupported(
            "native: multi-module program — outside rung 1".into(),
        ));
    }
    if !ir.top_lets.is_empty() {
        return Err(LowerError::Unsupported(
            "native: top-level lets — outside rung 1".into(),
        ));
    }
    let globals = std::collections::HashMap::new();
    let mut functions = Vec::new();
    for func in &ir.functions {
        if func.is_test {
            continue;
        }
        // PRECISION WALL (rung 2): the native renderer types a heap `Repr::Ptr`
        // param/result as a STRING — only sound when the DECLARED type says so.
        // Any signature outside {Int, Bool, String, Unit-ret} walls here, where
        // the Almide-level `Ty` is still visible (`MirParam` carries only reprs).
        use almide_lang::types::Ty;
        use almide_lang::types::constructor::TypeConstructorId;
        let is_scalar_list = |t: &Ty| {
            matches!(t, Ty::Applied(TypeConstructorId::List, a)
                if a.len() == 1 && matches!(a[0], Ty::Int | Ty::Bool))
        };
        // Rung-5 records slab: an ALL-SCALAR record is layout-identical to a
        // scalar list (the DynList block), so its params/returns ride the same
        // `&[i64]`/`Vec<i64>` convention on native.
        let is_scalar_record = |t: &Ty| -> bool {
            let Ty::Named(n, args) = t else { return false };
            if !args.is_empty() { return false; }
            record_layouts
                .get(n.as_str())
                .is_some_and(|(_, ftys)| ftys.iter().all(|(_, ft)| !crate::lower::is_heap_ty(ft)))
        };
        // A FLAT variant (every ctor scalar-only) is likewise one slot block
        // (tag@0, payload@1+) — same `&[i64]` convention (rung-5 variants slab).
        let is_flat_variant = |t: &Ty| -> bool {
            let Ty::Named(n, args) = t else { return false };
            if !args.is_empty() { return false; }
            variant_layouts.by_type.get(n.as_str()).is_some_and(|layout| {
                layout.cases.iter().all(|c| c.fields.iter().all(|(_, ft)| !crate::lower::is_heap_ty(ft)))
            })
        };
        // Rung-5 closures slab: a SCALAR closure type (`(Int) -> Int` — every param
        // and the return scalar) travels as its env block (`Vec<i64>`: [fnidx,
        // drop-header, captures…]); invocation dispatches through the generated
        // `__almd_ci_*` tables. Heap-param/-return closures stay wasm-only.
        let is_scalar_fn = |t: &Ty| {
            matches!(t, Ty::Fn { params, ret }
                if params.iter().all(|p| matches!(p, Ty::Int | Ty::Bool))
                    && matches!(**ret, Ty::Int | Ty::Bool))
        };
        let sig_ok = |t: &Ty| {
            matches!(t, Ty::Int | Ty::Bool | Ty::Float | Ty::String)
                || is_scalar_list(t)
                || is_scalar_record(t)
                || is_flat_variant(t)
                || is_scalar_fn(t)
        };
        for p in &func.params {
            if !sig_ok(&p.ty) {
                return Err(LowerError::Unsupported(format!(
                    "native: fn `{}` param `{:?}` type — outside the native rung subset",
                    func.name, p.ty
                )));
            }
        }
        if !matches!(func.ret_ty, Ty::Unit) && !sig_ok(&func.ret_ty) {
            return Err(LowerError::Unsupported(format!(
                "native: fn `{}` return type {:?} — outside the native rung subset",
                func.name, func.ret_ty
            )));
        }
        // ALL-OR-NOTHING: any unlowerable fn walls the program (the native rungs
        // have no per-fn fallback — a partial native binary cannot call into v0).
        let all = crate::lower::lower_function_all_with_globals(
            func,
            &globals,
            &std::collections::HashMap::new(),
            &record_layouts,
            &variant_layouts,
        )
        .map_err(|e| {
            LowerError::Unsupported(format!("native: fn `{}`: {e:?}", func.name))
        })?;
        functions.extend(all);
    }
    if !functions.iter().any(|f| f.name == "main") {
        return Err(LowerError::Unsupported(
            "native: main is outside the MIR-lowering subset".into(),
        ));
    }
    // The SIG-KIND table (rung 4): the declared param/return kinds, computed here
    // where the Almide-level `Ty` is visible, so the native render can type a heap
    // `Repr::Ptr` as `&str` vs `&[i64]` per the declaration.
    let mut sigs: crate::render_native::NativeSigs = Default::default();
    {
        use almide_lang::types::constructor::TypeConstructorId;
        use almide_lang::types::Ty;
        use crate::render_native::NativeSigKind;
        let kind = |t: &Ty| -> Option<NativeSigKind> {
            match t {
                Ty::Int | Ty::Bool => Some(NativeSigKind::I64),
                Ty::Float => Some(NativeSigKind::F64),
                Ty::String => Some(NativeSigKind::Str),
                Ty::Applied(TypeConstructorId::List, a)
                    if a.len() == 1 && matches!(a[0], Ty::Int | Ty::Bool) =>
                {
                    Some(NativeSigKind::ListI64)
                }
                // An all-scalar record travels as its slot block (see sig_ok).
                Ty::Named(n, args)
                    if args.is_empty()
                        && record_layouts
                            .get(n.as_str())
                            .is_some_and(|(_, ftys)| ftys.iter().all(|(_, ft)| !crate::lower::is_heap_ty(ft))) =>
                {
                    Some(NativeSigKind::ListI64)
                }
                // A flat variant travels as its tag+payload slot block.
                Ty::Named(n, args)
                    if args.is_empty()
                        && variant_layouts.by_type.get(n.as_str()).is_some_and(|layout| {
                            layout.cases.iter().all(|c| c.fields.iter().all(|(_, ft)| !crate::lower::is_heap_ty(ft)))
                        }) =>
                {
                    Some(NativeSigKind::ListI64)
                }
                // A scalar closure travels as its env block (rung-5 closures slab).
                Ty::Fn { params, ret }
                    if params.iter().all(|p| matches!(p, Ty::Int | Ty::Bool))
                        && matches!(**ret, Ty::Int | Ty::Bool) =>
                {
                    Some(NativeSigKind::ListI64)
                }
                _ => None,
            }
        };
        for func in &ir.functions {
            if func.is_test {
                continue;
            }
            let params: Option<Vec<_>> = func.params.iter().map(|p| kind(&p.ty)).collect();
            let ret = if matches!(func.ret_ty, Ty::Unit) {
                Some(None)
            } else {
                kind(&func.ret_ty).map(Some)
            };
            if let (Some(ps), Some(r)) = (params, ret) {
                sigs.insert(func.name.as_str().to_string(), (ps, r));
            }
        }
        // LIFTED lambdas exist only as MirFunctions (no IR sig): param 0 is the env
        // block (`&[i64]`), the rest are the lambda's own params — SCALAR reprs only
        // in this slab (a heap-param lambda walls the program: all-or-nothing, and
        // its dispatch arm could not type). The return kind is body-derived by the
        // renderer, so it is not declared here.
        for f in &functions {
            if !f.name.starts_with("__lambda_") {
                continue;
            }
            let mut ps = vec![crate::render_native::NativeSigKind::ListI64];
            for p in &f.params[1..] {
                match p.repr {
                    crate::Repr::Scalar { .. } => {
                        ps.push(crate::render_native::NativeSigKind::I64)
                    }
                    _ => {
                        return Err(LowerError::Unsupported(format!(
                            "native: lambda `{}` heap param — outside the closures slab",
                            f.name
                        )))
                    }
                }
            }
            sigs.insert(f.name.clone(), (ps, Some(crate::render_native::NativeSigKind::I64)));
        }
    }
    // #824: see the wasm leg's call above — `Op::MakeUnique` already renders to
    // nothing on native (render_native.rs's `Op::Consume | Op::Borrow |
    // Op::MakeUnique => {}`), so this is a no-op cleanup here, kept only so both
    // legs run the identical target-agnostic MIR pass list.
    crate::alias_safety::elide_unaliased_make_unique(&mut functions);
    crate::render_native::try_render_native_program(
        &MirProgram {
            functions,
            exports: Vec::new(),
            // Rung 1 walls every top-let above, so there are no mutable-global slots.
            mutable_global_count: 0,
        },
        &sigs,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // `Parser::parse()` is a recovery parser: an unparseable top-level bare
    // statement (never valid Almide grammar — only fn/effect fn/type/let/
    // var/trait/impl/test are top-level declarations) is dropped via
    // `skip_to_next_decl()`, but `parse()` still returns `Ok` as long as some
    // decl survived. Before this fix, `source_to_ir_with` only checked the
    // `Result` and never inspected `Parser::errors`, so a source like this
    // would silently compile with the bare `println` call missing from the
    // output — a wall was expected instead.
    #[test]
    fn dropped_top_level_statement_walls_instead_of_silently_compiling() {
        let source = r#"
let x = 1
let y = 2
println("top-level-statement-should-not-be-silently-dropped")
fn main() -> Unit = {
  println("from-main")
}
"#;
        let result = try_render_wasm_source(source, &[], false);
        assert!(
            result.is_err(),
            "a source with a dropped top-level statement must wall, not silently compile"
        );
    }
}
