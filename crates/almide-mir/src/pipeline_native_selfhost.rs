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
// certified, not trusted. A refused body walls unless every path to it is
// cut by a declared native_hof runtime call. Those calls keep the existing
// trusted-runtime boundary and must pass its per-site checks and witness.

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

/// The registry's call table: call name → (source, impl name), and source →
/// its entries. The first registry entry of a call name wins. Whether the
/// native runtime floor already serves a call (it then keeps that route) is
/// asked per call the program makes ([`served_natively`]), not per registry
/// entry: the floor's answer derives the call's runtime closure, and doing that
/// for every registry entry was half of a small program's `almide build`
/// (#4595).
type SelfHostCallTable = (
    std::collections::BTreeMap<&'static str, (&'static str, &'static str)>,
    std::collections::BTreeMap<&'static str, &'static [(&'static str, &'static str)]>,
);

/// Build the [`SelfHostCallTable`] from `self_host_runtime()`.
fn self_host_call_table() -> SelfHostCallTable {
    use std::collections::BTreeMap;
    // call name → (source, impl name); impl name → source.
    // A call the native runtime floor already serves keeps that route.
    let mut by_call: BTreeMap<&'static str, (&'static str, &'static str)> = BTreeMap::new();
    let mut entries_of: BTreeMap<&'static str, &'static [(&'static str, &'static str)]> = BTreeMap::new();
    for (src, pairs) in crate::mir_ops::self_host_runtime() {
        entries_of.entry(*src).or_insert(*pairs);
        for (impl_fn, call) in pairs.iter() {
            by_call.entry(*call).or_insert((*src, *impl_fn));
        }
    }
    (by_call, entries_of)
}

/// Does the native render serve `call` itself (a shim, the runtime floor, or
/// the list-concat lowering)? Such a call keeps that route and is never
/// retargeted onto a self-host body.
fn served_natively(call: &str) -> bool {
    crate::render_native_shims::shim(call).is_some()
        || crate::native_floor::declared(call).is_some()
        || crate::native_list_ops::is_list_concat(call)
}

/// The link's working state across its fixpoint rounds.
struct SelfHostLinker<'u> {
    user_ir: &'u almide_ir::IrProgram,
    by_call: std::collections::BTreeMap<&'static str, (&'static str, &'static str)>,
    entries_of: std::collections::BTreeMap<&'static str, &'static [(&'static str, &'static str)]>,
    user_names: std::collections::BTreeSet<String>,
    linked: Vec<LinkedSelfHost>,
    linked_srcs: std::collections::BTreeMap<&'static str, usize>,
    done: std::collections::BTreeSet<String>,
    refused: std::collections::BTreeMap<String, LowerError>,
}

