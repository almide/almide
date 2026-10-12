// ── The metered-region splice (#3669) ──
//
// `fan.bounded` / `fan.race` / `fan.timeout` meter their regions on the
// deterministic clock, and the meter is MIR: `charge_probe` places the charges
// on the lowered fns, so a region's spend is a function of the MIR. The whole-
// program native render is all-or-nothing, and it walls on constructs that have
// nothing to do with a region (an empty list literal in `main`, a
// `string.split` anywhere). The standard codegen then builds the program, and
// it hands every region to the module rendered here: the region fns and the
// metered clones of their callees, lowered, charged and rendered by the same
// steps `try_render_rust_source` runs. A region therefore reports the spend the
// whole-program render and the wasm leg report, and only the regions — not the
// whole program — have to be inside the native render's subset.

/// The region fns of `ir`, refused when one cannot cross the boundary to the
/// standard codegen ([`almide_ir::runtime_use::metered_region_crosses`]).
fn metered_region_roots(ir: &almide_ir::IrProgram) -> Result<Vec<String>, LowerError> {
    let in_modules = ir.modules.iter().flat_map(|m| &m.functions);
    if let Some(f) = in_modules.into_iter().find(|f| almide_ir::runtime_use::is_metered_region(f.name.as_str())) {
        return Err(LowerError::at(f.body.span, format!(
            "native: metered region `{}` inside a module — outside the region splice",
            f.name
        )));
    }
    let mut roots = Vec::new();
    for f in ir.functions.iter().filter(|f| almide_ir::runtime_use::is_metered_region(f.name.as_str())) {
        if !almide_ir::runtime_use::metered_region_crosses(f) {
            return Err(LowerError::at(f.body.span, format!(
                "native: metered region `{}` carries a value the standard codegen and the region render spell differently (params {:?}, return {:?})",
                f.name,
                f.params.iter().map(|p| &p.ty).collect::<Vec<_>>(),
                f.ret_ty
            )));
        }
        roots.push(f.name.as_str().to_string());
    }
    Ok(roots)
}

/// Lower the regions and every fn they reach through a call (lifted lambdas
/// come with the fn that lifts them). A callee outside `ir.functions` is left
/// to the render, which links it as a runtime shim or walls on it.
fn lower_region_closure(
    ir: &almide_ir::IrProgram,
    roots: &[String],
    record_layouts: &crate::lower::RecordLayouts,
    variant_layouts: &crate::lower::VariantLayouts,
) -> Result<Vec<crate::MirFunction>, LowerError> {
    let globals = HashMap::new();
    let global_inits = HashMap::new();
    let mut seen: std::collections::BTreeSet<String> = roots.iter().cloned().collect();
    let mut queue: std::collections::VecDeque<String> = roots.iter().cloned().collect();
    let mut functions = Vec::new();
    while let Some(name) = queue.pop_front() {
        let Some(func) = ir.functions.iter().find(|f| !f.is_test && f.name.as_str() == name) else { continue };
        native_sig_subset_check(func, record_layouts, variant_layouts)?;
        let all = crate::lower::lower_function_all_with_globals(func, &globals, &global_inits, record_layouts, variant_layouts)
            .map_err(|e| LowerError::at(func.body.span, format!("native: fn `{}`: {e:?}", func.name)))?;
        for callee in all.iter().flat_map(|f| f.ops.iter()).filter_map(|op| match op {
            crate::Op::CallFn { name, .. } => Some(name.as_str().to_string()),
            _ => None,
        }) {
            if seen.insert(callee.clone()) {
                queue.push_back(callee);
            }
        }
        functions.extend(all);
    }
    Ok(functions)
}

