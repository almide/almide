// The native render body (`render_native_parts`) and its phases —
// include!-spliced into `pipeline.rs` through pipeline_native_rungs.rs,
// sharing that module's imports. A pure code move out of its parent (the
// 800-line ceiling the codopsy gate holds this crate to), with the one long
// function split into named phase helpers in their original order; there is
// no boundary of meaning here.

/// One rendered function's certificate: `(property, function, witness)`.
type NativeCert = (String, String, String);

/// What [`render_native_parts`] returns: the native Rust source, the
/// certificates of the MIR it rendered from, and that MIR.
type NativeRenderParts = (String, Vec<NativeCert>, Vec<crate::MirFunction>);

/// Debug aid: ALMIDE_DUMP_MIR=1 prints every lowered fn's op stream (the
/// same view `debug_dump_mir` builds) before the native render runs.
fn dump_native_mir_if_requested(source: &str) {
    use std::io::Write;
    if !almide_base::env::flag("ALMIDE_DUMP_MIR") {
        return;
    }
    if let Ok(dump) = debug_dump_mir(source) {
        let _ = writeln!(std::io::stderr(), "{dump}");
    }
}

/// #4447: the native rungs render no `@export(c)` wrapper (the program's
/// `exports` are empty), so a program declaring one walls here and builds
/// through the standard codegen, which renders the C wrapper from the C ABI
/// table — never a binary that silently lacks the C symbol.
fn wall_native_c_exports(ir: &almide_ir::IrProgram) -> Result<(), LowerError> {
    if let Some(f) = ir.functions.iter().chain(ir.modules.iter().flat_map(|m| &m.functions))
        .find(|f| f.export_attrs.iter().any(|a| a.target.as_str() == "c")) {
        return Err(LowerError::at(
            f.body.span,
            format!("native: fn `{}` is @export(c) — its C wrapper is outside the native rungs", f.name),
        ));
    }
    Ok(())
}

/// #4006 A5: the tuples the generated record structs are derived for — every
/// tuple a signature, a record field or a variant payload names, then the
/// all-scalar tuples locals hold.
fn native_struct_tuples(
    ir: &almide_ir::IrProgram,
    record_layouts: &crate::lower::RecordLayouts,
    variant_layouts: &crate::lower::VariantLayouts,
) -> Vec<almide_lang::types::Ty> {
    let mut ts = Vec::new();
    let abi = almide_ir::effect_abi::effect_abi_facts(ir);
    for f in &ir.functions {
        f.params.iter().for_each(|p| crate::render_native_rec::collect_tuples(&p.ty, &mut ts));
        crate::render_native_rec::collect_tuples(&f.ret_ty, &mut ts);
        if abi.auto_wrap.contains(f.name.as_str()) {
            crate::render_native_rec::collect_tuples(&almide_lang::types::Ty::result(f.ret_ty.clone(), almide_lang::types::Ty::String), &mut ts);
        }
    }
    for (_, fs) in record_layouts.values() {
        fs.iter().for_each(|(_, t)| crate::render_native_rec::collect_tuples(t, &mut ts));
    }
    for l in variant_layouts.by_type.values() {
        l.cases.iter().flat_map(|c| c.fields.iter()).for_each(|(_, t)| crate::render_native_rec::collect_tuples(t, &mut ts));
    }
    collect_local_native_types(ir, &mut ts);
    ts
}

