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
mod rewrite;
mod propagation;

use std::collections::HashMap;
use almide_ir::*;
use almide_lang::types::Ty;

use utils::{MonoKey, BoundedParam, ty_contains_typevar};
use discovery::{discover_instances, discover_instances_in_frontier};
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
        return;
    }

    // Fixed-point loop: transitive monomorphization (A → B → C chains)
    // Converges when no new instances are discovered. Warns if instance count
    // exceeds 1000 (possible infinite expansion).
    let mut all_instances: HashMap<MonoKey, HashMap<String, Ty>> = HashMap::new();
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
        let new: HashMap<MonoKey, HashMap<String, Ty>> = instances.into_iter()
            .filter(|(k, _)| !all_instances.contains_key(k))
            .collect();
        if new.is_empty() {
            break; // convergence: no new instances
        }
        if all_instances.len() + new.len() > 1000 {
            eprintln!("[WARN] monomorphization: {}+ instances, possible infinite expansion", all_instances.len() + new.len());
            break;
        }

        // Specialize new functions (alpha-renaming: fresh VarIds per specialization)
        let mut new_functions = Vec::new();
        for ((fn_name, suffix), bindings) in &new {
            if let Some(orig) = program.functions.iter().find(|f| !f.is_test && f.name == *fn_name) {
                new_functions.push(specialize_function(orig, suffix, bindings, &mut program.var_table));
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

    // Post-mono guard: ALL TypeVars (including generic params) should be resolved
    verify_no_typevars_post_mono(program);
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
    use almide_ir::{IrExprKind, CallTarget};
    use almide_ir::visit_mut::{IrMutVisitor, walk_expr_mut};
    use almide_base::intern::sym;
    use discovery::collect_mono_bindings;
    use utils::{BoundedParam, ty_to_name, ty_contains_typevar};
    use specialization::specialize_function;

    // (module_idx, fn_idx, generic names, bounded param list)
    struct ModuleGeneric { mi: usize, fi: usize, name: String, bounds: Vec<BoundedParam> }

    let generics: Vec<ModuleGeneric> = program.modules.iter().enumerate()
        .flat_map(|(mi, m)| {
            m.functions.iter().enumerate().filter_map(move |(fi, f)| {
                let gs = f.generics.as_ref()?;
                if gs.is_empty() { return None; }
                let mut bounded = Vec::new();
                for g in gs.iter() {
                    for (i, param) in f.params.iter().enumerate() {
                        if ty_contains_typevar(&param.ty, &g.name) {
                            bounded.push(BoundedParam { param_idx: i, type_var: g.name.to_string() });
                        }
                    }
                }
                if bounded.is_empty() { return None; }
                Some(ModuleGeneric {
                    mi, fi, name: f.name.to_string(), bounds: bounded,
                })
            })
        })
        .collect();

    if generics.is_empty() { return; }

    // Fixed-point: each specialization's body may reference another bundled generic.
    // Track (module_name, fn_name, suffix) to avoid duplicates across rounds.
    let mut seen: std::collections::HashSet<(String, String, String)> = std::collections::HashSet::new();
    let mut rename: HashMap<(String, String, String), String> = HashMap::new(); // (mod, fn, suffix) → specialized name

    loop {
        // Discover call site instances
        struct Discover<'a> {
            generics: &'a [ModuleGeneric],
            param_types: Vec<Vec<Ty>>,
            out: Vec<(usize, usize, HashMap<String, Ty>, String)>, // (mi, fi, bindings, suffix)
        }
        impl<'a> IrMutVisitor for Discover<'a> {
            fn visit_expr_mut(&mut self, expr: &mut almide_ir::IrExpr) {
                walk_expr_mut(self, expr);
                if let IrExprKind::Call { target: CallTarget::Module { module, func }, args, .. } = &expr.kind {
                    let m = module.as_str();
                    let f = func.as_str();
                    for (gi, g) in self.generics.iter().enumerate() {
                        if g.name != f { continue; }
                        // Cheap module guard: we can't easily query the module name
                        // from a generic index without the program; rely on unique fn
                        // names for now — guards below skip non-matching bindings.
                        let ptys = &self.param_types[gi];
                        let bindings = collect_mono_bindings(&g.bounds, args, ptys);
                        let all_concrete = !bindings.is_empty() && bindings.values().all(|ty|
                            !matches!(ty, Ty::Unknown) && !ty.contains_unknown()
                            && !matches!(ty, Ty::TypeVar(_))
                            && !ty.contains_typevar()
                        );
                        if !all_concrete { continue; }
                        // Deterministic suffix
                        let generic_names: Vec<String> = self.generics[gi].bounds.iter()
                            .map(|b| b.type_var.clone()).collect::<std::collections::HashSet<_>>()
                            .into_iter().collect::<Vec<_>>();
                        let mut sorted = generic_names;
                        sorted.sort();
                        let suffix = sorted.iter()
                            .filter_map(|g| bindings.get(g))
                            .filter_map(|t| ty_to_name(t))
                            .collect::<Vec<_>>()
                            .join("_");
                        self.out.push((g.mi, g.fi, bindings, suffix));
                        break;
                    }
                }
            }
        }

        // Build param types snapshot for each generic
        let param_types: Vec<Vec<Ty>> = generics.iter().map(|g| {
            program.modules[g.mi].functions[g.fi].params.iter().map(|p| p.ty.clone()).collect()
        }).collect();

        let mut d = Discover { generics: &generics, param_types, out: Vec::new() };
        for func in &mut program.functions {
            d.visit_expr_mut(&mut func.body);
        }
        for tl in &mut program.top_lets {
            d.visit_expr_mut(&mut tl.value);
        }
        // Walk module bodies (avoid borrowing conflict by index)
        for mi in 0..program.modules.len() {
            let fn_count = program.modules[mi].functions.len();
            for fi in 0..fn_count {
                // Can't borrow both program.modules[mi] and Discover's program view;
                // take ownership, walk, restore.
                let mut body = std::mem::replace(&mut program.modules[mi].functions[fi].body, almide_ir::IrExpr {
                    kind: IrExprKind::Unit, ty: Ty::Unit, span: None,
                });
                d.visit_expr_mut(&mut body);
                program.modules[mi].functions[fi].body = body;
            }
            let tl_count = program.modules[mi].top_lets.len();
            for ti in 0..tl_count {
                let mut val = std::mem::replace(&mut program.modules[mi].top_lets[ti].value, almide_ir::IrExpr {
                    kind: IrExprKind::Unit, ty: Ty::Unit, span: None,
                });
                d.visit_expr_mut(&mut val);
                program.modules[mi].top_lets[ti].value = val;
            }
        }

        // Filter out already-seen, and specialize new ones
        let mut any_new = false;
        for (mi, fi, bindings, suffix) in d.out {
            let mod_name = program.modules[mi].name.to_string();
            let fn_name = program.modules[mi].functions[fi].name.to_string();
            let key = (mod_name.clone(), fn_name.clone(), suffix.clone());
            if !seen.insert(key.clone()) { continue; }
            any_new = true;
            // Specialize using the module's var_table
            let orig_body_ptr_hash = {
                let orig = &program.modules[mi].functions[fi];
                orig.name.to_string()
            };
            // Borrow split: take fn out, specialize against the module's var_table, put back both
            let orig = program.modules[mi].functions[fi].clone();
            let mod_vt = &mut program.modules[mi].var_table;
            let specialized = specialize_function(&orig, &suffix, &bindings, mod_vt);
            let new_name = specialized.name.to_string();
            let _ = orig_body_ptr_hash;
            program.modules[mi].functions.push(specialized);
            rename.insert(key, new_name);
        }

        if !any_new { break; }
    }

    if rename.is_empty() { return; }

    // Rewrite call sites: Module { m, f } + suffix context → Module { m, f_suffix }
    // The suffix for each call site is determined by the bindings we computed above;
    // we re-discover to apply. Simpler: re-walk the program and for each Module call
    // matching a generic, recompute suffix from arg types and look up `rename`.
    struct Rewriter<'a> {
        generics: &'a [ModuleGeneric],
        param_types: &'a [Vec<Ty>],
        rename: &'a HashMap<(String, String, String), String>,
        module_names: &'a [String],
    }
    impl<'a> IrMutVisitor for Rewriter<'a> {
        fn visit_expr_mut(&mut self, expr: &mut almide_ir::IrExpr) {
            walk_expr_mut(self, expr);
            if let IrExprKind::Call { target: CallTarget::Module { module, func }, args, .. } = &mut expr.kind {
                let m = module.as_str().to_string();
                let f = func.as_str().to_string();
                for (gi, g) in self.generics.iter().enumerate() {
                    if g.name != f { continue; }
                    if self.module_names[g.mi] != m { continue; }
                    let bindings = collect_mono_bindings(&g.bounds, args, &self.param_types[gi]);
                    let all_concrete = !bindings.is_empty() && bindings.values().all(|ty|
                        !matches!(ty, Ty::Unknown) && !ty.contains_unknown()
                        && !matches!(ty, Ty::TypeVar(_))
                        && !ty.contains_typevar()
                    );
                    if !all_concrete { break; }
                    let generic_names: std::collections::HashSet<String> = g.bounds.iter()
                        .map(|b| b.type_var.clone()).collect();
                    let mut sorted: Vec<String> = generic_names.into_iter().collect();
                    sorted.sort();
                    let suffix = sorted.iter()
                        .filter_map(|gn| bindings.get(gn))
                        .filter_map(|t| ty_to_name(t))
                        .collect::<Vec<_>>()
                        .join("_");
                    if let Some(new_name) = self.rename.get(&(m.clone(), f.clone(), suffix)) {
                        *func = sym(new_name);
                    }
                    break;
                }
            }
        }
    }

    let param_types: Vec<Vec<Ty>> = generics.iter().map(|g| {
        program.modules[g.mi].functions[g.fi].params.iter().map(|p| p.ty.clone()).collect()
    }).collect();
    let module_names: Vec<String> = program.modules.iter().map(|m| m.name.to_string()).collect();

    let mut rw = Rewriter {
        generics: &generics,
        param_types: &param_types,
        rename: &rename,
        module_names: &module_names,
    };
    for func in &mut program.functions {
        rw.visit_expr_mut(&mut func.body);
    }
    for tl in &mut program.top_lets {
        rw.visit_expr_mut(&mut tl.value);
    }
    for mi in 0..program.modules.len() {
        for fi in 0..program.modules[mi].functions.len() {
            let mut body = std::mem::replace(&mut program.modules[mi].functions[fi].body, almide_ir::IrExpr {
                kind: IrExprKind::Unit, ty: Ty::Unit, span: None,
            });
            rw.visit_expr_mut(&mut body);
            program.modules[mi].functions[fi].body = body;
        }
        for ti in 0..program.modules[mi].top_lets.len() {
            let mut val = std::mem::replace(&mut program.modules[mi].top_lets[ti].value, almide_ir::IrExpr {
                kind: IrExprKind::Unit, ty: Ty::Unit, span: None,
            });
            rw.visit_expr_mut(&mut val);
            program.modules[mi].top_lets[ti].value = val;
        }
    }

    // Remove generic source fns that have at least one specialization.
    let specialized_origins: std::collections::HashSet<(String, String)> = rename.keys()
        .map(|(m, f, _)| (m.clone(), f.clone())).collect();
    for module in &mut program.modules {
        let mname = module.name.to_string();
        module.functions.retain(|f| !specialized_origins.contains(&(mname.clone(), f.name.to_string()))
            || !f.generics.as_ref().map_or(false, |g| !g.is_empty()));
    }
}

