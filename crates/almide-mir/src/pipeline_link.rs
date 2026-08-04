// The self-host RUNTIME LINKER — synthesizing the runtime fns a program's
// CallFn names demand and linking them to a fixpoint (a linked body may
// itself call further runtime fns), plus the mutable-global init synthesis
// and the print_str floor. include!-spliced into `pipeline.rs` next to
// pipeline_c.rs, sharing that module's imports.

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
/// Extracted from `Self::synthesize_and_link_runtime_fns` (codopsy7 max-depth sweep): lower
/// ONE self-hosted runtime fn and link it in, verbatim (pure text move — was nested inside
/// `loop { for (rt_source, entries) in .. { if any_called && !any_defined { let rt = ..;
/// for f in &rt.functions { .. } } } }`, so even a single-level `if let Err`/`if let Ok`
/// pushed past the depth threshold purely from the surrounding context). Same order, same
/// verbose-eprintln gate, same call-name rewrite, same push — no behavior change.
fn lower_and_link_one_runtime_fn(
    f: &almide_ir::IrFunction,
    layouts: &PipelineLayouts,
    entries: &[(&str, &str)],
    verbose: bool,
    functions: &mut Vec<crate::MirFunction>,
) {
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
        if let Some((_, call)) = entries.iter().find(|(impl_fn, _)| &mir.name == impl_fn) {
            mir.name = call.to_string();
        }
        functions.push(mir);
    }
}

#[allow(clippy::too_many_arguments)]
fn synthesize_and_link_runtime_fns(
    functions: &mut Vec<crate::MirFunction>,
    mutable_tls: &[almide_ir::IrTopLet],
    layouts: &PipelineLayouts,
    verbose: bool,
) -> Result<(), LowerError> {
    if !mutable_tls.is_empty() {
        synthesize_mutable_global_init(functions, mutable_tls, layouts)?;
    }
    link_self_host_runtime_to_fixpoint(functions, layouts, verbose)?;
    rewrite_impl_names_to_call_names(functions);
    link_print_str_runtime(functions, layouts)?;
    Ok(())
}

/// The `__mg_init` synthesis: every mutable module-level `var`'s initializer as one
/// assignment body, lowered with the main region's globals. Extracted verbatim from
/// [`synthesize_and_link_runtime_fns`] (codopsy round-3 sweep, #852).
fn synthesize_mutable_global_init(
    functions: &mut Vec<crate::MirFunction>,
    mutable_tls: &[almide_ir::IrTopLet],
    layouts: &PipelineLayouts,
) -> Result<(), LowerError> {
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
    Ok(())
}

/// Whether ANY function calls something this registry entry defines. Beyond the plain
/// call-name match, two op families force their helper source in without naming a
/// registered call: a `Value` drop renders `(call $__drop_value …)` from value_core,
/// and a `Map[String, <flat heap>]` drop renders `(call $__drop_map_hval …)` from
/// map_hval. Extracted verbatim from [`synthesize_and_link_runtime_fns`] (codopsy
/// round-3 sweep, #852).
fn runtime_entry_is_called(
    entries: &[(&str, &str)],
    functions: &[crate::MirFunction],
) -> bool {
            let mut any_called = entries.iter().any(|(_, call)| {
                functions.iter().any(|f| {
                    f.ops.iter().any(|op| matches!(op, crate::Op::CallFn { name, .. } if name == call))
                })
            });
            // The msv/msb-family STATIC drops (`$__drop_map_msv`, `$__drop_list_omb`, …)
            // render from DropVariant ops, not CallFn — a program whose ONLY demand on
            // the module is a drop (a `List[Option[Map]]` literal that never calls a
            // map.* entry, Wave 4 L6) must still pull map_msv.almd, or the rendered
            // call dangles. Same principle as the value_core force below.
            if entries.iter().any(|(_, c)| *c == "map.get_or_msv") {
                const MSV_STATIC_DROPS: &[&str] =
                    &["map_msv", "map_msb", "list_str_mss", "list_str_msb", "list_omb", "list_mb"];
                any_called = any_called
                    || functions.iter().any(|f| {
                        f.ops.iter().any(|op| {
                            matches!(
                                op,
                                crate::Op::DropVariant { ty, .. }
                                    if MSV_STATIC_DROPS.contains(&ty.as_str())
                            )
                        })
                    });
            }
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
    any_called
}

/// Dedup the linked runtime functions BY NAME — a no-op merge ONLY when the two bodies are
/// the same function (one source linked via two registry paths).
///
/// Two DIFFERENT functions sharing a name (e.g. two modules' `__`-private helpers with
/// different arities — the `__hex_fill` 4-vs-5-arg collision between `hex_encode.almd` and
/// `int_hex.almd`, #1068) must NOT merge: keeping either rebinds the OTHER module's call
/// sites to a wrong signature, and the module then fails wasm validation AFTER the render
/// wall has already answered `Ok` — the invalid-wasm-as-Ok class the trust ledger audits as
/// zero. Wall it instead, so the failure is an honest compile error naming both arities.
///
/// A free fn (not inlined in the fixpoint loop) so the wall is unit-testable without
/// building a whole program: see the `dedup_*` tests below.
fn dedup_linked_by_name(functions: &mut Vec<crate::MirFunction>) -> Result<(), LowerError> {
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut kept: Vec<crate::MirFunction> = Vec::with_capacity(functions.len());
    for f in functions.drain(..) {
        match seen.get(f.name.as_str()) {
            None => {
                seen.insert(f.name.clone(), kept.len());
                kept.push(f);
            }
            Some(&i) if kept[i] == f => {}
            Some(&i) => {
                return Err(LowerError::at(
                    None,
                    format!(
                        "self-host link collision: two different functions are both named `{}` \
                         ({} vs {} param(s)) — module-local helpers must have distinct names; \
                         merging them would emit an invalid module",
                        f.name,
                        kept[i].params.len(),
                        f.params.len(),
                    ),
                ));
            }
        }
    }
    *functions = kept;
    Ok(())
}