/// Local containers use their checked type even when no function signature
/// mentions it. This includes empty lists and nested Option/Result values.
fn collect_local_native_types(ir: &almide_ir::IrProgram, ts: &mut Vec<almide_lang::types::Ty>) {
    // The shared frontend's table also contains locals of library functions
    // removed by linking. Those dead entries must not pull closure/container
    // representations into a program that never uses them. Keep declaration
    // types for all retained locals, including empty carriers without stores.
    let empty = std::collections::HashSet::new();
    let mut live: std::collections::HashSet<_> = ir.functions.iter()
        .flat_map(|f| f.params.iter().map(|p| p.var))
        .chain(ir.top_lets.iter().map(|t| t.var)).collect();
    for body in ir.functions.iter().map(|f| &f.body).chain(ir.top_lets.iter().map(|t| &t.value)) {
        live.extend(almide_ir::free_vars::bound_vars(body));
        live.extend(almide_ir::free_vars::free_vars(body, &empty));
    }
    for (id, info) in ir.var_table.entries.iter().enumerate() {
        if !live.contains(&almide_ir::VarId(id as u32)) { continue; }
        let mut found = Vec::new();
        crate::render_native_rec::collect_tuples(&info.ty, &mut found);
        for t in found {
            // Locals and signatures share the same declared representations,
            // including an empty nested carrier with no stores to infer from.
            if !ts.contains(&t) {
                ts.push(t);
            }
        }
    }
    // A computed call argument may never enter var_table. Its checked
    // expression type still owns a native representation (some(some(xs))).
    for f in &ir.functions { collect_expr_native_types(&f.body, ts); }
    for t in &ir.top_lets { collect_expr_native_types(&t.value, ts); }
}

fn collect_expr_native_types(expr: &almide_ir::IrExpr, ts: &mut Vec<almide_lang::types::Ty>) {
    use almide_ir::IrVisitor;
    struct Types<'a>(&'a mut Vec<almide_lang::types::Ty>);
    impl IrVisitor for Types<'_> {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            crate::render_native_rec::collect_tuples(&expr.ty, self.0);
            // Map literals lower to from_list over checked (key, value)
            // pairs. Register those introduced container types before MIR
            // lowering, so an empty Option element cannot erase the list's
            // identity when the renderer sees only its uniform slots.
            if matches!(expr.kind, almide_ir::IrExprKind::MapLiteral { .. }) {
                if let almide_lang::types::Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Map, args) = &expr.ty {
                    if args.len() == 2 {
                        crate::render_native_rec::collect_tuples(
                            &almide_lang::types::Ty::list(almide_lang::types::Ty::Tuple(args.clone())), self.0);
                    }
                }
            }
            almide_ir::walk_expr(self, expr);
        }
    }
    Types(ts).visit_expr(expr);
}

/// #4006 family I: top-level state lives in storage slots initialised
/// before `main`. A mutable `var` already has a slot (the lowering routes
/// its reads and writes through `mg_slot_addr`). An immutable `let` whose
/// initializer can fail or calls must run EAGERLY, at program start
/// (top_let_div_eager: an unused `let X = 1 / 0` aborts), so it gets a slot
/// too and is evaluated once, in declaration order, by the synthesized
/// `__mg_init` that `main` calls first. A total, call-free initializer
/// (literals, other top-level lets, operators that cannot abort) stays
/// materialized at each use: it can neither fail nor be observed.
fn prepare_native_top_lets(ir: &mut almide_ir::IrProgram) {
    // The shared accumulator tail-recursion rewrite (#2577), from the same
    // preconditions the wasm leg and the standard codegen apply it: one
    // recursive call per non-leaf instead of two. Skipped, as there, when the
    // program brackets a budget/timeout region (its loop head would be a
    // charge the meter sees).
    // Before it, the native legs' one-level self-recursive inlining (#4456):
    // a pure scalar recursion it rewrites keeps its calls for LLVM to merge
    // (no longer the accumulator's shape); one it declines still gets the loop.
    if !almide_ir::meter_plan::program_has_regions(ir) {
        let self_inline = !almide_base::env::flag("ALMIDE_SELF_INLINE_OFF");
        for f in ir.functions.iter_mut() {
            if !(self_inline && almide_ir::self_inline::rewrite(f, &mut ir.var_table)) {
                almide_ir::accum_tre::rewrite(f, &mut ir.var_table);
            }
        }
    }
    // An immutable alias of a stateful/eager global is a snapshot, not a
    // constant expression to re-evaluate at every read. Close the safe set
    // over dependencies before assigning storage to any declaration.
    let mut top_vars: std::collections::HashSet<_> = ir.top_lets.iter()
        .filter(|tl| !tl.mutable).map(|tl| tl.var).collect();
    loop {
        let remove: Vec<_> = ir.top_lets.iter()
            .filter(|tl| top_vars.contains(&tl.var) && !total_const_init(&tl.value, &top_vars))
            .map(|tl| tl.var).collect();
        if remove.is_empty() { break; }
        for id in remove { top_vars.remove(&id); }
    }
    let mut eager = false;
    for tl in ir.top_lets.iter_mut() {
        if !tl.mutable && !top_vars.contains(&tl.var) {
            tl.mutable = true;
            eager = true;
        }
    }
    if eager {
        crate::lower::install_effect_abi_facts(ir);
    }
    let order = almide_ir::top_let_storage::dependency_init_order(ir, &std::collections::HashMap::new());
    let positions: std::collections::HashMap<_, _> = order.into_iter().enumerate()
        .map(|(i, id)| (id, i)).collect();
    ir.top_lets.sort_by_key(|tl| positions.get(&tl.var).copied().unwrap_or(usize::MAX));
    if let Some(init) = synthesize_slot_init(ir) {
        ir.functions.push(init);
    }
}

