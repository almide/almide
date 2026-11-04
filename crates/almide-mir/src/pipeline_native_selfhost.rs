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
    bundled: almide_ir::IrProgram,
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
            if crate::render_native_shims::shim(call).is_none()
                && crate::native_floor::declared(call).is_none()
                && !crate::native_list_ops::is_list_concat(call)
            {
                by_call.entry(*call).or_insert((*src, *impl_fn));
            }
        }
    }
    let user_names: BTreeSet<String> = functions.iter().map(|f| f.name.clone()).collect();
    if let Some(f) = bundled.functions.iter().find(|f| user_names.contains(f.name.as_str())) {
        return Err(clash(f.name.as_str()));
    }
    // The bundled modules' own bodies (see [`take_bundled_modules`]) link as
    // siblings, by their `std_` names.
    let mut linked: Vec<LinkedSelfHost> = vec![LinkedSelfHost {
        record_layouts: crate::lower::build_record_layouts(&bundled.type_decls),
        variant_layouts: crate::lower::build_variant_layouts(&bundled.type_decls),
        ir: bundled,
    }];
    let mut linked_srcs: BTreeMap<&'static str, usize> = BTreeMap::new();
    linked_srcs.insert(BUNDLED_ORIGIN, 0);
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

/// The `linked_srcs` key of the bundled modules' bodies: no registry source
/// is empty, so it never names one.
const BUNDLED_ORIGIN: &str = "";

/// Whether the native render serves `module.fn` by another route than a
/// linked bundled body: a registry body, a floor routine, a shim or a list
/// concat. Such a call keeps its dotted spelling.
fn natively_served(call: &str) -> bool {
    registry_served_names().contains(call)
        || crate::render_native_shims::shim(call).is_some()
        || crate::native_floor::declared(call).is_some()
        || crate::native_list_ops::is_list_concat(call)
}

/// The name a bundled module fn links under on the native render, from the
/// mangled `almide_rt_<module>_<fn>` the sibling resolution gives its calls:
/// `std_` keeps it apart from the runtime floor's `almide_rt_` items.
fn std_link_name(mangled: &str) -> Option<String> {
    mangled.strip_prefix("almide_rt_").map(|rest| format!("std_{rest}"))
}