/// After monomorphization, no TypeVars of any kind should remain in the IR.
/// Generic type params (A, B, T) should have been substituted by monomorphization.
/// Inference vars (?0, ?1) should have been resolved by the type checker.
fn verify_no_typevars_post_mono(program: &almide_ir::IrProgram) {
    use almide_lang::types::Ty;
    fn has_any_typevar(ty: &Ty) -> bool {
        match ty {
            Ty::TypeVar(_) => true,
            Ty::Applied(_, args) => args.iter().any(has_any_typevar),
            Ty::Tuple(elems) => elems.iter().any(has_any_typevar),
            Ty::Fn { params, ret } => params.iter().any(has_any_typevar) || has_any_typevar(ret),
            Ty::Named(_, args) => args.iter().any(has_any_typevar),
            Ty::Record { fields } | Ty::OpenRecord { fields } => fields.iter().any(|(_, t)| has_any_typevar(t)),
            _ => false,
        }
    }
    let mut count = 0;
    for func in &program.functions {
        if has_any_typevar(&func.ret_ty) { count += 1; }
        for p in &func.params { if has_any_typevar(&p.ty) { count += 1; } }
    }
    for i in 0..program.var_table.len() {
        let info = program.var_table.get(almide_ir::VarId(i as u32));
        if has_any_typevar(&info.ty) { count += 1; }
    }
    if count > 0 {
        eprintln!("[ICE] {} TypeVar(s) remain after monomorphization. Generic params should be fully substituted.", count);
    }
}