/// Lower every non-test function of the program. ALL-OR-NOTHING: any
/// unlowerable fn walls the program (the native rungs have no per-fn
/// fallback — a partial native binary cannot call into v0).
fn lower_native_functions(
    ir: &almide_ir::IrProgram,
    record_layouts: &crate::lower::RecordLayouts,
    variant_layouts: &crate::lower::VariantLayouts,
) -> Result<Vec<crate::MirFunction>, LowerError> {
    let globals: std::collections::HashMap<_, _> = ir.top_lets.iter().map(|tl| (tl.var, tl.ty.clone())).collect();
    let global_inits: std::collections::HashMap<_, _> =
        ir.top_lets.iter().map(|tl| (tl.var, tl.value.clone())).collect();
    let mut functions = Vec::new();
    for func in &ir.functions {
        if func.is_test {
            continue;
        }
        native_sig_subset_check(func, record_layouts, variant_layouts)?;
        // ALL-OR-NOTHING: any unlowerable fn walls the program (the native rungs
        // have no per-fn fallback — a partial native binary cannot call into v0).
        let all = crate::lower::lower_function_all_with_globals(
            func,
            &globals,
            &global_inits,
            record_layouts,
            variant_layouts,
        )
        .map_err(|e| {
            LowerError::Unsupported(format!("native: fn `{}`: {e:?}", func.name))
        })?;
        functions.extend(all);
    }
    Ok(functions)
}

/// The MIR rewrites between the link and the signature table: the heap
/// tail-call wall, the Result/Option window rewrites, the slot and narrow
/// rewrites, and the `__mg_init` call at `main`'s head.
fn rewrite_native_linked_mir(
    ir: &almide_ir::IrProgram,
    linked: &[LinkedSelfHost],
    functions: &mut [crate::MirFunction],
    host_results: &NativeHostResults,
    test_harness: bool,
) -> Result<(), LowerError> {
    if let Some(f) = functions.iter().find(|f| recurses_on_a_heap_arg(f)) {
        return Err(LowerError::Unsupported(format!(
            "native: fn `{}` passes a heap value to its own tail call — no loop conversion on this render (#3550)",
            f.name
        )));
    }
    // A closure block only its direct calls named, all of them inlined, is
    // never built (no copy of what it captures).
    for f in functions.iter_mut() {
        crate::native_unread_closure::elide(f);
    }
    rewrite_native_result_windows_over(
        ir.functions.iter().chain(linked.iter().flat_map(|l| l.ir.functions.iter())),
        functions,
        host_results,
    );
    rewrite_native_result_str_windows_over(ir.functions.iter().chain(linked.iter().flat_map(|l| l.ir.functions.iter())), functions);
    rewrite_native_option_windows(ir, linked, functions);
    crate::native_slot_rewrite::rewrite_slot_ops(functions, crate::lower::mutable_global_slot_count());
    // Narrow-width arithmetic computes in its declared integer type (#4006).
    for f in functions.iter_mut() {
        crate::native_narrow_rewrite::rewrite_narrow_ops(f)?;
        crate::render_native_addr::sink_guarded_payload_reads(f);
    }
    call_slot_init_from_main(functions);
    if !test_harness && !functions.iter().any(|f| f.name == "main") {
        return Err(LowerError::Unsupported(
            "native: main is outside the MIR-lowering subset".into(),
        ));
    }
    Ok(())
}

