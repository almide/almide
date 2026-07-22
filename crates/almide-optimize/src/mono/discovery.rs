use std::collections::HashMap;
use almide_ir::*;
use almide_ir::visit::{IrVisitor, walk_expr};
use almide_lang::types::Ty;
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
    let mut visitor = DiscoverVisitor {
        bound_fns,
        program_functions: &program.functions,
        instances: HashMap::new(),
    };
    for func in &program.functions {
        visitor.visit_expr(&func.body);
    }
    for tl in &program.top_lets {
        visitor.visit_expr(&tl.value);
    }
    visitor.instances
}

/// Discover instances only in the given frontier functions (newly added specializations).
pub(super) fn discover_instances_in_frontier(
    frontier: &[IrFunction],
    bound_fns: &HashMap<String, Vec<BoundedParam>>,
    all_fns: &[IrFunction],
) -> HashMap<MonoKey, HashMap<String, Ty>> {
    let mut visitor = DiscoverVisitor {
        bound_fns,
        program_functions: all_fns,
        instances: HashMap::new(),
    };
    for func in frontier {
        visitor.visit_expr(&func.body);
    }
    visitor.instances
}

/// Also used by monomorphize_module_fns (which supplies its own expr).
pub(super) fn discover_in_expr(
    expr: &IrExpr,
    bound_fns: &HashMap<String, Vec<BoundedParam>>,
    program_functions: &[IrFunction],
    instances: &mut HashMap<MonoKey, HashMap<String, Ty>>,
) {
    let mut visitor = DiscoverVisitor { bound_fns, program_functions, instances: HashMap::new() };
    visitor.visit_expr(expr);
    instances.extend(visitor.instances);
}

// ── Visitor implementation ────────────────────────────────────────

struct DiscoverVisitor<'a> {
    bound_fns: &'a HashMap<String, Vec<BoundedParam>>,
    program_functions: &'a [IrFunction],
    instances: HashMap<MonoKey, HashMap<String, Ty>>,
}

impl<'a> IrVisitor for DiscoverVisitor<'a> {
    fn visit_expr(&mut self, expr: &IrExpr) {
        if let IrExprKind::Call { target, args, type_args } = &expr.kind {
            self.check_call(expr, target, args, type_args);
        }
        walk_expr(self, expr);
    }
}

impl<'a> DiscoverVisitor<'a> {
    fn check_call(&mut self, expr: &IrExpr, target: &CallTarget, args: &[IrExpr], type_args: &[Ty]) {
        // UFCS Method calls: check if method name matches a generic function.
        if let CallTarget::Method { object, method } = target {
            if let Some(bounded_params) = self.bound_fns.get::<str>(method) {
                let orig_fn = self.program_functions.iter().find(|f| !f.is_test && f.name == *method);
                let param_types: Vec<Ty> = orig_fn
                    .map(|f| f.params.iter().map(|p| p.ty.clone()).collect())
                    .unwrap_or_default();
                // Synthetic args: [object, ...args]
                let mut ufcs_args: Vec<IrExpr> = vec![(**object).clone()];
                ufcs_args.extend(args.iter().cloned());
                let mut bindings = collect_mono_bindings(bounded_params, &ufcs_args, &param_types);
                // Infer from return type
                self.infer_from_return_type(orig_fn, expr, &mut bindings);
                self.try_insert(method.to_string(), bindings);
            }
        }
        if let CallTarget::Named { name } = target {
            if let Some(bounded_params) = self.bound_fns.get::<str>(name) {
                let orig_fn = self.program_functions.iter().find(|f| !f.is_test && f.name == *name);
                let param_types: Vec<Ty> = orig_fn
                    .map(|f| f.params.iter().map(|p| p.ty.clone()).collect())
                    .unwrap_or_default();
                let mut bindings = collect_mono_bindings(bounded_params, args, &param_types);
                // Explicit type_args
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
                // Infer from return type
                self.infer_from_return_type(orig_fn, expr, &mut bindings);
                self.try_insert(name.to_string(), bindings);
            }
        }
    }

