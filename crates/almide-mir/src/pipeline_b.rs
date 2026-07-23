
/// The rest of the pipeline after [`build_ir_with_drops`]: collect globals/layouts,
/// lower every fn to MIR (main + linked module siblings), synthesize the global-init
/// and self-host-runtime auto-link fns, then render the final wasm module.
/// The four lookup tables [`collect_pipeline_layouts`] builds: globals (both the shared
/// program+modules union and the MAIN-region-bridged view), and the record/variant
/// layout registries — everything the fn-lowering calls below consult by reference.
struct PipelineLayouts {
    globals: HashMap<almide_ir::VarId, almide_lang::types::Ty>,
    global_inits: HashMap<almide_ir::VarId, almide_ir::IrExpr>,
    main_globals: HashMap<almide_ir::VarId, almide_lang::types::Ty>,
    main_global_inits: HashMap<almide_ir::VarId, almide_ir::IrExpr>,
    mutable_toplet_aliases: std::collections::HashMap<almide_ir::VarId, almide_ir::VarId>,
    record_layouts: crate::lower::RecordLayouts,
    variant_layouts: crate::lower::VariantLayouts,
}

/// Phase 2: collect top-level `let` globals (VarId -> Ty) + their INITIALIZER exprs
/// (union of program + modules), bridge the MAIN-region view across cross-module
/// references, and build the record/variant layout registries (aliasing each
/// UNIQUELY-owned base name onto its qualified layout).
fn collect_pipeline_layouts(ir: &almide_ir::IrProgram) -> PipelineLayouts {
    // Sequential-phase split (codopsy8 complexity sweep): the 4 phases below each build
    // ONE independent table (globals, then main-region globals — reads phase 1's finished
    // tables, then record layouts, then variant layouts) — a pure text-move of the
    // original top-to-bottom structure, no logic change.
    let (globals, global_inits) = collect_pipeline_globals(ir);
    let (main_globals, main_global_inits, mutable_toplet_aliases) =
        collect_pipeline_main_globals(ir, &globals, &global_inits);
    let record_layouts = collect_pipeline_record_layouts(ir);
    let variant_layouts = collect_pipeline_variant_layouts(ir);

    PipelineLayouts {
        globals,
        global_inits,
        main_globals,
        main_global_inits,
        mutable_toplet_aliases,
        record_layouts,
        variant_layouts,
    }
}

/// Extracted from `collect_pipeline_layouts` (codopsy8 complexity sweep): an UNANNOTATED
/// option-ctor top-let (`let MAYBE = some(Cfg { .. })`) leaves `tl.ty` Unknown(-payload) —
/// refine it from the ctor's payload type so the reference site materializes a REAL
/// tracked Option (see [`crate::lower::refine_option_toplet_ty`]; the same repair the
/// crossmod bridge applies). Shared by phases 1 and 2. Verbatim.
fn collect_pipeline_toplet_ty(tl: &almide_ir::IrTopLet) -> almide_lang::types::Ty {
    crate::lower::refine_option_toplet_ty(&tl.ty, &tl.value).unwrap_or_else(|| tl.ty.clone())
}

/// Extracted from `collect_pipeline_layouts` (codopsy8 complexity sweep, phase 1 of 4):
/// the shared globals union (main program + every module's top-lets, module entries win a
/// VarId collision — the pre-existing per-region behavior). Verbatim.
fn collect_pipeline_globals(
    ir: &almide_ir::IrProgram,
) -> (HashMap<almide_ir::VarId, almide_lang::types::Ty>, HashMap<almide_ir::VarId, almide_ir::IrExpr>) {
    let mut globals: HashMap<almide_ir::VarId, almide_lang::types::Ty> = HashMap::new();
    let mut global_inits: HashMap<almide_ir::VarId, almide_ir::IrExpr> = HashMap::new();
    for tl in &ir.top_lets {
        globals.insert(tl.var, collect_pipeline_toplet_ty(tl));
        global_inits.insert(tl.var, tl.value.clone());
    }
    for m in &ir.modules {
        for tl in &m.top_lets {
            globals.insert(tl.var, collect_pipeline_toplet_ty(tl));
            global_inits.insert(tl.var, tl.value.clone());
        }
    }
    (globals, global_inits)
}