/// The storage slots holding a heap value whose declared type has a native
/// kind (`native_slot_rewrite::heap_slot`). Non-default carriers use vacant
/// cells until initialized or republished. A threaded fan may coexist only
/// when its transitive callees and closure references cannot reach them.
fn native_heap_slots(
    functions: &[crate::MirFunction],
    record_layouts: &crate::lower::RecordLayouts,
    variant_layouts: &crate::lower::VariantLayouts,
) -> std::collections::BTreeMap<u32, crate::render_native::NTy> {
    if crate::fan_native::reaches_global_slots(functions) {
        return Default::default();
    }
    crate::lower::mutable_global_slot_types()
        .into_iter()
        .filter(|(_, t)| crate::lower::is_heap_ty(t))
        .filter_map(|(slot, t)| {
            let k = native_sig_kind(&t, record_layouts, variant_layouts)?;
            let nty = crate::render_native_closure::owned_nty(k);
            Some((slot, nty))
        })
        .collect()
}

/// When the program synthesized `__mg_init`, `main` calls it first.
fn call_slot_init_from_main(functions: &mut [crate::MirFunction]) {
    if !functions.iter().any(|f| f.name == SLOT_INIT) {
        return;
    }
    if let Some(main) = functions.iter_mut().find(|f| f.name == "main") {
        main.ops.insert(
            0,
            crate::Op::CallFn { dst: None, name: SLOT_INIT.to_string(), args: vec![], result: None },
        );
    }
}

/// The program's SIG-KIND table, plus each linked source's entries for the
/// linked functions the program kept.
fn native_sigs_with_linked(
    ir: &almide_ir::IrProgram,
    functions: &[crate::MirFunction],
    linked: &[LinkedSelfHost],
    record_layouts: &crate::lower::RecordLayouts,
    variant_layouts: &crate::lower::VariantLayouts,
) -> Result<crate::render_native::NativeSigs, LowerError> {
    crate::render_native_rec::extend_with(&crate::lambda_sigs::declared_types());
    crate::render_native_rec::install_cell_types();
    let mut sigs = build_native_sig_table(ir, functions, record_layouts, variant_layouts, &|_| true)?;
    // A linked source's sibling the program never calls is not read: its
    // signature would put types no rendered fn carries in the table (#4514).
    let rendered = |name: &str| functions.iter().any(|f| f.name == name);
    for l in linked {
        for (name, sig) in build_native_sig_table(&l.ir, &[], &l.record_layouts, &l.variant_layouts, &rendered)? {
            sigs.entry(name).or_insert(sig);
        }
    }
    Ok(sigs)
}

/// Stage 1 probe: same insertion point in pass order as the wasm leg.
/// T1-2 metered clones carry their base fn's declared signature — copy the
/// sig entry so a call to `__fuel__heavy` types exactly like `heavy` (without
/// this the repr fallback typed a Result-returning clone as String). The
/// clone→base pairs come from the pass that made the clones, never from a
/// name's spelling: a `__fuel` suffix test gave a user fn `heavy__fuel`
/// `heavy`'s signature (#3491).
fn insert_native_probe_charges(
    ir: &almide_ir::IrProgram,
    functions: &mut Vec<crate::MirFunction>,
    sigs: &mut crate::render_native::NativeSigs,
) {
    let exempt = crate::charge_probe::entry_exempt_set(ir);
    for (clone, base) in crate::charge_probe::insert_probe_charges(functions, &exempt) {
        if let Some(sig) = sigs.get(base.as_str()).cloned() {
            sigs.insert(clone, sig);
        }
    }
}