    fn infer_from_return_type(
        &self,
        orig_fn: Option<&IrFunction>,
        expr: &IrExpr,
        bindings: &mut HashMap<String, Ty>,
    ) {
        if bindings.values().any(|ty| matches!(ty, Ty::Unknown)) || bindings.is_empty() {
            if let Some(func) = orig_fn {
                if let Some(ref generics) = func.generics {
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
    }

    fn try_insert(&mut self, name: String, bindings: HashMap<String, Ty>) {
        let all_concrete = !bindings.is_empty() && bindings.values().all(|ty|
            !matches!(ty, Ty::Unknown) && !ty.contains_unknown()
            && !matches!(ty, Ty::TypeVar(_))
            && !ty.contains_typevar()
        );
        if all_concrete {
            let suffix = mangle_suffix(&bindings);
            self.instances.insert((name, suffix), bindings);
        }
    }
}

// ── Statement discovery (delegated to visitor via walk) ──────────

pub(super) fn discover_in_stmt(
    stmt: &IrStmt,
    bound_fns: &HashMap<String, Vec<BoundedParam>>,
    program_functions: &[IrFunction],
    instances: &mut HashMap<MonoKey, HashMap<String, Ty>>,
) {
    use almide_ir::visit::walk_stmt;
    let mut visitor = DiscoverVisitor { bound_fns, program_functions, instances: HashMap::new() };
    visitor.visit_stmt(stmt);
    instances.extend(visitor.instances);
}

/// Extract the concrete type for a TypeVar by matching parameter type structure against argument type.
pub(super) fn extract_typevar_binding(param_ty: &Ty, arg_ty: &Ty, var_name: &str) -> Ty {
    match (param_ty, arg_ty) {
        (Ty::TypeVar(n), _) if n == var_name => arg_ty.clone(),
        (Ty::Named(n, _), _) if n == var_name => arg_ty.clone(),
        (Ty::OpenRecord { .. }, _) if var_name.starts_with("__open_") => arg_ty.clone(),
        (Ty::Named(_, _), _) if var_name.starts_with("__open_") => arg_ty.clone(),
        (Ty::Fn { params: p_params, ret: p_ret }, Ty::Fn { params: a_params, ret: a_ret }) if p_params.len() == a_params.len() =>
            extract_typevar_binding_fn(p_params, p_ret, a_params, a_ret, var_name),
        // A QUALIFIED/bare Named pair (`cell.Cell[T]` param vs a bare `Cell[Int]`
        // arg, or vice versa) is the SAME nominal type under the checker's
        // names_match doctrine — descend into the type args instead of letting
        // the constructor_id string mismatch bind T to Unknown (#788's second
        // layer: `next_id.get()` never specialized, E0425 on the unsuffixed
        // call while its sibling `update` survived via the lambda arg).
        (Ty::Named(pn, p_args), Ty::Named(an, a_args))
            if p_args.len() == a_args.len()
                && !p_args.is_empty()
                && pn.as_str().rsplit('.').next() == an.as_str().rsplit('.').next() =>
            extract_typevar_binding_named_pair(p_args, a_args, var_name),
        _ => extract_typevar_binding_fallback(param_ty, arg_ty, var_name),
    }
}

fn extract_typevar_binding_fn(p_params: &[Ty], p_ret: &Ty, a_params: &[Ty], a_ret: &Ty, var_name: &str) -> Ty {
    for (p, a) in p_params.iter().zip(a_params.iter()) {
        let r = extract_typevar_binding(p, a, var_name);
        if !matches!(r, Ty::Unknown) { return r; }
    }
    extract_typevar_binding(p_ret, a_ret, var_name)
}

fn extract_typevar_binding_named_pair(p_args: &[Ty], a_args: &[Ty], var_name: &str) -> Ty {
    for (p, a) in p_args.iter().zip(a_args.iter()) {
        let r = extract_typevar_binding(p, a, var_name);
        if !matches!(r, Ty::Unknown) {
            return r;
        }
    }
    Ty::Unknown
}

fn extract_typevar_binding_fallback(param_ty: &Ty, arg_ty: &Ty, var_name: &str) -> Ty {
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
    if let (Ty::Tuple(pts), Ty::Tuple(ats)) = (param_ty, arg_ty) {
        if pts.len() == ats.len() {
            for (p, a) in pts.iter().zip(ats.iter()) {
                let r = extract_typevar_binding(p, a, var_name);
                if !matches!(r, Ty::Unknown) { return r; }
            }
        }
    }
    Ty::Unknown
}