/// Extracted from `collect_pipeline_layouts` (codopsy8 complexity sweep, phase 2 of 4):
/// PER-REGION globals — the shared union from phase 1 keys BOTH the main program's and
/// each module's top-let VarIds — two PRIVATE numbering regions that can COLLIDE (main-side
/// VarId(2) vs a module's VarId(2) are unrelated). MAIN functions must resolve through
/// main's own entries first (re-inserted last, winning collisions) plus the cross-module
/// NAME bridge (`toplib.SYSTEM` referenced through a main-side id); MODULE functions keep
/// the module-entries-win union (their region, as today). Verbatim.
fn collect_pipeline_main_globals(
    ir: &almide_ir::IrProgram,
    globals: &HashMap<almide_ir::VarId, almide_lang::types::Ty>,
    global_inits: &HashMap<almide_ir::VarId, almide_ir::IrExpr>,
) -> (
    HashMap<almide_ir::VarId, almide_lang::types::Ty>,
    HashMap<almide_ir::VarId, almide_ir::IrExpr>,
    std::collections::HashMap<almide_ir::VarId, almide_ir::VarId>,
) {
    let mut main_globals = globals.clone();
    let mut main_global_inits = global_inits.clone();
    let mut mutable_toplet_aliases: std::collections::HashMap<almide_ir::VarId, almide_ir::VarId> =
        std::collections::HashMap::new();
    crate::lower::bridge_cross_module_toplets(ir, &mut main_globals, &mut main_global_inits, &mut mutable_toplet_aliases);
    for tl in &ir.top_lets {
        main_globals.insert(tl.var, collect_pipeline_toplet_ty(tl));
        main_global_inits.insert(tl.var, tl.value.clone());
    }
    (main_globals, main_global_inits, mutable_toplet_aliases)
}

/// Extracted from `collect_pipeline_layouts` (codopsy8 complexity sweep, phase 3 of 4):
/// the record-layout registry (type name → fields) for the VALUE MODEL, aliasing each
/// UNIQUELY-owned base name to its qualified layout (a bare `Named` reference to a module
/// record must resolve its field layout); an ambiguous base stays qualified-only. Verbatim.
fn collect_pipeline_record_layouts(ir: &almide_ir::IrProgram) -> crate::lower::RecordLayouts {
    let mut record_layouts = crate::lower::build_record_layouts(&ir.type_decls);
    for m in &ir.modules {
        record_layouts.extend(crate::lower::build_record_layouts(&m.type_decls));
    }
    let mut owners: std::collections::HashMap<String, Vec<String>> = Default::default();
    for k in record_layouts.keys() {
        if let Some((_, base)) = k.rsplit_once('.') {
            owners.entry(base.to_string()).or_default().push(k.clone());
        }
    }
    for (base, ks) in owners {
        if ks.len() == 1 && !record_layouts.contains_key(&base) {
            let v = record_layouts.get(&ks[0]).cloned().expect("ks[0] came from record_layouts.keys() above, so the key is guaranteed present");
            record_layouts.insert(base, v);
        }
    }
    record_layouts
}

/// Extracted from `collect_pipeline_layouts` (codopsy8 complexity sweep, phase 4 of 4):
/// the variant-layout registry (type name → tag + per-constructor fields) for custom ADTs,
/// aliased the SAME way as [`collect_pipeline_record_layouts`]. Verbatim.
fn collect_pipeline_variant_layouts(ir: &almide_ir::IrProgram) -> crate::lower::VariantLayouts {
    let mut variant_layouts = crate::lower::build_variant_layouts(&ir.type_decls);
    for m in &ir.modules {
        let m_vl = crate::lower::build_variant_layouts(&m.type_decls);
        variant_layouts.by_type.extend(m_vl.by_type);
        variant_layouts.ctor_to_type.extend(m_vl.ctor_to_type);
        variant_layouts.ctor_field_defaults.extend(m_vl.ctor_field_defaults);
    }
    let mut owners: std::collections::HashMap<String, Vec<String>> = Default::default();
    for k in variant_layouts.by_type.keys() {
        if let Some((_, base)) = k.rsplit_once('.') {
            owners.entry(base.to_string()).or_default().push(k.clone());
        }
    }
    for (base, ks) in owners {
        if ks.len() == 1 && !variant_layouts.by_type.contains_key(&base) {
            let v = variant_layouts.by_type.get(&ks[0]).cloned().expect("ks[0] came from variant_layouts.by_type.keys() above, so the key is guaranteed present");
            variant_layouts.by_type.insert(base, v);
        }
    }
    variant_layouts
}

