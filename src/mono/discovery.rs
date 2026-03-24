use std::collections::HashMap;
use crate::ir::*;
use crate::types::Ty;
use super::utils::{MonoKey, BoundedParam, mangle_suffix};

/// Collect type variable bindings for a monomorphization call site.
pub(super) fn collect_mono_bindings(
    bounded_params: &[BoundedParam],
    args: &[IrExpr],
    param_types: &[Ty],
) -> HashMap<String, Ty> {
    bounded_params.iter()
        .filter(|bp| bp.param_idx < args.len())
        .map(|bp| {
            let arg_ty = &args[bp.param_idx].ty;
            let binding = param_types.get(bp.param_idx)
                .map(|pt| extract_typevar_binding(pt, arg_ty, &bp.type_var))
                .unwrap_or_else(|| arg_ty.clone());
            (bp.type_var.clone(), binding)
        })
        .collect()
}

/// Discover all concrete instantiations of structurally-bounded functions.
/// Scans all functions and top-level lets.
pub(super) fn discover_instances(
    program: &IrProgram,
    bound_fns: &HashMap<String, Vec<BoundedParam>>,
) -> HashMap<MonoKey, HashMap<String, Ty>> {
    let mut instances: HashMap<MonoKey, HashMap<String, Ty>> = HashMap::new();

    let fns = &program.functions;
    for func in fns {
        discover_in_expr(&func.body, bound_fns, fns, &mut instances);
    }
    for tl in &program.top_lets {
        discover_in_expr(&tl.value, bound_fns, fns, &mut instances);
    }

    instances
}

/// Discover instances only in the given frontier functions (newly added specializations).
/// Uses all_fns for looking up original function signatures.
pub(super) fn discover_instances_in_frontier(
    frontier: &[IrFunction],
    bound_fns: &HashMap<String, Vec<BoundedParam>>,
    all_fns: &[IrFunction],
) -> HashMap<MonoKey, HashMap<String, Ty>> {
    let mut instances: HashMap<MonoKey, HashMap<String, Ty>> = HashMap::new();
    for func in frontier {
        discover_in_expr(&func.body, bound_fns, all_fns, &mut instances);
    }
    instances
}