impl SelfHostLinker<'_> {
    /// A name clash walls at the user fn that holds the name.
    fn clash(&self, name: &str) -> LowerError {
        let span = self.user_ir.functions.iter().find(|f| f.name.as_str() == name).and_then(|f| f.body.span);
        LowerError::at(span, format!("native: the self-host body `{name}` shares its name with a user fn"))
    }

    /// Retarget one undefined call name onto its impl name. `Ok(None)` = not a
    /// registry call (left as is); `Ok(Some((src, impl)))` = served by `src`.
    fn retarget_call(
        &self,
        name: &mut String,
    ) -> Result<Option<(&'static str, String)>, LowerError> {
        match self.by_call.get(name.as_str()).filter(|_| !served_natively(name)) {
            Some(&(src, impl_fn)) => {
                if self.user_names.contains(impl_fn) {
                    return Err(self.clash(impl_fn));
                }
                *name = impl_fn.to_string();
                Ok(Some((src, impl_fn.to_string())))
            }
            None => match self.linked_srcs.iter().find(|(_, &i)| {
                self.linked[i].ir.functions.iter().any(|g| g.name.as_str() == name.as_str())
            }) {
                // A sibling helper of an already-linked body.
                Some((&src, _)) => Ok(Some((src, name.clone()))),
                None => Ok(None),
            },
        }
    }

    /// Retarget registry call names onto impl names, and collect what is missing.
    fn collect_wanted(
        &self,
        functions: &mut [crate::MirFunction],
    ) -> Result<Vec<(&'static str, String)>, LowerError> {
        let defined: std::collections::BTreeSet<String> = functions.iter().map(|f| f.name.clone()).collect();
        let mut wanted: Vec<(&'static str, String)> = Vec::new();
        for f in functions.iter_mut() {
            for op in &mut f.ops {
                let (crate::Op::CallFn { name, .. } | crate::Op::FuncRef { name, .. }) = op else { continue };
                if defined.contains(name.as_str()) {
                    continue;
                }
                let Some((src, impl_fn)) = self.retarget_call(name)? else { continue };
                if !defined.contains(&impl_fn) && !self.done.contains(&impl_fn) {
                    wanted.push((src, impl_fn));
                }
            }
        }
        Ok(wanted)
    }

    /// The index of `src`'s linked IR, lowering it on first use.
    fn source_index(&mut self, src: &'static str) -> Result<usize, LowerError> {
        if let Some(&i) = self.linked_srcs.get(src) {
            return Ok(i);
        }
        let ir = self_host_source_to_ir(src, self.entries_of[src])?;
        let record_layouts = crate::lower::build_record_layouts(&ir.type_decls);
        let variant_layouts = crate::lower::build_variant_layouts(&ir.type_decls);
        self.linked.push(LinkedSelfHost { ir, record_layouts, variant_layouts });
        self.linked_srcs.insert(src, self.linked.len() - 1);
        Ok(self.linked.len() - 1)
    }

    /// Link ONE wanted impl fn (and the helpers its lowering emits).
    fn link_one(
        &mut self,
        functions: &mut Vec<crate::MirFunction>,
        src: &'static str,
        impl_fn: String,
    ) -> Result<(), LowerError> {
        if self.done.contains(&impl_fn) {
            return Ok(());
        }
        if self.user_names.contains(&impl_fn) {
            return Err(self.clash(&impl_fn));
        }
        let at = self.source_index(src)?;
        let sh = &self.linked[at];
        let Some(func) = sh.ir.functions.iter().find(|g| g.name.as_str() == impl_fn) else {
            // Left unresolved: the render walls on the call itself.
            self.done.insert(impl_fn);
            return Ok(());
        };
        let recs = crate::render_native_rec::link_checkpoint();
        let lambdas = crate::lambda_sigs::snapshot();
        let options = crate::option_blocks::snapshot();
        let mut declared_types: Vec<_> = func.params.iter().map(|p| p.ty.clone())
            .chain(std::iter::once(func.ret_ty.clone())).collect();
        collect_expr_native_types(&func.body, &mut declared_types);
        crate::render_native_rec::extend_with(&declared_types);
        let lowered = native_sig_subset_check(func, &sh.record_layouts, &sh.variant_layouts).and_then(|()| {
            crate::lower::lower_function_all_with_globals(
                func,
                &Default::default(),
                &Default::default(),
                &sh.record_layouts,
                &sh.variant_layouts,
            )
            .map_err(|e| LowerError::at(func.body.span, format!("native: self-host fn `{impl_fn}`: {e:?}")))
        });
        let all = match lowered {
            Ok(all) => all,
            Err(error) => {
                // A failed attempt must not seed another function's type facts.
                crate::render_native_rec::restore_link_checkpoint(recs);
                crate::lambda_sigs::restore(lambdas);
                crate::option_blocks::restore(options);
                self.done.insert(impl_fn.clone());
                self.refused.insert(impl_fn, error);
                return Ok(());
            }
        };
        for g in &all {
            if self.user_names.contains(&g.name) {
                return Err(self.clash(&g.name));
            }
            self.done.insert(g.name.clone());
        }
        self.done.insert(impl_fn);
        functions.extend(all);
        Ok(())
    }
}

/// Link every registry body the program (transitively) calls. Returns the
/// linked sources, for the signature table and the Result rewrite.
fn link_native_self_host(
    functions: &mut Vec<crate::MirFunction>,
    user_ir: &almide_ir::IrProgram,
) -> Result<(Vec<LinkedSelfHost>, std::collections::BTreeSet<String>), LowerError> {
    let (by_call, entries_of) = self_host_call_table();
    let mut linker = SelfHostLinker {
        user_ir,
        by_call,
        entries_of,
        user_names: functions.iter().map(|f| f.name.clone()).collect(),
        linked: Vec::new(),
        linked_srcs: std::collections::BTreeMap::new(),
        done: std::collections::BTreeSet::new(),
        refused: std::collections::BTreeMap::new(),
    };
    loop {
        let wanted = linker.collect_wanted(functions)?;
        if wanted.is_empty() {
            break;
        }
        for (src, impl_fn) in wanted {
            linker.link_one(functions, src, impl_fn)?;
        }
    }
    let runtime = crate::native_link_routes::route(functions, &linker.user_names, &linker.refused)?;
    Ok((linker.linked, runtime))
}