/// Find functions that have structural bounds, protocol bounds, on generic type parameters,
/// OR direct OpenRecord parameters.
/// Returns function_name → list of bounded params.
fn find_structurally_bounded_fns(functions: &[IrFunction], type_decls: &[IrTypeDecl]) -> HashMap<String, Vec<BoundedParam>> {
    let mut result = HashMap::new();
    for func in functions {
        let mut bounded = Vec::new();
        let mut seen_tvars = std::collections::HashSet::new();
        // パターン A: generic functions (with or without structural bounds)
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
        // パターン A2: generic + protocol bound (fn f[T: Showable](x: T))
        if let Some(ref generics) = func.generics {
            for g in generics.iter() {
                if let Some(ref bounds) = g.bounds {
                    if !bounds.is_empty() && !seen_tvars.contains(&g.name) {
                        for (i, param) in func.params.iter().enumerate() {
                            if ty_contains_typevar(&param.ty, &g.name) {
                                bounded.push(BoundedParam { param_idx: i, type_var: g.name.to_string() });
                            }
                        }
                    }
                }
            }
        }
        // パターン B: 直接 OpenRecord パラメータ、または OpenRecord エイリアス
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
        // Include all generic functions, even those with no param-based TypeVars
        // (e.g., stack_new[T]() — no params, but has generics and type_args at call site)
        if !bounded.is_empty() || func.generics.as_ref().map_or(false, |g| !g.is_empty()) {
            result.insert(func.name.to_string(), bounded);
        }
    }
    result
}
