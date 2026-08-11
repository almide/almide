/// IR → IR monomorphization pass.
///
/// Input:    &mut IrProgram
/// Output:   IrProgram with specialized functions
/// Owns:     structural bound instantiation, function cloning, call rewriting
/// Does NOT: other optimizations, codegen
///
/// Specializes generic functions with structural bounds (e.g., `T: { name: String, .. }`)
/// into concrete versions for each call-site type. This enables Rust codegen
/// to emit functions that preserve the full concrete type.
///
/// Example:
///   fn set_name[T: { name: String, .. }](x: T, n: String) -> T
///   set_name(dog, "Max")     → set_name__Dog(x: Dog, n: String) -> Dog
///   set_name(person, "Bob")  → set_name__Person(x: Person, n: String) -> Person

mod utils;
mod discovery;
mod specialization;
mod varid_remap;
mod rewrite;
mod propagation;

use std::collections::HashMap;
use std::collections::BTreeMap;
use almide_ir::*;
use almide_lang::types::Ty;
use almide_base::Sym;

use utils::{module_mono_suffix, BoundedParam, MonoKey, ty_contains_typevar};
use almide_base::intern::sym;
use discovery::{collect_mono_bindings, discover_instances, discover_instances_in_frontier};
use specialization::specialize_function;
use rewrite::rewrite_calls;
use propagation::propagate_concrete_types;

/// Run the monomorphization pass on an IR program.
/// Specialize generic functions for concrete type arguments at each call site.
///
/// Uses frontier-based incremental discovery: after the first round scans all
/// functions, subsequent rounds only scan newly created specializations.
/// This reduces transitive discovery from O(N × total_functions) to O(N × new_functions).
pub fn monomorphize(program: &mut IrProgram) {
    monomorphize_module_fns(program);
    let bound_fns = find_structurally_bounded_fns(&program.functions, &program.type_decls);
    if bound_fns.is_empty() {
        // Mutual tail-call SCC collapse (#1043) rides monomorphize because
        // this is the ONE stage every consumer shares — v0 codegen, the v1
        // native/wasm renders and almide-interp all take this output — so
        // both exits of this fn must run it.
        crate::mutual_tco::run_mutual_tco(program);
        return;
    }

    // Fixed-point loop: transitive monomorphization (A → B → C chains)
    // Converges when no new instances are discovered. Warns if instance count
    // exceeds 1000 (possible infinite expansion).
    // BTreeMap, not HashMap: `new` (below) is iterated to append specialized
    // functions to program.functions, and a function's WASM index is its position
    // there. HashMap iteration order is host-pointer-width AND Sym-intern-order
    // dependent, so the wasm32 playground compiler would assign different indices
    // than x86-64 → a divergent/trapping module. MonoKey=(String,String) is Ord,
    // so BTreeMap iterates in content order = a pure function of the program.
    let mut all_instances: BTreeMap<MonoKey, HashMap<String, Ty>> = BTreeMap::new();
    let mut frontier_start: Option<usize> = None; // None = first round (scan all)

    loop {
        // Discovery: first round scans all functions + top_lets,
        // subsequent rounds only scan the frontier (newly added specializations)
        let instances = match frontier_start {
            None => discover_instances(program, &bound_fns),
            Some(start) => discover_instances_in_frontier(
                &program.functions[start..],
                &bound_fns,
                &program.functions,
            ),
        };

        // Filter to only new instances
        let new: BTreeMap<MonoKey, HashMap<String, Ty>> = instances.into_iter()
            .filter(|(k, _)| !all_instances.contains_key(k))
            .collect();
        if new.is_empty() {
            break; // convergence: no new instances
        }
        if all_instances.len() + new.len() > 1000 {
            eprintln!("[WARN] monomorphization: {}+ instances, possible infinite expansion", all_instances.len() + new.len());
            break;
        }

        // Specialize new functions (alpha-renaming: fresh VarIds per specialization).
        // Module-level globals are FREE vars — never alpha-renamed (#788).
        let global_vars: std::collections::HashSet<almide_ir::VarId> = program
            .top_lets
            .iter()
            .map(|tl| tl.var)
            .chain(program.modules.iter().flat_map(|m| m.top_lets.iter().map(|tl| tl.var)))
            .collect();
        let mut new_functions = Vec::new();
        for ((fn_name, suffix), bindings) in &new {
            if let Some(orig) = program.functions.iter().find(|f| !f.is_test && f.name == *fn_name) {
                new_functions.push(specialize_function(orig, suffix, bindings, &mut program.var_table, &global_vars));
            }
        }

        // Rewrite call sites (all instances, including previous rounds)
        all_instances.extend(new);

        // Add new specialized functions BEFORE rewriting, so self-recursive
        // calls within specialized functions also get rewritten.
        frontier_start = Some(program.functions.len());
        program.functions.extend(new_functions);

        rewrite_calls(program, &bound_fns, &all_instances);
    }

    // Remove generic functions: both those with specialized instances AND
    // those with no call sites (unused generics still carry TypeVars).
    //
    // IMPORTANT: tests may share a name with a function (e.g. `fn wrap_all[T]`
    // and `test "wrap_all"` both lower to `name = "wrap_all"`). Only drop
    // *generic non-test* functions — never a test, regardless of name.
    let mono_fn_names: std::collections::HashSet<String> = all_instances.keys().map(|(name, _)| name.clone()).collect();
    program.functions.retain(|f| {
        if f.is_test { return true; } // tests always survive mono
        if mono_fn_names.contains::<str>(&f.name) { return false; } // replaced by specialized
        // Also remove generic functions with no instances (unused)
        if f.generics.as_ref().map_or(false, |g| !g.is_empty()) {
            return false;
        }
        true
    });

    // Propagate concrete types: after rewrite, some expressions still have TypeVar
    // types (e.g., `let x = mono_fn(...)` where x.ty was set before mono).
    propagate_concrete_types(program);

    // The generic-program exit of the same #1043 rewrite the early return runs
    // — post-specialization, so a concrete instance pair can also form an SCC.
    crate::mutual_tco::run_mutual_tco(program);

    // Erase remaining TypeVars in VarTable. After mono + propagation, any
    // surviving TypeVars are from stdlib generic params (e.g., filter_map[A,B]'s
    // B leaking into a lambda param). These are resolved at runtime, not compile
    // time. Replace with Unknown so downstream passes handle them correctly.
    erase_orphan_typevars(&mut program.var_table);

    // Post-mono guard: ALL TypeVars (including generic params) should be resolved
    verify_no_typevars_post_mono(program);
}

