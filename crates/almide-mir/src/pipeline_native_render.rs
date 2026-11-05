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
    if let Some(f) = ir.functions.iter().find(|f| f.export_attrs.iter().any(|a| a.target.as_str() == "c")) {
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
    for f in &ir.functions {
        f.params.iter().for_each(|p| crate::render_native_rec::collect_tuples(&p.ty, &mut ts));
        crate::render_native_rec::collect_tuples(&f.ret_ty, &mut ts);
    }
    for (_, fs) in record_layouts.values() {
        fs.iter().for_each(|(_, t)| crate::render_native_rec::collect_tuples(t, &mut ts));
    }
    for l in variant_layouts.by_type.values() {
        l.cases.iter().flat_map(|c| c.fields.iter()).for_each(|(_, t)| crate::render_native_rec::collect_tuples(t, &mut ts));
    }
    collect_local_scalar_tuples(ir, &mut ts);
    ts
}

/// An all-scalar tuple a local holds (`let v: List[(Int, Int)]`) is a
/// by-value struct as well, declared in a signature or not.
fn collect_local_scalar_tuples(ir: &almide_ir::IrProgram, ts: &mut Vec<almide_lang::types::Ty>) {
    for info in &ir.var_table.entries {
        let mut found = Vec::new();
        crate::render_native_rec::collect_tuples(&info.ty, &mut found);
        for t in found {
            let scalar = matches!(&t, almide_lang::types::Ty::Tuple(es)
                if !es.is_empty() && es.iter().all(crate::render_native_rec::is_lane_scalar));
            // A map or set a local holds (`let m: Map[String, Int] = [:]`):
            // its generated type, for the empty container that starts it.
            let container = matches!(&t, almide_lang::types::Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Map, _))
                || crate::render_native_rec::is_native_set(&t);
            if (scalar || container) && !ts.contains(&t) {
                ts.push(t);
            }
        }
    }
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
    let top_vars: std::collections::HashSet<_> = ir.top_lets.iter().map(|tl| tl.var).collect();
    let mut eager = false;
    for tl in ir.top_lets.iter_mut() {
        if !tl.mutable && !total_const_init(&tl.value, &top_vars) {
            tl.mutable = true;
            eager = true;
        }
    }
    if eager {
        crate::lower::install_effect_abi_facts(ir);
    }
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
) -> Result<(), LowerError> {
    if let Some(f) = functions.iter().find(|f| recurses_on_a_heap_arg(f)) {
        return Err(LowerError::Unsupported(format!(
            "native: fn `{}` passes a heap value to its own tail call — no loop conversion on this render (#3550)",
            f.name
        )));
    }
    rewrite_native_result_windows_over(
        ir.functions.iter().chain(linked.iter().flat_map(|l| l.ir.functions.iter())),
        functions,
    );
    rewrite_native_result_str_windows(ir, functions);
    rewrite_native_option_windows(ir, functions);
    crate::native_slot_rewrite::rewrite_slot_ops(functions, crate::lower::mutable_global_slot_count());
    // Narrow-width arithmetic computes in its declared integer type (#4006).
    for f in functions.iter_mut() {
        crate::native_narrow_rewrite::rewrite_narrow_ops(f)?;
    }
    call_slot_init_from_main(functions);
    if !functions.iter().any(|f| f.name == "main") {
        return Err(LowerError::Unsupported(
            "native: main is outside the MIR-lowering subset".into(),
        ));
    }
    Ok(())
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
    let mut sigs = build_native_sig_table(ir, functions, record_layouts, variant_layouts)?;
    for l in linked {
        for (name, sig) in build_native_sig_table(&l.ir, &[], &l.record_layouts, &l.variant_layouts)? {
            if functions.iter().any(|f| f.name == name) {
                sigs.entry(name).or_insert(sig);
            }
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
fn native_ownership_certs(functions: &[crate::MirFunction]) -> Vec<NativeCert> {
    let mut certs: Vec<NativeCert> = functions
        .iter()
        .map(|f| ("ownership".to_string(), f.name.to_string(), crate::certificate::ownership_certificate(f)))
        .collect();
    certs.push((
        "native-calls".to_string(),
        "program".to_string(),
        crate::certificate::native_calls_witness(functions),
    ));
    certs
}

/// The body of [`try_render_rust_source_certified`]: the native render, the
/// certificates of the MIR it rendered from, and that MIR.
fn render_native_parts(source: &str) -> Result<NativeRenderParts, LowerError> {
    dump_native_mir_if_requested(source);
    crate::charge_probe::reset_budget_used();
    crate::lambda_sigs::clear();
    let _strict = crate::lower::StrictValuesGuard::set(true);
    // The native render loop-converts a self tail call over any heap carry
    // (#4006 class A, #3550): `lower::owned_tco_slots`.
    let _owned_tco = crate::lower::OwnedTcoSlotsGuard::set(true);
    let ir = {
        let _slots = SlotHeldGlobalsGuard::set(true);
        source_to_ir_with(source, &[])?
    };
    crate::charge_probe::note_program_regions(almide_ir::meter_plan::program_has_regions(&ir));
    // #4447: the native rungs render no `@export(c)` wrapper — the wall
    // comes before the fan lift.
    wall_native_c_exports(&ir)?;
    // A window the standard codegen allocates in its region arena: that leg
    // runs it at a fraction of the ownership render's cost (#4006, C2 on C1).
    if let Some(consume) = almide_codegen::pass_region_window::discovered_window(&ir) {
        let span = ir.functions.iter().find(|f| f.name == consume).and_then(|f| f.body.span);
        return Err(LowerError::at(span, format!("native: `{consume}` opens a region window the standard codegen allocates in its arena")));
    }
    let ir = crate::fan_native::lift_fan_arms(ir)?;
    // Rung-5 records slab: the layout registries the wasm leg threads — without
    // them a record literal lowers as an Opaque skeleton and every field read
    // strict-walls (the probe_native trap recorded in the trust-spine ledger).
    let record_layouts = crate::lower::build_record_layouts(&ir.type_decls);
    let variant_layouts = crate::lower::build_variant_layouts(&ir.type_decls);
    // #4006 A5: the generated record structs, derived from the declared layouts.
    let _records = crate::render_native_rec::install_derived(
        &record_layouts,
        &variant_layouts,
        native_struct_tuples(&ir, &record_layouts, &variant_layouts),
    );
    if !ir.modules.is_empty() {
        return Err(LowerError::Unsupported(
            "native: multi-module program — outside rung 1".into(),
        ));
    }
    let mut ir = ir;
    prepare_native_top_lets(&mut ir);
    let mut functions = lower_native_functions(&ir, &record_layouts, &variant_layouts)?;
    // #4006: the registry bodies the program calls, linked as certified MIR.
    let linked = link_native_self_host(&mut functions, &ir)?;
    rewrite_native_linked_mir(&ir, &linked, &mut functions)?;
    let mut sigs = native_sigs_with_linked(&ir, &functions, &linked, &record_layouts, &variant_layouts)?;
    insert_native_probe_charges(&ir, &mut functions, &mut sigs);
    // #824: see the wasm leg's call above — `Op::MakeUnique` already renders to
    // nothing on native (render_native.rs's `Op::Consume | Op::Borrow |
    // Op::MakeUnique => {}`), so this is a no-op cleanup here, kept only so both
    // legs run the identical target-agnostic MIR pass list.
    crate::alias_safety::elide_unaliased_make_unique(&mut functions);
    // #4006 class P: a heap param its body takes over becomes a move param,
    // and every call hands the argument over at the call.
    crate::native_move_params::rewrite(&mut functions);
    let mut certs = native_ownership_certs(&functions);
    let program = MirProgram {
        functions,
        exports: Vec::new(),
        // Rung 1 walls every top-let above, so there are no mutable-global slots.
        mutable_global_count: 0,
    };
    let rust = crate::render_native::try_render_native_program(&program, &sigs)?;
    // Row 4: each function's bounds witness, recorded as the render wrote its
    // element accesses (render_native_bounds).
    for (name, w) in crate::render_native_bounds::take() {
        certs.push(("bounds".to_string(), name, w));
    }
    Ok((rust, certs, program.functions))
}
