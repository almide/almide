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
    /// Each linked name's ROOTS: the registry impls user functions call whose
    /// links want it (a root is its own).
    roots: std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    /// The roots served by their declared runtime body instead of a link.
    route_served: std::collections::BTreeSet<String>,
    /// Every call name the link rewrote: (function, op, the name before).
    renamed: Vec<(usize, usize, String)>,
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
        &mut self,
        functions: &mut [crate::MirFunction],
    ) -> Result<Vec<(&'static str, String)>, LowerError> {
        let defined: std::collections::BTreeSet<String> = functions.iter().map(|f| f.name.clone()).collect();
        let mut wanted: Vec<(&'static str, String)> = Vec::new();
        let mut roots: Vec<(String, std::collections::BTreeSet<String>)> = Vec::new();
        for (fi, f) in functions.iter_mut().enumerate() {
            let caller_root = self.roots.get(&f.name).cloned();
            for (oi, op) in f.ops.iter_mut().enumerate() {
                let crate::Op::CallFn { name, .. } = op else { continue };
                if defined.contains(name.as_str()) {
                    continue;
                }
                let before = name.clone();
                let retargeted = self.retarget_call(name)?;
                if *name != before {
                    self.renamed.push((fi, oi, before));
                }
                let Some((src, impl_fn)) = retargeted else { continue };
                if self.route_served.contains(&impl_fn) {
                    continue;
                }
                roots.push((impl_fn.clone(), caller_root.clone().unwrap_or_else(|| [impl_fn.clone()].into())));
                if !defined.contains(&impl_fn) && !self.done.contains(&impl_fn) {
                    wanted.push((src, impl_fn));
                }
            }
        }
        for (n, r) in roots {
            self.roots.entry(n).or_default().extend(r);
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
        let root = self.roots.get(&impl_fn).cloned().unwrap_or_else(|| [impl_fn.clone()].into());
        for g in &all {
            if self.user_names.contains(&g.name) {
                return Err(self.clash(&g.name));
            }
            self.done.insert(g.name.clone());
            self.roots.entry(g.name.clone()).or_default().extend(root.iter().cloned());
        }
        self.done.insert(impl_fn);
        functions.extend(all);
        Ok(())
    }
}

/// The registry bodies linked into a native render, and the registry impls
/// whose calls their declared runtime body serves instead.
struct NativeLink {
    linked: Vec<LinkedSelfHost>,
    route_served: std::collections::BTreeSet<String>,
}

/// Link every registry body the program (transitively) calls. Returns the
/// linked sources, for the signature table and the Result rewrite.
///
/// A body written for the wasm leg's representation may not lower here (it
/// reads a list of heap elements through raw slots). When the registry impl
/// a user function calls has a runtime route — the body its stdlib
/// DECLARATION names (`crate::native_hof`) — and its link, or the link of a
/// helper it wants, refuses, that impl is not linked: its calls stay named
/// for it and the render serves them through the runtime body, or walls
/// there. The link then starts over without it. A refusal under an impl with
/// no runtime route walls the program, as before.
fn link_native_self_host(
    functions: &mut Vec<crate::MirFunction>,
    user_ir: &almide_ir::IrProgram,
) -> Result<NativeLink, LowerError> {
    let own = functions.len();
    let mut route_served = std::collections::BTreeSet::new();
    loop {
        let (by_call, entries_of) = self_host_call_table();
        let mut linker = SelfHostLinker {
            user_ir,
            by_call,
            entries_of,
            user_names: functions[..own].iter().map(|f| f.name.clone()).collect(),
            linked: Vec::new(),
            linked_srcs: std::collections::BTreeMap::new(),
            done: std::collections::BTreeSet::new(),
            roots: std::collections::BTreeMap::new(),
            route_served: route_served.clone(),
            renamed: Vec::new(),
        };
        let refused = match linker.link_all(functions) {
            Ok(()) => return Ok(NativeLink { linked: linker.linked, route_served }),
            Err(refused) => refused,
        };
        // Every root under the refusal that has a runtime route takes it.
        let routed: Vec<String> = refused.1.iter().filter(|r| crate::native_hof::of(r).is_some()).cloned().collect();
        let grew = routed.into_iter().fold(false, |g, r| route_served.insert(r) || g);
        match refused {
            _ if grew => {
                // Start over from the program's own functions, their call
                // names as they were.
                functions.truncate(own);
                for (fi, oi, name) in linker.renamed.into_iter().rev() {
                    if let Some(crate::Op::CallFn { name: n, .. }) = functions.get_mut(fi).and_then(|f| f.ops.get_mut(oi)) {
                        *n = name;
                    }
                }
            }
            (e, _) => return Err(e),
        }
    }
}

impl SelfHostLinker<'_> {
    /// The link's fixpoint; a refusal carries the root impl it happened under.
    fn link_all(
        &mut self,
        functions: &mut Vec<crate::MirFunction>,
    ) -> Result<(), (LowerError, std::collections::BTreeSet<String>)> {
        loop {
            let wanted = self.collect_wanted(functions).map_err(|e| (e, Default::default()))?;
            if wanted.is_empty() {
                return Ok(());
            }
            for (src, impl_fn) in wanted {
                let roots = self.roots.get(&impl_fn).cloned().unwrap_or_default();
                self.link_one(functions, src, impl_fn).map_err(|e| (e, roots))?;
            }
        }
    }
}