/// A generic fn living inside a module, plus where it lives and which of its
/// params carry the type variables.
struct ModuleGeneric {
    mi: usize,
    fi: usize,
    name: String,
    bounds: Vec<BoundedParam>,
}

/// Are a call site's inferred bindings fully concrete — no `Unknown`, no
/// `TypeVar`, at any depth? Only then may the instance be specialized: a
/// half-inferred binding would mint a specialization whose body still carries
/// type variables past the post-ConcretizeTypes audit.
fn bindings_all_concrete(bindings: &HashMap<String, Ty>) -> bool {
    !bindings.is_empty()
        && bindings.values().all(|ty| {
            !matches!(ty, Ty::Unknown | Ty::TypeVar(_))
                && !ty.contains_unknown()
                && !ty.contains_typevar()
        })
}

/// Run `v` over EVERY expression in the program: top-level fn bodies and
/// top-lets, then each module's fn bodies and top-lets.
///
/// Module bodies are taken out and put back (`mem::replace` with a Unit
/// placeholder) because the visitor cannot borrow `program.modules[mi]` while
/// it also holds the program-wide view it was built from.
fn walk_program_exprs<V: almide_ir::visit_mut::IrMutVisitor>(
    program: &mut IrProgram,
    v: &mut V,
) {
    use almide_ir::IrExprKind;
    fn placeholder() -> almide_ir::IrExpr {
        almide_ir::IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None }
    }
    for func in &mut program.functions {
        v.visit_expr_mut(&mut func.body);
    }
    for tl in &mut program.top_lets {
        v.visit_expr_mut(&mut tl.value);
    }
    for mi in 0..program.modules.len() {
        for fi in 0..program.modules[mi].functions.len() {
            let mut body =
                std::mem::replace(&mut program.modules[mi].functions[fi].body, placeholder());
            v.visit_expr_mut(&mut body);
            program.modules[mi].functions[fi].body = body;
        }
        for ti in 0..program.modules[mi].top_lets.len() {
            let mut val =
                std::mem::replace(&mut program.modules[mi].top_lets[ti].value, placeholder());
            v.visit_expr_mut(&mut val);
            program.modules[mi].top_lets[ti].value = val;
        }
    }
}

