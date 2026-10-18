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

/// The module's own `std`: the real one, except that `std::process::exit`
/// flushes the host's buffered stdout first. The region render exits through
/// `std::process::exit` (a division by zero, an index out of bounds) and
/// prints unbuffered; the standard codegen buffers stdout, and an exit that
/// skipped the flush would lose the lines the program printed before the
/// region. Shadowing the path, not rewriting the text, leaves a string
/// literal that spells it untouched.
const SPLICE_STD: &str = "mod std {
    pub use ::std::*;
    pub mod process {
        pub use ::std::process::*;
        pub fn exit(code: i32) -> ! { super::super::super::almide_stdout_flush(); ::std::process::exit(code) }
    }
}
";

/// Wrap the rendered region items in their module. The region fns are
/// exported under their own names and the meter prims under the runtime
/// names the standard codegen calls.
fn wrap_metered_splice(items: &str, roots: &[String]) -> String {
    let module = almide_ir::runtime_use::METERED_SPLICE_MODULE;
    // ADR-0013: the regions share the host program's Almide String. A second
    // copy of the block inside the module would be a distinct `AlmideStr`
    // type, and every String crossing the splice would fail to type-check.
    let mut body = items.replacen(
        &crate::render_native_str::almide_string_alias(),
        "use super::{AlmideStr, AlmideString};\n",
        1,
    );
    for r in roots {
        // A rendered fn starts its own line; a string literal cannot (the
        // render escapes every newline it emits).
        let m = crate::render_native::mangle(r);
        body = body.replace(&format!("\nfn {m}("), &format!("\npub(super) fn {m}("));
    }
    let mut out = format!("\n// The metered regions, rendered by the v1 trust spine (#3669).\nmod {module} {{\n{body}\n");
    out.push_str(SPLICE_STD);
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

/// The charge-site entry points of the standard codegen's own meter (#3673,
/// `MeterChargePass`) and the `for`-head adapter: a charge, then the cut test
/// the v1 render's charge sites run (fuel below zero, else the deadline). The
/// unwind flag carries a cut in an inline stdlib callback out to the fn that
/// made the call, which is where the wasm leg's cut returns from.
const METER_ENTRY_POINTS: &str = "pub(super) fn __almd_meter_tick(cost: i64) -> bool {
    __almd_charge(0, cost, false);
    __almd_fuel_lt0() || __almd_wall_hit()
}
pub(super) fn __almd_meter_tick_len(len: usize) -> bool {
    __almd_charge_dyn(0, len as i64, false);
    __almd_fuel_lt0() || __almd_wall_hit()
}
pub(super) fn __almd_meter_pending() -> bool {
    __almd_fuel_lt0() || __ALMD_T_HIT.with(|h| h.get())
}
thread_local! {
    static __ALMD_UNWIND: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
pub(super) fn __almd_meter_unwind() {
    __ALMD_UNWIND.with(|u| u.set(true));
}
pub(super) fn __almd_meter_unwinding() -> bool {
    __ALMD_UNWIND.with(|u| u.get())
}
pub(super) fn __almd_meter_unwind_take() -> bool {
    __ALMD_UNWIND.with(|u| u.replace(false))
}
pub(super) struct __AlmdMeterIter<I>(I);
impl<I: Iterator> Iterator for __AlmdMeterIter<I> {
    type Item = I::Item;
    fn next(&mut self) -> Option<I::Item> {
        __almd_charge(0, 1, false);
        if !__almd_fuel_lt0() {
            __almd_wall_hit();
        }
        self.0.next()
    }
}
pub(super) fn __almd_meter_iter<I: IntoIterator>(it: I) -> __AlmdMeterIter<I::IntoIter> {
    __AlmdMeterIter(it.into_iter())
}
";

/// The deterministic meter the standard codegen's own charges call (#3673):
/// the v1 render's meter shims — the same fuel, budget and deadline
/// arithmetic its regions run — and the charge-site entry points, in a module
/// the CLI appends to the codegen's output. The meter prims the region
/// desugars call are exported under their runtime names.
pub fn native_meter_runtime() -> String {
    use crate::render_native_shims::{BUDGET_SHIM, CHARGE_DYN_SHIM, CHARGE_SHIM, COUNTER_SHIM, FUEL_LT0_SHIM, TIMEOUT_SHIM};
    let shims = [COUNTER_SHIM, CHARGE_SHIM, CHARGE_DYN_SHIM, FUEL_LT0_SHIM, BUDGET_SHIM.as_str(), TIMEOUT_SHIM.as_str()].join("\n\n");
    let module = "__almd_meter";
    let mut out = format!(
        "\n// The deterministic meter of the standard codegen's own charges (#3673).\nmod {module} {{\n#![allow(unused_variables, unused_mut, dead_code, non_snake_case, non_upper_case_globals)]\n{shims}\n{METER_ENTRY_POINTS}"
    );
    let prims = meter_prim_exports(&shims);
    for (prim, sig_and_body) in &prims {
        out.push_str(&format!("pub(super) fn {prim}{sig_and_body}\n"));
    }
    out.push_str("}\n");
    let entry_points = [
        "__almd_meter_tick",
        "__almd_meter_tick_len",
        "__almd_meter_pending",
        "__almd_meter_iter",
        "__almd_meter_unwind",
        "__almd_meter_unwinding",
        "__almd_meter_unwind_take",
    ];
    let names: Vec<&str> = prims.iter().map(|(p, _)| p.as_str()).chain(entry_points).collect();
    out.push_str(&format!("use self::{module}::{{{}}};\n", names.join(", ")));
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
    let exempt = crate::charge_probe::entry_exempt_set(&ir);
    for (clone, base) in crate::charge_probe::insert_probe_charges(&mut functions, &exempt) {
        if let Some(sig) = sigs.get(base.as_str()).cloned() {
            sigs.insert(clone, sig);
        }
    }
    crate::alias_safety::elide_unaliased_make_unique(&mut functions);
    let program = MirProgram { functions, exports: Vec::new(), mutable_global_count: 0 };
    let items = crate::render_native::render_native_items(&program, &sigs)?;
    Ok(wrap_metered_splice(&items, &roots))
}