/// The [`inline_and_classify_cross_module_fns`] outputs: the mangled linked-module
/// sibling fns, MAIN's tail-inlined fns, and the union of both (the whole-program set
/// the ABI-registry classification needs).
struct CrossModuleFns {
    module_fn_sibs: Vec<almide_ir::IrFunction>,
    inlined_fns: Vec<almide_ir::IrFunction>,
    all_fns: Vec<almide_ir::IrFunction>,
}

/// Phase 3: PROGRAM pre-pass — inline mutual-recursive tail siblings (semantics-preserving
/// TCO exposure). The input is the WHOLE program — main's functions PLUS every linked
/// user-module sibling under its MANGLED `almide_rt_<m>_<f>` name (bodies already
/// reference siblings by that name, post-`resolve_user_module_calls`). Without the
/// siblings, the never-err/auto-wrap ABI registries were populated from MAIN's functions
/// only: a cross-module effect callee (`m.estep`) was UNCLASSIFIED, so the caller kept
/// its auto-`?` Try (expecting a heap Result handle) while the separately-lowered callee
/// returned its raw scalar — the crossmod_shape_matrix i64/i32 invalid-wasm class. One
/// combined classification makes caller and callee agree by construction; the returned
/// rewritten bodies are then split back into the main / module lowering regions (each
/// keeps its own globals union) — iterated to a FIXPOINT (the #485 effect_assign shape).
fn inline_and_classify_cross_module_fns(
    ir: &almide_ir::IrProgram,
    main_globals: &HashMap<almide_ir::VarId, almide_lang::types::Ty>,
    record_layouts: &crate::lower::RecordLayouts,
) -> CrossModuleFns {
    let mut module_fn_sibs: Vec<almide_ir::IrFunction> = ir
        .modules
        .iter()
        .filter(|m| !almide_lang::stdlib_info::is_any_stdlib(m.name.as_str()))
        .flat_map(|m| {
            let mname = m.name.as_str().to_string();
            // INTRA-MODULE bare sibling calls resolve MODULE-LOCALLY (the #692 rule:
            // current-module qualified > bare > any-module) — a clone body left with a
            // bare `route(x, 100)` linked MAIN's same-named 0-arg `route` and shipped
            // invalid wasm as "v1-verified" (values remaining on stack at the callee's
            // arity mismatch — wasm_same_name_crossmod_test).
            let sibs: std::collections::HashSet<String> =
                m.functions.iter().map(|f| f.name.as_str().to_string()).collect();
            m.functions.iter().filter(|f| !f.is_test).map(move |f| {
                let mut nf = f.clone();
                nf.name = almide_lang::intern::sym(&user_module_fn_name(&mname, f.name.as_str()));
                struct Rw<'a> {
                    mname: &'a str,
                    sibs: &'a std::collections::HashSet<String>,
                }
                impl almide_ir::IrMutVisitor for Rw<'_> {
                    fn visit_expr_mut(&mut self, e: &mut almide_ir::IrExpr) {
                        almide_ir::walk_expr_mut(self, e);
                        if let almide_ir::IrExprKind::Call { target, .. } = &mut e.kind {
                            if let almide_ir::CallTarget::Named { name } = target {
                                let f = name.as_str();
                                if !f.starts_with("almide_rt_") && self.sibs.contains(f) {
                                    *target = almide_ir::CallTarget::Named {
                                        name: almide_lang::intern::sym(&user_module_fn_name(
                                            self.mname, f,
                                        )),
                                    };
                                }
                            }
                        }
                    }
                }
                let mut rw = Rw { mname: &mname, sibs: &sibs };
                almide_ir::IrMutVisitor::visit_expr_mut(&mut rw, &mut nf.body);
                nf
            })
        })
        .collect();
    let mut all_fns: Vec<almide_ir::IrFunction> = ir.functions.clone();
    all_fns.extend(module_fn_sibs.iter().cloned());
    // The combined run POPULATES the name-keyed ABI registries over the WHOLE program
    // (that is the crossmod fix — caller and callee classify identically); only MAIN's
    // rewritten bodies are kept. The module siblings lower below from their ORIGINAL
    // bodies: feeding the pre-pass's REWRITTEN module bodies through the module loop
    // regressed the intra-module tail-call shape (`route(x, 100)` left values on the
    // wasm stack — wasm_same_name_crossmod_test), and the registries alone are what the
    // module-side lowering consults by MANGLED name.
    let mut inlined_fns =
        crate::lower::inline_mutual_tail_recursion(&ir.functions, main_globals, record_layouts);
    // WIDEN the ABI registries over the whole program AFTER the main pre-pass (whose own
    // population is main-only, the pre-batch behavior its rewrites were verified under):
    // every LOWERING-time keyed lookup (never-err strip exclusions, AUTO_WRAP body.ty
    // override, `ret_is_result_abi`) then sees module callees by their mangled names —
    // the crossmod caller/callee ABI agreement — without the pre-pass rewrites ever
    // touching module bodies.
    crate::lower::populate_abi_registries(&all_fns, record_layouts);
    // The registries above are program-wide, but the never-err REWRITES ran with MAIN-ONLY
    // sets (inside the pre-pass) and never touched module bodies at all. So a MAIN caller of
    // a cross-module never-err callee kept its lifted `Try`/Result-typed call, and a MODULE
    // sibling caller kept its own — both then lower Result-handle reads (`local.set`) over
    // the raw/void ABI the callee's def actually has: the #786 invalid-wasm class. Re-run
    // the never-err rewrite family with the PROGRAM-WIDE sets over BOTH regions, so every
    // caller agrees with the combined classification by construction (idempotent where the
    // main-only pass already fired; call TARGETS are never renamed, so the module-side name
    // resolution the original-bodies rule protects is untouched).
    //
    // FIXPOINT (the #485 effect_assign shape): the strips consult AUTO_WRAP (an
    // auto-wrapped callee's `!`/`??` must NOT strip) while AUTO_WRAP itself is derived
    // from the bodies (has a stmt-position propagating unwrap). A strip can remove a
    // callee's LAST propagating unwrap (`plain_assign`'s `x = step(x)` Try), after which
    // its REAL lowered ABI is bare — but the stale registry still said "wrapped", so its
    // own def lowered bare while every `plain_assign()!` site kept the Result-handle
    // read: invalid wasm (def/callsite ABI split). Iterate populate → rewrite until the
    // registries describe the rewritten bodies verbatim (monotone — strips only remove
    // nodes, AUTO_WRAP only shrinks — so this terminates; the cap is a safety net).
    let mut prev_auto_wrap: Option<std::collections::HashSet<String>> = None;
    for _ in 0..8 {
        let mut all_rewritten: Vec<almide_ir::IrFunction> = inlined_fns.clone();
        all_rewritten.extend(module_fn_sibs.iter().cloned());
        crate::lower::populate_abi_registries(&all_rewritten, record_layouts);
        let cur = crate::lower::auto_wrap_abi_snapshot();
        if prev_auto_wrap.as_ref() == Some(&cur) {
            break;
        }
        prev_auto_wrap = Some(cur);
        let wide_can_err = crate::lower::compute_can_err(&all_rewritten);
        let wide_lifted = crate::lower::lifted_effect_fn_names(&all_rewritten);
        for f in inlined_fns.iter_mut().chain(module_fn_sibs.iter_mut()) {
            let self_name = f.name.as_str().to_string();
            crate::lower::strip_never_err_unwraps(
                &mut f.body,
                &wide_can_err,
                &wide_lifted,
                &self_name,
            );
            crate::lower::rewrite_never_err_effect_match(&mut f.body, &wide_can_err, &wide_lifted);
            crate::lower::unwrap_never_err_call_types(&mut f.body, &wide_can_err, &wide_lifted);
            crate::lower::rewrap_never_err_into_result_targets(
                &mut f.body,
                &wide_can_err,
                &wide_lifted,
                record_layouts,
            );
        }
    }
    CrossModuleFns { module_fn_sibs, inlined_fns, all_fns }
}