/// Each generic's param types, snapshotted from the module it lives in.
fn generic_param_types(program: &IrProgram, generics: &[ModuleGeneric]) -> Vec<Vec<Ty>> {
    generics
        .iter()
        .map(|g| {
            program.modules[g.mi].functions[g.fi]
                .params
                .iter()
                .map(|p| p.ty.clone())
                .collect()
        })
        .collect()
}

/// The module-scoped generic fns worth specializing.
///
/// `@inline_rust` / `@wasm_intrinsic` bundled fns are dispatch metadata: their
/// body is `_` and the actual implementation is the per-target template (Rust
/// runtime fn / hand-written WASM runtime). Templates are type-erased —
/// `list.len[A]` expands to `almide_rt_list_len(&{xs})` regardless of `A`.
/// Specializing them just produces bare-body clones whose names (`len__Int`) the
/// WASM dispatcher's per-module match arms cannot recognise, which would trip the
/// inline `panic!("[ICE] ...")` fallback each dispatcher carries. Skip them so the
/// call site stays `Module { list, len }` and the dispatcher sees the unsuffixed
/// name.
fn collect_module_generics(program: &IrProgram) -> Vec<ModuleGeneric> {
    program
        .modules
        .iter()
        .enumerate()
        .flat_map(|(mi, m)| {
            m.functions.iter().enumerate().filter_map(move |(fi, f)| {
                let gs = f.generics.as_ref()?;
                if gs.is_empty() {
                    return None;
                }
                let is_template_dispatch = f.attrs.iter().any(|a| {
                    matches!(a.name.as_str(), "inline_rust" | "wasm_intrinsic" | "intrinsic")
                });
                if is_template_dispatch {
                    return None;
                }
                let mut bounded = Vec::new();
                for g in gs.iter() {
                    for (i, param) in f.params.iter().enumerate() {
                        if ty_contains_typevar(&param.ty, &g.name) {
                            bounded.push(BoundedParam {
                                param_idx: i,
                                type_var: g.name.to_string(),
                            });
                        }
                    }
                }
                if bounded.is_empty() {
                    return None;
                }
                Some(ModuleGeneric { mi, fi, name: f.name.to_string(), bounds: bounded })
            })
        })
        .collect()
}

/// One discovery round's call-site scan: every `(module, generic)` call whose
/// arg types pin the type variables concretely.
struct Discover<'a> {
    generics: &'a [ModuleGeneric],
    param_types: Vec<Vec<Ty>>,
    module_names: &'a [String],
    /// (mi, fi, bindings, suffix)
    out: Vec<(usize, usize, HashMap<String, Ty>, String)>,
}

impl almide_ir::visit_mut::IrMutVisitor for Discover<'_> {
    fn visit_expr_mut(&mut self, expr: &mut almide_ir::IrExpr) {
        use almide_ir::{CallTarget, IrExprKind};
        almide_ir::visit_mut::walk_expr_mut(self, expr);
        if let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &expr.kind {
            self.record_flattened_call(name.as_str(), args);
        }
        if let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } =
            &expr.kind
        {
            self.record_module_call(module.as_str(), func.as_str(), args);
        }
    }
}