/// The ownership certificate of every function, then the program's
/// `native-calls` witness.
fn native_ownership_certs(
    functions: &[crate::MirFunction],
    runtime_sites: &std::collections::BTreeMap<(String, usize), String>,
) -> Vec<NativeCert> {
    let mut certs: Vec<NativeCert> = functions
        .iter()
        .map(|f| ("ownership".to_string(), f.name.to_string(), crate::certificate::ownership_certificate(f)))
        .collect();
    certs.push((
        "native-calls".to_string(),
        "program".to_string(),
        crate::certificate::native_calls_witness_with(functions, runtime_sites),
    ));
    certs
}

/// The native render of `program`, with the calls `crate::native_hof` serves
/// through their runtime body: when it served any, the linked bodies those
/// calls left unreached are dropped and the program rendered again, so the
/// render and the certificates speak of the same functions; and the closure
/// blocks only those calls read are not built (their slots reach the call
/// as a stack env).
fn render_native_with_runtime_hofs(
    program: &mut MirProgram,
    sigs: &crate::render_native::NativeSigs,
    linked: &std::collections::BTreeSet<String>,
    runtime: &std::collections::BTreeSet<String>,
) -> Result<String, LowerError> {
    let first = render_native_hof_pass(program, sigs, linked, runtime);
    if first.is_ok() { return first; }
    // A body can lower successfully yet refuse native representation (e.g.
    // set slots). Apply the same declared boundary graph as a lowering refusal.
    // User roots and function references still cannot be routed away.
    let mut runtime = runtime.clone();
    runtime.extend(linked.iter().filter(|n| !program.functions.iter().any(|f| f.name == **n)).cloned());
    let recs = crate::render_native_rec::snapshot();
    crate::native_hof::reset(runtime.clone(), Default::default());
    let refused = crate::render_native::refused_native_bodies(program, sigs, linked);
    crate::render_native_rec::restore(recs);
    let refused = refused?;
    if refused.is_empty() { return first; }
    let roots = program.functions.iter().filter(|f| !linked.contains(&f.name))
        .map(|f| f.name.clone()).collect();
    let mut routed = crate::native_link_routes::route(&mut program.functions, &roots, &refused)?;
    if routed.is_empty() { return first; }
    routed.extend(runtime.iter().cloned());
    render_native_hof_pass(program, sigs, linked, &routed)
}

fn render_native_hof_pass(
    program: &mut MirProgram,
    sigs: &crate::render_native::NativeSigs,
    linked: &std::collections::BTreeSet<String>,
    runtime: &std::collections::BTreeSet<String>,
) -> Result<String, LowerError> {
    crate::native_hof::reset(runtime.clone(), Default::default());
    let recs = crate::render_native_rec::snapshot();
    let first = crate::render_native::try_render_native_program(program, sigs);
    let sites = crate::native_hof::sites();
    if sites.is_empty() {
        return first;
    }
    let mut dropped = crate::native_hof::prune(&mut program.functions, linked, &sites);
    let elided = crate::native_hof::elidable(&program.functions, &sites);
    if dropped.is_empty() && elided.is_empty() {
        return first;
    }
    crate::render_native_rec::restore(recs);
    dropped.extend(runtime.iter().cloned());
    crate::native_hof::reset(dropped, elided);
    crate::render_native::try_render_native_program(program, sigs)
}

/// The body of [`try_render_rust_source_certified`]: the native render, the
/// certificates of the MIR it rendered from, and that MIR.
fn render_native_parts(source: &str) -> Result<NativeRenderParts, LowerError> {
    dump_native_mir_if_requested(source);
    render_native_parts_with(|| program_to_ir_mode(parse_or_wall(source)?, &[], true))
}

