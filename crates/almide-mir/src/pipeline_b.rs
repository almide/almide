
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
    /// Per-module globals for the MODULE-region lowering: the shared union plus that
    /// module's own cross-module top-let references, bridged by name (#904). Keyed by
    /// module name; a module with no such reference is absent and falls back to the
    /// shared union. Two modules' reference ids can COLLIDE numerically (each region
    /// numbers from 0), so this must stay per-module — one merged map would let one
    /// module's `v.ROW` resolve another's unrelated id.
    module_globals: HashMap<String, (HashMap<almide_ir::VarId, almide_lang::types::Ty>, HashMap<almide_ir::VarId, almide_ir::IrExpr>)>,
    /// Mangled sibling fn name → owning module name, so the lowering picks that
    /// module's `module_globals` entry.
    fn_module: HashMap<String, String>,
    mutable_toplet_aliases: std::collections::HashMap<almide_ir::VarId, almide_ir::VarId>,
    record_layouts: crate::lower::RecordLayouts,
    variant_layouts: crate::lower::VariantLayouts,
}

/// Rewrite every occurrence of the mapped VarIds in one region's exprs/stmts.
/// A mutable module-level var is never REBOUND inside its region's fns (VarIds
/// are unique within a region), so every occurrence of a mapped id IS the
/// global — use sites only (`Var`, assign/insert targets); `Bind` binds fresh
/// locals and is deliberately not remapped.
struct MutGlobalIdRw<'a> {
    map: &'a std::collections::HashMap<almide_ir::VarId, almide_ir::VarId>,
}

impl almide_ir::IrMutVisitor for MutGlobalIdRw<'_> {
    fn visit_expr_mut(&mut self, expr: &mut almide_ir::IrExpr) {
        if let almide_ir::IrExprKind::Var { id } = &mut expr.kind {
            if let Some(n) = self.map.get(id) {
                *id = *n;
            }
        }
        almide_ir::visit_mut::walk_expr_mut(self, expr);
    }
    fn visit_stmt_mut(&mut self, stmt: &mut almide_ir::IrStmt) {
        use almide_ir::IrStmtKind as K;
        let target = match &mut stmt.kind {
            K::Assign { var, .. } => Some(var),
            K::IndexAssign { target, .. }
            | K::MapInsert { target, .. }
            | K::FieldAssign { target, .. } => Some(target),
            _ => None,
        };
        if let Some(t) = target {
            if let Some(n) = self.map.get(t) {
                *t = *n;
            }
        }
        almide_ir::visit_mut::walk_stmt_mut(self, stmt);
    }
}

/// Make ALL module-level top-let ids unique ACROSS regions (#881). Every unit
/// numbers its VarIds from 0 — the main program and each module are PRIVATE
/// numbering regions — but both the mutable-global slot map AND the shared
/// globals/global-inits union ([`collect_pipeline_globals`]) are keyed by the
/// RAW id, so `var cached_scene` (main, VarId 0) and `var _dirty` (a module,
/// VarId 0) collided and the program walled — and an IMMUTABLE collision was
/// worse: `let FRICTION = 0.035` (scroll) silently WON another module's
/// same-numbered toplet in the union ("later module wins"), so that module's
/// reader materialized 0.035 where a `view.Color` lived (a silent wrong value
/// whenever the types happened to agree). Remap EVERY module top-let id
/// (mutable and immutable alike) to fresh ids ABOVE every region's var-table
/// length (disjoint from every real id by construction), rewriting the
/// declaration and every use inside that module's own fns and top-let inits —
/// and EXTEND the module's var table so the new id still indexes the var's
/// info (the cross-module NAME bridge looks the top-let's name/mutability up
/// BY INDEX; without the extension the bridge went blind and every
/// cross-module mutable reference became an unbound var). Cross-module
/// references resolve BY NAME afterwards (`bridge_cross_module_toplets`), so
/// they see the remapped ids automatically.
pub(crate) fn disambiguate_module_global_regions(ir: &mut almide_ir::IrProgram) {
    let mut next: u32 = std::iter::once(ir.var_table.entries.len())
        .chain(ir.modules.iter().map(|m| m.var_table.entries.len()))
        .max()
        .unwrap_or(0) as u32;
    for m in &mut ir.modules {
        let mut map: std::collections::HashMap<almide_ir::VarId, almide_ir::VarId> =
            std::collections::HashMap::new();
        for tl in &mut m.top_lets {
            let old = tl.var;
            let fresh = almide_ir::VarId(next);
            next += 1;
            // Keep the by-index var-table lookup alive for the fresh id: pad
            // with clones up to the new index (the pad entries are never
            // indexed — only the fresh id is) and place the var's own info
            // there. A top-let whose old id had no entry keeps having none.
            if let Some(info) = m.var_table.entries.get(old.0 as usize).cloned() {
                while m.var_table.entries.len() < fresh.0 as usize {
                    m.var_table.entries.push(info.clone());
                }
                m.var_table.entries.push(info);
            }
            map.insert(old, fresh);
            tl.var = fresh;
        }
        if map.is_empty() {
            continue;
        }
        let mut rw = MutGlobalIdRw { map: &map };
        for f in &mut m.functions {
            almide_ir::IrMutVisitor::visit_expr_mut(&mut rw, &mut f.body);
        }
        for tl in &mut m.top_lets {
            almide_ir::IrMutVisitor::visit_expr_mut(&mut rw, &mut tl.value);
        }
    }
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
    let (module_globals, fn_module) = collect_pipeline_module_globals(ir, &globals, &global_inits);
    let record_layouts = collect_pipeline_record_layouts(ir);
    let variant_layouts = collect_pipeline_variant_layouts(ir);

    PipelineLayouts {
        globals,
        global_inits,
        main_globals,
        main_global_inits,
        module_globals,
        fn_module,
        mutable_toplet_aliases,
        record_layouts,
        variant_layouts,
    }
}