impl Discover<'_> {
    /// A CROSS-MODULE generic call the frontend already FLATTENED to its v0 name
    /// (`m.stash(41)` → `Named { almide_rt_m_stash }` — the #788/#782 crossmod
    /// cell): match the exact flatten spelling per generic (module list + fn name
    /// — no string parsing), so the call site instantiates exactly like a
    /// `Module { m, f }` one.
    fn record_flattened_call(&mut self, name: &str, args: &[almide_ir::IrExpr]) {
        for (gi, g) in self.generics.iter().enumerate() {
            let flat = format!("almide_rt_{}_{}", self.module_names[g.mi], g.name);
            if name != flat {
                continue;
            }
            self.record(gi, args, None);
            break;
        }
    }

    fn record_module_call(&mut self, m: &str, f: &str, args: &[almide_ir::IrExpr]) {
        for (gi, g) in self.generics.iter().enumerate() {
            if g.name != f {
                continue;
            }
            // Module guard: the same fn name can live in several modules (e.g.
            // option.filter / list.filter / result.filter). Without this, the
            // first name-match wins and the specialization is registered under
            // the wrong (mod, fn, suffix) key — the rewriter (which DOES filter
            // by module) then misses the lookup and the call stays as unsuffixed
            // `Module { m, f }`.
            if self.module_names[g.mi] != m {
                continue;
            }
            self.record(gi, args, Some((m, f)));
            break;
        }
    }

    /// Bind generic `gi`'s type vars from `args` and, if every binding is
    /// concrete, queue the instance. `debug_call` names the call site for
    /// `ALMIDE_MONO_DEBUG`.
    fn record(&mut self, gi: usize, args: &[almide_ir::IrExpr], debug_call: Option<(&str, &str)>) {
        let g = &self.generics[gi];
        let bindings = collect_mono_bindings(&g.bounds, args, &self.param_types[gi]);
        let all_concrete = bindings_all_concrete(&bindings);
        if let Some((m, f)) = debug_call {
            if std::env::var_os("ALMIDE_MONO_DEBUG").is_some() {
                let atys: Vec<_> = args.iter().map(|a| &a.ty).collect();
                let ptys = &self.param_types[gi];
                eprintln!(
                    "[mono-debug] {m}.{f} args={atys:?} ptys={ptys:?} \
                     bindings={bindings:?} concrete={all_concrete}"
                );
            }
        }
        if !all_concrete {
            return;
        }
        let suffix = module_mono_suffix(&g.bounds, &bindings);
        self.out.push((g.mi, g.fi, bindings, suffix));
    }
}

/// Rewrite every call site of a specialized module generic to its suffixed name.
struct Rewriter<'a> {
    generics: &'a [ModuleGeneric],
    param_types: &'a [Vec<Ty>],
    rename: &'a HashMap<(String, String, String), String>,
    module_names: &'a [String],
}

impl almide_ir::visit_mut::IrMutVisitor for Rewriter<'_> {
    fn visit_expr_mut(&mut self, expr: &mut almide_ir::IrExpr) {
        use almide_ir::{CallTarget, IrExprKind};
        almide_ir::visit_mut::walk_expr_mut(self, expr);
        if let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &mut expr.kind {
            self.rewrite_flattened_call(name, args);
        }
        if let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } =
            &mut expr.kind
        {
            self.rewrite_module_call(module.as_str(), func, args);
        }
    }
}