/// Reuse an entry already parsed without recovery errors. The caller must reject
/// parser diagnostics before calling; native checking and preparation remain shared.
pub fn try_render_rust_program(source: &str, program: almide_lang::ast::Program) -> Result<String, LowerError> {
    dump_native_mir_if_requested(source);
    render_native_parts_with(|| program_to_ir_mode(program, &[], true)).map(|(rust, _, _)| rust)
}

/// Render the CLI's checked, unoptimized IR. The caller must have rejected
/// entry/module diagnostics and enforced its project gates before calling.
/// This consumes the same frontend environment that produced the IR: module
/// identities, convention methods and inferred types must not be reconstructed.
pub fn try_render_rust_checked_certified(
    source: &str,
    ir: almide_ir::IrProgram,
    checker: Checker,
) -> Result<(String, Vec<NativeCert>), LowerError> {
    dump_native_mir_if_requested(source);
    render_native_parts_with(|| prepare_checked_ir(ir, checker, true))
        .map(|(rust, certs, _)| (rust, certs))
}

/// The checked frontend route with the project's filesystem policy. Scope
/// guards and their trusted host support are emitted before artifact binding.
pub fn try_render_rust_checked_scoped_certified(
    source: &str,
    ir: almide_ir::IrProgram,
    checker: Checker,
    fs_roots: Option<Vec<String>>,
) -> Result<(String, Vec<NativeCert>), LowerError> {
    let _scope = crate::native_fs_scope::ScopeGuard::enter(fs_roots);
    try_render_rust_checked_certified(source, ir, checker)
}

fn render_native_parts_with(prepare: impl FnOnce() -> Result<almide_ir::IrProgram, LowerError>) -> Result<NativeRenderParts, LowerError> {
    render_native_parts_for(prepare, false)
}

/// Native test-body preparation, with libtest framing bound into the returned
/// artifact. The CLI switches to this route only after its full harness gates.
pub fn try_render_rust_checked_tests_certified(
    source: &str,
    mut ir: almide_ir::IrProgram,
    checker: Checker,
    fs_roots: Option<Vec<String>>,
) -> Result<(String, Vec<NativeCert>), LowerError> {
    dump_native_mir_if_requested(source);
    let plan = crate::native_test_harness::Plan::prepare(&mut ir);
    let _entries = plan.enter();
    let _assertions = crate::native_test_assert::Scope::enter();
    let _scope = crate::native_fs_scope::ScopeGuard::enter(fs_roots);
    let (mut rust, certs, functions) = render_native_parts_for(|| {
        let mut ir = prepare_checked_ir(ir, checker, true)?;
        plan.promote(&mut ir)?;
        Ok(ir)
    }, true)?;
    rust.push_str(&plan.render(&functions)?);
    Ok((rust, certs))
}