pub(super) fn discover_in_expr(
    expr: &IrExpr,
    bound_fns: &HashMap<String, Vec<BoundedParam>>,
    program_functions: &[IrFunction],
    instances: &mut HashMap<MonoKey, HashMap<String, Ty>>,
) {
    match &expr.kind {
        IrExprKind::Call { target, args, type_args } => {
            if let CallTarget::Named { name } = target {
                if let Some(bounded_params) = bound_fns.get::<str>(name) {
                    // Find the original function to get parameter types and generics
                    let orig_fn = program_functions.iter().find(|f| f.name == *name);
                    let param_types: Vec<Ty> = orig_fn
                        .map(|f| f.params.iter().map(|p| p.ty.clone()).collect())
                        .unwrap_or_default();

                    let mut bindings = collect_mono_bindings(bounded_params, args, &param_types);

                    // Also collect bindings from explicit type_args (e.g., stack_new[Int]())
                    if !type_args.is_empty() {
                        if let Some(func) = orig_fn {
                            if let Some(ref generics) = func.generics {
                                for (g, ta) in generics.iter().zip(type_args.iter()) {
                                    if !bindings.contains_key(&*g.name) {
                                        bindings.insert(g.name.to_string(), ta.clone());
                                    }
                                }
                            }
                        }
                    }

                    // Also try: infer from return type usage
                    // If the call result is stored in a typed variable, use that type
                    if bindings.values().any(|ty| matches!(ty, Ty::Unknown)) || bindings.is_empty() {
                        if let Some(func) = orig_fn {
                            if let Some(ref generics) = func.generics {
                                // Try to infer from call expr.ty vs function ret_ty
                                let ret_ty = &func.ret_ty;
                                for g in generics {
                                    if !bindings.contains_key(&*g.name) || matches!(bindings.get(&*g.name), Some(Ty::Unknown)) {
                                        let extracted = extract_typevar_binding(ret_ty, &expr.ty, &g.name);
                                        if !matches!(extracted, Ty::Unknown) {
                                            bindings.insert(g.name.to_string(), extracted);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Skip bindings with Unknown or unresolved inference vars
                    let all_concrete = !bindings.is_empty() && bindings.values().all(|ty|
                        !matches!(ty, Ty::Unknown) && !ty.contains_unknown()
                        && !matches!(ty, Ty::TypeVar(n) if n.starts_with('?'))
                    );
                    if all_concrete {
                        let suffix = mangle_suffix(&bindings);
                        instances.insert((name.to_string(), suffix), bindings);
                    }
                }
            }
            for arg in args { discover_in_expr(arg, bound_fns, program_functions, instances); }
            match target {
                CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } => {
                    discover_in_expr(object, bound_fns, program_functions, instances);
                }
                _ => {}
            }
        }
        IrExprKind::BinOp { left, right, .. } => {
            discover_in_expr(left, bound_fns, program_functions, instances);
            discover_in_expr(right, bound_fns, program_functions, instances);
        }
        IrExprKind::UnOp { operand, .. } => discover_in_expr(operand, bound_fns, program_functions, instances),
        IrExprKind::If { cond, then, else_ } => {
            discover_in_expr(cond, bound_fns, program_functions, instances);
            discover_in_expr(then, bound_fns, program_functions, instances);
            discover_in_expr(else_, bound_fns, program_functions, instances);
        }
        IrExprKind::Match { subject, arms } => {
            discover_in_expr(subject, bound_fns, program_functions, instances);
            for arm in arms {
                if let Some(g) = &arm.guard { discover_in_expr(g, bound_fns, program_functions, instances); }
                discover_in_expr(&arm.body, bound_fns, program_functions, instances);
            }
        }
        IrExprKind::Block { stmts, expr } => {
            for s in stmts { discover_in_stmt(s, bound_fns, program_functions, instances); }
            if let Some(e) = expr { discover_in_expr(e, bound_fns, program_functions, instances); }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            discover_in_expr(iterable, bound_fns, program_functions, instances);
            for s in body { discover_in_stmt(s, bound_fns, program_functions, instances); }
        }
        IrExprKind::While { cond, body } => {
            discover_in_expr(cond, bound_fns, program_functions, instances);
            for s in body { discover_in_stmt(s, bound_fns, program_functions, instances); }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { discover_in_expr(e, bound_fns, program_functions, instances); }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, e) in fields { discover_in_expr(e, bound_fns, program_functions, instances); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            discover_in_expr(base, bound_fns, program_functions, instances);
            for (_, e) in fields { discover_in_expr(e, bound_fns, program_functions, instances); }
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries {
                discover_in_expr(k, bound_fns, program_functions, instances);
                discover_in_expr(v, bound_fns, program_functions, instances);
            }
        }
        IrExprKind::Range { start, end, .. } => {
            discover_in_expr(start, bound_fns, program_functions, instances);
            discover_in_expr(end, bound_fns, program_functions, instances);
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            discover_in_expr(object, bound_fns, program_functions, instances);
        }
        IrExprKind::IndexAccess { object, index } => {
            discover_in_expr(object, bound_fns, program_functions, instances);
            discover_in_expr(index, bound_fns, program_functions, instances);
        }
        IrExprKind::MapAccess { object, key } => {
            discover_in_expr(object, bound_fns, program_functions, instances);
            discover_in_expr(key, bound_fns, program_functions, instances);
        }
        IrExprKind::Lambda { body, .. } => discover_in_expr(body, bound_fns, program_functions, instances),
        IrExprKind::StringInterp { parts } => {
            for part in parts {
                if let IrStringPart::Expr { expr } = part {
                    discover_in_expr(expr, bound_fns, program_functions, instances);
                }
            }
        }
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
        | IrExprKind::Await { expr } => discover_in_expr(expr, bound_fns, program_functions, instances),
        _ => {}
    }
}

pub(super) fn discover_in_stmt(
    stmt: &IrStmt,
    bound_fns: &HashMap<String, Vec<BoundedParam>>,
    program_functions: &[IrFunction],
    instances: &mut HashMap<MonoKey, HashMap<String, Ty>>,
) {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } => discover_in_expr(value, bound_fns, program_functions, instances),
        IrStmtKind::IndexAssign { index, value, .. } => {
            discover_in_expr(index, bound_fns, program_functions, instances);
            discover_in_expr(value, bound_fns, program_functions, instances);
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            discover_in_expr(key, bound_fns, program_functions, instances);
            discover_in_expr(value, bound_fns, program_functions, instances);
        }
        IrStmtKind::FieldAssign { value, .. } => discover_in_expr(value, bound_fns, program_functions, instances),
        IrStmtKind::Expr { expr } => discover_in_expr(expr, bound_fns, program_functions, instances),
        IrStmtKind::Guard { cond, else_ } => {
            discover_in_expr(cond, bound_fns, program_functions, instances);
            discover_in_expr(else_, bound_fns, program_functions, instances);
        }
        IrStmtKind::Comment { .. } => {}
    }
}

/// Extract the concrete type for a TypeVar by matching parameter type structure against argument type.
/// Uses Ty::constructor_id() and type_args() for uniform container matching.
pub(super) fn extract_typevar_binding(param_ty: &Ty, arg_ty: &Ty, var_name: &str) -> Ty {
    match (param_ty, arg_ty) {
        (Ty::TypeVar(n), _) if n == var_name => arg_ty.clone(),
        (Ty::Named(n, _), _) if n == var_name => arg_ty.clone(),
        // OpenRecord param (or its Named alias) maps directly to the concrete arg type
        (Ty::OpenRecord { .. }, _) if var_name.starts_with("__open_") => arg_ty.clone(),
        (Ty::Named(_, _), _) if var_name.starts_with("__open_") => arg_ty.clone(),
        // Fn types: match params and return type
        (Ty::Fn { params: p_params, ret: p_ret }, Ty::Fn { params: a_params, ret: a_ret }) if p_params.len() == a_params.len() => {
            for (p, a) in p_params.iter().zip(a_params.iter()) {
                let r = extract_typevar_binding(p, a, var_name);
                if !matches!(r, Ty::Unknown) { return r; }
            }
            extract_typevar_binding(p_ret, a_ret, var_name)
        }
        _ => {
            // If same constructor, recursively match type args
            if param_ty.constructor_id() == arg_ty.constructor_id() {
                let p_args = param_ty.type_args();
                let a_args = arg_ty.type_args();
                if p_args.len() == a_args.len() {
                    for (p, a) in p_args.iter().zip(a_args.iter()) {
                        let r = extract_typevar_binding(p, a, var_name);
                        if !matches!(r, Ty::Unknown) { return r; }
                    }
                }
            }
            // Tuple: same logic via children()
            if let (Ty::Tuple(pts), Ty::Tuple(ats)) = (param_ty, arg_ty) {
                if pts.len() == ats.len() {
                    for (p, a) in pts.iter().zip(ats.iter()) {
                        let r = extract_typevar_binding(p, a, var_name);
                        if !matches!(r, Ty::Unknown) { return r; }
                    }
                    return Ty::Unknown;
                }
            }
            Ty::Unknown // no match for this var_name in this branch
        }
    }
}