impl Rewriter<'_> {
    /// The specialized name for generic `gi` at a call site with these args, if
    /// one was minted; `None` when the bindings are not concrete or the instance
    /// was never specialized.
    fn specialized_name(&self, gi: usize, m: &str, args: &[almide_ir::IrExpr]) -> Option<&String> {
        let g = &self.generics[gi];
        let bindings = collect_mono_bindings(&g.bounds, args, &self.param_types[gi]);
        if !bindings_all_concrete(&bindings) {
            return None;
        }
        let suffix = module_mono_suffix(&g.bounds, &bindings);
        self.rename.get(&(m.to_string(), g.name.clone(), suffix))
    }

    /// The FLATTENED cross-module generic call (the Discover Named arm's twin):
    /// rewrite `Named { almide_rt_m_stash }` to the specialized instance's own
    /// flatten spelling (`almide_rt_m_stash__Int`) — the SAME name the module-fn
    /// flattening gives the pushed instance.
    fn rewrite_flattened_call(&self, name: &mut Sym, args: &[almide_ir::IrExpr]) {
        let n = name.as_str().to_string();
        for (gi, g) in self.generics.iter().enumerate() {
            let m = self.module_names[g.mi].clone();
            if n != format!("almide_rt_{}_{}", m, g.name) {
                continue;
            }
            if let Some(new_name) = self.specialized_name(gi, &m, args) {
                *name = sym(&format!("almide_rt_{}_{}", m, new_name));
            }
            break;
        }
    }

    fn rewrite_module_call(&self, m: &str, func: &mut Sym, args: &[almide_ir::IrExpr]) {
        let f = func.as_str().to_string();
        for (gi, g) in self.generics.iter().enumerate() {
            if g.name != f || self.module_names[g.mi] != m {
                continue;
            }
            if let Some(new_name) = self.specialized_name(gi, m, args) {
                *func = sym(new_name);
            }
            break;
        }
    }
}

/// Specialize every newly-discovered instance into its own module, recording the
/// `(module, fn, suffix) → specialized name` mapping. Returns whether any round
/// produced something new (the fixed-point's continue condition).
fn specialize_discovered(
    program: &mut IrProgram,
    found: Vec<(usize, usize, HashMap<String, Ty>, String)>,
    seen: &mut std::collections::HashSet<(String, String, String)>,
    rename: &mut HashMap<(String, String, String), String>,
) -> bool {
    let mut any_new = false;
    for (mi, fi, bindings, suffix) in found {
        let mod_name = program.modules[mi].name.to_string();
        let fn_name = program.modules[mi].functions[fi].name.to_string();
        let key = (mod_name, fn_name, suffix.clone());
        if !seen.insert(key.clone()) {
            continue;
        }
        any_new = true;
        // Borrow split: clone the fn out, specialize against the module's own
        // var_table, push the instance back. The module's OWN top-lets
        // (`var _dirty`) are free vars in the body — never alpha-renamed (#788),
        // the same rule as the top-level driver.
        let module_globals: std::collections::HashSet<almide_ir::VarId> =
            program.modules[mi].top_lets.iter().map(|tl| tl.var).collect();
        let orig = program.modules[mi].functions[fi].clone();
        let mod_vt = &mut program.modules[mi].var_table;
        let specialized = specialize_function(&orig, &suffix, &bindings, mod_vt, &module_globals);
        let new_name = specialized.name.to_string();
        program.modules[mi].functions.push(specialized);
        rename.insert(key, new_name);
    }
    any_new
}

/// Remove all generic source fns from every IR module — bundled stdlib and user
/// packages alike. Specialized instances are already in `module.functions`;
/// unspecialized generics with no call sites are dead code (the source still has
/// TypeVar params and would fail the post-ConcretizeTypes audit). The Rust
/// target's later optimizer would remove them anyway; the WASM emitter does not,
/// so we prune here as the canonical invariant: post-mono, no module fn carries
/// TypeVars.
///
/// Exception: bundled stdlib fns carrying `@inline_rust` or `@wasm_intrinsic` are
/// dispatch *metadata*, not emitted code. Their generic signatures stay in the IR
/// so `pass_stdlib_lowering` can locate them by (module, func) and render call
/// sites as `IrExprKind::InlineRust`. Without this carve-out, every
/// Stdlib-Unification bundled module (option, result, list, ...) loses its
/// attribute table the moment mono runs.
fn prune_generic_module_fns(program: &mut IrProgram) {
    for module in &mut program.modules {
        module.functions.retain(|f| {
            let is_generic = f.generics.as_ref().is_some_and(|g| !g.is_empty());
            !is_generic
                || f.attrs
                    .iter()
                    .any(|a| matches!(a.name.as_str(), "inline_rust" | "wasm_intrinsic"))
        });
    }
}

