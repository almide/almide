// The native self-host link (#4006) — include!-spliced into `pipeline.rs`
// through pipeline_native_rungs.rs, sharing that module's imports.
//
// A stdlib call the program makes by a registry CALL name (`list.get_str`,
// `__list_concat_rc`, …) is served by a pure-Almide body that DECLARES itself
// (`@implements`, `self_host_registry::self_host_runtime()` — the table the
// wasm leg and the interp oracle link from). The native render links the same
// bodies: each one is lowered to MIR from its embedded source, under its
// registry IMPL name (the name the meter plan keys its entry exemption on),
// and every call site is retargeted from the call name to that impl name.
// The linked functions go through the same subset check, ownership
// verification and certificate as user functions — a linked body is
// certified, not trusted — and a body that does not lower walls the program
// (no per-fn fallback on this render, so no silent drop).

/// One linked registry source: its IR (signatures, Result shapes) and layouts.
struct LinkedSelfHost {
    ir: almide_ir::IrProgram,
    record_layouts: crate::lower::RecordLayouts,
    variant_layouts: crate::lower::VariantLayouts,
}

/// Lower ONE registry source the way the interp oracle does: checked as its
/// owning module (the one module every entry names, if any) and lowered in
/// stdlib mode, so its internal prims are admitted.
fn self_host_source_to_ir(
    source: &str,
    entries: &[(&str, &'static str)],
) -> Result<almide_ir::IrProgram, LowerError> {
    let mut modules = entries.iter().filter_map(|(_, call)| call.split_once('.').map(|(m, _)| m));
    let owner = modules.next().filter(|first| modules.all(|m| m == *first));
    let mut prog = parse_or_wall(source)?;
    let canon = canonicalize::canonicalize_program_in(&prog, std::iter::empty(), owner);
    let mut checker = Checker::from_env(canon.env);
    infer_or_wall(&mut checker, &mut prog)?;
    Ok(almide_frontend::lower::lower_stdlib_program(&prog, &checker.env, &checker.type_map))
}

/// Link every registry body the program (transitively) calls. Returns the
/// linked sources, for the signature table and the Result rewrite.
fn link_native_self_host(
    functions: &mut Vec<crate::MirFunction>,
    user_ir: &almide_ir::IrProgram,
) -> Result<Vec<LinkedSelfHost>, LowerError> {
    // A name clash walls at the user fn that holds the name.
    let clash = |name: &str| {
        let span = user_ir.functions.iter().find(|f| f.name.as_str() == name).and_then(|f| f.body.span);
        LowerError::at(span, format!("native: the self-host body `{name}` shares its name with a user fn"))
    };
    use std::collections::{BTreeMap, BTreeSet};
    // call name → (source, impl name); impl name → source.
    // A call the native runtime floor already serves keeps that route.
    let mut by_call: BTreeMap<&'static str, (&'static str, &'static str)> = BTreeMap::new();
    let mut entries_of: BTreeMap<&'static str, &'static [(&'static str, &'static str)]> = BTreeMap::new();
    for (src, pairs) in crate::mir_ops::self_host_runtime() {
        entries_of.entry(*src).or_insert(*pairs);
        for (impl_fn, call) in pairs.iter() {
            if crate::render_native_shims::shim(call).is_none() {
                by_call.entry(*call).or_insert((*src, *impl_fn));
            }
        }
    }
    let user_names: BTreeSet<String> = functions.iter().map(|f| f.name.clone()).collect();
    let mut linked: Vec<LinkedSelfHost> = Vec::new();
    let mut linked_srcs: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut done: BTreeSet<String> = BTreeSet::new();
    loop {
        // Retarget registry call names onto impl names, and collect what is missing.
        let defined: BTreeSet<String> = functions.iter().map(|f| f.name.clone()).collect();
        let mut wanted: Vec<(&'static str, String)> = Vec::new();
        for f in functions.iter_mut() {
            for op in &mut f.ops {
                let crate::Op::CallFn { name, .. } = op else { continue };
                if defined.contains(name.as_str()) {
                    continue;
                }
                let (src, impl_fn) = match by_call.get(name.as_str()) {
                    Some(&(src, impl_fn)) => {
                        if user_names.contains(impl_fn) {
                            return Err(clash(impl_fn));
                        }
                        *name = impl_fn.to_string();
                        (src, impl_fn.to_string())
                    }
                    None => match linked_srcs.iter().find(|(_, &i)| {
                        linked[i].ir.functions.iter().any(|g| g.name.as_str() == name.as_str())
                    }) {
                        // A sibling helper of an already-linked body.
                        Some((&src, _)) => (src, name.clone()),
                        None => continue,
                    },
                };
                if !defined.contains(&impl_fn) && !done.contains(&impl_fn) {
                    wanted.push((src, impl_fn));
                }
            }
        }
        if wanted.is_empty() {
            break;
        }
        for (src, impl_fn) in wanted {
            if done.contains(&impl_fn) {
                continue;
            }
            if user_names.contains(&impl_fn) {
                return Err(clash(&impl_fn));
            }
            let at = match linked_srcs.get(src) {
                Some(&i) => i,
                None => {
                    let ir = self_host_source_to_ir(src, entries_of[src])?;
                    let record_layouts = crate::lower::build_record_layouts(&ir.type_decls);
                    let variant_layouts = crate::lower::build_variant_layouts(&ir.type_decls);
                    linked.push(LinkedSelfHost { ir, record_layouts, variant_layouts });
                    linked_srcs.insert(src, linked.len() - 1);
                    linked.len() - 1
                }
            };
            let sh = &linked[at];
            let Some(func) = sh.ir.functions.iter().find(|g| g.name.as_str() == impl_fn) else {
                // Left unresolved: the render walls on the call itself.
                done.insert(impl_fn);
                continue;
            };
            crate::render_native_rec::extend_with(
                &func.params.iter().map(|p| p.ty.clone()).chain(std::iter::once(func.ret_ty.clone())).collect::<Vec<_>>(),
            );
            native_sig_subset_check(func, &sh.record_layouts, &sh.variant_layouts)?;
            let all = crate::lower::lower_function_all_with_globals(
                func,
                &Default::default(),
                &Default::default(),
                &sh.record_layouts,
                &sh.variant_layouts,
            )
            .map_err(|e| LowerError::at(func.body.span, format!("native: self-host fn `{impl_fn}`: {e:?}")))?;
            for g in &all {
                if user_names.contains(&g.name) {
                    return Err(clash(&g.name));
                }
                done.insert(g.name.clone());
            }
            done.insert(impl_fn);
            functions.extend(all);
        }
    }
    Ok(linked)
}
