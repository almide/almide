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