/// Take the bundled stdlib modules out of `ir` (#4006, selfhost-link-v2 on the
/// native render). The frontend linked them as siblings: a call of a bundled
/// fn with a pure-Almide body that no registry body serves is a `Named` call
/// of its mangled `almide_rt_<m>_<f>` (a generic one's mono instance
/// included), and a `mut` param's write-back is already in place at each call
/// site. Each such call is retargeted:
/// - to the dotted `m.f` when the native render serves `m.f` another way
///   (floor, shim, registry), so that route is unchanged;
/// - else to `std_<m>_<f>`, the fn the returned program defines: the module
///   body, renamed, with its own calls retargeted the same way.
///
/// The returned bodies are lowered, verified and certified like user fns
/// (linked, not trusted). A body that reads a module-level binding is left
/// out (this render threads no module global): its calls resolve nowhere and
/// wall.
fn take_bundled_modules(ir: &mut almide_ir::IrProgram) -> (almide_ir::IrProgram, std::collections::BTreeSet<String>) {
    use almide_ir::{walk_expr_mut, CallTarget, IrExprKind, IrMutVisitor};
    use almide_lang::intern::sym;
    use std::collections::{BTreeMap, BTreeSet};
    let modules = std::mem::take(&mut ir.modules);
    // mangled name → (module, fn) for every module fn.
    let mut index: BTreeMap<String, (String, String)> = BTreeMap::new();
    for m in &modules {
        for f in &m.functions {
            index.insert(
                user_module_fn_name(m.name.as_str(), f.name.as_str()),
                (m.name.as_str().to_string(), f.name.as_str().to_string()),
            );
        }
    }
    struct Rw<'a> {
        index: &'a BTreeMap<String, (String, String)>,
        /// The mangled names of the bodies that link.
        linkable: &'a BTreeSet<String>,
        /// Inside a module body: its name and its fns with no Almide body.
        own: Option<(String, BTreeSet<String>)>,
    }
    impl IrMutVisitor for Rw<'_> {
        fn visit_expr_mut(&mut self, e: &mut almide_ir::IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Call { target, .. } = &mut e.kind else { return };
            // A dotted call in a module body the sibling resolution did not
            // see (a mono instance's protocol pivot, `float.__int_exact`)
            // links the same way when its body links. A user fn's dotted
            // call stays: the lowering's own routes (in-place mutators,
            // host ops) own it.
            if let CallTarget::Module { module, func, .. } = target {
                let mangled = user_module_fn_name(module.as_str(), func.as_str());
                if self.own.is_some() && self.linkable.contains(&mangled) && !natively_served(&format!("{module}.{func}")) {
                    *target = CallTarget::Named { name: sym(&std_link_name(&mangled).expect("mangled")) };
                }
                return;
            }
            let CallTarget::Named { name } = target else { return };
            if let Some((m, f)) = self.index.get(name.as_str()) {
                let dotted = format!("{m}.{f}");
                *target = if natively_served(&dotted) {
                    CallTarget::Module { module: sym(m), func: sym(f), def_id: None }
                } else {
                    CallTarget::Named { name: sym(&std_link_name(name.as_str()).expect("an index key is mangled")) }
                };
            } else if let Some((m, intrinsic)) = &self.own {
                // A bare call of the module's own intrinsic is its floor call.
                if intrinsic.contains(name.as_str()) {
                    let f = name.as_str().to_string();
                    *target = CallTarget::Module { module: sym(m), func: sym(&f), def_id: None };
                }
            }
        }
    }
    let linkable: BTreeSet<String> = modules
        .iter()
        .flat_map(|m| {
            let top: BTreeSet<almide_ir::VarId> = m.top_lets.iter().map(|tl| tl.var).collect();
            m.functions
                .iter()
                .filter(move |f| is_pure_almide_fn(f) && !reads_any(f, &top))
                .map(move |f| user_module_fn_name(m.name.as_str(), f.name.as_str()))
        })
        .collect();
    let mut rw = Rw { index: &index, linkable: &linkable, own: None };
    for f in ir.functions.iter_mut() {
        rw.visit_expr_mut(&mut f.body);
    }
    for tl in ir.top_lets.iter_mut() {
        rw.visit_expr_mut(&mut tl.value);
    }
    let mut out = almide_ir::IrProgram::default();
    // The linked names of the bodies declared `@ordered` (ADR-0024 D3).
    let mut ordered: BTreeSet<String> = BTreeSet::new();
    for m in modules {
        let top: BTreeSet<almide_ir::VarId> = m.top_lets.iter().map(|tl| tl.var).collect();
        let intrinsic: BTreeSet<String> =
            m.functions.iter().filter(|f| !is_pure_almide_fn(f)).map(|f| f.name.as_str().to_string()).collect();
        rw.own = Some((m.name.as_str().to_string(), intrinsic));
        for mut f in m.functions {
            if !is_pure_almide_fn(&f) || reads_any(&f, &top) {
                continue;
            }
            rw.visit_expr_mut(&mut f.body);
            let Some(name) = std_link_name(&user_module_fn_name(m.name.as_str(), f.name.as_str())) else { continue };
            if almide_lang::stdlib_info::is_ordered_stdlib_fn(m.name.as_str(), almide_ir::mono_base_or_self(f.name.as_str())) {
                ordered.insert(name.clone());
            }
            f.name = sym(&name);
            out.functions.push(f);
        }
        out.type_decls.extend(m.type_decls);
    }
    (out, ordered)
}