/// Monomorphize generic fns defined inside `program.modules[*].functions`.
///
/// For each such fn, scan all call sites (top-level functions, top_lets,
/// every module body) for `CallTarget::Module { module: <owning>, func: <generic> }`
/// and collect the concrete type bindings. Specialize each instance via the
/// same `specialize_function` helper used for top-level generics, push the
/// result into the same module's `functions`, and rewrite the call sites to
/// point at the suffixed name. The call target stays `Module { ... }`, so
/// codegen on every backend continues to go through the same stdlib
/// dispatch path — bundled fns are treated as first-class module members,
/// not lifted to top-level.
fn monomorphize_module_fns(program: &mut IrProgram) {
    let generics = collect_module_generics(program);
    if generics.is_empty() {
        return;
    }

    // Fixed-point: each specialization's body may reference another bundled
    // generic. `seen` keys on (module, fn, suffix) so a repeat round adds nothing.
    let mut seen: std::collections::HashSet<(String, String, String)> =
        std::collections::HashSet::new();
    let mut rename: HashMap<(String, String, String), String> = HashMap::new();
    loop {
        let module_names: Vec<String> =
            program.modules.iter().map(|m| m.name.to_string()).collect();
        let mut d = Discover {
            generics: &generics,
            param_types: generic_param_types(program, &generics),
            module_names: &module_names,
            out: Vec::new(),
        };
        walk_program_exprs(program, &mut d);
        let found = std::mem::take(&mut d.out);
        drop(d);
        if !specialize_discovered(program, found, &mut seen, &mut rename) {
            break;
        }
    }

    // Skip the rewrite when there are no specializations — there is nothing to
    // redirect — but DON'T early-return: the prune below must always run so
    // unused generic source fns (no call sites → no specializations → empty
    // rename) are still dropped from `program.modules`. Without this, the
    // ConcretizeTypes audit on the WASM pipeline trips on bundled `list.iterate`'s
    // body in any program that imports `list` but never calls `iterate`.
    if !rename.is_empty() {
        let param_types = generic_param_types(program, &generics);
        let module_names: Vec<String> =
            program.modules.iter().map(|m| m.name.to_string()).collect();
        let mut rw = Rewriter {
            generics: &generics,
            param_types: &param_types,
            rename: &rename,
            module_names: &module_names,
        };
        walk_program_exprs(program, &mut rw);
    }

    prune_generic_module_fns(program);
}

/// Replace remaining TypeVars in VarTable with Unknown.
///
/// After mono + propagation, surviving TypeVars are from stdlib generic params
/// (e.g., `filter_map[A, B]`'s B leaking into a lambda param type). These don't
/// affect correctness — the WASM emitter handles Unknown as I32 (pointer).
fn erase_orphan_typevars(vt: &mut VarTable) {
    fn erase(ty: &Ty) -> Ty {
        match ty {
            Ty::TypeVar(_) => Ty::Unknown,
            _ => ty.map_children(&erase),
        }
    }
    for i in 0..vt.len() {
        let has_tv = utils::has_typevar(&vt.entries[i].ty);
        if has_tv {
            let erased = erase(&vt.entries[i].ty);
            vt.entries[i].ty = erased;
        }
    }
}

/// After monomorphization, no TypeVars should remain in LIVE code.
/// Generic type params (A, B, T) should have been substituted by monomorphization.
/// Inference vars (?0, ?1) should have been resolved by the type checker.
///
/// Only checks VarTable entries that are referenced by remaining functions
/// or top_lets. Orphaned entries from removed generic functions are ignored —
/// they have TypeVar types but are not used by any live code.
fn verify_no_typevars_post_mono(program: &almide_ir::IrProgram) {
    use std::collections::HashSet;

    // Collect all VarIds referenced by live code
    let mut live_vars: HashSet<u32> = HashSet::new();
    for func in &program.functions {
        for p in &func.params { live_vars.insert(p.var.0); }
        collect_live_vars(&func.body, &mut live_vars);
    }
    for tl in &program.top_lets {
        collect_live_vars(&tl.value, &mut live_vars);
    }

    let count = count_fn_sig_typevars(&program.functions) + count_live_var_typevars(program, &live_vars);
    if count > 0 {
        eprintln!("[ICE] {} TypeVar(s) remain after monomorphization. Generic params should be fully substituted.", count);
    }
}