/// Phase 4: cross-module DERIVED-METHOD name bridge (#790 codec row, piece 2 of the
/// pinned design): a MAIN-region `T.encode` / `T.decode` reference whose type `T` is
/// declared by exactly ONE linked module (and not by main) resolves to that module's
/// MANGLED derived fn (`almide_rt_<m>_T.encode`) — the same unique-owner rule the
/// variant-layout bridging above uses. Without this the reference stays unlinked and
/// the whole program walls (honest, but the direct-method shapes are fully lowerable).
/// Container helpers (`__encode_list_<m>.T`) stay walled — their v1 lowering is the
/// recorded remainder of the bridge design. Rewrites `inlined_fns`/`module_fn_sibs`
/// bodies in place (both regions: main's derived fns reference the imported payload
/// type's codec methods, and the OWNING module's own derived fns reference their
/// sibling types' methods by the same bare `T.method` names).
fn bridge_cross_module_derived_methods(
    ir: &almide_ir::IrProgram,
    inlined_fns: &mut [almide_ir::IrFunction],
    module_fn_sibs: &mut [almide_ir::IrFunction],
) {
    let main_types: std::collections::HashSet<&str> =
        ir.type_decls.iter().map(|td| td.name.as_str()).collect();
    let mut owners: std::collections::HashMap<&str, Vec<&str>> = std::collections::HashMap::new();
    for m in &ir.modules {
        if almide_lang::stdlib_info::is_any_stdlib(m.name.as_str()) {
            continue;
        }
        for td in &m.type_decls {
            // Module type names may arrive QUALIFIED (`varlib.Pigment`) — key the
            // owner map by the BASE name (the same normalization the variant-layout
            // bridging above applies).
            let base = td.name.as_str().rsplit('.').next().unwrap_or(td.name.as_str());
            owners.entry(base).or_default().push(m.name.as_str());
        }
    }
    struct Rw<'a> {
        main_types: &'a std::collections::HashSet<&'a str>,
        owners: &'a std::collections::HashMap<&'a str, Vec<&'a str>>,
    }
    impl almide_ir::IrMutVisitor for Rw<'_> {
        fn visit_expr_mut(&mut self, e: &mut almide_ir::IrExpr) {
            almide_ir::walk_expr_mut(self, e);
            if let almide_ir::IrExprKind::Call {
                target: almide_ir::CallTarget::Named { name },
                ..
            } = &mut e.kind
            {
                let n = name.as_str();
                if n.starts_with("almide_rt_") || n.starts_with("__") {
                    return;
                }
                let Some((ty_name, method)) = n.rsplit_once('.') else { return };
                if method != "encode" && method != "decode" {
                    return;
                }
                // `varlib.Pigment.decode` → qualifier "varlib" + base "Pigment";
                // `Pigment.decode` → base only. A qualified ref must match the
                // owner; a bare ref must not shadow a MAIN type of the same name.
                let (qualifier, base) = match ty_name.rsplit_once('.') {
                    Some((q, b)) => (Some(q), b),
                    None => (None, ty_name),
                };
                if qualifier.is_none() && self.main_types.contains(base) {
                    return;
                }
                if let Some(ms) = self.owners.get(base) {
                    if let [only] = ms.as_slice() {
                        if qualifier.is_none() || qualifier == Some(only) {
                            *name = almide_lang::intern::sym(&user_module_fn_name(
                                only,
                                &format!("{base}.{method}"),
                            ));
                        }
                    }
                }
            }
        }
    }
    let mut rw = Rw { main_types: &main_types, owners: &owners };
    for f in inlined_fns.iter_mut().chain(module_fn_sibs.iter_mut()) {
        almide_ir::IrMutVisitor::visit_expr_mut(&mut rw, &mut f.body);
    }
    // …and publish the unique-owner map for the DESUGAR-time resolution: the
    // `T.method` Named names are FORMED inside the per-fn lowering (from Method
    // targets), after this pipeline pass — the registry is how they see it.
    let derived_owners: std::collections::HashMap<String, String> = owners
        .iter()
        .filter(|(t, ms)| ms.len() == 1 && !main_types.contains(*t))
        .map(|(t, ms)| (t.to_string(), ms[0].to_string()))
        .collect();
    crate::lower::set_derived_type_owners(derived_owners);
}

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
    for (id, init) in &subs {
        // MAIN-region readers take the BIND form instead of the raw expression
        // substitution: `let __g_init = default_gap(); …Var(__g_init)…` — a call
        // spliced into an arbitrary position (a record FIELD — the #785 shape)
        // rendered as a dst-less bare call (invalid wasm), while a call in BIND
        // position plus a Var reference is the proven single-file form. The bind
        // goes at the fn-body top (the init is pure, so hoisting its evaluation
        // is unobservable); fns that never reference the global are untouched.
        for f in inlined_fns.iter_mut() {
            fn references(e: &almide_ir::IrExpr, id: almide_ir::VarId) -> bool {
                use almide_ir::visit::{walk_expr, IrVisitor};
                struct V(almide_ir::VarId, bool);
                impl IrVisitor for V {
                    fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
                        if matches!(&e.kind, almide_ir::IrExprKind::Var { id } if *id == self.0)
                        {
                            self.1 = true;
                        }
                        walk_expr(self, e);
                    }
                }
                let mut v = V(id, false);
                v.visit_expr(e);
                v.1
            }
            if !references(&f.body, *id) {
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
            f.body = almide_ir::substitute::substitute_var_in_expr(&f.body, *id, &nv_ref);
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

/// Phase 7: lower every non-test MAIN fn to MIR (a fn that walls is silently skipped,
/// listed to stderr under `verbose`), then lower every linked USER-module sibling the
/// target's resolved `almide_rt_<m>_<f>` references — under the SAME mangled name, so
/// every keyed lookup (never-err strip, AUTO_WRAP ABI, `ret_is_result_abi`) sees what
/// callers use via the combined registry population. Each module fn lowers SEPARATELY
/// (its own VarId region + shared globals); one already defined (from `inlined_fns`,
/// the main-region tail-inlining pass) or one that itself walls is silently skipped
/// (the caller then fails the unlinked-call render wall if it truly needed it — stdlib
/// modules stay out, self-host-linked below).
fn lower_main_and_sibling_fns(
    inlined_fns: &[almide_ir::IrFunction],
    module_fn_sibs: &[almide_ir::IrFunction],
    layouts: &PipelineLayouts,
    total_ir_fn_count: usize,
    verbose: bool,
) -> Vec<crate::MirFunction> {
    let mut functions = Vec::new();
    let mut walled = Vec::new();
    for func in inlined_fns {
        // `test "…"` blocks lower to fns calling the test harness (no wasm def) — never reachable
        // from `_start`/`main`, so skip them (rendering one would pull a dangling `(call $assert_eq)`).
        if func.is_test {
            continue;
        }
        match crate::lower::lower_function_all_with_globals(
            func,
            &layouts.main_globals,
            &layouts.main_global_inits,
            &layouts.record_layouts,
            &layouts.variant_layouts,
        ) {
            Ok(mirs) => functions.extend(mirs),
            Err(e) => walled.push(format!("{}: {e:?}", func.name.as_str())),
        }
    }
    if !walled.is_empty() && verbose {
        eprintln!(
            "[render_program] {} of {} function(s) outside the lowering subset (NOT rendered):",
            walled.len(),
            total_ir_fn_count
        );
        for w in &walled {
            eprintln!("  {w}");
        }
    }

    let already: std::collections::HashSet<String> =
        functions.iter().map(|f| f.name.clone()).collect();
    for func in module_fn_sibs {
        // ORIGINAL bodies under the mangled name — every keyed lookup (never-err strip,
        // AUTO_WRAP ABI, `ret_is_result_abi`) sees the SAME name callers use via the
        // combined registry population above.
        if already.contains(func.name.as_str()) {
            continue;
        }
        if let Ok(mirs) = crate::lower::lower_function_all_with_globals(
            func,
            &layouts.globals,
            &layouts.global_inits,
            &layouts.record_layouts,
            &layouts.variant_layouts,
        ) {
            functions.extend(mirs);
        }
    }
    functions
}