/// The first fan block of `ir` one of whose arms reaches, through user fns
/// and linked bundled bodies, an `@ordered` stdlib fn (ADR-0024 D3): such an
/// arm's effect is process-local state a sibling arm may read, so the arms
/// must run in arm order, and this render runs them on threads. `Some(span)`
/// declines the program, as the standard codegen's fan lowering orders it.
fn fan_reaching_ordered(
    ir: &almide_ir::IrProgram,
    bundled: &almide_ir::IrProgram,
    ordered: &std::collections::BTreeSet<String>,
) -> Option<Option<almide_ir::Span>> {
    use almide_ir::visit::{walk_expr, IrVisitor};
    use almide_ir::{CallTarget, IrExpr, IrExprKind};
    use std::collections::{BTreeMap, BTreeSet};
    let fns: BTreeMap<&str, &almide_ir::IrFunction> =
        ir.functions.iter().chain(bundled.functions.iter()).map(|f| (f.name.as_str(), f)).collect();
    // Does `e` reach an @ordered fn, following the calls it makes?
    let reaches = |e: &IrExpr| -> bool {
        struct Reach<'a> {
            fns: &'a BTreeMap<&'a str, &'a almide_ir::IrFunction>,
            ordered: &'a BTreeSet<String>,
            seen: BTreeSet<String>,
            hit: bool,
        }
        impl IrVisitor for Reach<'_> {
            fn visit_expr(&mut self, e: &IrExpr) {
                if self.hit {
                    return;
                }
                if let IrExprKind::Call { target, .. } | IrExprKind::TailCall { target, .. } = &e.kind {
                    match target {
                        CallTarget::Module { module, func, .. } => {
                            self.hit |= almide_lang::stdlib_info::is_ordered_stdlib_fn(module.as_str(), func.as_str());
                        }
                        CallTarget::Named { name } => {
                            let n = name.as_str();
                            self.hit |= self.ordered.contains(n);
                            if let Some(f) = self.fns.get(n) {
                                if self.seen.insert(n.to_string()) {
                                    self.visit_expr(&f.body);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                walk_expr(self, e);
            }
        }
        let mut r = Reach { fns: &fns, ordered, seen: BTreeSet::new(), hit: false };
        r.visit_expr(e);
        r.hit
    };
    struct Find<'a> {
        reaches: &'a dyn Fn(&IrExpr) -> bool,
        found: Option<Option<almide_ir::Span>>,
    }
    impl IrVisitor for Find<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if self.found.is_some() {
                return;
            }
            if let IrExprKind::Fan { exprs } = &e.kind {
                if exprs.len() >= 2 && exprs.iter().any(|a| (self.reaches)(a)) {
                    self.found = Some(e.span);
                    return;
                }
            }
            walk_expr(self, e);
        }
    }
    let mut find = Find { reaches: &reaches, found: None };
    for f in ir.functions.iter().filter(|f| !f.is_test) {
        find.visit_expr(&f.body);
    }
    find.found
}

/// Whether `f`'s body reads any of `vars`.
fn reads_any(f: &almide_ir::IrFunction, vars: &std::collections::BTreeSet<almide_ir::VarId>) -> bool {
    struct Reads<'a>(&'a std::collections::BTreeSet<almide_ir::VarId>, bool);
    impl almide_ir::visit::IrVisitor for Reads<'_> {
        fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
            if let almide_ir::IrExprKind::Var { id } = &e.kind {
                self.1 |= self.0.contains(id);
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    if vars.is_empty() {
        return false;
    }
    let mut r = Reads(vars, false);
    almide_ir::visit::IrVisitor::visit_expr(&mut r, &f.body);
    r.1
}

/// The bundled modules, not linked into `ir`, that a dotted call in it names
/// with a pure-Almide body no other route serves.
fn unloaded_bundled_callees(ir: &almide_ir::IrProgram) -> Vec<String> {
    use almide_ir::{CallTarget, IrExprKind};
    struct Scan<'a>(&'a std::collections::BTreeSet<String>, std::collections::BTreeSet<String>);
    impl almide_ir::visit::IrVisitor for Scan<'_> {
        fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
            if let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. } = &e.kind {
                let m = module.as_str();
                if !self.0.contains(m)
                    && almide_lang::stdlib_info::is_bundled_module(m)
                    && !natively_served(&format!("{m}.{func}"))
                    && almide_ir::mut_args::stdlib_fn_has_almide_body(m, func.as_str())
                {
                    self.1.insert(m.to_string());
                }
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    let loaded: std::collections::BTreeSet<String> = ir.modules.iter().map(|m| m.name.as_str().to_string()).collect();
    let mut scan = Scan(&loaded, Default::default());
    for f in ir.functions.iter().chain(ir.modules.iter().flat_map(|m| m.functions.iter())) {
        almide_ir::visit::IrVisitor::visit_expr(&mut scan, &f.body);
    }
    scan.1.into_iter().collect()
}