/// The meter prims the standard codegen's call sites name, exported from the
/// shims that implement them: a shim `fn __almd_<rest>(params) -> i64` whose
/// `<rest>` continues a [`almide_ir::runtime_use::METERED_PRIM_PREFIXES`] entry
/// is the prim `almide_rt_prim_<rest>`. The prim names come from that one
/// declaration and the shims the render emitted, never from a list here.
/// Returns `(prim, "(params) -> i64 { shim(args) }")`.
fn meter_prim_exports(body: &str) -> Vec<(String, String)> {
    let families: Vec<&str> = almide_ir::runtime_use::METERED_PRIM_PREFIXES
        .iter()
        .filter_map(|p| p.strip_prefix("almide_rt_prim_"))
        .collect();
    body.lines()
        .filter_map(|l| l.strip_prefix("fn __almd_"))
        .filter(|rest| families.iter().any(|f| rest.starts_with(f)))
        .filter_map(|rest| {
            let (name, tail) = rest.split_once('(')?;
            let (params, after) = tail.split_once(')')?;
            after.trim_start().starts_with("-> i64").then(|| {
                let args: Vec<&str> = params.split(',').filter_map(|p| p.split(':').next()).map(str::trim).filter(|p| !p.is_empty()).collect();
                (format!("almide_rt_prim_{name}"), format!("({params}) -> i64 {{ __almd_{name}({}) }}", args.join(", ")))
            })
        })
        .collect()
}

/// Wrap the rendered region items in their module. The region fns are
/// exported under their own names, the meter prims under the runtime names the
/// standard codegen calls, and a shim's `process::exit` flushes the host's
/// buffered stdout first — the region render prints unbuffered, the standard
/// codegen does not, and an exit that skipped the flush would lose lines the
/// program printed before the region.
fn wrap_metered_splice(items: &str, roots: &[String]) -> String {
    let module = almide_ir::runtime_use::METERED_SPLICE_MODULE;
    let mut body = items.replace("std::process::exit(", "__almd_host_exit(");
    for r in roots {
        let m = crate::render_native::mangle(r);
        body = body.replace(&format!("\nfn {m}("), &format!("\npub(super) fn {m}("));
    }
    let mut out = format!("\n// The metered regions, rendered by the v1 trust spine (#3669).\nmod {module} {{\n{body}\n");
    out.push_str("fn __almd_host_exit(code: i32) -> ! { super::almide_stdout_flush(); std::process::exit(code) }\n");
    for r in roots {
        out.push_str(&format!("pub(super) use self::{} as {r};\n", crate::render_native::mangle(r)));
    }
    let prims = meter_prim_exports(&body);
    for (prim, sig_and_body) in &prims {
        out.push_str(&format!("pub(super) fn {prim}{sig_and_body}\n"));
    }
    out.push_str("}\n");
    if !prims.is_empty() {
        let names: Vec<&str> = prims.iter().map(|(p, _)| p.as_str()).collect();
        out.push_str(&format!("use self::{module}::{{{}}};\n", names.join(", ")));
    }
    out
}

/// Render the metered regions of `source` as a module the standard codegen's
/// output links (#3669). Refuses — a wall the caller reports — when a region,
/// or a fn a region reaches, is outside the native render's subset.
pub fn try_render_metered_splice(source: &str) -> Result<String, LowerError> {
    crate::charge_probe::reset_budget_used();
    let _strict = crate::lower::StrictValuesGuard::set(true);
    let ir = source_to_ir_with(source, &[])?;
    let record_layouts = crate::lower::build_record_layouts(&ir.type_decls);
    let variant_layouts = crate::lower::build_variant_layouts(&ir.type_decls);
    let roots = metered_region_roots(&ir)?;
    let mut functions = lower_region_closure(&ir, &roots, &record_layouts, &variant_layouts)?;
    if let Some(f) = functions.iter().find(|f| recurses_on_a_heap_arg(f)) {
        return Err(LowerError::at(None, format!(
            "native: fn `{}` passes a heap value to its own tail call — no loop conversion on this render (#3550)",
            f.name
        )));
    }
    rewrite_native_result_windows(&ir, &mut functions);
    let mut sigs = build_native_sig_table(&ir, &functions, &record_layouts, &variant_layouts)?;
    for (clone, base) in crate::charge_probe::insert_probe_charges(&mut functions) {
        if let Some(sig) = sigs.get(base.as_str()).cloned() {
            sigs.insert(clone, sig);
        }
    }
    crate::alias_safety::elide_unaliased_make_unique(&mut functions);
    let program = MirProgram { functions, exports: Vec::new(), mutable_global_count: 0 };
    let items = crate::render_native::render_native_items(&program, &sigs)?;
    Ok(wrap_metered_splice(&items, &roots))
}