/// Extracted from `collect_pipeline_layouts`: the MODULE-region globals — the shared
/// union PLUS each module's own `mod.NAME` references bridged by name (see
/// [`crate::lower::module_region_toplet_bridges`]). A module whose bridge adds nothing
/// is left out of the map entirely so its fns keep using the shared union verbatim (no
/// clone, no behavior change). The second return is the mangled-fn-name → module-name
/// index the sibling lowering uses to pick the right map — the mangling
/// (`user_module_fn_name`) is the same one `inline_and_classify_cross_module_fns`
/// applies, and call targets are never renamed after it, so the key is stable.
#[allow(clippy::type_complexity)]
fn collect_pipeline_module_globals(
    ir: &almide_ir::IrProgram,
    globals: &HashMap<almide_ir::VarId, almide_lang::types::Ty>,
    global_inits: &HashMap<almide_ir::VarId, almide_ir::IrExpr>,
) -> (
    HashMap<String, (HashMap<almide_ir::VarId, almide_lang::types::Ty>, HashMap<almide_ir::VarId, almide_ir::IrExpr>)>,
    HashMap<String, String>,
) {
    let bridges = crate::lower::module_region_toplet_bridges(ir, globals);
    let mut module_globals = HashMap::new();
    let mut fn_module = HashMap::new();
    for m in &ir.modules {
        let mname = m.name.as_str().to_string();
        let Some((add_g, add_gi)) = bridges.get(&mname) else { continue };
        let mut g = globals.clone();
        let mut gi = global_inits.clone();
        g.extend(add_g.iter().map(|(k, v)| (*k, v.clone())));
        gi.extend(add_gi.iter().map(|(k, v)| (*k, v.clone())));
        for f in &m.functions {
            fn_module.insert(user_module_fn_name(&mname, f.name.as_str()), mname.clone());
        }
        module_globals.insert(mname, (g, gi));
    }
    (module_globals, fn_module)
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
        // An intrinsic-bearing bundled module contributes only its pure-Almide
        // extensions (`list.split_at`); its intrinsic-backed fns stay on the
        // registry. `linkable_module_fns` is the single source of truth shared
        // with the call-site rewrite, so a linked CALL always has a linked DEF.
        .map(|m| (m, crate::pipeline::linkable_module_fns(m)))
        .filter(|(_, fns)| !fns.is_empty())
        .flat_map(|(m, linkable)| {
            let mname = m.name.as_str().to_string();
            // INTRA-MODULE bare sibling calls resolve MODULE-LOCALLY (the #692 rule:
            // current-module qualified > bare > any-module) — a clone body left with a
            // bare `route(x, 100)` linked MAIN's same-named 0-arg `route` and shipped
            // invalid wasm as "v1-verified" (values remaining on stack at the callee's
            // arity mismatch — wasm_same_name_crossmod_test).
            let sibs: std::collections::HashSet<String> =
                m.functions.iter().map(|f| f.name.as_str().to_string()).collect();
            m.functions
                .iter()
                .filter(move |f| !f.is_test && linkable.contains(f.name.as_str()))
                .map(move |f| {
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
        // Declared param types per fn — the rewrap pass's call-argument targets.
        let param_sigs: std::collections::HashMap<String, Vec<almide_lang::types::Ty>> =
            all_rewritten
                .iter()
                .map(|f| {
                    (f.name.as_str().to_string(), f.params.iter().map(|p| p.ty.clone()).collect())
                })
                .collect();
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
                &param_sigs,
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
        if !crate::pipeline::is_linkable_module(m) {
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


/// Phase 7: lower every non-test MAIN fn to MIR (a fn that walls is silently skipped,
/// listed to stderr under `verbose`), then lower every linked USER-module sibling the
/// target's resolved `almide_rt_<m>_<f>` references — under the SAME mangled name, so
/// every keyed lookup (never-err strip, AUTO_WRAP ABI, `ret_is_result_abi`) sees what
/// callers use via the combined registry population. Each module fn lowers SEPARATELY
/// (its own VarId region + shared globals); one already defined (from `inlined_fns`,
/// the main-region tail-inlining pass) or one that itself walls is silently skipped
/// (the caller then fails the unlinked-call render wall if it truly needed it — stdlib
/// modules stay out, self-host-linked below).
///
/// `fn_walls` collects EVERY walled fn's own reason by name — not just `main`'s. The
/// exported-`pub fn` decline one layer up reports the ENCLOSING construct, and without
/// this map it had no inner cause to name; a reader then saw "the wasm module must carry
/// its export" for a wall that was really a receiver-shape decline one level down, which
/// is the exact mis-attribution that cost hours on #904 (#906).
fn lower_main_and_sibling_fns(
    inlined_fns: &[almide_ir::IrFunction],
    module_fn_sibs: &[almide_ir::IrFunction],
    layouts: &PipelineLayouts,
    total_ir_fn_count: usize,
    verbose: bool,
    fn_walls: &mut std::collections::HashMap<String, String>,
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
            Err(e) => {
                // Every walled fn's own reason is worth keeping, `main`'s most of
                // all: when `main` walls there is no `$main` and the whole module
                // declines, and reporting only the absence turned every distinct
                // cause into one unattributable bucket — a third of the fuzzer's
                // wall histogram (#812). An exported `pub fn` declines the module
                // the same way, so it reads its reason out of here too (#906).
                fn_walls.insert(func.name.as_str().to_string(), format!("{e:?}"));
                walled.push(format!("{}: {e:?}", func.name.as_str()));
            }
        }
    }
    if !walled.is_empty() && verbose {
        eprintln!(
            "[v1-wall] {} of {} function(s) outside the lowering subset (NOT rendered):",
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
        // The OWNING module's globals when its cross-module `mod.NAME` references were
        // bridged (#904), else the shared union. Per-module because two modules' synthesized
        // reference ids collide numerically — see `collect_pipeline_module_globals`.
        let (g, gi) = layouts
            .fn_module
            .get(func.name.as_str())
            .and_then(|m| layouts.module_globals.get(m))
            .map(|(g, gi)| (g, gi))
            .unwrap_or((&layouts.globals, &layouts.global_inits));
        match crate::lower::lower_function_all_with_globals(
            func,
            g,
            gi,
            &layouts.record_layouts,
            &layouts.variant_layouts,
        ) {
            Ok(mirs) => functions.extend(mirs),
            // A walled module sibling is NOT fatal here — an unreferenced one
            // simply isn't rendered; a referenced one is caught by the
            // unlinked-call gate. But it must not wall SILENTLY: the gate's
            // "no wasm definition" message is undiagnosable without the
            // sibling's own reason (#881 — eight aliased-signature fns walled
            // invisibly while their callers rendered).
            Err(e) if verbose => {
                eprintln!("[v1-wall] module sibling {}: {e:?}", func.name.as_str());
            }
            Err(_) => {}
        }
    }
    functions
}