fn has_any_typevar(ty: &Ty) -> bool {
    match ty {
        Ty::TypeVar(_) => true,
        Ty::Applied(_, args) => args.iter().any(has_any_typevar),
        Ty::Tuple(elems) => elems.iter().any(has_any_typevar),
        Ty::Fn { params, ret, is_effect: _ } => params.iter().any(has_any_typevar) || has_any_typevar(ret),
        Ty::Named(_, args) => args.iter().any(has_any_typevar),
        Ty::Record { fields } | Ty::OpenRecord { fields } => fields.iter().any(|(_, t)| has_any_typevar(t)),
        _ => false,
    }
}

/// Count TypeVars leaking into function signatures (return type + params).
fn count_fn_sig_typevars(functions: &[almide_ir::IrFunction]) -> usize {
    let mut count = 0;
    for func in functions {
        if has_any_typevar(&func.ret_ty) { count += 1; }
        for p in &func.params { if has_any_typevar(&p.ty) { count += 1; } }
    }
    count
}

/// Count TypeVars leaking into VarTable entries that are actually referenced by live code.
fn count_live_var_typevars(program: &almide_ir::IrProgram, live_vars: &std::collections::HashSet<u32>) -> usize {
    let mut count = 0;
    for &vid in live_vars {
        if (vid as usize) < program.var_table.len() {
            let info = program.var_table.get(almide_ir::VarId(vid));
            if has_any_typevar(&info.ty) { count += 1; }
        }
    }
    count
}

/// Collect all VarIds referenced in an expression tree.
fn collect_live_vars(expr: &IrExpr, vars: &mut std::collections::HashSet<u32>) {
    use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};
    struct VarCollector<'a> { vars: &'a mut std::collections::HashSet<u32> }
    impl IrVisitor for VarCollector<'_> {
        fn visit_expr(&mut self, expr: &IrExpr) {
            match &expr.kind {
                IrExprKind::Var { id } => { self.vars.insert(id.0); }
                IrExprKind::Lambda { params, .. } => {
                    for (vid, _) in params { self.vars.insert(vid.0); }
                }
                IrExprKind::ForIn { var, var_tuple, .. } => {
                    self.vars.insert(var.0);
                    if let Some(tvs) = var_tuple { for v in tvs { self.vars.insert(v.0); } }
                }
                _ => {}
            }
            walk_expr(self, expr);
        }
        fn visit_stmt(&mut self, stmt: &IrStmt) {
            match &stmt.kind {
                IrStmtKind::Bind { var, .. } => { self.vars.insert(var.0); }
                IrStmtKind::Assign { var, .. } => { self.vars.insert(var.0); }
                IrStmtKind::BindDestructure { pattern, .. } => collect_pattern_vars(pattern, self.vars),
                _ => {}
            }
            walk_stmt(self, stmt);
        }
    }
    VarCollector { vars }.visit_expr(expr);
}

fn collect_pattern_vars(pattern: &IrPattern, vars: &mut std::collections::HashSet<u32>) {
    match pattern {
        IrPattern::Bind { var, .. } => { vars.insert(var.0); }
        IrPattern::Constructor { args, .. } => { for a in args { collect_pattern_vars(a, vars); } }
        IrPattern::Tuple { elements } => { for e in elements { collect_pattern_vars(e, vars); } }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
            collect_pattern_vars(inner, vars);
        }
        IrPattern::RecordPattern { fields, .. } => {
            for f in fields { if let Some(p) = &f.pattern { collect_pattern_vars(p, vars); } }
        }
        _ => {}
    }
}

