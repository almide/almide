// ── tail of pipeline.rs, include!-spliced back at module level ──
//
// A pure code move: this file continues its parent verbatim. The split exists
// only so the parent stays under the 800-line ceiling the codopsy gate holds
// this crate to; there is no boundary of meaning here, and `include!` at module
// level is the one splice Rust allows (an impl-item position rejects it).

fn build_ir_with_drops(
    source: &str,
    self_modules: &[(String, almide_lang::ast::Program, bool)],
    test_mode: bool,
) -> Result<almide_ir::IrProgram, LowerError> {
    // STRICT VALUE MODE is owned by the caller, not by this phase. Nothing between here
    // and the return reads `strict_values()`: this builds the linked IR, and the mode
    // gates MIR *op* lowering, which runs after. A guard here would look like the
    // protection and be none.
    let ir = source_to_ir_with(source, self_modules)?;
    // ADT brick 5b: GENERATE the recursive-drop fns (`__drop_<T>`) for nested-variant types and
    // re-lower with them in scope. v1-trust-spine-only — v0 manages its own memory. Two-pass.
    let anon_recs = crate::lower::collect_recursive_anon_records(&ir);
    let mut all_type_decls = ir.type_decls.clone();
    for m in &ir.modules {
        all_type_decls.extend(m.type_decls.iter().cloned());
    }
    // A GENERIC user variant instantiated with concrete args as a `List[<...>]` LITERAL element
    // type (`List[Either[Int,String]]`) needs a SHADOW `type <inst>` + `$__drop_<inst>`/
    // `$__drop_list_<inst>` generated for THIS SPECIFIC instantiation — the raw declaration
    // (`Either[L,R]`) carries unresolved type-parameter placeholders the drop generator can't
    // classify (see `is_rich_variant_ty`'s doc comment, mod_p2.rs). Built from a PRE-relower
    // `VariantLayouts` (this program's OWN declared generics/cases, before the drops text is
    // appended) so discovery sees the ORIGINAL `Left(1)`/`Right("y")` list literal. The shadow
    // type declaration text is prepended to `drops` below; the shadow `IrTypeDecl` is spliced
    // into `all_type_decls` so the SAME `generate_variant_drop_sources` call already below
    // covers it too (no separate/duplicate drop-generation call).
    let pre_relower_variant_layouts = crate::lower::build_variant_layouts(&all_type_decls);
    let generic_variant_list_insts = crate::lower::discover_generic_variant_list_instantiations(
        &ir,
        &pre_relower_variant_layouts,
    );
    let (generic_variant_type_decl_src, generic_variant_synthetic_decls) =
        crate::lower::generate_generic_variant_instantiation_type_decls(
            &generic_variant_list_insts,
            &pre_relower_variant_layouts,
        );
    // The SHADOW decls exist only to hang `$__drop_<inst>` on — they must NOT reach the
    // REPR generator: their synthetic ctor names (`C__Either_Int_String_0`) would render
    // into `${…}` output (native prints the REAL `Left(…)`), and their emitted
    // `__repr_<inst>` would collide with the instantiation-keyed repr of the same name
    // that the interp section generates from the REAL generic decl. Snapshot the
    // shadow-free list for the repr call below.
    let repr_type_decls = all_type_decls.clone();
    all_type_decls.extend(generic_variant_synthetic_decls);
    // ONE fused walk answers every injection-gate question below (#1232) —
    // the per-gate `program_uses_*` fns each traversed the whole IR.
    let usage = crate::lower::measure_program_usage(&ir, &all_type_decls);
    let uses_result_opt_str = usage.result_option_str;
    // Also consulted by the `list_str_drop` gate below.
    let uses_closures = usage.closures;
    // First-class function values need the UNIFORM closure-block release
    // (`$__drop_closure` — self-describing recursive drop, DropVariant "closure").
    let closure_drop = gated(uses_closures, crate::lower::CLOSURE_DROP_SRC);
    // A `List[<Fn>]` LITERAL (`[(x)=>x+1, (x)=>x*2]`) routes its scope-end drop to the
    // generated `$__drop_list_closure` (per-element `$__drop_closure` — required, not a
    // blind rc_dec, since a captured heap slot would otherwise leak). Needs
    // `CLOSURE_DROP_SRC` in scope, which `program_uses_closures` already guarantees
    // whenever a closure LIST exists (the list's elements are Lambda exprs).
    let list_closure_drop = gated(usage.closure_list, crate::lower::LIST_CLOSURE_DROP_SRC);
    // A `Map[String, <Fn>]` (the closure-valued map — mclo class) routes its scope-end
    // drop to `$__drop_map_mclo` (per-value `$__drop_closure` over the split layout).
    // Needs `CLOSURE_DROP_SRC` in scope, which `program_uses_closures` guarantees
    // whenever a closure-valued map exists (its values are Fn-typed exprs).
    let map_mclo_drop = gated(usage.map_closure, crate::lower::MAP_MCLO_DROP_SRC);
    // A `List[(String, <Fn>)]` pairs literal (the closure-valued map's from_list
    // input) routes its scope-end drop to `$__drop_list_str_clo` (per-tuple: key
    // rc_dec + `$__drop_closure` on the value slot).
    let list_str_clo_drop = gated(usage.str_clo_pairs, crate::lower::LIST_STR_CLO_DROP_SRC);
    // An `Option[(String, String)]` (the if-merged `some((s1, s2))` ctor) routes
    // its scope-end drop to `$__drop_opt_str_str`.
    let opt_str_str_drop = gated(usage.opt_str_str, crate::lower::OPT_STR_STR_DROP_SRC);
    // A `List[Option/Result]` literal with owned-handle-slot elements routes its drop to the
    // generated `$__drop_list_lenlist` (the shared `lenlist_elem_class` decides both sides).
    let lenlist_drop = gated(usage.lenlist_elem_lists, crate::lower::LENLIST_DROP_SRC);
    // `__drop_list_str` (a `List[String]` record OR variant ctor field, OR a closure's
    // nested-heap capture — `CLOSURE_DROP_SRC`'s `__drop_closure_loop` unconditionally
    // references it once ANY closure exists, since a capture's concrete type isn't known
    // at this gate without re-running `lift_lambda`'s own free-vars scan; conservatively
    // widened on `program_uses_closures` rather than precisely detecting a List[String]
    // capture — always correct, occasionally includes an unused routine) — SHARED between
    // the record and variant drop generators, so it is emitted ONCE here rather than by
    // either generator inline (two independent copies would be a duplicate-fn compile
    // error).
    let list_str_drop = gated(
        usage.list_str_drop_field || usage.anon_list_str_record || uses_closures,
        crate::lower::LIST_STR_DROP_SRC,
    );
    // `Result[List[Int], List[String]]` (result.collect) routes its drop to the
    // TAG-AWARE `$__drop_res_ilsl` (Err → recursive string free; Ok → flat).
    let res_ilsl_drop = gated(usage.res_intlist_strlist, crate::lower::RES_ILSL_DROP_SRC);
    // `Result[Map[String, <scalar>], String]` / its chunked List-of-maps sibling
    // (the fs.fold_lines msi twins) route their drops to the TAG-AWARE
    // `$__drop_res_msi` / `$__drop_res_lmsi` (Ok → the skv key sweep; Err → the
    // message). One gate injects both — `res_lmsi` shares `res_msi`'s key loop.
    let res_msi_drop = gated(usage.res_map_si, crate::lower::RES_MSI_DROP_SRC);
    let res_lmsi_drop = gated(usage.res_map_si, crate::lower::RES_LMSI_DROP_SRC);
    // An `Option[(String, <scalar>)]` (map.find's result, or a plain `some((s, n))` ctor)
    // routes its drop to the TAG-AWARE `$__drop_opt_str_int` (Some → recursive String-slot
    // free; None → nothing) — a blind flat `rc_dec` of the Option's payload slot would only
    // free the TUPLE's own refcount, leaking its String. Type-driven gate (#840): the old
    // `map.find` name-heuristic missed the literal-ctor producer and left the routed call
    // dangling in the WAT.
    let opt_str_int_drop = gated(usage.opt_str_scalar, crate::lower::OPT_STR_INT_DROP_SRC);
    let drops = format!(
        "{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}",
        generic_variant_type_decl_src,
        crate::lower::generate_variant_drop_sources(&all_type_decls),
        crate::lower::generate_record_drop_sources(
            &all_type_decls,
            &anon_recs,
            uses_result_opt_str
        ),
        crate::lower::generate_variant_repr_sources(
            &repr_type_decls,
            &crate::lower::collect_interp_anon_records(&ir),
            &crate::lower::collect_interp_repr_containers(&ir)
        ),
        crate::lower::generate_krec_sources(&ir, &all_type_decls),
        closure_drop,
        res_ilsl_drop,
        res_msi_drop,
        res_lmsi_drop,
        lenlist_drop,
        list_str_drop,
        list_closure_drop,
        map_mclo_drop,
        list_str_clo_drop,
        opt_str_str_drop,
        opt_str_int_drop,
    );
    // The generated drops free a `Value` field via value_core's INTERNAL `__drop_value` — bring
    // value_core's source into scope for the re-lower's type check; the auto-link dedups it.
    let needs_value_core = drops.contains("__drop_value")
        || drops.contains("__drop_list_value")
        // A generated repr calls value_core's JSON serializer by its IMPL name
        // (`value_stringify` — the `${Value}` / Value-field C-060 reprs).
        || drops.contains("value_stringify");
    let value_core_src: &str = if needs_value_core {
        include_str!("../../../stdlib/value_core.almd")
    } else {
        ""
    };
    crate::trace::trace("ALMIDE_DUMP_DROPS", || {
        format!("=== ALMIDE_DUMP_DROPS ===\n{drops}\n=== end ===")
    });
    let mut ir = if drops.trim().is_empty() {
        ir
    } else {
        source_to_ir_with(
            &format!("{source}\n{value_core_src}\n{drops}"),
            self_modules,
        )?
    };
    // #881: module-level top-let ids are per-region — make them globally
    // unique BEFORE any layout/slot phase keys a map by the raw id (the
    // globals union and the mutable-slot map both key raw ids).
    //
    // This runs BEFORE the test-runner synthesis (#1233): the runner's per-test
    // re-init needs to name MODULE top-lets from a MAIN-region `Assign`, which
    // is only sound once every module id is program-unique. Disambiguation only
    // rewrites `ir.modules` (declarations, module fn bodies, top-let inits) and
    // never reads `ir.functions`, so moving it ahead of the synthesis is a pure
    // reordering for every other file.
    disambiguate_module_global_regions(&mut ir);
    if test_mode {
        synthesize_test_runner_main(&mut ir)?;
    }
    Ok(ir)
}

include!("pipeline_test_runner.rs");
include!("pipeline_global_slots.rs");
include!("pipeline_b.rs");
include!("pipeline_c.rs");
include!("pipeline_link.rs");
include!("pipeline_native_rungs.rs");