fn render_native_parts_for(prepare: impl FnOnce() -> Result<almide_ir::IrProgram, LowerError>, test_harness: bool) -> Result<NativeRenderParts, LowerError> {
    let _foreign_originals = crate::native_foreign::OriginalScope::enter();
    let _error_displays = crate::native_display::ErrorDisplayScope::enter();
    crate::charge_probe::reset_budget_used();
    crate::lambda_sigs::clear();
    crate::option_blocks::clear();
    let _strict = crate::lower::StrictValuesGuard::set(true);
    let _declared_routes = crate::lower::registry_sig::DeclaredRouteGuard::set(|name| crate::native_floor::declared(name).is_some() || crate::native_hof::declared_call(name) || crate::native_parallel_maps::declared_call(name));
    // The native render loop-converts a self tail call over any heap carry
    // (#4006 class A, #3550): `lower::owned_tco_slots`.
    let _owned_tco = crate::lower::OwnedTcoSlotsGuard::set(true);
    let ir = {
        let _slots = SlotHeldGlobalsGuard::set(true);
        prepare()?
    };
    crate::charge_probe::note_program_regions(almide_ir::meter_plan::program_has_regions(&ir));
    // #4447: the native rungs render no `@export(c)` wrapper — the wall
    // comes before the fan lift.
    wall_native_c_exports(&ir)?;
    // #4006 C2: a discovered region window runs in the arena on this render
    // too — the `__rgn_` twins and a window fn the MIR brackets below.
    let mut ir = native_bundled::lower_list_patterns(ir);
    crate::native_fn_aliases::expand(&mut ir);
    crate::native_never::expand(&mut ir);
    if test_harness {
        crate::native_test_panic::outline(&mut ir);
        crate::native_test_assert::outline(&mut ir);
    }
    crate::native_defaults::expand(&mut ir);
    crate::native_nested_places::expand(&mut ir);
    crate::native_mutation_places::expand(&mut ir);
    almide_codegen::pass_region_window_native::native_region_windows(&mut ir);
    let windows = crate::native_region_window::window_fns(&ir);
    let mut ir = crate::fan_native::lift_fan_arms(ir)?;
    if let Some(f) = crate::charge_probe::region_reaching_callback(&ir) {
        let span = ir.functions.iter().find(|g| g.name.as_str() == f).and_then(|g| g.body.span);
        return Err(LowerError::at(span, format!(
            "native: metered region fn `{f}` hands a callback on, which the v1 meter does not charge as the meter plan does"
        )));
    }
    // Rung-5 records slab: the layout registries the wasm leg threads — without
    // them a record literal lowers as an Opaque skeleton and every field read
    // strict-walls (the probe_native trap recorded in the trust-spine ledger).
    let record_layouts = crate::lower::build_record_layouts(&ir.type_decls);
    let variant_layouts = crate::lower::build_variant_layouts(&ir.type_decls);
    crate::native_display::expand(&mut ir, &record_layouts, &variant_layouts);
    crate::native_patterns::expand(&mut ir, &variant_layouts);
    crate::native_map_eq::expand(&mut ir);
    // #4006 A5: the generated record structs, derived from the declared layouts.
    let _order = crate::render_native_rec::install_declared_order(&ir.type_decls);
    let _records = crate::render_native_rec::install_derived(
        &record_layouts,
        &variant_layouts,
        native_struct_tuples(&ir, &record_layouts, &variant_layouts),
        ir.codegen_annotations.region_enums.iter().cloned().collect(),
    );
    if !ir.modules.is_empty() {
        return Err(LowerError::Unsupported(
            "native: multi-module program — outside rung 1".into(),
        ));
    }
    let mut ir = ir;
    let _foreign = crate::native_foreign::Scope::enter(&ir);
    prepare_native_top_lets(&mut ir);
    let _fusion = crate::native_fusion::Scope::enter(&ir);
    let mut functions = lower_native_functions(&ir, &record_layouts, &variant_layouts)?;
    let own: std::collections::BTreeSet<String> = functions.iter().map(|f| f.name.clone()).collect();
    // #4006: the registry bodies the program calls, linked as certified MIR.
    let (linked, runtime, host_results) = link_native_self_host(&mut functions, &ir)?;
    let linked_names: std::collections::BTreeSet<String> =
        functions.iter().filter(|f| !own.contains(&f.name)).map(|f| f.name.clone()).collect();
    crate::native_slot_rewrite::set_heap_slots(native_heap_slots(&functions, &record_layouts, &variant_layouts));
    // No function is known to leave the heap slots alone until this program's are read.
    crate::native_slot_takes::set_quiet(&[]);
    rewrite_native_linked_mir(&ir, &linked, &mut functions, &host_results, test_harness)?;
    let mut sigs = native_sigs_with_linked(&ir, &functions, &linked, &record_layouts, &variant_layouts)?;
    insert_native_probe_charges(&ir, &mut functions, &mut sigs);
    // #824: see the wasm leg's call above — `Op::MakeUnique` already renders to
    // nothing on native (render_native.rs's `Op::Consume | Op::Borrow |
    // Op::MakeUnique => {}`), so this is a no-op cleanup here, kept only so both
    // legs run the identical target-agnostic MIR pass list.
    crate::alias_safety::elide_unaliased_make_unique(&mut functions);
    // #4006 class P: a heap param its body takes over becomes a move param,
    // and every call hands the argument over at the call.
    let before_modes = functions.clone();
    crate::native_move_params::rewrite(&mut functions, &own, &sigs);
    crate::native_region_window::bracket_windows(&mut functions, &windows)?;
    crate::native_slot_takes::delay_publications(&mut functions)?;
    crate::native_slot_takes::set_quiet(&functions);
    let mut program = MirProgram {
        functions,
        exports: Vec::new(),
        // Rung 1 walls every top-let above, so there are no mutable-global slots.
        mutable_global_count: 0,
    };
    crate::native_closure_decl::set(
        ir.functions.iter().chain(linked.iter().flat_map(|l| l.ir.functions.iter())),
        linked_names.clone(),
    );
    let _entry = crate::native_entry::EntryGuard::set(ir.functions.iter().any(|f| f.name.as_str() == "main" && f.is_effect));
    let initial_records = crate::render_native_rec::snapshot();
    let mut rust = render_native_with_runtime_hofs(&mut program, &sigs, &linked_names, &runtime)?;
    // Runtime adapters can make linked indirect callers unreachable. Such
    // callers must not keep pinning the surviving callbacks to borrowing:
    // a fold would then clone its growing accumulator on every iteration.
    // Start from pristine MIR: repeating move insertion on already rewritten
    // functions would duplicate transfers and invalidate their call witnesses.
    if program.functions.len() < before_modes.len() {
        let retained: std::collections::BTreeSet<_> = program.functions.iter().map(|f| f.name.clone()).collect();
        let mut fresh: Vec<_> = before_modes.into_iter().filter(|f| retained.contains(&f.name)).collect();
        crate::render_native_rec::restore(initial_records);
        crate::native_move_params::rewrite(&mut fresh, &own, &sigs);
        crate::native_region_window::bracket_windows(&mut fresh, &windows)?;
        crate::native_slot_takes::delay_publications(&mut fresh)?;
        crate::native_slot_takes::set_quiet(&fresh);
        program.functions = fresh;
        let mut routed = runtime.clone();
        routed.extend(linked_names.iter().filter(|n| !retained.contains(*n)).cloned());
        rust = render_native_with_runtime_hofs(&mut program, &sigs, &linked_names, &routed)?;
    }
    rust.push_str(&crate::native_callbacks::render(&ir, &sigs)?);
    let mut certs = native_ownership_certs(&program.functions, &crate::native_hof::sites());
    // Row 4: each function's bounds witness, recorded as the render wrote its
    // element accesses (render_native_bounds).
    for (name, w) in crate::render_native_bounds::take() {
        certs.push(("bounds".to_string(), name, w));
    }
    Ok((rust, certs, program.functions))
}

#[cfg(test)]
mod reachable_callback_modes_tests {
    #[test]
    fn a_runtime_fold_takes_its_accumulator_after_unreachable_callers_are_pruned() {
        std::thread::Builder::new().stack_size(512 << 20).spawn(|| {
            let source = include_str!("../../../spec/wasm_cross/json_set_path_depth_bound.almd");
            let (_, _, functions) = super::render_native_parts(source).unwrap();
            for name in ["__lambda_path_of_0", "__lambda_nest_0"] {
                let f = functions.iter().find(|f| f.name == name).unwrap();
                assert!(!f.owned_params.contains(&f.params[0].value), "the environment stays borrowed");
                assert!(f.owned_params.contains(&f.params[1].value), "{name} must take the growing accumulator");
            }
        }).unwrap().join().unwrap();
    }
}