/// Find functions that have structural bounds, protocol bounds, on generic type parameters,
/// OR direct OpenRecord parameters.
/// Returns function_name → list of bounded params.
fn find_structurally_bounded_fns(functions: &[IrFunction], type_decls: &[IrTypeDecl]) -> HashMap<String, Vec<BoundedParam>> {
    let mut result = HashMap::new();
    for func in functions {
        let bounded = find_bounded_params_for_fn(func, type_decls);
        // Include all generic functions, even those with no param-based TypeVars
        // (e.g., stack_new[T]() — no params, but has generics and type_args at call site)
        if !bounded.is_empty() || func.generics.as_ref().map_or(false, |g| !g.is_empty()) {
            result.insert(func.name.to_string(), bounded);
        }
    }
    result
}

/// Compute a single function's bounded params: structural-bound generics (パターン A),
/// protocol-bound generics not already covered by A (パターン A2), and direct/aliased
/// OpenRecord params (パターン B).
fn find_bounded_params_for_fn(func: &IrFunction, type_decls: &[IrTypeDecl]) -> Vec<BoundedParam> {
    let (mut bounded, seen_tvars) = bounded_from_structural_generics(func);
    bounded.extend(bounded_from_protocol_generics(func, &seen_tvars));
    bounded.extend(bounded_from_open_record_params(func, type_decls));
    bounded
}

/// パターン A: generic functions (with or without structural bounds).
/// Also returns the set of type-var names seen, so パターン A2 can skip duplicates.
fn bounded_from_structural_generics(func: &IrFunction) -> (Vec<BoundedParam>, std::collections::HashSet<Sym>) {
    let mut seen_tvars = std::collections::HashSet::new();
    let mut bounded = Vec::new();
    if let Some(ref generics) = func.generics {
        bounded.extend(
            generics.iter()
                .flat_map(|g| {
                    seen_tvars.insert(g.name.clone());
                    func.params.iter().enumerate()
                        .filter(|(_, param)| ty_contains_typevar(&param.ty, &g.name))
                        .map(|(i, _)| BoundedParam { param_idx: i, type_var: g.name.to_string() })
                })
        );
    }
    (bounded, seen_tvars)
}

/// パターン A2: generic + protocol bound (fn f[T: Showable](x: T)), skipping type
/// vars already covered by パターン A.
fn bounded_from_protocol_generics(func: &IrFunction, seen_tvars: &std::collections::HashSet<Sym>) -> Vec<BoundedParam> {
    let mut bounded = Vec::new();
    let Some(ref generics) = func.generics else { return bounded };
    for g in generics.iter() {
        let Some(ref bounds) = g.bounds else { continue };
        if bounds.is_empty() || seen_tvars.contains(&g.name) { continue; }
        for (i, param) in func.params.iter().enumerate() {
            if ty_contains_typevar(&param.ty, &g.name) {
                bounded.push(BoundedParam { param_idx: i, type_var: g.name.to_string() });
            }
        }
    }
    bounded
}

/// パターン B: 直接 OpenRecord パラメータ、または OpenRecord エイリアス.
fn bounded_from_open_record_params(func: &IrFunction, type_decls: &[IrTypeDecl]) -> Vec<BoundedParam> {
    let mut bounded = Vec::new();
    for (i, param) in func.params.iter().enumerate() {
        let is_open = matches!(&param.ty, Ty::OpenRecord { .. })
            || matches!(&param.ty, Ty::Named(name, args) if args.is_empty()
                && type_decls.iter().any(|td| td.name == *name
                    && matches!(&td.kind, IrTypeDeclKind::Alias { target } if matches!(target, Ty::OpenRecord { .. }))));
        if is_open {
            // OpenRecord パラメータ用の仮の type_var 名を生成
            let tv_name = format!("__open_{}", i);
            bounded.push(BoundedParam {
                param_idx: i,
                type_var: tv_name,
            });
        }
    }
    bounded
}
