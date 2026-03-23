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
use crate::ir::*;
use crate::types::Ty;

use utils::{MonoKey, BoundedParam, has_typevar, ty_contains_typevar};
use discovery::{discover_instances, discover_instances_in_frontier};
use specialization::{specialize_function, substitute_ty, update_var_table_types};
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

        // Specialize new functions
        let mut new_functions = Vec::new();
        for ((fn_name, suffix), bindings) in &new {
            if let Some(orig) = program.functions.iter().find(|f| f.name == *fn_name) {
                new_functions.push(specialize_function(orig, suffix, bindings));
            }
        }

        // Update VarTable types for specialized functions
        for (func, ((_, _), bindings)) in new_functions.iter().zip(new.iter()) {
            update_var_table_types(&func.body, bindings, &mut program.var_table);
            for param in &func.params {
                let old = &program.var_table.get(param.var).ty;
                let new_ty = substitute_ty(old, bindings);
                program.var_table.entries[param.var.0 as usize].ty = new_ty;
            }
        }

        // Rewrite call sites (all instances, including previous rounds)
        all_instances.extend(new);
        rewrite_calls(program, &bound_fns, &all_instances);

        // Track frontier: next round only scans these new functions
        frontier_start = Some(program.functions.len());
        program.functions.extend(new_functions);
    }

    // Remove generic functions: both those with specialized instances AND
    // those with no call sites (unused generics still carry TypeVars)
    let mono_fn_names: std::collections::HashSet<String> = all_instances.keys().map(|(name, _)| name.clone()).collect();
    program.functions.retain(|f| {
        if mono_fn_names.contains(&f.name) { return false; } // replaced by specialized
        // Also remove generic functions with no instances (unused)
        if f.generics.as_ref().map_or(false, |g| !g.is_empty()) && !f.is_test {
            return false;
        }
        true
    });

    // Propagate concrete types: after rewrite, some expressions still have TypeVar
    // types (e.g., `let x = mono_fn(...)` where x.ty was set before mono).
    propagate_concrete_types(program);

}

#[allow(dead_code)]
fn audit_remaining_typevars(program: &IrProgram) {
    for func in &program.functions {
        audit_expr(&func.body, &func.name, &program.var_table);
        for param in &func.params {
            if has_typevar(&param.ty) {
                eprintln!("[AUDIT] fn {} param '{}' ty={:?}", func.name, param.name, param.ty);
            }
        }
        if has_typevar(&func.ret_ty) {
            eprintln!("[AUDIT] fn {} ret_ty={:?}", func.name, func.ret_ty);
        }
    }
}

#[allow(dead_code)]
fn audit_expr(expr: &IrExpr, fn_name: &str, vt: &VarTable) {
    if has_typevar(&expr.ty) {
        let kind_name = match &expr.kind {
            IrExprKind::Var { id } => format!("Var({}:'{}')", id.0, vt.get(*id).name),
            IrExprKind::Call { target, type_args, .. } => format!("Call({:?}, type_args={:?})", target, type_args),
            IrExprKind::Match { .. } => "Match".to_string(),
            IrExprKind::LitInt { .. } => "LitInt".to_string(),
            IrExprKind::Block { .. } => "Block".to_string(),
            _ => format!("{:?}", std::mem::discriminant(&expr.kind)),
        };
        eprintln!("[AUDIT] fn {} expr {} ty={:?}", fn_name, kind_name, expr.ty);
    }
    // Recurse
    match &expr.kind {
        IrExprKind::BinOp { left, right, .. } => { audit_expr(left, fn_name, vt); audit_expr(right, fn_name, vt); }
        IrExprKind::UnOp { operand, .. } => audit_expr(operand, fn_name, vt),
        IrExprKind::If { cond, then, else_ } => { audit_expr(cond, fn_name, vt); audit_expr(then, fn_name, vt); audit_expr(else_, fn_name, vt); }
        IrExprKind::Match { subject, arms } => {
            audit_expr(subject, fn_name, vt);
            for arm in arms { audit_expr(&arm.body, fn_name, vt); }
        }
        IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
            for s in stmts { audit_stmt(s, fn_name, vt); }
            if let Some(e) = expr { audit_expr(e, fn_name, vt); }
        }
        IrExprKind::Call { target, args, .. } => {
            match target { CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } => audit_expr(object, fn_name, vt), _ => {} }
            for a in args { audit_expr(a, fn_name, vt); }
        }
        IrExprKind::ForIn { iterable, body, .. } => { audit_expr(iterable, fn_name, vt); for s in body { audit_stmt(s, fn_name, vt); } }
        IrExprKind::While { cond, body } => { audit_expr(cond, fn_name, vt); for s in body { audit_stmt(s, fn_name, vt); } }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => { for e in elements { audit_expr(e, fn_name, vt); } }
        IrExprKind::Record { fields, .. } | IrExprKind::SpreadRecord { fields, .. } => { for (_, e) in fields { audit_expr(e, fn_name, vt); } }
        IrExprKind::Lambda { body, .. } => audit_expr(body, fn_name, vt),
        IrExprKind::OptionSome { expr: e } | IrExprKind::ResultOk { expr: e } | IrExprKind::ResultErr { expr: e }
        | IrExprKind::Try { expr: e } | IrExprKind::Clone { expr: e } | IrExprKind::Deref { expr: e } => audit_expr(e, fn_name, vt),
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } | IrExprKind::IndexAccess { object, .. } => audit_expr(object, fn_name, vt),
        IrExprKind::StringInterp { parts } => { for p in parts { if let IrStringPart::Expr { expr: e } = p { audit_expr(e, fn_name, vt); } } }
        _ => {}
    }
}

#[allow(dead_code)]
fn audit_stmt(stmt: &IrStmt, fn_name: &str, vt: &VarTable) {
    match &stmt.kind {
        IrStmtKind::Bind { var, ty, value, .. } => {
            if has_typevar(ty) {
                eprintln!("[AUDIT] fn {} Bind {:?} '{}' ty={:?} value.ty={:?}", fn_name, var, vt.get(*var).name, ty, value.ty);
            }
            audit_expr(value, fn_name, vt);
        }
        IrStmtKind::BindDestructure { value, .. } | IrStmtKind::Assign { value, .. } => audit_expr(value, fn_name, vt),
        IrStmtKind::IndexAssign { index, value, .. } => { audit_expr(index, fn_name, vt); audit_expr(value, fn_name, vt); }
        IrStmtKind::Expr { expr } => audit_expr(expr, fn_name, vt),
        IrStmtKind::Guard { cond, else_ } => { audit_expr(cond, fn_name, vt); audit_expr(else_, fn_name, vt); }
        _ => {}
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
                            .map(|(i, _)| BoundedParam { param_idx: i, type_var: g.name.clone() })
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
                                bounded.push(BoundedParam { param_idx: i, type_var: g.name.clone() });
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
            result.insert(func.name.clone(), bounded);
        }
    }
    result
}