/// Auto-link the self-hosted stdlib runtime (int.to_string, string.concat, …) when an
/// entry is called but not defined, renaming its impl fn to the call name. A linked impl
/// may call ANOTHER registry entry, so this iterates to a FIXPOINT. Extracted verbatim
/// from [`synthesize_and_link_runtime_fns`] (codopsy round-3 sweep, #852).
fn link_self_host_runtime_to_fixpoint(
    functions: &mut Vec<crate::MirFunction>,
    layouts: &PipelineLayouts,
    verbose: bool,
) -> Result<(), LowerError> {
    loop {
        let before = functions.len();
        for (rt_source, entries) in crate::render_wasm::self_host_runtime() {
            let any_called = runtime_entry_is_called(entries, functions);
            let any_defined =
                entries.iter().any(|(_, call)| functions.iter().any(|f| &f.name == call));
            if any_called && !any_defined {
                let rt = source_to_ir(rt_source).map_err(|e| LowerError::Unsupported(format!("in registry source (first entry {:?}): {e:?}", entries.first())))?;
                let linked_from = functions.len();
                for f in &rt.functions {
                    lower_and_link_one_runtime_fn(f, layouts, entries, verbose, functions);
                }
                // The self-append rewrite runs on USER functions before this
                // linker (it has to — the fixpoint scan below is what links the
                // append callees it introduces), so the registry bodies linked
                // HERE never met it: json.parse's own `list.push` accumulator
                // loops stayed full-copy concats — O(n²) inside the very stdlib,
                // while identical user-written loops were amortized (#939).
                // Rewriting each batch as it links closes that, and a callee the
                // rewrite introduces (`__list_append1[_rc]`) is picked up by the
                // NEXT fixpoint round's call scan like any other.
                crate::concat_to_append::rewrite_self_append(&mut functions[linked_from..]);
            }
        }
        dedup_linked_by_name(functions)?;
        if functions.len() == before {
            break;
        }
    }
    Ok(())
}

/// A self-hosted runtime fn may call ANOTHER registered impl by its IMPL name, but the
/// auto-link RENAMED that def to its call_name. Rewrite those call sites to the
/// call_name. Extracted verbatim from [`synthesize_and_link_runtime_fns`] (codopsy
/// round-3 sweep, #852).
fn rewrite_impl_names_to_call_names(functions: &mut [crate::MirFunction]) {
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
}

/// Auto-link the self-hosted runtime `print_str` (`println` → `PrintStr` → `(call
/// $print_str)`). Extracted verbatim from [`synthesize_and_link_runtime_fns`] (codopsy
/// round-3 sweep, #852).
fn link_print_str_runtime(
    functions: &mut Vec<crate::MirFunction>,
    layouts: &PipelineLayouts,
) -> Result<(), LowerError> {
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


/// The #1068 collision wall, tested at the seam rather than through a whole program:
/// two self-host modules whose `__`-private helpers collide must FAIL the link, not
/// silently merge into a module that fails wasm validation after `Ok`.
#[cfg(test)]
mod dedup_link_tests {
    use crate::{MirFunction, MirParam, Repr, ScalarWidth, ValueId};

    fn f(name: &str, arity: usize) -> MirFunction {
        MirFunction {
            name: name.to_string(),
            params: (0..arity)
                .map(|i| MirParam {
                    value: ValueId(i as u32),
                    repr: Repr::Scalar { width: ScalarWidth::Double },
                })
                .collect(),
            ..MirFunction::default()
        }
    }

    #[test]
    fn dedup_merges_two_links_of_the_same_function() {
        // One source reached through two registry paths: identical bodies, a real no-op merge.
        let mut fns = vec![f("__hex_fill", 5), f("__hex_fill", 5), f("other", 1)];
        super::dedup_linked_by_name(&mut fns).expect("identical bodies must merge silently");
        assert_eq!(fns.len(), 2);
        assert_eq!(fns[0].name, "__hex_fill");
        assert_eq!(fns[1].name, "other");
    }

    #[test]
    fn dedup_walls_a_name_collision_between_different_functions() {
        // The exact #1068 shape: hex_encode's 5-param `__hex_fill` vs int_hex's 4-param one.
        // Merging by NAME rebound the loser's call sites to the wrong signature and wasmtime
        // rejected the module ("expected i64 but nothing on stack") AFTER the render wall
        // answered Ok. The link must refuse instead.
        let mut fns = vec![f("__hex_fill", 5), f("__hex_fill", 4)];
        let err = super::dedup_linked_by_name(&mut fns)
            .expect_err("a name collision between DIFFERENT functions must wall");
        let msg = format!("{err:?}");
        assert!(msg.contains("self-host link collision"), "unexpected error: {msg}");
        assert!(msg.contains("__hex_fill"), "the error must name the collision: {msg}");
        assert!(msg.contains("5 vs 4"), "the error must name both arities: {msg}");
    }
}
